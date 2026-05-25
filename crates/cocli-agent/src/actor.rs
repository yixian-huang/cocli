//! `AgentActor<S>` — typestate machine over a per-agent subprocess.
//!
//! Phase 0a transitions implemented here:
//! - `Idle::start` — spawn bridge config + claude child, drain stdout
//!   until the first `system` event yields a session_id, then move into
//!   `Running` with the event-receiver embedded.
//! - `Running::deliver` — wrap the delivery in stream-json, write to
//!   stdin, emit ack (and accepted, when seq>0 && attempt>0).
//! - `Running::stop` — SIGTERM (or SIGKILL when force=true) the child,
//!   transition to `Stopping`.
//! - `Running::turn_cancel` — route through `DriverProcess::interrupt`
//!   (claude → SIGINT signal; codex/kimi → stdin interrupt RPC).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{broadcast, mpsc, Mutex};
use uuid::Uuid;

use cocli_bridge_config::{write_mcp_config, BridgeConfig};
use cocli_driver::{
    BusyDeliveryMode, DriverAction, DriverProcess, EncodedStdin, Event, InterruptAction,
    MessageKind, OutboundMessage, SpawnContext as DriverCtx,
};
use cocli_pidfile::write_agent_pidfile;
use cocli_protocol::{
    daemon_msg::{
        AgentActivityMsg, AgentDeliverAcceptedMsg, AgentDeliverAckMsg, AgentSessionMsg,
        AgentStatusMsg, AgentTurnMsg,
    },
    types::TrajectoryEntry,
    AgentDeliverMsg, DaemonMsg,
};

use crate::format::format_delivery_bundle;
use crate::obs::AgentObservationChanged;
use crate::state::{AgentState, Idle, RespawnCtx, Running, Stopping};
use crate::types::{AgentCmd, AgentStateChange};

pub struct AgentActor<S: AgentState> {
    pub id: String,
    pub mailbox: mpsc::Receiver<AgentCmd>,
    pub outbound: mpsc::Sender<DaemonMsg>,
    pub state_tx: mpsc::Sender<AgentStateChange>,
    pub obs_tx: broadcast::Sender<AgentObservationChanged>,
    pub state: S,
}

/// Inputs for `AgentActor<Idle>::start` — populated by `AgentRouter::handle_start`.
pub struct StartCfg {
    pub claude_binary: PathBuf,
    pub registry: std::sync::Arc<cocli_runtime_pool::RuntimeRegistry>,
    pub runtime_name: String,
    pub bridge_binary: PathBuf,
    pub workspace_root: PathBuf,
    pub server_url: String,
    pub auth_token: String,
    pub channel_id: Uuid,
    pub channel_name: String,
    pub model: String,
    pub launch_id: String,
    pub resume_session: Option<String>,
    /// System prompt passed to the driver (empty for Phase 0a claude).
    pub system_prompt: String,
    /// Extra env vars forwarded to the runtime subprocess.
    pub env_vars: HashMap<String, String>,
    /// If true, skip writing the MCP bridge config (test scenarios only).
    pub no_bridge: bool,
    /// Pre-marshalled MCP bridge argv (driver-specific; empty for claude).
    pub chat_bridge_args: Vec<String>,
}

/// Returned from `AgentActor<Idle>::start` on failure — `error_detail`
/// is sent verbatim in the subsequent `AgentStatusMsg{status:"error"}`.
#[derive(Debug)]
pub struct StartError {
    pub message: String,
}

impl std::fmt::Display for StartError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for StartError {}

impl AgentActor<Idle> {
    /// Spawn claude + collect session_id. On success returns the actor
    /// promoted to `Running` (event_rx embedded so the outer run-loop can
    /// `select!` mailbox + events together).
    ///
    /// **On failure** the caller MUST emit `AgentStatusMsg{status:"error"}`
    /// and MUST NOT emit `AgentSessionEndMsg` (no session was ever
    /// created — Go parity, codex review v2 fix #3).
    pub async fn start(self, cfg: StartCfg) -> Result<AgentActor<Running>, StartError> {
        let work_dir = cfg.workspace_root.join(&self.id);
        tokio::fs::create_dir_all(&work_dir)
            .await
            .map_err(|e| StartError {
                message: format!("mkdir workspace: {e}"),
            })?;

        // Write `.mcp-config.json` into the workspace before spawning. The
        // ClaudeDriver looks for this path verbatim when `no_bridge=false`.
        //
        // The bridge issues HTTP requests against `<server_url>/internal/agent/<id>/...`,
        // but our daemon-wide config holds the WS URL (`ws://` or `wss://`) used
        // to reach the server. Translate the scheme so the bridge can make HTTP
        // calls; otherwise it would try `ws://.../internal/agent/...` and every
        // bridge-mediated reply silently fails. Go parity: the Go daemon hides
        // this difference by injecting `BRIDGE_SERVER_URL` pointing at its own
        // local proxy. Phase A keeps it simple and points the bridge directly
        // at the upstream server.
        let bridge_http_url = derive_http_base_url(&cfg.server_url);
        if !cfg.no_bridge {
            write_mcp_config(
                &work_dir,
                &BridgeConfig {
                    bridge_binary: &cfg.bridge_binary,
                    agent_id: &self.id,
                    server_url: &bridge_http_url,
                    auth_token: &cfg.auth_token,
                },
            )
            .map_err(|e| StartError {
                message: format!("write mcp config: {e}"),
            })?;
        }

        // Capability-driven dispatch: look up the runtime in the shared
        // registry, then prepare workspace + spawn via the generic Driver
        // trait. The returned `DriverProcess` is kept around for Task 13/14,
        // which will replace the direct stdout-pump + stdin-write paths.
        let driver = cfg.registry.get(&cfg.runtime_name).ok_or(StartError {
            message: format!("runtime not available: {}", cfg.runtime_name),
        })?;

        // Merge per-agent env vars. Inject model as CHATRS_MODEL so
        // chatrs driver's prepare_workspace picks up the binding model
        // rather than its "claude-haiku-4-5" fallback. Other drivers
        // ignore unknown env vars, so this is safe to inject unconditionally.
        let mut spawn_env = cfg.env_vars.clone();
        if !cfg.model.is_empty() {
            spawn_env
                .entry("CHATRS_MODEL".to_string())
                .or_insert_with(|| cfg.model.clone());
        }

        let spawn_ctx = DriverCtx {
            agent_id: self.id.clone(),
            workdir: work_dir.clone(),
            system_prompt: cfg.system_prompt.clone(),
            env_vars: spawn_env,
            resume_session: cfg.resume_session.clone(),
            // Drivers that pass server_url through to their bridge argv
            // (codex --server-url, etc.) need the HTTP URL too — same
            // reason as the BridgeConfig above.
            server_url: bridge_http_url.clone(),
            auth_token: cfg.auth_token.clone(),
            bridge_bin_path: cfg.bridge_binary.clone(),
            no_bridge: cfg.no_bridge,
            chat_bridge_args: cfg.chat_bridge_args.clone(),
            initial_message: None,
        };
        driver
            .prepare_workspace(&spawn_ctx)
            .await
            .map_err(|e| StartError {
                message: format!("prepare_workspace: {e}"),
            })?;
        let spawn_result = driver.spawn(spawn_ctx).await.map_err(|e| StartError {
            message: format!("spawn: {e}"),
        })?;
        let mut child = spawn_result.child;

        let claude_pid = child.id().ok_or(StartError {
            message: "claude pid missing after spawn".to_string(),
        })?;
        write_agent_pidfile(&self.id, claude_pid).map_err(|e| StartError {
            message: format!("write pidfile: {e}"),
        })?;

        // stdin is None for null-stdin drivers (e.g. gemini SingleShotPerTurn).
        let mut stdin_opt = child.stdin.take();
        let stdout = child.stdout.take().ok_or(StartError {
            message: "claude stdout missing after spawn".to_string(),
        })?;

        // stdout pump: every line → Vec<Event> on `event_tx`; every line also
        // emits an `StdoutSeen` observation so HealthActor can reset its
        // idle timer. The pump task lives for the lifetime of the child.
        //
        // Note: a single line may produce multiple generic Events (e.g., an
        // assistant message containing both text + tool_use blocks once we
        // expand parse_line_to_events to fan out). We forward each one in
        // order on the channel.
        // Wrap process in Arc<Mutex> so the pump and the actor can both
        // access it: pump calls parse_line() per stdout line; actor calls
        // encode_stdin / interrupt / take_pending_actions on deliver/cancel.
        let process: Arc<Mutex<Box<dyn DriverProcess>>> =
            Arc::new(Mutex::new(spawn_result.process));

        let (event_tx, mut event_rx) = mpsc::channel::<Event>(64);
        let id_for_pump = self.id.clone();
        let obs_for_pump = self.obs_tx.clone();
        let proc_for_pump = Arc::clone(&process);
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            'pump: loop {
                match lines.next_line().await {
                    Ok(Some(line)) => {
                        let _ = obs_for_pump.send(AgentObservationChanged::StdoutSeen {
                            agent_id: id_for_pump.clone(),
                        });
                        // Use the driver's own parse_line so kimi / chatrs /
                        // gemini output is correctly decoded instead of being
                        // silently dropped by the claude parser.
                        let events = proc_for_pump.lock().await.parse_line(&line);
                        for ev in events {
                            if event_tx.send(ev).await.is_err() {
                                break 'pump; // receiver dropped — actor stopped
                            }
                        }
                    }
                    Ok(None) => break, // EOF
                    Err(e) => {
                        tracing::warn!(agent_id = %id_for_pump, error = %e, "stdout pump read error");
                        break;
                    }
                }
            }
        });

        // Step 1: Startup sequence — drivers that need explicit initialization
        // (codex: `initialize` + `thread/start`) return these lines before the
        // bootstrap. Others (claude/kimi/chatrs) return empty; gemini has no
        // stdin at all. Writes happen before bootstrap and before collect_session_id.
        let did_startup_sequence = {
            let startup_lines = process.lock().await.startup_sequence();
            if startup_lines.is_empty() {
                false
            } else {
                if let Some(ref mut stdin) = stdin_opt {
                    let mut ok = true;
                    for line in &startup_lines {
                        if let Err(e) = stdin.write_all(line.as_bytes()).await {
                            tracing::warn!(agent_id = %self.id, error = %e, "actor: startup_sequence write failed");
                            ok = false;
                            break;
                        }
                    }
                    if ok {
                        if let Err(e) = stdin.flush().await {
                            tracing::warn!(agent_id = %self.id, error = %e, "actor: startup_sequence flush failed");
                        } else {
                            tracing::info!(agent_id = %self.id, lines = startup_lines.len(), "actor: startup_sequence written");
                        }
                    }
                }
                true
            }
        };

        // Step 2: Bootstrap prompt.
        // — Drivers WITHOUT startup_sequence (kimi, chatrs): write BEFORE
        //   collect_session_id so the first stdin message unblocks their
        //   TurnBegin → SessionStarted emission.
        // — Drivers WITH startup_sequence (codex): write AFTER collect_session_id
        //   because SessionStarted comes from the thread/start response, and the
        //   bootstrap is sent as the first turn/start once the session is open.
        // — Gemini: null stdin; initial_message passed via spawn argv, skip.
        if !did_startup_sequence && !cfg.system_prompt.is_empty() {
            if let Some(ref mut stdin) = stdin_opt {
                let bootstrap_msg = OutboundMessage {
                    kind: MessageKind::User,
                    text: cfg.system_prompt.clone(),
                };
                match process.lock().await.encode_stdin(&bootstrap_msg) {
                    Ok(EncodedStdin::Bytes(data)) => {
                        if let Err(e) = stdin.write_all(data.as_bytes()).await {
                            tracing::warn!(agent_id = %self.id, error = %e, "actor: failed to write bootstrap prompt to stdin");
                        } else if let Err(e) = stdin.flush().await {
                            tracing::warn!(agent_id = %self.id, error = %e, "actor: failed to flush bootstrap prompt");
                        } else {
                            tracing::info!(agent_id = %self.id, bytes = data.len(), "actor: bootstrap prompt written to stdin");
                        }
                    }
                    Ok(EncodedStdin::Empty) => {} // null-stdin driver
                    Err(e) => {
                        tracing::warn!(agent_id = %self.id, error = %e, "actor: failed to encode bootstrap prompt");
                    }
                }
            }
        }

        // Drain events until SessionStarted or timeout. Non-session events
        // arriving before the session line are rare; we drop them per
        // Phase 0a (the agent hasn't been signalled yet so they can't be
        // meaningful tool calls). For drivers that emit SessionStarted on the
        // first TurnBegin (kimi, chatrs), the bootstrap write above unblocks
        // this wait.
        let session_id_collect_deadline = Duration::from_secs(30);
        let session_id = match tokio::time::timeout(
            session_id_collect_deadline,
            collect_session_id(&mut event_rx),
        )
        .await
        {
            Ok(Some(sid)) => sid,
            Ok(None) => {
                return Err(StartError {
                    message: "runtime exited before emitting session_id".to_string(),
                });
            }
            Err(_) => {
                return Err(StartError {
                    message: format!(
                        "session_id timeout after {}s — runtime may not support --wire/--afk",
                        session_id_collect_deadline.as_secs()
                    ),
                });
            }
        };

        // Step 4 (codex): session is now open; write the bootstrap as the
        // first turn/start. This is deferred from Step 2 for drivers that
        // use startup_sequence (thread created by thread/start, so
        // SessionStarted arrived from thread/start response; bootstrap is
        // now the first user turn, NOT the session-creation step).
        if did_startup_sequence && !cfg.system_prompt.is_empty() {
            if let Some(ref mut stdin) = stdin_opt {
                let bootstrap_msg = OutboundMessage {
                    kind: MessageKind::User,
                    text: cfg.system_prompt.clone(),
                };
                match process.lock().await.encode_stdin(&bootstrap_msg) {
                    Ok(EncodedStdin::Bytes(data)) => {
                        if let Err(e) = stdin.write_all(data.as_bytes()).await {
                            tracing::warn!(agent_id = %self.id, error = %e, "actor: post-session bootstrap write failed");
                        } else if let Err(e) = stdin.flush().await {
                            tracing::warn!(agent_id = %self.id, error = %e, "actor: post-session bootstrap flush failed");
                        } else {
                            tracing::info!(agent_id = %self.id, bytes = data.len(), "actor: post-session bootstrap written");
                        }
                    }
                    Ok(EncodedStdin::Empty) => {}
                    Err(e) => {
                        tracing::warn!(agent_id = %self.id, error = %e, "actor: post-session bootstrap encode failed");
                    }
                }
            }
        }

        // Broadcast the Started observation so HealthActor seeds its table.
        let _ = self.obs_tx.send(AgentObservationChanged::Started {
            agent_id: self.id.clone(),
            session_id: session_id.clone(),
            channel_id: cfg.channel_id,
            channel_name: cfg.channel_name.clone(),
        });

        // For Respawn drivers (gemini): store the driver + spawn context template
        // so deliver() can re-invoke driver.spawn() per turn.
        let respawn_ctx = if driver.capabilities().busy_delivery_mode == BusyDeliveryMode::Respawn {
            Some(Arc::new(RespawnCtx {
                driver: Arc::clone(&driver),
                spawn_ctx_template: DriverCtx {
                    agent_id: self.id.clone(),
                    workdir: work_dir.clone(),
                    system_prompt: cfg.system_prompt.clone(),
                    env_vars: {
                        let mut e = cfg.env_vars.clone();
                        if !cfg.model.is_empty() {
                            e.entry("CHATRS_MODEL".to_string())
                                .or_insert_with(|| cfg.model.clone());
                        }
                        e
                    },
                    resume_session: None, // overwritten per-respawn from process.session_id()
                    server_url: bridge_http_url.clone(),
                    auth_token: cfg.auth_token.clone(),
                    bridge_bin_path: cfg.bridge_binary.clone(),
                    no_bridge: cfg.no_bridge,
                    chat_bridge_args: cfg.chat_bridge_args.clone(),
                    initial_message: None, // overwritten per-respawn with the delivery body
                },
            }))
        } else {
            None
        };

        Ok(AgentActor {
            id: self.id,
            mailbox: self.mailbox,
            outbound: self.outbound,
            state_tx: self.state_tx,
            obs_tx: self.obs_tx,
            state: Running {
                process_child: child,
                process_stdin: stdin_opt,
                process,
                event_rx,
                session_id,
                channel_id: cfg.channel_id,
                channel_name: cfg.channel_name,
                last_stdout_at: Instant::now(),
                turn_count: 0,
                launch_id: cfg.launch_id,
                respawn_ctx,
            },
        })
    }
}

/// Consume `event_rx` until a `SessionStarted` arrives. Returns `None` if
/// the sender drops first (claude exited / pump died).
async fn collect_session_id(rx: &mut mpsc::Receiver<Event>) -> Option<String> {
    while let Some(ev) = rx.recv().await {
        if let Event::SessionStarted { session_id } = ev {
            return Some(session_id);
        }
    }
    None
}

impl AgentActor<Running> {
    /// Deliver a server-side message to the agent. Emits `AgentDeliverAccepted`
    /// (when seq>0 && attempt>0) before the write, and always emits
    /// `AgentDeliverAck` afterwards with the `routeAction` derived from
    /// `delivery_tier`.
    pub async fn deliver(&mut self, msg: AgentDeliverMsg) -> Result<(), String> {
        tracing::info!(
            agent_id = %self.id,
            seq = msg.seq,
            attempt = msg.attempt,
            content_len = msg.message.content.len(),
            "actor: deliver enter"
        );
        let route = route_for_tier(&msg.delivery_tier);

        if msg.seq > 0 && msg.attempt > 0 {
            let _ = self
                .outbound
                .send(DaemonMsg::AgentDeliverAccepted(AgentDeliverAcceptedMsg {
                    agent_id: self.id.clone(),
                    seq: msg.seq,
                    attempt: msg.attempt,
                    channel_id: msg.message.channel_id,
                    route_action: route.clone(),
                }))
                .await;
        }

        let body = format_delivery_bundle(&msg.message, &msg.context);
        let outbound = OutboundMessage {
            kind: MessageKind::User,
            text: body.clone(),
        };
        let encoded = self
            .state
            .process
            .lock()
            .await
            .encode_stdin(&outbound)
            .map_err(|e| format!("encode_stdin: {e}"))?;

        let write_result: Result<(), String> = match encoded {
            EncodedStdin::Bytes(payload) => {
                if let Some(ref mut stdin) = self.state.process_stdin {
                    tracing::info!(
                        agent_id = %self.id,
                        seq = msg.seq,
                        bytes = payload.len(),
                        "actor: writing to stdin"
                    );
                    stdin
                        .write_all(payload.as_bytes())
                        .await
                        .map_err(|e| format!("stdin write: {e}"))?;
                    stdin
                        .flush()
                        .await
                        .map_err(|e| format!("stdin flush: {e}"))?;
                    tracing::info!(
                        agent_id = %self.id,
                        seq = msg.seq,
                        bytes = payload.len(),
                        "actor: stdin write+flush ok"
                    );
                } else {
                    tracing::warn!(
                        agent_id = %self.id,
                        seq = msg.seq,
                        bytes = payload.len(),
                        "actor: stdin is null (SingleShot driver) but encode_stdin returned Bytes"
                    );
                }
                Ok(())
            }
            EncodedStdin::Empty => {
                // SingleShotPerTurn driver (e.g. gemini): re-spawn with the
                // delivery body as the new process's initial_message.
                if let Some(respawn_ctx) = self.state.respawn_ctx.clone() {
                    self.respawn_and_deliver(&body, &respawn_ctx)
                        .await
                        .map_err(|e| format!("respawn_and_deliver: {e}"))
                } else {
                    tracing::warn!(
                        agent_id = %self.id,
                        "encode_stdin returned Empty but no respawn_ctx — message dropped"
                    );
                    Ok(())
                }
            }
        };

        // Drain pending actions from the process state machine. For most
        // drivers this is empty; codex uses it for P3/G rejected-steer replay.
        for action in self.state.process.lock().await.take_pending_actions() {
            match action {
                DriverAction::WriteStdin(payload) => {
                    if let Some(ref mut stdin) = self.state.process_stdin {
                        if let Err(e) = stdin.write_all(payload.as_bytes()).await {
                            tracing::warn!(
                                agent_id = %self.id,
                                error = %e,
                                "deliver: pending_actions WriteStdin failed"
                            );
                        }
                        let _ = stdin.flush().await;
                    }
                }
                DriverAction::EmitEvent(_e) => {}
            }
        }

        // Ack only when server has a queue item to release (attempt > 0).
        // For immediate dispatch (attempt = 0) sending an ack would trigger
        // server-side "ignoring ack with missing attempt" warnings.
        if msg.attempt > 0 {
            let _ = self
                .outbound
                .send(DaemonMsg::AgentDeliverAck(AgentDeliverAckMsg {
                    agent_id: self.id.clone(),
                    seq: msg.seq,
                    attempt: msg.attempt,
                    channel_id: msg.message.channel_id,
                    route_action: route,
                }))
                .await;
        }

        write_result
    }

    /// Kill the current child and re-spawn with `message` as the
    /// `initial_message` for `BusyDeliveryMode::Respawn` drivers (gemini).
    /// Updates `self.state` in place: new child, stdin, process, event_rx.
    async fn respawn_and_deliver(&mut self, message: &str, ctx: &RespawnCtx) -> Result<(), String> {
        // Kill old process (best-effort; it may have already exited).
        let _ = self.state.process_child.kill().await;

        // Carry forward the session_id for --resume so gemini continues
        // the same conversation thread.
        let resume_session = self
            .state
            .process
            .lock()
            .await
            .session_id()
            .map(str::to_owned);

        let mut new_spawn_ctx = ctx.spawn_ctx_template.clone();
        new_spawn_ctx.initial_message = Some(message.to_string());
        new_spawn_ctx.resume_session = resume_session;

        let spawn_result = ctx
            .driver
            .spawn(new_spawn_ctx)
            .await
            .map_err(|e| format!("spawn: {e}"))?;
        let mut new_child = spawn_result.child;

        if let Some(pid) = new_child.id() {
            write_agent_pidfile(&self.id, pid).map_err(|e| format!("write pidfile: {e}"))?;
        }

        let new_stdin = new_child.stdin.take();
        let new_stdout = new_child
            .stdout
            .take()
            .ok_or_else(|| "respawn: child stdout missing".to_string())?;
        let new_stderr = new_child.stderr.take();

        let new_process: Arc<Mutex<Box<dyn DriverProcess>>> =
            Arc::new(Mutex::new(spawn_result.process));

        // Drain stderr so we can log errors from the new gemini process.
        if let Some(stderr) = new_stderr {
            let id_for_stderr = self.id.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    tracing::warn!(agent_id = %id_for_stderr, stderr = %line, "respawn: gemini stderr");
                }
            });
        }

        // New event channel; old event_rx replaced below (pump for old child
        // will notice its sender is dropped and exit cleanly).
        let (new_event_tx, new_event_rx) = mpsc::channel::<Event>(64);
        let id_for_pump = self.id.clone();
        let obs_for_pump = self.obs_tx.clone();
        let proc_for_pump = Arc::clone(&new_process);
        tokio::spawn(async move {
            let mut lines = BufReader::new(new_stdout).lines();
            'pump: loop {
                match lines.next_line().await {
                    Ok(Some(line)) => {
                        tracing::debug!(agent_id = %id_for_pump, line = %line, "respawn pump: stdout line");
                        let _ = obs_for_pump.send(AgentObservationChanged::StdoutSeen {
                            agent_id: id_for_pump.clone(),
                        });
                        let events = proc_for_pump.lock().await.parse_line(&line);
                        tracing::debug!(agent_id = %id_for_pump, event_count = events.len(), "respawn pump: parse_line emitted");
                        for ev in events {
                            if new_event_tx.send(ev).await.is_err() {
                                tracing::warn!(agent_id = %id_for_pump, "respawn pump: event_rx dropped");
                                break 'pump;
                            }
                        }
                    }
                    Ok(None) => {
                        tracing::info!(agent_id = %id_for_pump, "respawn pump: stdout EOF");
                        break;
                    }
                    Err(e) => {
                        tracing::warn!(
                            agent_id = %id_for_pump,
                            error = %e,
                            "respawn pump: read error"
                        );
                        break;
                    }
                }
            }
        });

        self.state.process_child = new_child;
        self.state.process_stdin = new_stdin;
        self.state.process = new_process;
        self.state.event_rx = new_event_rx;

        tracing::info!(agent_id = %self.id, "respawn_and_deliver: new gemini process spawned");
        Ok(())
    }

    /// Interrupt the active turn via the driver's per-spawn state machine.
    ///
    /// FPC #12 semantics: claude returns `SignalSent(SIGINT)` here — treated
    /// as "interrupt current inference, keep session/process alive". Codex /
    /// kimi (future) return `WroteToStdin(payload)` carrying their RPC
    /// interrupt envelope. Best-effort: if the child has already exited or
    /// the PID is gone, the signal branch is a no-op.
    pub async fn turn_cancel(&mut self) -> Result<(), String> {
        let action = self
            .state
            .process
            .lock()
            .await
            .interrupt()
            .map_err(|e| format!("interrupt: {e}"))?;
        match action {
            InterruptAction::WroteToStdin(payload) => {
                if let Some(ref mut stdin) = self.state.process_stdin {
                    stdin
                        .write_all(payload.as_bytes())
                        .await
                        .map_err(|e| format!("write interrupt: {e}"))?;
                    stdin
                        .flush()
                        .await
                        .map_err(|e| format!("flush interrupt: {e}"))?;
                    tracing::info!(
                        agent_id = %self.id,
                        bytes = payload.len(),
                        "actor: turn_cancel — interrupt RPC written to stdin"
                    );
                }
            }
            InterruptAction::SignalSent(sig) => {
                if let Some(pid) = self.state.process_child.id() {
                    #[cfg(unix)]
                    {
                        let res =
                            nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid as i32), sig);
                        tracing::info!(
                            agent_id = %self.id,
                            pid = pid,
                            signal = ?sig,
                            sig_ok = res.is_ok(),
                            "actor: turn_cancel — signal sent to child"
                        );
                    }
                    #[cfg(not(unix))]
                    {
                        let _ = (pid, sig);
                    }
                } else {
                    tracing::warn!(
                        agent_id = %self.id,
                        signal = ?sig,
                        "actor: turn_cancel — no child pid (already exited)"
                    );
                }
            }
        }
        Ok(())
    }

    /// Drive the actor through its `Running` lifetime: consume mailbox
    /// commands (`Deliver` / `TurnCancel` / `Stop`) and claude stdout
    /// events in a single `select!` loop. Emits `agent:activity`,
    /// `agent:turn`, and `agent:session` daemon messages as events arrive.
    ///
    /// Returns when the agent is stopped (mailbox `Stop` or event_rx
    /// closure / EOF). Phase 0a stretch — does NOT yet emit
    /// `agent:session:end` (deferred to Phase 0b).
    ///
    /// Borrow-checker note: we deliberately destructure `mailbox` and
    /// `state.event_rx` out of `self` first so the two `select!` branch
    /// futures borrow disjoint fields. The dispatched handler functions
    /// receive `&mut self` *after* the select! yields, so no borrows are
    /// held across the await point.
    pub async fn run_loop(mut self, launch_id: String) {
        // Per-turn accumulator. Reset after each AgentTurn emission.
        let mut current_turn_entries: Vec<TrajectoryEntry> = Vec::new();
        // Tracks whether we've emitted AgentSession yet (after first
        // deliver lands). Phase 0a — only the first user-deliver triggers
        // it; subsequent deliveries are no-ops on the session front.
        let mut session_emitted = false;
        let agent_id = self.id.clone();
        // For Respawn drivers (gemini): the single-shot process exits after
        // each turn, closing event_rx. We park the event_rx arm so the
        // select! doesn't spin on None. The arm is re-enabled after
        // respawn_and_deliver installs a fresh receiver.
        let is_respawn_driver = self.state.respawn_ctx.is_some();
        let mut event_rx_live = true;

        loop {
            tokio::select! {
                cmd = self.mailbox.recv() => {
                    match cmd {
                        Some(AgentCmd::Deliver(m)) => {
                            // Update channel context from the latest delivery
                            // (StartCfg.channel_id is nil — channel arrives
                            // with messages, not start).
                            self.state.channel_id = m.message.channel_id;
                            if !m.message.channel_name.is_empty() {
                                self.state.channel_name.clone_from(&m.message.channel_name);
                            }

                            if !session_emitted {
                                emit_session(
                                    &self.outbound,
                                    &agent_id,
                                    &self.state.session_id,
                                    self.state.channel_id,
                                    &launch_id,
                                ).await;
                                session_emitted = true;
                            }

                            if let Err(e) = self.deliver(m).await {
                                tracing::warn!(agent_id = %agent_id, error = %e, "run_loop: deliver failed");
                            }
                            // After a Respawn deliver, event_rx was swapped
                            // in for the new process — re-enable the arm.
                            if is_respawn_driver {
                                event_rx_live = true;
                            }
                        }
                        Some(AgentCmd::TurnCancel) => {
                            if let Err(e) = self.turn_cancel().await {
                                tracing::warn!(agent_id = %agent_id, error = %e, "run_loop: turn_cancel failed");
                            }
                        }
                        Some(AgentCmd::Stop { force }) => {
                            let _stopping = self.stop(force);
                            // Child reaped via kill_on_drop when `_stopping`
                            // is dropped at scope exit. Phase 0b will add an
                            // explicit `child.wait()` here + AgentSessionEnd.
                            break;
                        }
                        None => {
                            // Mailbox sender (router) dropped — router shut down.
                            break;
                        }
                    }
                }
                ev = self.state.event_rx.recv(), if event_rx_live => {
                    match ev {
                        Some(event) => {
                            handle_event(
                                event,
                                &mut self.state.turn_count,
                                &mut current_turn_entries,
                                &mut self.state.last_stdout_at,
                                &self.outbound,
                                &self.obs_tx,
                                &agent_id,
                                &self.state.session_id,
                                self.state.channel_id,
                                &self.state.channel_name,
                                &launch_id,
                            ).await;
                        }
                        None => {
                            if is_respawn_driver {
                                // Gemini single-shot turn finished; park event_rx
                                // until the next delivery triggers a respawn.
                                tracing::info!(
                                    agent_id = %agent_id,
                                    "run_loop: gemini turn finished, parking event_rx"
                                );
                                event_rx_live = false;
                            } else {
                                // event_rx closed — persistent process exited.
                                // Phase 0b will emit AgentSessionEnd + AgentStatus{inactive}.
                                tracing::info!(agent_id = %agent_id, "run_loop: event_rx closed, exiting");
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    /// Transition into `Stopping` by sending SIGTERM (or SIGKILL when
    /// `force=true`). The caller is responsible for reaping the child via
    /// `child.wait()` from the outer run-loop.
    pub fn stop(self, force: bool) -> AgentActor<Stopping> {
        let pid = self.state.process_child.id();
        if let Some(p) = pid {
            #[cfg(unix)]
            {
                let sig = if force {
                    nix::sys::signal::Signal::SIGKILL
                } else {
                    nix::sys::signal::Signal::SIGTERM
                };
                let _ = nix::sys::signal::kill(nix::unistd::Pid::from_raw(p as i32), sig);
            }
            let _ = p;
        }
        AgentActor {
            id: self.id,
            mailbox: self.mailbox,
            outbound: self.outbound,
            state_tx: self.state_tx,
            obs_tx: self.obs_tx,
            state: Stopping {
                started_at: Instant::now(),
                force,
            },
        }
    }
}

/// Translate a WebSocket URL into its HTTP companion so the bridge can hit
/// the same host over plain HTTP. Idempotent for already-HTTP URLs.
///
/// - `ws://x` → `http://x`
/// - `wss://x` → `https://x`
/// - anything else passes through unchanged (caller already supplied an
///   HTTP base, or a future scheme we don't recognise — we don't want to
///   silently rewrite that).
fn derive_http_base_url(server_url: &str) -> String {
    if let Some(rest) = server_url.strip_prefix("ws://") {
        format!("http://{rest}")
    } else if let Some(rest) = server_url.strip_prefix("wss://") {
        format!("https://{rest}")
    } else {
        server_url.to_string()
    }
}

/// Current wall-clock millis (used as TrajectoryEntry.ts). Falls back to
/// 0 if the system clock is somehow before UNIX epoch (impossible but
/// keeps the type non-fallible at call sites).
fn now_unix_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Emit `agent:session` once the first delivery lands (so we have a
/// channel_id to attach). Phase 0a: isNew=true, no resume, basic prompt.
async fn emit_session(
    outbound: &mpsc::Sender<DaemonMsg>,
    agent_id: &str,
    session_id: &str,
    channel_id: Uuid,
    launch_id: &str,
) {
    let _ = outbound
        .send(DaemonMsg::AgentSession(AgentSessionMsg {
            agent_id: agent_id.to_string(),
            session_id: session_id.to_string(),
            channel_id,
            is_new: true,
            resumed_from: String::new(),
            active_sessions: 1,
            queue_depth: 0,
            prompt_layer: "basic".to_string(),
            prompt_tokens: 0,
            launch_id: launch_id.to_string(),
            ..Default::default()
        }))
        .await;
}

/// Map a generic driver `Event` to the appropriate daemon-side emission(s)
/// and mutate per-turn state (entries accumulator + turn_count).
///
/// Drivers translate their runtime-specific stdout into this common enum;
/// the actor treats all runtimes uniformly from here. Variants not relevant
/// to claude (Init / CompactStarted / CompactFinished / PlanDisplay) become
/// no-ops in Phase 1 — codex/kimi drivers will exercise them later.
#[allow(clippy::too_many_arguments)]
async fn handle_event(
    event: Event,
    turn_count: &mut u64,
    entries: &mut Vec<TrajectoryEntry>,
    last_stdout_at: &mut Instant,
    outbound: &mpsc::Sender<DaemonMsg>,
    obs_tx: &broadcast::Sender<AgentObservationChanged>,
    agent_id: &str,
    session_id: &str,
    channel_id: Uuid,
    channel_name: &str,
    launch_id: &str,
) {
    *last_stdout_at = Instant::now();

    match event {
        Event::SessionStarted { .. } => {
            // First SessionStarted was already consumed by Idle::start.
            // A second one would indicate session rotation — Phase 0b handles.
        }
        Event::Init { .. } => {
            // Init covers codex/kimi-style "init" lines that may or may not
            // carry a session_id. Claude doesn't emit it; for runtimes that
            // do, Phase 0b will pull provider/model into observation state.
        }
        Event::MessageStart => {}
        Event::Thinking { text } => {
            entries.push(TrajectoryEntry {
                kind: "thinking".to_string(),
                text,
                ts: now_unix_ms(),
                ..Default::default()
            });
        }
        Event::Text { text, role: _ } => {
            entries.push(TrajectoryEntry {
                kind: "text".to_string(),
                text,
                ts: now_unix_ms(),
                ..Default::default()
            });
        }
        Event::ToolUse { id, name, input } => {
            let entry = TrajectoryEntry {
                kind: "tool_call".to_string(),
                id: id.clone(),
                text: name,
                input: Some(input),
                ts: now_unix_ms(),
                ..Default::default()
            };
            entries.push(entry.clone());
            // Emit a working activity ping so the UI surfaces the tool
            // call live (parity with Go agent_io.emitActivity tool-call
            // path). Phase 0a: minimal field set.
            let _ = outbound
                .send(DaemonMsg::AgentActivity(AgentActivityMsg {
                    agent_id: agent_id.to_string(),
                    activity: "working".to_string(),
                    attention_state: "working".to_string(),
                    entries: vec![entry],
                    channel_id,
                    channel_name: channel_name.to_string(),
                    launch_id: launch_id.to_string(),
                    ..Default::default()
                }))
                .await;
        }
        Event::ToolDone { id, output, error } => {
            let entry = TrajectoryEntry {
                kind: "tool_result".to_string(),
                id,
                result: output.unwrap_or_default(),
                error: error.unwrap_or_default(),
                ts: now_unix_ms(),
                ..Default::default()
            };
            entries.push(entry.clone());
            let _ = outbound
                .send(DaemonMsg::AgentActivity(AgentActivityMsg {
                    agent_id: agent_id.to_string(),
                    activity: "working".to_string(),
                    attention_state: "working".to_string(),
                    entries: vec![entry],
                    channel_id,
                    channel_name: channel_name.to_string(),
                    launch_id: launch_id.to_string(),
                    ..Default::default()
                }))
                .await;
        }
        Event::MessageStop { .. } => {
            // claude emits MessageStop before the `result` line; the
            // entries accumulator is flushed on TurnEnd, not here.
        }
        Event::TurnEnd {
            status: _,
            input_tokens,
            output_tokens,
            cached_input_tokens: _,
            cost_usd,
            context_window: _,
            cache_creation_tokens,
            cache_read_tokens,
        } => {
            *turn_count += 1;
            // Phase 0a: contextUsagePct = 0 (context_window plumbed but not
            // yet propagated downstream); session_type empty (chat default).
            let turn_entries = std::mem::take(entries);
            let _ = outbound
                .send(DaemonMsg::AgentTurn(AgentTurnMsg {
                    agent_id: agent_id.to_string(),
                    session_id: session_id.to_string(),
                    launch_id: launch_id.to_string(),
                    turn_number: *turn_count as i32,
                    entries: turn_entries,
                    input_tokens: input_tokens as i32,
                    output_tokens: output_tokens as i32,
                    cost_usd,
                    cache_creation_tokens: cache_creation_tokens as i32,
                    cache_read_tokens: cache_read_tokens as i32,
                    channel_id,
                    channel_name: channel_name.to_string(),
                    context_usage_pct: 0.0,
                    ..Default::default()
                }))
                .await;
            let _ = obs_tx.send(AgentObservationChanged::TurnEnded {
                agent_id: agent_id.to_string(),
                turn_count: *turn_count,
            });
        }
        Event::RateLimit { .. } => {
            // Phase 0a: surface as a no-op; activity msg with rate-limit
            // fields is deferred to Phase 0b.
        }
        Event::Error {
            message,
            code,
            retryable: _,
            terminal: _,
            overflow: _,
        } => {
            tracing::warn!(
                agent_id = %agent_id,
                code = ?code,
                error = %message,
                "run_loop: driver error event"
            );
            // Phase 0a stretch: non-fatal — emit AgentStatus{error} but
            // do NOT kill the loop. Next deliver will retry. The retryable
            // / terminal / overflow flags are reserved for Phase 0b.
            let _ = outbound
                .send(DaemonMsg::AgentStatus(AgentStatusMsg {
                    agent_id: agent_id.to_string(),
                    status: "error".to_string(),
                    launch_id: launch_id.to_string(),
                    error_detail: message,
                }))
                .await;
        }
        Event::CompactStarted | Event::CompactFinished => {
            // kimi-style context-window compaction lifecycle; claude does
            // not emit. Phase 0b will broadcast a compact observation.
        }
        Event::PlanDisplay { .. } => {
            // codex-style "[plan] ..." trajectory entry. Phase 0b will
            // push a TrajectoryEntry{kind:"plan"} here.
        }
        Event::Unknown => {}
    }
}

/// Map a server-side `delivery_tier` to the daemon-side `routeAction` we
/// echo back in the deliver ack:
/// - empty or unknown   → "inbox"
/// - "digest" | "tierDigest"   → "tierDigest"
/// - "delayed" | "tierDelayed" → "tierDelayed"
fn route_for_tier(tier: &str) -> String {
    match tier {
        "digest" | "tierDigest" => "tierDigest".to_string(),
        "delayed" | "tierDelayed" => "tierDelayed".to_string(),
        _ => "inbox".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_for_tier_known_values() {
        assert_eq!(route_for_tier(""), "inbox");
        assert_eq!(route_for_tier("inbox"), "inbox");
        assert_eq!(route_for_tier("digest"), "tierDigest");
        assert_eq!(route_for_tier("tierDigest"), "tierDigest");
        assert_eq!(route_for_tier("delayed"), "tierDelayed");
        assert_eq!(route_for_tier("tierDelayed"), "tierDelayed");
    }

    #[test]
    fn route_for_tier_unknown_falls_back_to_inbox() {
        assert_eq!(route_for_tier("garbage"), "inbox");
        assert_eq!(route_for_tier("priority"), "inbox");
    }

    #[test]
    fn derive_http_base_url_translates_ws_schemes() {
        assert_eq!(
            derive_http_base_url("ws://127.0.0.1:8090"),
            "http://127.0.0.1:8090"
        );
        assert_eq!(
            derive_http_base_url("wss://api.cocli.ai"),
            "https://api.cocli.ai"
        );
    }

    #[test]
    fn derive_http_base_url_passes_through_http() {
        assert_eq!(
            derive_http_base_url("http://localhost:8080"),
            "http://localhost:8080"
        );
        assert_eq!(
            derive_http_base_url("https://api.cocli.ai/v1"),
            "https://api.cocli.ai/v1"
        );
    }

    #[test]
    fn derive_http_base_url_passes_through_unknown() {
        // Caller may have supplied something we don't recognise (future
        // proxy proto, mistaken value); don't try to be clever.
        assert_eq!(derive_http_base_url("foo://bar"), "foo://bar");
        assert_eq!(derive_http_base_url(""), "");
    }
}
