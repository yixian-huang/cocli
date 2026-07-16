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
//! - `Running::turn_cancel` — route through the core `TurnInterruptor`
//!   capability when available, otherwise use SIGINT.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{broadcast, mpsc};
use uuid::Uuid;

use cocli_driver_core::types::{DriverAgentConfig, MessageMode, SpawnConfig, TurnStatus};
use cocli_driver_core::{Driver, DriverEvent};
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
use crate::prompt::compose_session_bootstrap_prompt;
use crate::state::{ActorStdinStorage, AgentState, Idle, RespawnCtx, Running, Stopping};
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
    pub registry: std::sync::Arc<cocli_runtime_pool::RuntimeRegistry>,
    pub runtime_name: String,
    pub workspace_root: PathBuf,
    pub server_url: String,
    pub auth_token: String,
    pub channel_id: Uuid,
    pub channel_name: String,
    pub model: String,
    pub launch_id: String,
    pub resume_session: Option<String>,
    /// Persistent system contract passed to the driver and workspace hooks.
    pub system_prompt: String,
    /// Per-spawn user turn used to initialize the runtime session.
    pub initial_prompt: String,
    /// Extra env vars forwarded to the runtime subprocess.
    pub env_vars: HashMap<String, String>,
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
    /// Spawn the configured runtime through the shared core contract and
    /// promote the actor to `Running`.
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

        let bridge_http_url = derive_http_base_url(&cfg.server_url);
        let registry_driver = cfg.registry.get(&cfg.runtime_name).ok_or(StartError {
            message: format!("runtime not available: {}", cfg.runtime_name),
        })?;
        let mut spawn_env: Vec<(String, String)> = cfg.env_vars.into_iter().collect();
        spawn_env.sort_by(|a, b| a.0.cmp(&b.0));
        if !cfg.model.is_empty() && !spawn_env.iter().any(|(key, _)| key == "CHATRS_MODEL") {
            spawn_env.push(("CHATRS_MODEL".to_string(), cfg.model.clone()));
        }

        let spawn_cfg = SpawnConfig {
            working_dir: &work_dir,
            model: &cfg.model,
            mcp_config: None,
            resume_session: cfg.resume_session.as_deref(),
            agent_id: &self.id,
            server_url: &bridge_http_url,
            auth_token: &cfg.auth_token,
            system_prompt: &cfg.system_prompt,
            initial_prompt: &cfg.initial_prompt,
            env_vars: &spawn_env,
        };

        let driver: Arc<dyn Driver> = match registry_driver.as_process_factory() {
            Some(factory) => Arc::from(factory.new_process(&spawn_cfg)),
            None => Arc::clone(&registry_driver),
        };
        let driver_agent_config = DriverAgentConfig {
            runtime: &cfg.runtime_name,
            model: &cfg.model,
            working_runtime: &cfg.runtime_name,
            working_model: &cfg.model,
            env_vars: &spawn_env,
        };
        driver
            .prepare_workspace(
                &work_dir,
                &driver_agent_config,
                &self.id,
                &cfg.system_prompt,
            )
            .map_err(|e| StartError {
                message: format!("prepare_workspace: {e}"),
            })?;
        let mut child = driver.spawn(&spawn_cfg).map_err(|e| StartError {
            message: format!("spawn {}: {e}", driver.name()),
        })?;

        let child_pid = child.id().ok_or(StartError {
            message: "child pid missing after spawn".to_string(),
        })?;
        write_agent_pidfile(&self.id, child_pid).map_err(|e| StartError {
            message: format!("write pidfile: {e}"),
        })?;

        let mut stdin_opt = child.stdin.take();
        let stdout = child.stdout.take().ok_or(StartError {
            message: "child stdout missing after spawn".to_string(),
        })?;
        let (event_tx, mut event_rx) = mpsc::channel::<DriverEvent>(64);
        if let Some(stderr) = child.stderr.take() {
            spawn_stderr_drain(
                stderr,
                self.id.clone(),
                Arc::clone(&driver),
                event_tx.clone(),
            );
        }
        if let Some(initializer) = driver.as_process_initializer() {
            let stdin = stdin_opt.as_mut().ok_or(StartError {
                message: format!(
                    "{} needs stdin for initialization but spawn closed it",
                    driver.name()
                ),
            })?;
            let mut init_bytes = Vec::new();
            initializer
                .write_init_sequence(&mut init_bytes)
                .map_err(|e| StartError {
                    message: format!("write init sequence: {e}"),
                })?;
            stdin.write_all(&init_bytes).await.map_err(|e| StartError {
                message: format!("write init sequence to stdin: {e}"),
            })?;
            stdin.flush().await.map_err(|e| StartError {
                message: format!("flush init sequence: {e}"),
            })?;
        }
        let mut stdin_storage = resolve_stdin_storage(stdin_opt, driver.as_ref())?;

        let id_for_pump = self.id.clone();
        let obs_for_pump = self.obs_tx.clone();
        let driver_for_pump = Arc::clone(&driver);
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            loop {
                match lines.next_line().await {
                    Ok(Some(line)) => {
                        let _ = obs_for_pump.send(AgentObservationChanged::StdoutSeen {
                            agent_id: id_for_pump.clone(),
                        });
                        for ev in driver_for_pump.parse_event(&line) {
                            if event_tx.send(ev).await.is_err() {
                                return;
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

        let should_send_initial_prompt = !cfg.initial_prompt.is_empty()
            && !driver.is_turn_exit()
            && driver.requires_initial_prompt();
        let initial_prompt_sent = if should_send_initial_prompt {
            try_write_encoded_message(
                &mut stdin_storage,
                driver.as_ref(),
                &cfg.initial_prompt,
                None,
            )
            .await
            .map_err(|e| StartError {
                message: format!("write pre-session initial prompt: {e}"),
            })?
        } else {
            false
        };

        let session_id =
            collect_session_id(driver.as_ref(), &mut event_rx, &mut stdin_storage, &self.id)
                .await?;
        if should_send_initial_prompt && !initial_prompt_sent {
            write_encoded_message(
                &mut stdin_storage,
                driver.as_ref(),
                &cfg.initial_prompt,
                Some(&session_id),
            )
            .await
            .map_err(|e| StartError {
                message: format!("write initial prompt: {e}"),
            })?;
        }

        let _ = self.obs_tx.send(AgentObservationChanged::Started {
            agent_id: self.id.clone(),
            session_id: session_id.clone(),
            channel_id: cfg.channel_id,
            channel_name: cfg.channel_name.clone(),
        });

        let respawn_ctx = if driver.is_turn_exit() {
            Some(Arc::new(RespawnCtx {
                driver: registry_driver,
                work_dir: work_dir.clone(),
                model: cfg.model.clone(),
                server_url: bridge_http_url,
                auth_token: cfg.auth_token.clone(),
                system_prompt: cfg.system_prompt.clone(),
                env_vars: spawn_env,
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
                child,
                stdin_storage,
                driver,
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

async fn collect_session_id(
    driver: &dyn Driver,
    rx: &mut mpsc::Receiver<DriverEvent>,
    stdin: &mut ActorStdinStorage,
    agent_id: &str,
) -> Result<String, StartError> {
    if driver.defers_session_id_to_turn_end() {
        return Ok(String::new());
    }
    let deadline = Duration::from_secs(30);
    let wait_for_session = async {
        while let Some(event) = rx.recv().await {
            match event {
                DriverEvent::SessionStarted { session_id } => return Some(session_id),
                DriverEvent::Write { data } => {
                    if let Err(error) = write_raw_message(stdin, driver, data.as_bytes()).await {
                        tracing::warn!(agent_id = %agent_id, %error, "pre-session driver write failed");
                    }
                }
                _ => {}
            }
        }
        None
    };
    match tokio::time::timeout(deadline, wait_for_session).await {
        Ok(Some(session_id)) => Ok(session_id),
        Ok(None) => Err(StartError {
            message: "runtime exited before emitting session_id".to_string(),
        }),
        Err(_) => Err(StartError {
            message: format!("session_id timeout after {}s", deadline.as_secs()),
        }),
    }
}

fn resolve_stdin_storage(
    stdin: Option<tokio::process::ChildStdin>,
    driver: &dyn Driver,
) -> Result<ActorStdinStorage, StartError> {
    match driver.as_stdin_binder() {
        Some(binder) => {
            let stdin = stdin.ok_or(StartError {
                message: format!(
                    "{} uses StdinBinder but spawn returned no stdin",
                    driver.name()
                ),
            })?;
            binder.bind_stdin(stdin);
            Ok(ActorStdinStorage::ViaBinder)
        }
        None => Ok(match stdin {
            Some(stdin) => ActorStdinStorage::Local(stdin),
            None => ActorStdinStorage::Closed,
        }),
    }
}

async fn write_encoded_message(
    stdin: &mut ActorStdinStorage,
    driver: &dyn Driver,
    text: &str,
    session_id: Option<&str>,
) -> Result<(), String> {
    if try_write_encoded_message(stdin, driver, text, session_id).await? {
        Ok(())
    } else {
        Err(format!(
            "{} encode_stdin_message returned no payload",
            driver.name()
        ))
    }
}

async fn try_write_encoded_message(
    stdin: &mut ActorStdinStorage,
    driver: &dyn Driver,
    text: &str,
    session_id: Option<&str>,
) -> Result<bool, String> {
    let Some(encoded) = driver.encode_stdin_message(text, session_id, MessageMode::User) else {
        return Ok(false);
    };
    write_raw_message(stdin, driver, encoded.as_bytes()).await?;
    Ok(true)
}

async fn write_raw_message(
    stdin: &mut ActorStdinStorage,
    driver: &dyn Driver,
    payload: &[u8],
) -> Result<(), String> {
    let mut bytes = payload.to_vec();
    if bytes.last() != Some(&b'\n') {
        bytes.push(b'\n');
    }
    match stdin {
        ActorStdinStorage::Local(stdin) => {
            stdin
                .write_all(&bytes)
                .await
                .map_err(|e| format!("stdin write: {e}"))?;
            stdin.flush().await.map_err(|e| format!("stdin flush: {e}"))
        }
        ActorStdinStorage::ViaBinder => {
            let binder = driver
                .as_stdin_binder()
                .ok_or_else(|| format!("{} lost its StdinBinder after binding", driver.name()))?;
            binder
                .write_stdin(&bytes)
                .await
                .map_err(|e| format!("binder write: {e}"))
        }
        ActorStdinStorage::Closed => Err(format!("{} has no writable stdin", driver.name())),
    }
}

fn spawn_stderr_drain(
    stderr: tokio::process::ChildStderr,
    agent_id: String,
    driver: Arc<dyn Driver>,
    event_tx: mpsc::Sender<DriverEvent>,
) {
    tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            tracing::debug!(agent_id = %agent_id, line = %line, "agent stderr");
            if let Some(event) = driver.classify_stderr_line(&line) {
                if event_tx.send(event).await.is_err() {
                    break;
                }
            }
        }
    });
}

impl AgentActor<Running> {
    /// Executes a raw stdin write requested by the active runtime driver.
    pub async fn write_driver_request(&mut self, data: &str) -> Result<(), String> {
        write_raw_message(
            &mut self.state.stdin_storage,
            self.state.driver.as_ref(),
            data.as_bytes(),
        )
        .await
    }

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
        let write_result = if let Some(respawn_ctx) = self.state.respawn_ctx.clone() {
            self.respawn_and_deliver(&body, &respawn_ctx)
                .await
                .map_err(|e| format!("respawn_and_deliver: {e}"))
        } else {
            write_encoded_message(
                &mut self.state.stdin_storage,
                self.state.driver.as_ref(),
                &body,
                Some(&self.state.session_id),
            )
            .await
        };

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

    /// Kill the current turn-exit child and re-spawn with `message` as the
    /// next initial prompt.
    async fn respawn_and_deliver(&mut self, message: &str, ctx: &RespawnCtx) -> Result<(), String> {
        let _ = self.state.child.kill().await;
        let initial_prompt = compose_session_bootstrap_prompt(&ctx.system_prompt, message);
        let spawn_cfg = SpawnConfig {
            working_dir: &ctx.work_dir,
            model: &ctx.model,
            mcp_config: None,
            resume_session: Some(self.state.session_id.as_str()).filter(|s| !s.is_empty()),
            agent_id: &self.id,
            server_url: &ctx.server_url,
            auth_token: &ctx.auth_token,
            system_prompt: &ctx.system_prompt,
            initial_prompt: &initial_prompt,
            env_vars: &ctx.env_vars,
        };
        let driver: Arc<dyn Driver> = match ctx.driver.as_process_factory() {
            Some(factory) => Arc::from(factory.new_process(&spawn_cfg)),
            None => Arc::clone(&ctx.driver),
        };
        let driver_config = DriverAgentConfig {
            runtime: driver.name(),
            model: &ctx.model,
            working_runtime: driver.name(),
            working_model: &ctx.model,
            env_vars: &ctx.env_vars,
        };
        driver
            .prepare_workspace(&ctx.work_dir, &driver_config, &self.id, &ctx.system_prompt)
            .map_err(|e| format!("prepare_workspace: {e}"))?;
        let mut new_child = driver
            .spawn(&spawn_cfg)
            .map_err(|e| format!("spawn: {e}"))?;

        if let Some(pid) = new_child.id() {
            write_agent_pidfile(&self.id, pid).map_err(|e| format!("write pidfile: {e}"))?;
        }

        let mut new_stdin = new_child.stdin.take();
        let new_stdout = new_child
            .stdout
            .take()
            .ok_or_else(|| "respawn: child stdout missing".to_string())?;
        let (new_event_tx, mut new_event_rx) = mpsc::channel::<DriverEvent>(64);
        if let Some(stderr) = new_child.stderr.take() {
            spawn_stderr_drain(
                stderr,
                self.id.clone(),
                Arc::clone(&driver),
                new_event_tx.clone(),
            );
        }
        if let Some(initializer) = driver.as_process_initializer() {
            let stdin = new_stdin
                .as_mut()
                .ok_or_else(|| "respawn: initializer requires stdin".to_string())?;
            let mut init_bytes = Vec::new();
            initializer
                .write_init_sequence(&mut init_bytes)
                .map_err(|e| format!("respawn init sequence: {e}"))?;
            stdin
                .write_all(&init_bytes)
                .await
                .map_err(|e| format!("respawn init write: {e}"))?;
            stdin
                .flush()
                .await
                .map_err(|e| format!("respawn init flush: {e}"))?;
        }
        let mut stdin_storage =
            resolve_stdin_storage(new_stdin, driver.as_ref()).map_err(|e| e.to_string())?;
        let id_for_pump = self.id.clone();
        let obs_for_pump = self.obs_tx.clone();
        let driver_for_pump = Arc::clone(&driver);
        tokio::spawn(async move {
            let mut lines = BufReader::new(new_stdout).lines();
            loop {
                match lines.next_line().await {
                    Ok(Some(line)) => {
                        let _ = obs_for_pump.send(AgentObservationChanged::StdoutSeen {
                            agent_id: id_for_pump.clone(),
                        });
                        for ev in driver_for_pump.parse_event(&line) {
                            if new_event_tx.send(ev).await.is_err() {
                                return;
                            }
                        }
                    }
                    Ok(None) => break,
                    Err(e) => {
                        tracing::warn!(agent_id = %id_for_pump, error = %e, "respawn pump read error");
                        break;
                    }
                }
            }
        });
        let new_session_id = collect_session_id(
            driver.as_ref(),
            &mut new_event_rx,
            &mut stdin_storage,
            &self.id,
        )
        .await
        .map_err(|e| e.to_string())?;

        self.state.child = new_child;
        self.state.stdin_storage = stdin_storage;
        self.state.driver = driver;
        self.state.event_rx = new_event_rx;
        if !new_session_id.is_empty() {
            self.state.session_id = new_session_id;
        }

        tracing::info!(agent_id = %self.id, "respawn_and_deliver: turn-exit process spawned");
        Ok(())
    }

    /// Interrupt through the optional core subtrait, or fall back to SIGINT
    /// for runtimes that advertise cancellation without a native RPC.
    pub async fn turn_cancel(&mut self) -> Result<(), String> {
        if let Some(interruptor) = self.state.driver.as_turn_interruptor() {
            return interruptor
                .interrupt_turn()
                .await
                .map_err(|e| format!("interrupt_turn: {e}"));
        }
        if !self.state.driver.supports_turn_cancel() {
            return Err(format!(
                "{} does not support turn cancellation",
                self.state.driver.name()
            ));
        }
        if let Some(pid) = self.state.child.id() {
            #[cfg(unix)]
            nix::sys::signal::kill(
                nix::unistd::Pid::from_raw(pid as i32),
                nix::sys::signal::Signal::SIGINT,
            )
            .map_err(|e| format!("send SIGINT: {e}"))?;
        }
        Ok(())
    }

    /// Redirect the active turn through the runtime-native steering API.
    pub async fn turn_steer(&self, input: &str) -> Result<(), String> {
        if !self.state.driver.supports_turn_steer() {
            return Err(format!(
                "{} does not support turn steering",
                self.state.driver.name()
            ));
        }
        self.state
            .driver
            .turn_steer(input)
            .await
            .map_err(|error| format!("turn_steer: {error}"))
    }

    /// Fork the active runtime thread and update the actor's session identity.
    pub async fn thread_fork(&mut self) -> Result<String, String> {
        if !self.state.driver.supports_thread_fork() {
            return Err(format!(
                "{} does not support thread fork",
                self.state.driver.name()
            ));
        }
        let session_id = self
            .state
            .driver
            .fork_thread(&self.state.session_id)
            .await
            .map_err(|error| format!("thread_fork: {error}"))?;
        self.state.session_id.clone_from(&session_id);
        emit_session(
            &self.outbound,
            &self.id,
            &session_id,
            self.state.channel_id,
            &self.state.launch_id,
        )
        .await;
        Ok(session_id)
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
                        Some(DriverEvent::SessionStarted { session_id }) => {
                            self.state.session_id = session_id;
                        }
                        Some(DriverEvent::Write { data }) => {
                            if let Err(error) = self.write_driver_request(&data).await {
                                tracing::warn!(agent_id = %agent_id, %error, "driver-requested stdin write failed");
                            }
                        }
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
        let pid = self.state.child.id();
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

/// Map a generic `DriverEvent` to daemon-side emissions
/// and mutate per-turn state (entries accumulator + turn_count).
#[allow(clippy::too_many_arguments)]
async fn handle_event(
    event: DriverEvent,
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
        DriverEvent::SessionStarted { .. } => {
            // First SessionStarted was already consumed by Idle::start.
            // A second one would indicate session rotation — Phase 0b handles.
        }
        DriverEvent::MessageStart => {}
        DriverEvent::ThinkingDelta { text } => {
            entries.push(TrajectoryEntry {
                kind: "thinking".to_string(),
                text,
                ts: now_unix_ms(),
                ..Default::default()
            });
        }
        DriverEvent::TextDelta { text } => {
            entries.push(TrajectoryEntry {
                kind: "text".to_string(),
                text,
                ts: now_unix_ms(),
                ..Default::default()
            });
        }
        DriverEvent::ToolCall { id, name, input } => {
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
        DriverEvent::ToolResult { id, output, error }
        | DriverEvent::ToolDone {
            id,
            result: output,
            error,
        } => {
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
        DriverEvent::MessageStop { .. } => {
            // claude emits MessageStop before the `result` line; the
            // entries accumulator is flushed on TurnEnd, not here.
        }
        DriverEvent::TurnEnd {
            status,
            input_tokens,
            output_tokens,
            cost_usd,
            cache_creation_tokens,
            cache_read_tokens,
            context_window_tokens: _,
        } => {
            if matches!(status, TurnStatus::Failed) {
                tracing::warn!(agent_id = %agent_id, "runtime reported failed turn");
            }
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
        DriverEvent::RateLimit { .. } => {
            // Phase 0a: surface as a no-op; activity msg with rate-limit
            // fields is deferred to Phase 0b.
        }
        DriverEvent::Error {
            message,
            code,
            severity: _,
            http_status: _,
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
        DriverEvent::CompactStarted | DriverEvent::CompactFinished => {
            // kimi-style context-window compaction lifecycle; claude does
            // not emit. Phase 0b will broadcast a compact observation.
        }
        DriverEvent::Signal { .. } | DriverEvent::Write { .. } | DriverEvent::Unknown => {}
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
