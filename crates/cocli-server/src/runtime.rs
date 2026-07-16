use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use cocli_agent::context::{classify_context_pressure, default_backstop_pct, ContextPressureTier};
use cocli_agent::fork_reason::classify_fork_reason;
use cocli_agent::state::Idle;
use cocli_agent::{AgentActor, AgentMetrics, StartCfg};
use cocli_api::{
    RuntimeError, RuntimeForkResult, RuntimeInfo, RuntimeMetricsSnapshot, RuntimeService,
    RuntimeSessionStatus,
};
use cocli_driver_chatrs::ChatrsDriver;
use cocli_driver_claude::ClaudeDriver;
use cocli_driver_codex::CodexDriver;
use cocli_driver_core::types::TurnStatus;
use cocli_driver_core::{Driver, DriverEvent};
use cocli_driver_cursor::CursorDriver;
use cocli_driver_gemini::GeminiDriver;
use cocli_driver_grok::GrokDriver;
use cocli_driver_kimi::KimiDriver;
use cocli_driver_opencode::OpenCodeDriver;
use cocli_protocol::types::DeliveryMessage;
use cocli_protocol::AgentDeliverMsg;
use cocli_runtime_pool::{
    initial_oss_runtime_specs, RuntimeCapabilities, RuntimeCatalog, RuntimeCatalogEntry,
    RuntimeProbe, RuntimeRegistry, SystemRuntimeProbe,
};
use cocli_store::{Agent, Message};
use tokio::sync::{broadcast, mpsc, oneshot, Mutex, RwLock};
use tokio::time::Instant;
use uuid::Uuid;

const INITIALIZATION_PROMPT: &str = "\
You are a local cocli agent. Reply to tasks in plain text. \
Do not call messaging or collaboration tools. \
For this initialization turn, reply with exactly READY.";

/// Inputs used to discover and run local CLI runtimes.
#[derive(Clone, Debug)]
pub struct LocalRuntimeConfig {
    /// Parent directory for per-agent runtime workspaces.
    pub workspace_root: PathBuf,
    /// Loopback URL passed to the optional MCP bridge.
    pub server_url: String,
    /// Local bridge capability token.
    pub auth_token: String,
    /// Maximum time allowed for one initialization or task turn.
    pub turn_timeout: Duration,
}

impl LocalRuntimeConfig {
    /// Creates a local runtime configuration with a two-minute turn budget.
    pub fn new(workspace_root: PathBuf, server_url: String) -> Self {
        Self {
            workspace_root,
            server_url,
            auth_token: String::new(),
            turn_timeout: Duration::from_secs(120),
        }
    }
}

/// Runtime service backed by the shared registry and `AgentActor` lifecycle.
pub struct LocalRuntimeService {
    registry: Arc<RuntimeRegistry>,
    catalog: Vec<RuntimeInfo>,
    config: LocalRuntimeConfig,
    sessions: Mutex<HashMap<Uuid, LocalSessionHandle>>,
    session_locks: Mutex<HashMap<Uuid, Arc<Mutex<()>>>>,
    metrics: Arc<AgentMetrics>,
}

#[derive(Clone)]
struct LocalSessionHandle {
    command_tx: mpsc::Sender<LocalSessionCommand>,
    status: Arc<RwLock<RuntimeSessionStatus>>,
    started_at: Instant,
}

enum LocalSessionCommand {
    Reply {
        delivery: AgentDeliverMsg,
        reply: oneshot::Sender<Result<String, RuntimeError>>,
    },
    Cancel {
        reply: oneshot::Sender<Result<(), RuntimeError>>,
    },
    Steer {
        input: String,
        reply: oneshot::Sender<Result<(), RuntimeError>>,
    },
    Fork {
        reply: oneshot::Sender<Result<String, RuntimeError>>,
    },
    Stop {
        reply: oneshot::Sender<()>,
    },
}

struct ActiveReply {
    reply: Option<oneshot::Sender<Result<String, RuntimeError>>>,
    text: String,
    last_error: Option<String>,
    deadline: Option<Instant>,
}

impl LocalRuntimeService {
    /// Discovers the OSS runtime matrix from `PATH`.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeSetupError`] when the current executable path cannot
    /// be inspected while resolving the optional bridge binary.
    pub fn discover(config: LocalRuntimeConfig) -> Result<Self, RuntimeSetupError> {
        let probe = SystemRuntimeProbe::from_environment();
        let bridge = resolve_bridge(&probe)?;
        let specs = initial_oss_runtime_specs();
        let mut registry = RuntimeRegistry::new();

        for spec in &specs {
            let Some(binary) = probe.resolve_binary(&spec.command) else {
                continue;
            };
            if let Some(driver) = create_driver(&spec.name, binary, bridge.clone()) {
                registry.register(driver);
            }
        }

        let catalog = registry.discover(&specs, &probe);
        Ok(Self::from_catalog(registry, catalog, config))
    }

    fn from_catalog(
        registry: RuntimeRegistry,
        catalog: RuntimeCatalog,
        config: LocalRuntimeConfig,
    ) -> Self {
        Self {
            registry: Arc::new(registry),
            catalog: catalog.runtimes.iter().map(runtime_info).collect(),
            config,
            sessions: Mutex::new(HashMap::new()),
            session_locks: Mutex::new(HashMap::new()),
            metrics: Arc::new(AgentMetrics::default()),
        }
    }

    async fn spawn_session(&self, agent: &Agent) -> Result<LocalSessionHandle, RuntimeError> {
        let (_command_tx, command_rx) = mpsc::channel(4);
        let (outbound_tx, _outbound_rx) = mpsc::channel(16);
        let (state_tx, _state_rx) = mpsc::channel(4);
        let (obs_tx, _obs_rx) = broadcast::channel(16);
        let actor = AgentActor::<Idle> {
            id: agent.id.to_string(),
            mailbox: command_rx,
            outbound: outbound_tx,
            state_tx,
            obs_tx,
            state: Idle,
        };
        let start = actor.start(StartCfg {
            registry: Arc::clone(&self.registry),
            runtime_name: agent.runtime.clone(),
            workspace_root: self.config.workspace_root.clone(),
            server_url: self.config.server_url.clone(),
            auth_token: self.config.auth_token.clone(),
            channel_id: agent.channel_id,
            channel_name: "local".to_owned(),
            model: agent.model.clone().unwrap_or_default(),
            launch_id: Uuid::new_v4().to_string(),
            resume_session: None,
            system_prompt: INITIALIZATION_PROMPT.to_owned(),
            env_vars: HashMap::new(),
        });
        let mut running = tokio::time::timeout(self.config.turn_timeout, start)
            .await
            .map_err(|_| RuntimeError::Delivery("runtime startup timed out".to_owned()))?
            .map_err(|error| RuntimeError::Delivery(error.to_string()))?;

        if let Err(error) = wait_for_turn(&mut running, self.config.turn_timeout, false).await {
            let _stopping = running.stop(true);
            return Err(error);
        }

        let session_id = running.state.session_id.clone();
        let driver = Arc::clone(&running.state.driver);
        let context_window_tokens = driver.context_window_tokens().unwrap_or_default() as u64;
        let status = Arc::new(RwLock::new(RuntimeSessionStatus {
            agent_id: agent.id,
            session_id,
            runtime: agent.runtime.clone(),
            model: agent.model.clone(),
            running: true,
            active_turn: false,
            supports_turn_cancel: driver.supports_turn_cancel(),
            supports_turn_steer: driver.supports_turn_steer(),
            supports_thread_fork: driver.supports_thread_fork(),
            input_tokens: 0,
            context_window_tokens,
            context_util_pct: 0.0,
            tier: ContextPressureTier::Healthy.as_str().to_owned(),
            fork_suggested: false,
            session_age_seconds: 0,
        }));
        let (command_tx, command_rx) = mpsc::channel(16);
        let status_for_task = Arc::clone(&status);
        let timeout = self.config.turn_timeout;
        let metrics = Arc::clone(&self.metrics);
        metrics.inc_local_session_started();
        metrics.inc_local_active_sessions();
        tokio::spawn(async move {
            run_local_session(running, command_rx, status_for_task, timeout, metrics).await;
        });
        Ok(LocalSessionHandle {
            command_tx,
            status,
            started_at: Instant::now(),
        })
    }

    async fn session_lock(&self, agent_id: Uuid) -> Arc<Mutex<()>> {
        let mut locks = self.session_locks.lock().await;
        Arc::clone(
            locks
                .entry(agent_id)
                .or_insert_with(|| Arc::new(Mutex::new(()))),
        )
    }

    async fn session_handle(&self, agent: &Agent) -> Result<LocalSessionHandle, RuntimeError> {
        if let Some(handle) = self.live_session(agent.id).await {
            self.metrics.inc_local_session_reused();
            return Ok(handle);
        }

        let start_lock = self.session_lock(agent.id).await;
        let _guard = start_lock.lock().await;
        if let Some(handle) = self.live_session(agent.id).await {
            self.metrics.inc_local_session_reused();
            return Ok(handle);
        }

        let handle = self.spawn_session(agent).await?;
        self.sessions.lock().await.insert(agent.id, handle.clone());
        Ok(handle)
    }

    async fn live_session(&self, agent_id: Uuid) -> Option<LocalSessionHandle> {
        let handle = self.sessions.lock().await.get(&agent_id).cloned()?;
        if !handle.command_tx.is_closed() && handle.status.read().await.running {
            return Some(handle);
        }
        self.sessions.lock().await.remove(&agent_id);
        None
    }

    async fn existing_session(&self, agent_id: Uuid) -> Result<LocalSessionHandle, RuntimeError> {
        self.live_session(agent_id)
            .await
            .ok_or_else(|| RuntimeError::NotFound("agent runtime is not started".to_owned()))
    }

    async fn snapshot(handle: &LocalSessionHandle) -> RuntimeSessionStatus {
        let mut status = handle.status.read().await.clone();
        status.session_age_seconds = handle.started_at.elapsed().as_secs();
        status
    }

    async fn run_reply(&self, agent: &Agent, message: &Message) -> Result<String, RuntimeError> {
        let handle = self.session_handle(agent).await?;
        let delivery = AgentDeliverMsg {
            agent_id: agent.id.to_string(),
            message: DeliveryMessage {
                channel_id: agent.channel_id,
                sender_name: "user".to_owned(),
                sender_type: "user".to_owned(),
                content: message.content.clone(),
                channel_name: "local".to_owned(),
                channel_type: "channel".to_owned(),
                ..Default::default()
            },
            ..Default::default()
        };
        let (reply_tx, reply_rx) = oneshot::channel();
        handle
            .command_tx
            .send(LocalSessionCommand::Reply {
                delivery,
                reply: reply_tx,
            })
            .await
            .map_err(|_| RuntimeError::NotFound("agent runtime session exited".to_owned()))?;
        reply_rx
            .await
            .map_err(|_| RuntimeError::NotFound("agent runtime reply channel closed".to_owned()))?
    }
}

#[async_trait]
impl RuntimeService for LocalRuntimeService {
    async fn list(&self) -> Vec<RuntimeInfo> {
        self.catalog.clone()
    }

    async fn reply(&self, agent: &Agent, message: &Message) -> Result<String, RuntimeError> {
        self.run_reply(agent, message).await
    }

    async fn start(&self, agent: &Agent) -> Result<RuntimeSessionStatus, RuntimeError> {
        let handle = self.session_handle(agent).await?;
        Ok(Self::snapshot(&handle).await)
    }

    async fn stop(&self, agent_id: Uuid) -> Result<(), RuntimeError> {
        let Some(handle) = self.sessions.lock().await.remove(&agent_id) else {
            return Ok(());
        };
        let (reply_tx, reply_rx) = oneshot::channel();
        handle
            .command_tx
            .send(LocalSessionCommand::Stop { reply: reply_tx })
            .await
            .map_err(|_| RuntimeError::NotFound("agent runtime session exited".to_owned()))?;
        let _ = tokio::time::timeout(Duration::from_secs(5), reply_rx).await;
        Ok(())
    }

    async fn cancel(&self, agent_id: Uuid) -> Result<(), RuntimeError> {
        let handle = self.existing_session(agent_id).await?;
        let (reply_tx, reply_rx) = oneshot::channel();
        handle
            .command_tx
            .send(LocalSessionCommand::Cancel { reply: reply_tx })
            .await
            .map_err(|_| RuntimeError::NotFound("agent runtime session exited".to_owned()))?;
        reply_rx
            .await
            .map_err(|_| RuntimeError::NotFound("turn cancel reply channel closed".to_owned()))?
    }

    async fn steer(&self, agent_id: Uuid, input: &str) -> Result<(), RuntimeError> {
        let handle = self.existing_session(agent_id).await?;
        let (reply_tx, reply_rx) = oneshot::channel();
        handle
            .command_tx
            .send(LocalSessionCommand::Steer {
                input: input.to_owned(),
                reply: reply_tx,
            })
            .await
            .map_err(|_| RuntimeError::NotFound("agent runtime session exited".to_owned()))?;
        reply_rx
            .await
            .map_err(|_| RuntimeError::NotFound("turn steer reply channel closed".to_owned()))?
    }

    async fn fork(&self, agent: &Agent, reason: &str) -> Result<RuntimeForkResult, RuntimeError> {
        let handle = self.session_handle(agent).await?;
        let status = Self::snapshot(&handle).await;
        if status.active_turn {
            return Err(RuntimeError::Busy(
                "cannot fork while a turn is active".to_owned(),
            ));
        }

        if status.supports_thread_fork {
            let (reply_tx, reply_rx) = oneshot::channel();
            handle
                .command_tx
                .send(LocalSessionCommand::Fork { reply: reply_tx })
                .await
                .map_err(|_| RuntimeError::NotFound("agent runtime session exited".to_owned()))?;
            let fork_id = reply_rx.await.map_err(|_| {
                RuntimeError::NotFound("thread fork reply channel closed".to_owned())
            })??;
            return Ok(RuntimeForkResult {
                fork_id,
                native: true,
            });
        }

        tracing::info!(
            agent_id = %agent.id,
            fork_kind = ?classify_fork_reason(reason),
            "restarting local runtime with fresh context for thread fork"
        );
        self.stop(agent.id).await?;
        let restarted = self.session_handle(agent).await?;
        let restarted_status = Self::snapshot(&restarted).await;
        Ok(RuntimeForkResult {
            fork_id: if restarted_status.session_id.is_empty() {
                Uuid::new_v4().to_string()
            } else {
                restarted_status.session_id
            },
            native: false,
        })
    }

    async fn status(&self, agent: &Agent) -> Result<RuntimeSessionStatus, RuntimeError> {
        let Some(handle) = self.live_session(agent.id).await else {
            return Ok(inactive_session_status(agent));
        };
        Ok(Self::snapshot(&handle).await)
    }

    async fn metrics(&self) -> RuntimeMetricsSnapshot {
        let snapshot = self.metrics.snapshot();
        RuntimeMetricsSnapshot {
            counters: snapshot.counters,
            gauges: snapshot.gauges,
        }
    }
}

async fn run_local_session(
    mut running: AgentActor<cocli_agent::state::Running>,
    mut commands: mpsc::Receiver<LocalSessionCommand>,
    status: Arc<RwLock<RuntimeSessionStatus>>,
    turn_timeout: Duration,
    metrics: Arc<AgentMetrics>,
) {
    let mut active: Option<ActiveReply> = None;
    loop {
        let deadline = active
            .as_ref()
            .and_then(|turn| turn.deadline)
            .unwrap_or_else(|| Instant::now() + Duration::from_secs(365 * 24 * 60 * 60));
        let deadline_live = active.as_ref().and_then(|turn| turn.deadline).is_some();

        tokio::select! {
            command = commands.recv() => match command {
                Some(LocalSessionCommand::Reply { delivery, reply }) => {
                    if active.is_some() {
                        let _ = reply.send(Err(RuntimeError::Busy(
                            "agent already has an active turn".to_owned(),
                        )));
                        continue;
                    }
                    match running.deliver(delivery).await {
                        Ok(()) => {
                            metrics.inc_local_turn_started();
                            metrics.inc_local_active_turns();
                            active = Some(ActiveReply {
                                reply: Some(reply),
                                text: String::new(),
                                last_error: None,
                                deadline: Some(Instant::now() + turn_timeout),
                            });
                            status.write().await.active_turn = true;
                        }
                        Err(error) => {
                            let _ = reply.send(Err(RuntimeError::Delivery(error)));
                        }
                    }
                }
                Some(LocalSessionCommand::Cancel { reply }) => {
                    let result = if active.is_none() {
                        Err(RuntimeError::NotFound("agent has no active turn".to_owned()))
                    } else {
                        running.turn_cancel().await.map_err(RuntimeError::Delivery)
                    };
                    let _ = reply.send(result);
                }
                Some(LocalSessionCommand::Steer { input, reply }) => {
                    let result = if active.is_none() {
                        Err(RuntimeError::NotFound("agent has no active turn".to_owned()))
                    } else {
                        running.turn_steer(&input).await.map_err(|error| {
                            if running.state.driver.supports_turn_steer() {
                                RuntimeError::Delivery(error)
                            } else {
                                RuntimeError::Unsupported(error)
                            }
                        })
                    };
                    let _ = reply.send(result);
                }
                Some(LocalSessionCommand::Fork { reply }) => {
                    let result = if active.is_some() {
                        Err(RuntimeError::Busy(
                            "cannot fork while a turn is active".to_owned(),
                        ))
                    } else {
                        running.thread_fork().await.map_err(|error| {
                            if running.state.driver.supports_thread_fork() {
                                RuntimeError::Delivery(error)
                            } else {
                                RuntimeError::Unsupported(error)
                            }
                        })
                    };
                    if let Ok(session_id) = &result {
                        status.write().await.session_id.clone_from(session_id);
                    }
                    let _ = reply.send(result);
                }
                Some(LocalSessionCommand::Stop { reply }) => {
                    if let Some(turn) = active.as_mut() {
                        if let Some(reply) = turn.reply.take() {
                            let _ = reply.send(Err(RuntimeError::Delivery(
                                "agent runtime stopped during active turn".to_owned(),
                            )));
                        }
                        metrics.inc_local_turn_cancelled();
                    }
                    let _stopping = running.stop(true);
                    let mut state = status.write().await;
                    state.running = false;
                    state.active_turn = false;
                    let _ = reply.send(());
                    break;
                }
                None => {
                    let _stopping = running.stop(true);
                    let mut state = status.write().await;
                    state.running = false;
                    state.active_turn = false;
                    break;
                }
            },
            event = running.state.event_rx.recv() => match event {
                Some(DriverEvent::SessionStarted { session_id }) => {
                    running.state.session_id.clone_from(&session_id);
                    status.write().await.session_id = session_id;
                }
                Some(DriverEvent::TextDelta { text }) => {
                    if let Some(turn) = active.as_mut() {
                        turn.text.push_str(&text);
                    }
                }
                Some(DriverEvent::Error { message, .. }) => {
                    if let Some(turn) = active.as_mut() {
                        turn.last_error = Some(message);
                    }
                }
                Some(DriverEvent::TurnEnd {
                    status: turn_status,
                    input_tokens,
                    context_window_tokens,
                    ..
                }) => {
                    update_context_status(
                        &status,
                        &running,
                        input_tokens,
                        context_window_tokens,
                    ).await;
                    match &turn_status {
                        TurnStatus::Failed => metrics.inc_local_turn_failed(),
                        TurnStatus::Cancelled => metrics.inc_local_turn_cancelled(),
                        TurnStatus::Completed | TurnStatus::MaxSteps | TurnStatus::Unknown(_) => {
                            metrics.inc_local_turn_completed();
                        }
                    }
                    if let Some(mut turn) = active.take() {
                        if let Some(reply) = turn.reply.take() {
                            let result = completed_turn_result(
                                turn_status,
                                turn.text,
                                turn.last_error,
                            );
                            let _ = reply.send(result);
                        }
                    }
                    metrics.dec_local_active_turns();
                    status.write().await.active_turn = false;
                }
                Some(DriverEvent::Write { data }) => {
                    if let Err(error) = running.write_driver_request(&data).await {
                        if let Some(turn) = active.as_mut() {
                            turn.last_error = Some(error);
                        }
                    }
                }
                Some(_) => {}
                None => {
                    if let Some(mut turn) = active.take() {
                        metrics.inc_local_turn_failed();
                        metrics.dec_local_active_turns();
                        if let Some(reply) = turn.reply.take() {
                            let _ = reply.send(Err(RuntimeError::Delivery(
                                turn.last_error.unwrap_or_else(|| {
                                    "runtime exited before completing the turn".to_owned()
                                }),
                            )));
                        }
                    }
                    let mut state = status.write().await;
                    state.running = false;
                    state.active_turn = false;
                    break;
                }
            },
            _ = tokio::time::sleep_until(deadline), if deadline_live => {
                if let Some(turn) = active.as_mut() {
                    metrics.inc_local_turn_timed_out();
                    turn.deadline = None;
                    if let Some(reply) = turn.reply.take() {
                        let _ = reply.send(Err(RuntimeError::Delivery(
                            "runtime turn timed out".to_owned(),
                        )));
                    }
                }
                if let Err(error) = running.turn_cancel().await {
                    tracing::warn!(agent_id = %running.id, %error, "failed to cancel timed-out turn");
                    let _stopping = running.stop(true);
                    let mut state = status.write().await;
                    state.running = false;
                    state.active_turn = false;
                    break;
                }
            }
        }
    }
    if active.is_some() {
        metrics.dec_local_active_turns();
    }
    metrics.inc_local_session_stopped();
    metrics.dec_local_active_sessions();
    let mut state = status.write().await;
    state.running = false;
    state.active_turn = false;
}

async fn update_context_status(
    status: &RwLock<RuntimeSessionStatus>,
    running: &AgentActor<cocli_agent::state::Running>,
    input_tokens: u64,
    event_context_window_tokens: u64,
) {
    let context_window_tokens = if event_context_window_tokens > 0 {
        event_context_window_tokens
    } else {
        running
            .state
            .driver
            .context_window_tokens()
            .unwrap_or_default() as u64
    };
    let window_i32 = context_window_tokens.min(i32::MAX as u64) as i32;
    let mut state = status.write().await;
    let previous_tier = match state.tier.as_str() {
        "warn" => ContextPressureTier::Warn,
        "crit" => ContextPressureTier::Crit,
        _ => ContextPressureTier::Healthy,
    };
    let pressure = classify_context_pressure(
        input_tokens,
        window_i32,
        previous_tier,
        default_backstop_pct(window_i32),
    );
    state.input_tokens = input_tokens;
    state.context_window_tokens = context_window_tokens;
    state.context_util_pct = pressure.util_pct;
    pressure.tier.as_str().clone_into(&mut state.tier);
    state.fork_suggested = pressure.fork_suggested;
}

fn completed_turn_result(
    status: TurnStatus,
    text: String,
    last_error: Option<String>,
) -> Result<String, RuntimeError> {
    if matches!(status, TurnStatus::Failed | TurnStatus::Cancelled) {
        return Err(RuntimeError::Delivery(
            last_error.unwrap_or_else(|| format!("runtime turn ended: {status:?}")),
        ));
    }
    if text.trim().is_empty() {
        return Err(RuntimeError::Delivery(last_error.unwrap_or_else(|| {
            "runtime completed without a text reply".to_owned()
        })));
    }
    Ok(text)
}

fn inactive_session_status(agent: &Agent) -> RuntimeSessionStatus {
    RuntimeSessionStatus {
        agent_id: agent.id,
        session_id: String::new(),
        runtime: agent.runtime.clone(),
        model: agent.model.clone(),
        running: false,
        active_turn: false,
        supports_turn_cancel: false,
        supports_turn_steer: false,
        supports_thread_fork: false,
        input_tokens: 0,
        context_window_tokens: 0,
        context_util_pct: 0.0,
        tier: ContextPressureTier::Healthy.as_str().to_owned(),
        fork_suggested: false,
        session_age_seconds: 0,
    }
}

async fn wait_for_turn(
    running: &mut AgentActor<cocli_agent::state::Running>,
    timeout: Duration,
    collect_text: bool,
) -> Result<String, RuntimeError> {
    tokio::time::timeout(timeout, async {
        let mut text = String::new();
        let mut last_error = None;
        while let Some(event) = running.state.event_rx.recv().await {
            match event {
                DriverEvent::TextDelta { text: delta } if collect_text => text.push_str(&delta),
                DriverEvent::Error { message, .. } => last_error = Some(message),
                DriverEvent::TurnEnd { status, .. } => {
                    if matches!(status, TurnStatus::Failed | TurnStatus::Cancelled) {
                        return Err(RuntimeError::Delivery(
                            last_error.unwrap_or_else(|| format!("runtime turn ended: {status:?}")),
                        ));
                    }
                    if collect_text && text.trim().is_empty() {
                        return Err(RuntimeError::Delivery(last_error.unwrap_or_else(|| {
                            "runtime completed without a text reply".to_owned()
                        })));
                    }
                    return Ok(text);
                }
                DriverEvent::Write { data } => running
                    .write_driver_request(&data)
                    .await
                    .map_err(RuntimeError::Delivery)?,
                _ => {}
            }
        }
        Err(RuntimeError::Delivery(last_error.unwrap_or_else(|| {
            "runtime exited before completing the turn".to_owned()
        })))
    })
    .await
    .map_err(|_| RuntimeError::Delivery("runtime turn timed out".to_owned()))?
}

fn resolve_bridge(probe: &dyn RuntimeProbe) -> Result<PathBuf, RuntimeSetupError> {
    if let Some(binary) = probe.resolve_binary(Path::new("cocli-bridge")) {
        return Ok(binary);
    }
    let sibling = std::env::current_exe()
        .map_err(RuntimeSetupError::CurrentExecutable)?
        .with_file_name("cocli-bridge");
    Ok(sibling)
}

fn create_driver(name: &str, binary: PathBuf, bridge: PathBuf) -> Option<Arc<dyn Driver>> {
    match name {
        "chatrs" => Some(Arc::new(ChatrsDriver::new(binary, bridge))),
        "claude" => Some(Arc::new(ClaudeDriver::new(binary, bridge))),
        "cursor" => Some(Arc::new(CursorDriver::new(binary, bridge))),
        "codex" => Some(Arc::new(CodexDriver::new(binary, bridge))),
        "gemini" => Some(Arc::new(GeminiDriver::new(binary, bridge))),
        "grok" => Some(Arc::new(GrokDriver::new(binary))),
        "kimi" => Some(Arc::new(KimiDriver::new(binary, bridge))),
        "opencode" => Some(Arc::new(OpenCodeDriver::new(binary, bridge))),
        _ => None,
    }
}

fn runtime_info(entry: &RuntimeCatalogEntry) -> RuntimeInfo {
    RuntimeInfo {
        name: entry.name.clone(),
        installed: entry.installed && entry.unavailable_reason.is_none(),
        binary: entry
            .binary
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned()),
        version: entry.version.clone(),
        models: entry.models.iter().map(|model| model.id.clone()).collect(),
        capabilities: entry
            .capabilities
            .as_ref()
            .map(capability_names)
            .unwrap_or_default(),
        unavailable_reason: entry.unavailable_reason.clone(),
    }
}

fn capability_names(capabilities: &RuntimeCapabilities) -> Vec<String> {
    let mut names = vec![
        format!("delivery:{}", capabilities.busy_delivery_mode),
        format!("env:{}", capabilities.env_propagation),
        format!("skills:{}", capabilities.skill_compatibility),
    ];
    if capabilities.supports_turn_cancel {
        names.push("turn_cancel".to_owned());
    }
    if capabilities.supports_turn_steer {
        names.push("turn_steer".to_owned());
    }
    if capabilities.supports_thread_fork {
        names.push("thread_fork".to_owned());
    }
    if capabilities.is_turn_exit {
        names.push("turn_exit".to_owned());
    }
    names
}

/// Errors raised while assembling the local runtime service.
#[derive(Debug, thiserror::Error)]
pub enum RuntimeSetupError {
    /// The current executable path could not be inspected.
    #[error("failed to inspect the current executable while resolving cocli-bridge")]
    CurrentExecutable(#[source] std::io::Error),
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::process::Stdio;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex as StdMutex;

    use cocli_driver_core::types::{
        BusyDeliveryMode, DriverAgentConfig, EnvPropagation, MessageMode, SkillCompatibility,
        SpawnConfig,
    };
    use cocli_driver_core::{DriverError, DriverEvent};
    use cocli_pidfile::TestPidDirGuard;
    use cocli_runtime_pool::{RuntimeModel, RuntimeSpec};
    use tempfile::tempdir;
    use tokio::process::Command;

    use super::*;

    struct FakeDriver;

    #[async_trait]
    impl Driver for FakeDriver {
        fn name(&self) -> &str {
            "fake"
        }

        fn mcp_tool_prefix(&self) -> &str {
            ""
        }

        fn busy_delivery_mode(&self) -> BusyDeliveryMode {
            BusyDeliveryMode::Direct
        }

        fn requires_initial_prompt(&self) -> bool {
            true
        }

        fn env_propagation(&self) -> EnvPropagation {
            EnvPropagation::Inherit
        }

        fn skill_compatibility(&self) -> SkillCompatibility {
            SkillCompatibility::Supported
        }

        fn prepare_workspace(
            &self,
            _work_dir: &Path,
            _config: &DriverAgentConfig,
            _agent_id: &str,
            _system_prompt: &str,
        ) -> Result<(), DriverError> {
            Ok(())
        }

        fn spawn(&self, _cfg: &SpawnConfig) -> Result<tokio::process::Child, DriverError> {
            Command::new("/bin/sh")
                .arg("-c")
                .arg(
                    "printf 'session\\n'; while IFS= read -r line; do printf 'text:%s\\n' \"$line\"; printf 'turn\\n'; done",
                )
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true)
                .spawn()
                .map_err(DriverError::Io)
        }

        fn parse_event(&self, line: &str) -> Vec<DriverEvent> {
            if line == "session" {
                vec![DriverEvent::SessionStarted {
                    session_id: "session-1".to_owned(),
                }]
            } else if let Some(text) = line.strip_prefix("text:") {
                vec![DriverEvent::TextDelta {
                    text: text.to_owned(),
                }]
            } else if line == "turn" {
                vec![DriverEvent::TurnEnd {
                    status: TurnStatus::Completed,
                    input_tokens: 1,
                    output_tokens: 1,
                    cost_usd: 0.0,
                    cache_creation_tokens: 0,
                    cache_read_tokens: 0,
                    context_window_tokens: 1024,
                }]
            } else {
                vec![DriverEvent::Unknown]
            }
        }

        fn encode_stdin_message(
            &self,
            text: &str,
            _session_id: Option<&str>,
            _mode: MessageMode,
        ) -> Option<String> {
            Some(text.to_owned())
        }

        fn supports_turn_cancel(&self) -> bool {
            true
        }

        fn skill_search_paths(&self, _workspace: &Path) -> Vec<PathBuf> {
            Vec::new()
        }
    }

    struct BootstrapWriteDriver;

    #[async_trait]
    impl Driver for BootstrapWriteDriver {
        fn name(&self) -> &str {
            "bootstrap-write"
        }

        fn mcp_tool_prefix(&self) -> &str {
            ""
        }

        fn busy_delivery_mode(&self) -> BusyDeliveryMode {
            BusyDeliveryMode::Direct
        }

        fn env_propagation(&self) -> EnvPropagation {
            EnvPropagation::Inherit
        }

        fn skill_compatibility(&self) -> SkillCompatibility {
            SkillCompatibility::Supported
        }

        fn prepare_workspace(
            &self,
            _work_dir: &Path,
            _config: &DriverAgentConfig,
            _agent_id: &str,
            _system_prompt: &str,
        ) -> Result<(), DriverError> {
            Ok(())
        }

        fn spawn(&self, _cfg: &SpawnConfig) -> Result<tokio::process::Child, DriverError> {
            Command::new("/bin/sh")
                .arg("-c")
                .arg(
                    "printf 'session\\n'; while IFS= read -r line; do printf 'text:%s\\n' \"$line\"; printf 'turn\\n'; done",
                )
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true)
                .spawn()
                .map_err(DriverError::Io)
        }

        fn parse_event(&self, line: &str) -> Vec<DriverEvent> {
            if line == "session" {
                vec![
                    DriverEvent::SessionStarted {
                        session_id: "session-2".to_owned(),
                    },
                    DriverEvent::Write {
                        data: "bootstrap".to_owned(),
                    },
                ]
            } else {
                FakeDriver.parse_event(line)
            }
        }

        fn encode_stdin_message(
            &self,
            text: &str,
            _session_id: Option<&str>,
            _mode: MessageMode,
        ) -> Option<String> {
            Some(text.to_owned())
        }

        fn supports_turn_cancel(&self) -> bool {
            true
        }

        fn skill_search_paths(&self, _workspace: &Path) -> Vec<PathBuf> {
            Vec::new()
        }
    }

    struct ControllableDriver {
        spawn_count: Arc<AtomicUsize>,
        fork_count: Arc<AtomicUsize>,
        steers: Arc<StdMutex<Vec<String>>>,
        native_fork: bool,
    }

    #[async_trait]
    impl Driver for ControllableDriver {
        fn name(&self) -> &str {
            "controllable"
        }

        fn mcp_tool_prefix(&self) -> &str {
            ""
        }

        fn busy_delivery_mode(&self) -> BusyDeliveryMode {
            BusyDeliveryMode::Direct
        }

        fn requires_initial_prompt(&self) -> bool {
            true
        }

        fn env_propagation(&self) -> EnvPropagation {
            EnvPropagation::Inherit
        }

        fn skill_compatibility(&self) -> SkillCompatibility {
            SkillCompatibility::Supported
        }

        fn context_window_tokens(&self) -> Option<u32> {
            Some(1_000)
        }

        fn prepare_workspace(
            &self,
            _work_dir: &Path,
            _config: &DriverAgentConfig,
            _agent_id: &str,
            _system_prompt: &str,
        ) -> Result<(), DriverError> {
            Ok(())
        }

        fn spawn(&self, _cfg: &SpawnConfig) -> Result<tokio::process::Child, DriverError> {
            self.spawn_count.fetch_add(1, Ordering::Relaxed);
            Command::new("/bin/sh")
                .arg("-c")
                .arg(
                    "printf 'session\\n'; n=0; while IFS= read -r line; do n=$((n+1)); printf 'text:%s\\n' \"$line\"; if [ \"$n\" -gt 1 ]; then sleep 0.15; fi; printf 'turn\\n'; done",
                )
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true)
                .spawn()
                .map_err(DriverError::Io)
        }

        fn parse_event(&self, line: &str) -> Vec<DriverEvent> {
            if line == "session" {
                vec![DriverEvent::SessionStarted {
                    session_id: "session-control".to_owned(),
                }]
            } else if let Some(text) = line.strip_prefix("text:") {
                vec![DriverEvent::TextDelta {
                    text: text.to_owned(),
                }]
            } else if line == "turn" {
                vec![DriverEvent::TurnEnd {
                    status: TurnStatus::Completed,
                    input_tokens: 800,
                    output_tokens: 20,
                    cost_usd: 0.0,
                    cache_creation_tokens: 0,
                    cache_read_tokens: 0,
                    context_window_tokens: 1_000,
                }]
            } else {
                vec![DriverEvent::Unknown]
            }
        }

        fn encode_stdin_message(
            &self,
            text: &str,
            _session_id: Option<&str>,
            _mode: MessageMode,
        ) -> Option<String> {
            Some(text.to_owned())
        }

        fn supports_turn_cancel(&self) -> bool {
            true
        }

        fn supports_turn_steer(&self) -> bool {
            true
        }

        fn supports_thread_fork(&self) -> bool {
            self.native_fork
        }

        async fn turn_steer(&self, input: &str) -> Result<(), DriverError> {
            self.steers
                .lock()
                .expect("steer lock")
                .push(input.to_owned());
            Ok(())
        }

        async fn fork_thread(&self, _thread_id: &str) -> Result<String, DriverError> {
            let fork = self.fork_count.fetch_add(1, Ordering::Relaxed) + 1;
            Ok(format!("session-forked-{fork}"))
        }

        fn skill_search_paths(&self, _workspace: &Path) -> Vec<PathBuf> {
            Vec::new()
        }
    }

    fn test_agent(runtime: &str) -> Agent {
        Agent {
            id: Uuid::new_v4(),
            channel_id: Uuid::new_v4(),
            name: "builder".to_owned(),
            runtime: runtime.to_owned(),
            model: Some("test-model".to_owned()),
            status: cocli_store::AgentStatus::Running,
            created_at: chrono::Utc::now(),
        }
    }

    fn test_message(agent: &Agent, seq: i64, content: &str) -> Message {
        Message {
            id: Uuid::new_v4(),
            channel_id: agent.channel_id,
            seq,
            agent_id: None,
            role: cocli_store::MessageRole::User,
            content: content.to_owned(),
            created_at: chrono::Utc::now(),
        }
    }

    #[tokio::test]
    async fn completes_a_reply_through_agent_actor() {
        let temp = tempdir().expect("temp directory");
        let _pid_guard = TestPidDirGuard::new(&temp.path().join("pids"));
        let mut registry = RuntimeRegistry::new();
        registry.register(Arc::new(FakeDriver));
        let catalog = registry.discover(
            &[RuntimeSpec::new("fake", "/bin/sh")
                .with_models(vec![RuntimeModel::new("test-model", "Test Model")])],
            &SystemRuntimeProbe::with_path(None),
        );
        let service = LocalRuntimeService::from_catalog(
            registry,
            catalog,
            LocalRuntimeConfig {
                workspace_root: temp.path().join("workspaces"),
                server_url: "http://127.0.0.1:8090".to_owned(),
                auth_token: String::new(),
                turn_timeout: Duration::from_secs(2),
            },
        );
        let agent = Agent {
            id: Uuid::new_v4(),
            channel_id: Uuid::new_v4(),
            name: "builder".to_owned(),
            runtime: "fake".to_owned(),
            model: Some("test-model".to_owned()),
            status: cocli_store::AgentStatus::Running,
            created_at: chrono::Utc::now(),
        };
        let message = Message {
            id: Uuid::new_v4(),
            channel_id: agent.channel_id,
            seq: 1,
            agent_id: None,
            role: cocli_store::MessageRole::User,
            content: "ship it".to_owned(),
            created_at: chrono::Utc::now(),
        };

        let reply = service
            .reply(&agent, &message)
            .await
            .expect("runtime reply");

        assert!(reply.contains("ship it"));
        assert_eq!(service.list().await[0].models, vec!["test-model"]);
    }

    #[test]
    fn creates_every_runtime_in_the_oss_catalog() {
        let binary = PathBuf::from("/bin/false");
        let bridge = PathBuf::from("/bin/cocli-bridge");

        for runtime in initial_oss_runtime_specs() {
            let driver = create_driver(&runtime.name, binary.clone(), bridge.clone())
                .unwrap_or_else(|| panic!("missing local driver constructor: {}", runtime.name));
            assert_eq!(driver.name(), runtime.name);
        }
    }

    #[tokio::test]
    async fn executes_driver_requested_bootstrap_write_before_waiting_for_turn() {
        let temp = tempdir().expect("temp directory");
        let _pid_guard = TestPidDirGuard::new(&temp.path().join("pids"));
        let mut registry = RuntimeRegistry::new();
        registry.register(Arc::new(BootstrapWriteDriver));
        let service = LocalRuntimeService::from_catalog(
            registry,
            RuntimeCatalog::default(),
            LocalRuntimeConfig {
                workspace_root: temp.path().join("workspaces"),
                server_url: "http://127.0.0.1:8090".to_owned(),
                auth_token: String::new(),
                turn_timeout: Duration::from_secs(2),
            },
        );
        let agent = Agent {
            id: Uuid::new_v4(),
            channel_id: Uuid::new_v4(),
            name: "builder".to_owned(),
            runtime: "bootstrap-write".to_owned(),
            model: Some("test-model".to_owned()),
            status: cocli_store::AgentStatus::Running,
            created_at: chrono::Utc::now(),
        };
        let message = Message {
            id: Uuid::new_v4(),
            channel_id: agent.channel_id,
            seq: 1,
            agent_id: None,
            role: cocli_store::MessageRole::User,
            content: "ship it".to_owned(),
            created_at: chrono::Utc::now(),
        };

        let reply = service
            .reply(&agent, &message)
            .await
            .expect("runtime reply");

        assert!(reply.contains("ship it"));
    }

    #[tokio::test]
    async fn reuses_one_live_process_and_tracks_context_pressure() {
        let temp = tempdir().expect("temp directory");
        let _pid_guard = TestPidDirGuard::new(&temp.path().join("pids"));
        let spawn_count = Arc::new(AtomicUsize::new(0));
        let mut registry = RuntimeRegistry::new();
        registry.register(Arc::new(ControllableDriver {
            spawn_count: Arc::clone(&spawn_count),
            fork_count: Arc::new(AtomicUsize::new(0)),
            steers: Arc::new(StdMutex::new(Vec::new())),
            native_fork: true,
        }));
        let service = LocalRuntimeService::from_catalog(
            registry,
            RuntimeCatalog::default(),
            LocalRuntimeConfig {
                workspace_root: temp.path().join("workspaces"),
                server_url: "http://127.0.0.1:8090".to_owned(),
                auth_token: String::new(),
                turn_timeout: Duration::from_secs(2),
            },
        );
        let agent = test_agent("controllable");

        let first = service
            .reply(&agent, &test_message(&agent, 1, "first"))
            .await
            .expect("first reply");
        let second = service
            .reply(&agent, &test_message(&agent, 2, "second"))
            .await
            .expect("second reply");

        assert!(first.contains("first"));
        assert!(second.contains("second"));
        assert_eq!(spawn_count.load(Ordering::Relaxed), 1);
        let status = service.status(&agent).await.expect("runtime status");
        assert!(status.running);
        assert!(!status.active_turn);
        assert_eq!(status.session_id, "session-control");
        assert_eq!(status.input_tokens, 800);
        assert_eq!(status.context_window_tokens, 1_000);
        assert_eq!(status.tier, "warn");
        assert!(status.fork_suggested);
        let metrics = service.metrics().await;
        assert_eq!(metrics.counters["local_agent_session_started_total"], 1);
        assert_eq!(metrics.counters["local_agent_session_reused_total"], 1);
        assert_eq!(metrics.counters["local_agent_turn_started_total"], 2);
        assert_eq!(metrics.counters["local_agent_turn_completed_total"], 2);
        assert_eq!(metrics.gauges["local_agent_active_sessions"], 1.0);
        assert_eq!(metrics.gauges["local_agent_active_turns"], 0.0);
    }

    #[tokio::test]
    async fn steers_active_turn_and_forks_idle_session_natively() {
        let temp = tempdir().expect("temp directory");
        let _pid_guard = TestPidDirGuard::new(&temp.path().join("pids"));
        let spawn_count = Arc::new(AtomicUsize::new(0));
        let fork_count = Arc::new(AtomicUsize::new(0));
        let steers = Arc::new(StdMutex::new(Vec::new()));
        let mut registry = RuntimeRegistry::new();
        registry.register(Arc::new(ControllableDriver {
            spawn_count: Arc::clone(&spawn_count),
            fork_count: Arc::clone(&fork_count),
            steers: Arc::clone(&steers),
            native_fork: true,
        }));
        let service = Arc::new(LocalRuntimeService::from_catalog(
            registry,
            RuntimeCatalog::default(),
            LocalRuntimeConfig {
                workspace_root: temp.path().join("workspaces"),
                server_url: "http://127.0.0.1:8090".to_owned(),
                auth_token: String::new(),
                turn_timeout: Duration::from_secs(2),
            },
        ));
        let agent = test_agent("controllable");
        service.start(&agent).await.expect("session starts");

        let reply_task = {
            let service = Arc::clone(&service);
            let agent = agent.clone();
            tokio::spawn(async move {
                service
                    .reply(&agent, &test_message(&agent, 1, "redirectable"))
                    .await
            })
        };
        for _ in 0..50 {
            if service
                .status(&agent)
                .await
                .expect("runtime status")
                .active_turn
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(
            service
                .status(&agent)
                .await
                .expect("runtime status")
                .active_turn
        );

        service
            .steer(agent.id, "change direction")
            .await
            .expect("turn steer");
        assert_eq!(
            steers.lock().expect("steer lock").as_slice(),
            ["change direction"]
        );
        let reply = reply_task
            .await
            .expect("reply task")
            .expect("runtime reply");
        assert!(reply.contains("redirectable"));

        let fork = service
            .fork(&agent, "context_reset: test")
            .await
            .expect("thread fork");
        assert!(fork.native);
        assert_eq!(fork.fork_id, "session-forked-1");
        assert_eq!(fork_count.load(Ordering::Relaxed), 1);
        assert_eq!(spawn_count.load(Ordering::Relaxed), 1);
        assert_eq!(
            service
                .status(&agent)
                .await
                .expect("runtime status")
                .session_id,
            "session-forked-1"
        );
    }

    #[tokio::test]
    async fn unsupported_native_fork_restarts_with_a_fresh_process() {
        let temp = tempdir().expect("temp directory");
        let _pid_guard = TestPidDirGuard::new(&temp.path().join("pids"));
        let spawn_count = Arc::new(AtomicUsize::new(0));
        let mut registry = RuntimeRegistry::new();
        registry.register(Arc::new(ControllableDriver {
            spawn_count: Arc::clone(&spawn_count),
            fork_count: Arc::new(AtomicUsize::new(0)),
            steers: Arc::new(StdMutex::new(Vec::new())),
            native_fork: false,
        }));
        let service = LocalRuntimeService::from_catalog(
            registry,
            RuntimeCatalog::default(),
            LocalRuntimeConfig {
                workspace_root: temp.path().join("workspaces"),
                server_url: "http://127.0.0.1:8090".to_owned(),
                auth_token: String::new(),
                turn_timeout: Duration::from_secs(2),
            },
        );
        let agent = test_agent("controllable");
        service.start(&agent).await.expect("session starts");

        let fork = service
            .fork(&agent, "context_reset: fresh context")
            .await
            .expect("restart-backed fork");

        assert!(!fork.native);
        assert_eq!(fork.fork_id, "session-control");
        assert_eq!(spawn_count.load(Ordering::Relaxed), 2);
        assert!(
            service
                .status(&agent)
                .await
                .expect("runtime status")
                .running
        );
    }
}
