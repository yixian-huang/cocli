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
//! - `Running::turn_cancel` — SIGINT the child (best-effort).

use std::path::PathBuf;
use std::time::{Duration, Instant};

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{broadcast, mpsc};
use uuid::Uuid;

use cocli_bridge_config::{write_mcp_config, BridgeConfig};
use cocli_driver_claude::{parse_line as parse_claude_line, spawn_claude, ClaudeEvent, SpawnContext};
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
use crate::state::{AgentState, Idle, Running, Stopping};
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
    pub bridge_binary: PathBuf,
    pub workspace_root: PathBuf,
    pub server_url: String,
    pub auth_token: String,
    pub channel_id: Uuid,
    pub channel_name: String,
    pub model: String,
    pub launch_id: String,
    pub resume_session: Option<String>,
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

        let mcp_config_path = write_mcp_config(
            &work_dir,
            &BridgeConfig {
                bridge_binary: &cfg.bridge_binary,
                agent_id: &self.id,
                server_url: &cfg.server_url,
                auth_token: &cfg.auth_token,
            },
        )
        .map_err(|e| StartError {
            message: format!("write mcp config: {e}"),
        })?;

        let mut child = spawn_claude(&SpawnContext {
            claude_binary: &cfg.claude_binary,
            working_dir: &work_dir,
            model: &cfg.model,
            mcp_config: Some(&mcp_config_path),
            resume_session: cfg.resume_session.as_deref(),
        })
        .map_err(|e| StartError {
            message: format!("spawn claude: {e}"),
        })?;

        let claude_pid = child.id().ok_or(StartError {
            message: "claude pid missing after spawn".to_string(),
        })?;
        write_agent_pidfile(&self.id, claude_pid).map_err(|e| StartError {
            message: format!("write pidfile: {e}"),
        })?;

        let stdin = child.stdin.take().ok_or(StartError {
            message: "claude stdin missing after spawn".to_string(),
        })?;
        let stdout = child.stdout.take().ok_or(StartError {
            message: "claude stdout missing after spawn".to_string(),
        })?;

        // stdout pump: every line → ClaudeEvent on `event_tx`; every line also
        // emits an `StdoutSeen` observation so HealthActor can reset its
        // idle timer. The pump task lives for the lifetime of the child.
        let (event_tx, mut event_rx) = mpsc::channel::<ClaudeEvent>(64);
        let id_for_pump = self.id.clone();
        let obs_for_pump = self.obs_tx.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            loop {
                match lines.next_line().await {
                    Ok(Some(line)) => {
                        let _ = obs_for_pump.send(AgentObservationChanged::StdoutSeen {
                            agent_id: id_for_pump.clone(),
                        });
                        let ev = parse_claude_line(&line);
                        if event_tx.send(ev).await.is_err() {
                            break; // receiver dropped — actor stopped
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

        // Drain events until SessionStarted or timeout. Non-session events
        // arriving before the session line are rare; we drop them per
        // Phase 0a (the agent hasn't been signalled yet so they can't be
        // meaningful tool calls).
        let session_id_collect_deadline = Duration::from_secs(15);
        let session_id = match tokio::time::timeout(
            session_id_collect_deadline,
            collect_session_id(&mut event_rx),
        )
        .await
        {
            Ok(Some(sid)) => sid,
            Ok(None) => {
                return Err(StartError {
                    message: "claude exited before emitting session_id".to_string(),
                });
            }
            Err(_) => {
                return Err(StartError {
                    message: format!(
                        "session_id timeout after {}s",
                        session_id_collect_deadline.as_secs()
                    ),
                });
            }
        };

        // Broadcast the Started observation so HealthActor seeds its table.
        let _ = self.obs_tx.send(AgentObservationChanged::Started {
            agent_id: self.id.clone(),
            session_id: session_id.clone(),
            channel_id: cfg.channel_id,
            channel_name: cfg.channel_name.clone(),
        });

        Ok(AgentActor {
            id: self.id,
            mailbox: self.mailbox,
            outbound: self.outbound,
            state_tx: self.state_tx,
            obs_tx: self.obs_tx,
            state: Running {
                claude: child,
                claude_stdin: stdin,
                event_rx,
                session_id,
                channel_id: cfg.channel_id,
                channel_name: cfg.channel_name,
                last_stdout_at: Instant::now(),
                turn_count: 0,
                launch_id: cfg.launch_id,
            },
        })
    }
}

/// Consume `event_rx` until a `SessionStarted` arrives. Returns `None` if
/// the sender drops first (claude exited / pump died).
async fn collect_session_id(rx: &mut mpsc::Receiver<ClaudeEvent>) -> Option<String> {
    while let Some(ev) = rx.recv().await {
        if let ClaudeEvent::SessionStarted { session_id } = ev {
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
        // session_id only goes on the wire for resume payloads; new sessions
        // omit it (parity with cocli-driver-claude::encode_user_message contract).
        let wrapped = cocli_driver_claude::encode_user_message(&body, None);
        let line = format!("{}\n", wrapped);

        let write_res: Result<(), std::io::Error> = async {
            self.state.claude_stdin.write_all(line.as_bytes()).await?;
            self.state.claude_stdin.flush().await?;
            Ok(())
        }
        .await;

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

        write_res.map_err(|e| format!("stdin write: {e}"))
    }

    /// SIGINT the child (best-effort) to abort the current turn.
    ///
    /// FPC #12 semantics: real claude treats SIGINT as "interrupt current
    /// inference, keep session/process alive". The signal is best-effort —
    /// if the child has already exited or PID is gone, this is a no-op.
    pub async fn turn_cancel(&mut self) -> Result<(), String> {
        let pid = self.state.claude.id();
        if let Some(pid) = pid {
            #[cfg(unix)]
            {
                let res = nix::sys::signal::kill(
                    nix::unistd::Pid::from_raw(pid as i32),
                    nix::sys::signal::Signal::SIGINT,
                );
                tracing::info!(
                    agent_id = %self.id,
                    pid = pid,
                    sigint_ok = res.is_ok(),
                    "actor: turn_cancel — SIGINT sent to claude child"
                );
            }
            // Suppress unused-variable on non-unix targets.
            let _ = pid;
        } else {
            tracing::warn!(
                agent_id = %self.id,
                "actor: turn_cancel — no claude pid (child not running)"
            );
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
                ev = self.state.event_rx.recv() => {
                    match ev {
                        Some(event) => {
                            handle_claude_event(
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
                            // event_rx closed — claude exited / pump died.
                            // Phase 0a: log + break. Phase 0b will emit
                            // AgentSessionEnd + AgentStatus{inactive}.
                            tracing::info!(agent_id = %agent_id, "run_loop: event_rx closed, exiting");
                            break;
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
        let pid = self.state.claude.id();
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

/// Map a `ClaudeEvent` to the appropriate daemon-side emission(s) and
/// mutate per-turn state (entries accumulator + turn_count).
#[allow(clippy::too_many_arguments)]
async fn handle_claude_event(
    event: ClaudeEvent,
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
        ClaudeEvent::SessionStarted { .. } => {
            // First SessionStarted was already consumed by Idle::start.
            // A second one would indicate session rotation — Phase 0b handles.
        }
        ClaudeEvent::MessageStart => {}
        ClaudeEvent::ThinkingDelta { text } => {
            entries.push(TrajectoryEntry {
                kind: "thinking".to_string(),
                text,
                ts: now_unix_ms(),
                ..Default::default()
            });
        }
        ClaudeEvent::TextDelta { text } => {
            entries.push(TrajectoryEntry {
                kind: "text".to_string(),
                text,
                ts: now_unix_ms(),
                ..Default::default()
            });
        }
        ClaudeEvent::ToolCall { id, name, input } => {
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
        ClaudeEvent::ToolResult { id, output, error } => {
            let entry = TrajectoryEntry {
                kind: "tool_result".to_string(),
                id,
                result: output,
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
        ClaudeEvent::MessageStop { .. } => {
            // claude emits MessageStop before the `result` line; the
            // entries accumulator is flushed on TurnEnd, not here.
        }
        ClaudeEvent::TurnEnd {
            input_tokens,
            output_tokens,
            cost_usd,
            cache_creation_tokens,
            cache_read_tokens,
        } => {
            *turn_count += 1;
            // Phase 0a: contextUsagePct = 0 (no context_window known yet);
            // session_type empty (chat default).
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
        ClaudeEvent::RateLimit { .. } => {
            // Phase 0a: surface as a no-op; activity msg with rate-limit
            // fields is deferred to Phase 0b.
        }
        ClaudeEvent::Error { message, code } => {
            tracing::warn!(
                agent_id = %agent_id,
                code = ?code,
                error = %message,
                "run_loop: claude error event"
            );
            // Phase 0a stretch: non-fatal — emit AgentStatus{error} but
            // do NOT kill the loop. Next deliver will retry.
            let _ = outbound
                .send(DaemonMsg::AgentStatus(AgentStatusMsg {
                    agent_id: agent_id.to_string(),
                    status: "error".to_string(),
                    launch_id: launch_id.to_string(),
                    error_detail: message,
                }))
                .await;
        }
        ClaudeEvent::Unknown => {}
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
}
