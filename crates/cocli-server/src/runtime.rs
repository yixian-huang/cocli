use std::collections::{BTreeSet, HashMap};
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant as StdInstant};

use async_trait::async_trait;
use cocli_agent::context::{classify_context_pressure, default_backstop_pct, ContextPressureTier};
use cocli_agent::fork_reason::classify_fork_reason;
use cocli_agent::prompt::{
    build_local_system_prompt, compose_session_bootstrap_prompt, LocalPromptConfig,
    LOCAL_INITIALIZATION_PROMPT,
};
use cocli_agent::recovery::{ProbeResult, RecoveryStore};
use cocli_agent::state::Idle;
use cocli_agent::watchdog::{WatchdogStore, AUTO_RETRY_MAX};
use cocli_agent::{AgentActor, AgentMetrics, StartCfg};
use cocli_api::{
    LiveEvent, LiveEventSink, RuntimeBridgeTokenProvider, RuntimeError, RuntimeForkResult,
    RuntimeHistoryEvent, RuntimeHistorySink, RuntimeInfo, RuntimeKnowledgeProvider,
    RuntimeKnowledgeSnapshot, RuntimeMetricsSnapshot, RuntimeRecoveryProbeResult,
    RuntimeRecoveryStatus, RuntimeService, RuntimeSessionStatus, RuntimeSkill,
    RuntimeSkillCompatibility, RuntimeSkillFileContent, RuntimeSkillFileEntry,
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
use cocli_store::{Agent, Message, SkillLibraryFile};
use tokio::sync::{broadcast, mpsc, oneshot, Mutex, RwLock};
use tokio::time::Instant;
use uuid::Uuid;

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
    /// Retry delays for the five watchdog restart attempts.
    pub watchdog_backoff: [Duration; 5],
    /// Retry delays for quota/rate-limit recovery probes.
    pub recovery_backoff: [Duration; 5],
}

impl LocalRuntimeConfig {
    /// Creates a local runtime configuration with a two-minute turn budget.
    pub fn new(workspace_root: PathBuf, server_url: String) -> Self {
        Self {
            workspace_root,
            server_url,
            auth_token: String::new(),
            turn_timeout: Duration::from_secs(120),
            watchdog_backoff: [
                Duration::from_secs(1),
                Duration::from_secs(2),
                Duration::from_secs(5),
                Duration::from_secs(10),
                Duration::from_secs(30),
            ],
            recovery_backoff: [
                Duration::from_secs(5 * 60),
                Duration::from_secs(10 * 60),
                Duration::from_secs(30 * 60),
                Duration::from_secs(60 * 60),
                Duration::from_secs(120 * 60),
            ],
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
    recovery: Arc<Mutex<RecoveryStore>>,
    history_sink: OnceLock<Arc<dyn RuntimeHistorySink>>,
    knowledge_provider: OnceLock<Arc<dyn RuntimeKnowledgeProvider>>,
    bridge_token_provider: OnceLock<Arc<dyn RuntimeBridgeTokenProvider>>,
    live_event_sink: OnceLock<Arc<dyn LiveEventSink>>,
}

#[derive(Clone)]
struct LocalSessionHandle {
    command_tx: mpsc::Sender<LocalSessionCommand>,
    status: Arc<RwLock<RuntimeSessionStatus>>,
    started_at: Arc<RwLock<Instant>>,
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
        reply: oneshot::Sender<Result<(), RuntimeError>>,
    },
}

struct ActiveReply {
    reply: Option<oneshot::Sender<Result<String, RuntimeError>>>,
    text: String,
    last_error: Option<String>,
    deadline: Option<Instant>,
    started_at: chrono::DateTime<chrono::Utc>,
    channel_id: Uuid,
    source_message_id: Option<Uuid>,
    entries: Vec<serde_json::Value>,
}

struct LocalSessionContext {
    status: Arc<RwLock<RuntimeSessionStatus>>,
    started_at: Arc<RwLock<Instant>>,
    registry: Arc<RuntimeRegistry>,
    config: LocalRuntimeConfig,
    agent: Agent,
    metrics: Arc<AgentMetrics>,
    recovery: Arc<Mutex<RecoveryStore>>,
    history_sink: Option<Arc<dyn RuntimeHistorySink>>,
    knowledge_provider: Option<Arc<dyn RuntimeKnowledgeProvider>>,
    bridge_token_provider: Option<Arc<dyn RuntimeBridgeTokenProvider>>,
    live_event_sink: Option<Arc<dyn LiveEventSink>>,
}

#[derive(Default)]
struct SessionTotals {
    turn_count: i64,
    input_tokens: i64,
    output_tokens: i64,
    cost_usd: f64,
    context_window: i64,
}

struct QuotaRecovery {
    expected_at: Option<StdInstant>,
}

struct RuntimeRestartContext<'a> {
    registry: &'a Arc<RuntimeRegistry>,
    config: &'a LocalRuntimeConfig,
    agent: &'a Agent,
    metrics: &'a AgentMetrics,
    knowledge_provider: Option<&'a Arc<dyn RuntimeKnowledgeProvider>>,
    bridge_token_provider: Option<&'a Arc<dyn RuntimeBridgeTokenProvider>>,
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
            recovery: Arc::new(Mutex::new(RecoveryStore::new())),
            history_sink: OnceLock::new(),
            knowledge_provider: OnceLock::new(),
            bridge_token_provider: OnceLock::new(),
            live_event_sink: OnceLock::new(),
        }
    }

    async fn spawn_session(&self, agent: &Agent) -> Result<LocalSessionHandle, RuntimeError> {
        let knowledge_provider = self.knowledge_provider.get().cloned();
        let bridge_token_provider = self.bridge_token_provider.get().cloned();
        let live_event_sink = self.live_event_sink.get().cloned();
        let running = start_running_actor(
            &self.registry,
            &self.config,
            agent,
            knowledge_provider.as_ref(),
            bridge_token_provider.as_ref(),
        )
        .await?;
        let status = Arc::new(RwLock::new(runtime_status_for_running(agent, &running)));
        let started_at = Arc::new(RwLock::new(Instant::now()));
        let (command_tx, command_rx) = mpsc::channel(16);
        let status_for_task = Arc::clone(&status);
        let started_at_for_task = Arc::clone(&started_at);
        let config = self.config.clone();
        let registry = Arc::clone(&self.registry);
        let agent = agent.clone();
        let metrics = Arc::clone(&self.metrics);
        let recovery = Arc::clone(&self.recovery);
        let history_sink = self.history_sink.get().cloned();
        if let Err(error) = record_session_start(&history_sink, &agent, &running).await {
            if let Err(stop_error) = running.stop(true).await {
                tracing::error!(
                    agent_id = %agent.id,
                    %stop_error,
                    "failed to reap runtime after durable session-start failure"
                );
            }
            return Err(error);
        }
        let agent_id = agent.id;
        metrics.inc_local_session_started();
        metrics.inc_local_active_sessions();
        let cleanup_status = Arc::clone(&status_for_task);
        let cleanup_metrics = Arc::clone(&metrics);
        tokio::spawn(async move {
            let result = run_local_session(
                running,
                command_rx,
                LocalSessionContext {
                    status: status_for_task,
                    started_at: started_at_for_task,
                    registry,
                    config,
                    agent,
                    metrics,
                    recovery,
                    history_sink,
                    knowledge_provider,
                    bridge_token_provider,
                    live_event_sink,
                },
            )
            .await;
            if let Err(error) = result {
                tracing::error!(%agent_id, %error, "local runtime session stopped after durable history failure");
            }
            let mut state = cleanup_status.write().await;
            if state.active_turn {
                cleanup_metrics.dec_local_active_turns();
            }
            cleanup_metrics.inc_local_session_stopped();
            cleanup_metrics.dec_local_active_sessions();
            state.running = false;
            state.recovering = false;
            state.active_turn = false;
        });
        Ok(LocalSessionHandle {
            command_tx,
            status,
            started_at,
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
            self.refresh_runtime_knowledge(agent).await?;
            self.metrics.inc_local_session_reused();
            return Ok(handle);
        }

        let start_lock = self.session_lock(agent.id).await;
        let _guard = start_lock.lock().await;
        if let Some(handle) = self.live_session(agent.id).await {
            self.refresh_runtime_knowledge(agent).await?;
            self.metrics.inc_local_session_reused();
            return Ok(handle);
        }

        let handle = self.spawn_session(agent).await?;
        self.sessions.lock().await.insert(agent.id, handle.clone());
        Ok(handle)
    }

    async fn live_session(&self, agent_id: Uuid) -> Option<LocalSessionHandle> {
        let handle = self.sessions.lock().await.get(&agent_id).cloned()?;
        let status = handle.status.read().await;
        if !handle.command_tx.is_closed() && (status.running || status.recovering) {
            drop(status);
            return Some(handle);
        }
        drop(status);
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
        status.session_age_seconds = handle.started_at.read().await.elapsed().as_secs();
        status
    }

    async fn run_reply(&self, agent: &Agent, message: &Message) -> Result<String, RuntimeError> {
        let handle = self.session_handle(agent).await?;
        let delivery = AgentDeliverMsg {
            agent_id: agent.id.to_string(),
            message: DeliveryMessage {
                channel_id: message.channel_id,
                sender_name: "user".to_owned(),
                sender_type: "user".to_owned(),
                content: message.content.clone(),
                channel_name: "local".to_owned(),
                channel_type: "channel".to_owned(),
                seq: message.seq,
                message_id: message.id.to_string(),
                timestamp: message.created_at.to_rfc3339(),
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

    async fn refresh_runtime_knowledge(&self, agent: &Agent) -> Result<(), RuntimeError> {
        let Some(provider) = self.knowledge_provider.get() else {
            return Ok(());
        };
        let snapshot = provider.snapshot(agent).await?;
        materialize_runtime_knowledge(&self.config, agent, &snapshot).await
    }
}

#[async_trait]
impl RuntimeService for LocalRuntimeService {
    fn set_history_sink(&self, sink: Arc<dyn RuntimeHistorySink>) {
        let _ = self.history_sink.set(sink);
    }

    fn set_knowledge_provider(&self, provider: Arc<dyn RuntimeKnowledgeProvider>) {
        let _ = self.knowledge_provider.set(provider);
    }

    fn set_bridge_token_provider(&self, provider: Arc<dyn RuntimeBridgeTokenProvider>) {
        let _ = self.bridge_token_provider.set(provider);
    }

    fn set_live_event_sink(&self, sink: Arc<dyn LiveEventSink>) {
        let _ = self.live_event_sink.set(sink);
    }

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
        let stop_lock = self.session_lock(agent_id).await;
        let _guard = stop_lock.lock().await;
        let Some(handle) = self.sessions.lock().await.get(&agent_id).cloned() else {
            return Ok(());
        };
        let (reply_tx, reply_rx) = oneshot::channel();
        if handle
            .command_tx
            .send(LocalSessionCommand::Stop { reply: reply_tx })
            .await
            .is_err()
        {
            self.live_session(agent_id).await;
            return Err(RuntimeError::NotFound(
                "agent runtime session exited".to_owned(),
            ));
        }
        let result = match tokio::time::timeout(stop_reply_timeout(), reply_rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(RuntimeError::NotFound(
                "agent runtime stop reply channel closed".to_owned(),
            )),
            Err(_) => Err(RuntimeError::Delivery(
                "agent runtime stop timed out".to_owned(),
            )),
        };
        if result.is_ok() {
            let mut sessions = self.sessions.lock().await;
            if sessions
                .get(&agent_id)
                .is_some_and(|registered| registered.command_tx.same_channel(&handle.command_tx))
            {
                sessions.remove(&agent_id);
            }
        }
        result
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
            let mut status = inactive_session_status(agent);
            if let Some(recovery) = self.recovery.lock().await.status(&agent.id.to_string()) {
                status.recovery = Some(RuntimeRecoveryStatus {
                    provider: recovery.provider,
                    reason: recovery.stop_reason,
                    expected_recovery_at_ms: 0,
                });
            }
            return Ok(status);
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

    fn skill_compatibility(&self, runtime: &str) -> RuntimeSkillCompatibility {
        crate::skills::compatibility(&self.registry, runtime)
    }

    async fn list_skills(&self, agent: &Agent) -> Result<Vec<RuntimeSkill>, RuntimeError> {
        crate::skills::list(&self.registry, &self.config, agent).await
    }

    async fn inspect_skills(
        &self,
        agent: &Agent,
    ) -> Result<cocli_api::RuntimeSkillInspection, RuntimeError> {
        crate::skills::inspect(&self.registry, &self.config, agent).await
    }

    async fn inspect_machine_skills(
        &self,
        runtime: &str,
    ) -> Result<cocli_api::RuntimeSkillInspection, RuntimeError> {
        crate::skills::inspect_machine(&self.registry, &self.config, runtime).await
    }

    async fn install_skill(
        &self,
        agent: &Agent,
        skill_name: &str,
        files: &[SkillLibraryFile],
    ) -> Result<String, RuntimeError> {
        let install_path =
            crate::skills::install(&self.registry, &self.config, agent, skill_name, files).await?;
        self.stop(agent.id).await?;
        Ok(install_path)
    }

    async fn uninstall_skill(&self, agent: &Agent, install_path: &str) -> Result<(), RuntimeError> {
        crate::skills::uninstall(&self.registry, &self.config, agent, install_path).await?;
        self.stop(agent.id).await
    }

    async fn list_skill_files(
        &self,
        agent: &Agent,
        install_path: &str,
    ) -> Result<Vec<RuntimeSkillFileEntry>, RuntimeError> {
        crate::skills::list_files(&self.registry, &self.config, agent, install_path).await
    }

    async fn read_skill_file(
        &self,
        agent: &Agent,
        install_path: &str,
        relative_path: &str,
    ) -> Result<RuntimeSkillFileContent, RuntimeError> {
        crate::skills::read_file(
            &self.registry,
            &self.config,
            agent,
            install_path,
            relative_path,
        )
        .await
    }

    async fn probe_recovery(
        &self,
        agent: &Agent,
    ) -> Result<RuntimeRecoveryProbeResult, RuntimeError> {
        let agent_id = agent.id.to_string();
        if !self.recovery.lock().await.contains(&agent_id) {
            return Ok(RuntimeRecoveryProbeResult {
                result: ProbeResult::NoState.as_wire().to_owned(),
                detail: String::new(),
            });
        }

        self.metrics.inc_recovery_probe_scheduled();
        self.stop(agent.id).await?;
        match self.session_handle(agent).await {
            Ok(_) => {
                let mut recovery = self.recovery.lock().await;
                recovery.clear(&agent_id);
                self.metrics.inc_recovery_probe_recovered();
                self.metrics.set_recovery_tracked_agents(recovery.len());
                Ok(RuntimeRecoveryProbeResult {
                    result: ProbeResult::Recovered.as_wire().to_owned(),
                    detail: "local runtime restarted successfully".to_owned(),
                })
            }
            Err(error) => {
                let mut recovery = self.recovery.lock().await;
                recovery.complete_probe(&agent_id, StdInstant::now(), ProbeResult::Error);
                self.metrics.inc_recovery_probe_error();
                self.metrics.set_recovery_tracked_agents(recovery.len());
                Ok(RuntimeRecoveryProbeResult {
                    result: ProbeResult::Error.as_wire().to_owned(),
                    detail: error.to_string(),
                })
            }
        }
    }
}

const MEMORY_MANIFEST_PATH: &str = "memory/.cocli-memory-manifest.json";
const MAX_PROMPT_MEMORY_INDEX_CHARS: usize = 16 * 1024;

fn stop_reply_timeout() -> Duration {
    if cfg!(test) {
        Duration::from_millis(100)
    } else {
        Duration::from_secs(5)
    }
}

async fn materialize_runtime_knowledge(
    config: &LocalRuntimeConfig,
    agent: &Agent,
    snapshot: &RuntimeKnowledgeSnapshot,
) -> Result<(), RuntimeError> {
    let workspace_dir = config.workspace_root.join(agent.id.to_string());
    tokio::fs::create_dir_all(&workspace_dir)
        .await
        .map_err(|error| runtime_knowledge_io_error("create workspace", &error))?;

    let previous_paths = read_memory_manifest(&workspace_dir).await;
    let mut current_paths = BTreeSet::new();
    for file in &snapshot.files {
        if !current_paths.insert(file.relative_path.clone()) {
            return Err(RuntimeError::Delivery(format!(
                "duplicate runtime knowledge path: {}",
                file.relative_path
            )));
        }
        let target = managed_workspace_path(&workspace_dir, &file.relative_path)
            .map_err(|error| runtime_knowledge_io_error("resolve memory mirror path", &error))?;
        atomic_write_file(&target, file.content.as_bytes(), true)
            .await
            .map_err(|error| runtime_knowledge_io_error("write memory mirror", &error))?;
    }

    for stale in previous_paths.difference(&current_paths) {
        let target = match managed_workspace_path(&workspace_dir, stale) {
            Ok(target) => target,
            Err(error) => {
                tracing::warn!(path = stale, %error, "ignored invalid stale memory mirror path");
                continue;
            }
        };
        match tokio::fs::remove_file(&target).await {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                tracing::warn!(
                    path = %target.display(),
                    %error,
                    "failed to remove stale memory mirror file"
                );
            }
        }
    }

    let manifest_path = managed_workspace_path(&workspace_dir, MEMORY_MANIFEST_PATH)
        .map_err(|error| runtime_knowledge_io_error("resolve memory manifest", &error))?;
    let managed: Vec<&str> = current_paths.iter().map(String::as_str).collect();
    let manifest = serde_json::to_vec_pretty(&managed).map_err(|error| {
        RuntimeError::Delivery(format!(
            "failed to serialize runtime memory manifest: {error}"
        ))
    })?;
    atomic_write_file(&manifest_path, &manifest, false)
        .await
        .map_err(|error| runtime_knowledge_io_error("write memory manifest", &error))?;
    Ok(())
}

async fn read_memory_manifest(workspace_dir: &Path) -> BTreeSet<String> {
    let path = match managed_workspace_path(workspace_dir, MEMORY_MANIFEST_PATH) {
        Ok(path) => path,
        Err(error) => {
            tracing::warn!(%error, "failed to resolve runtime memory manifest");
            return BTreeSet::new();
        }
    };
    let bytes = match tokio::fs::read(&path).await {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return BTreeSet::new(),
        Err(error) => {
            tracing::warn!(path = %path.display(), %error, "failed to read runtime memory manifest");
            return BTreeSet::new();
        }
    };
    match serde_json::from_slice::<Vec<String>>(&bytes) {
        Ok(paths) => paths.into_iter().collect(),
        Err(error) => {
            tracing::warn!(path = %path.display(), %error, "ignored invalid runtime memory manifest");
            BTreeSet::new()
        }
    }
}

fn managed_workspace_path(workspace_dir: &Path, relative_path: &str) -> io::Result<PathBuf> {
    let relative = Path::new(relative_path);
    if relative.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "absolute runtime knowledge path is not allowed",
        ));
    }
    let mut components = relative.components();
    if components.next() != Some(Component::Normal("memory".as_ref())) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "runtime knowledge path must be inside memory/",
        ));
    }
    if components.any(|component| !matches!(component, Component::Normal(_))) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "runtime knowledge path contains unsafe components",
        ));
    }
    let file_name = relative.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "runtime knowledge path must name a file",
        )
    })?;
    let parent = relative.parent().unwrap_or(Path::new(""));
    let parent_relative = parent.to_str().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "runtime knowledge path must be valid UTF-8",
        )
    })?;
    let resolved_parent = cocli_agent::workspace::resolve_within(workspace_dir, parent_relative)?;
    Ok(resolved_parent.join(file_name))
}

async fn atomic_write_file(path: &Path, content: &[u8], read_only: bool) -> io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "runtime knowledge file has no parent",
        )
    })?;
    tokio::fs::create_dir_all(parent).await?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("memory");
    let temporary = parent.join(format!(".{file_name}.{}.tmp", Uuid::new_v4()));
    tokio::fs::write(&temporary, content).await?;
    set_runtime_file_permissions(&temporary, read_only).await?;
    if let Err(error) = tokio::fs::rename(&temporary, path).await {
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err(error);
    }
    Ok(())
}

#[cfg(unix)]
async fn set_runtime_file_permissions(path: &Path, read_only: bool) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mode = if read_only { 0o400 } else { 0o600 };
    tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).await
}

#[cfg(not(unix))]
async fn set_runtime_file_permissions(path: &Path, read_only: bool) -> io::Result<()> {
    let mut permissions = tokio::fs::metadata(path).await?.permissions();
    permissions.set_readonly(read_only);
    tokio::fs::set_permissions(path, permissions).await
}

fn runtime_knowledge_io_error(action: &str, error: &io::Error) -> RuntimeError {
    RuntimeError::Delivery(format!("{action}: {error}"))
}

fn truncate_memory_index(index: &str) -> String {
    let mut chars = index.chars();
    let truncated: String = chars.by_ref().take(MAX_PROMPT_MEMORY_INDEX_CHARS).collect();
    if chars.next().is_some() {
        format!("{truncated}\n\n<!-- index truncated for runtime prompt -->")
    } else {
        truncated
    }
}

async fn start_running_actor(
    registry: &Arc<RuntimeRegistry>,
    config: &LocalRuntimeConfig,
    agent: &Agent,
    knowledge_provider: Option<&Arc<dyn RuntimeKnowledgeProvider>>,
    bridge_token_provider: Option<&Arc<dyn RuntimeBridgeTokenProvider>>,
) -> Result<AgentActor<cocli_agent::state::Running>, RuntimeError> {
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
    let workspace_dir = config.workspace_root.join(agent.id.to_string());
    let knowledge = match knowledge_provider {
        Some(provider) => {
            let knowledge = provider.snapshot(agent).await?;
            materialize_runtime_knowledge(config, agent, &knowledge).await?;
            knowledge
        }
        None => RuntimeKnowledgeSnapshot::default(),
    };
    let current_date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let model = agent.model.as_deref().unwrap_or_default();
    let system_prompt = build_local_system_prompt(&LocalPromptConfig {
        agent_id: &agent.id.to_string(),
        agent_name: &agent.name,
        instructions: agent.instructions.as_deref().unwrap_or_default(),
        runtime: &agent.runtime,
        model,
        workspace_dir: &workspace_dir,
        current_date: &current_date,
        agent_memory_index: &truncate_memory_index(&knowledge.agent_index),
        channel_memory_index: &truncate_memory_index(&knowledge.channel_index),
    });
    let initial_prompt =
        compose_session_bootstrap_prompt(&system_prompt, LOCAL_INITIALIZATION_PROMPT);
    let auth_token = match bridge_token_provider {
        Some(provider) => provider.token(agent.id).await?,
        None => config.auth_token.clone(),
    };
    let start = actor.start(StartCfg {
        registry: Arc::clone(registry),
        runtime_name: agent.runtime.clone(),
        workspace_root: config.workspace_root.clone(),
        server_url: config.server_url.clone(),
        auth_token,
        channel_id: Uuid::nil(),
        channel_name: String::new(),
        model: model.to_owned(),
        launch_id: Uuid::new_v4().to_string(),
        resume_session: None,
        system_prompt,
        initial_prompt,
        env_vars: HashMap::new(),
    });
    let mut running = tokio::time::timeout(config.turn_timeout, start)
        .await
        .map_err(|_| RuntimeError::Delivery("runtime startup timed out".to_owned()))?
        .map_err(|error| RuntimeError::Delivery(error.to_string()))?;

    if let Err(error) = wait_for_turn(&mut running, config.turn_timeout, false).await {
        let _stopping = running.stop(true).await;
        return Err(error);
    }
    Ok(running)
}

fn runtime_status_for_running(
    agent: &Agent,
    running: &AgentActor<cocli_agent::state::Running>,
) -> RuntimeSessionStatus {
    let driver = &running.state.driver;
    RuntimeSessionStatus {
        agent_id: agent.id,
        session_id: running.state.session_id.clone(),
        runtime: agent.runtime.clone(),
        model: agent.model.clone(),
        running: true,
        recovering: false,
        active_turn: false,
        supports_turn_cancel: driver.supports_turn_cancel(),
        supports_turn_steer: driver.supports_turn_steer(),
        supports_thread_fork: driver.supports_thread_fork(),
        input_tokens: 0,
        context_window_tokens: driver.context_window_tokens().unwrap_or_default() as u64,
        context_util_pct: 0.0,
        tier: ContextPressureTier::Healthy.as_str().to_owned(),
        fork_suggested: false,
        session_age_seconds: 0,
        recovery: None,
    }
}

async fn restart_with_watchdog(
    context: RuntimeRestartContext<'_>,
    commands: &mut mpsc::Receiver<LocalSessionCommand>,
) -> Option<AgentActor<cocli_agent::state::Running>> {
    let RuntimeRestartContext {
        registry,
        config,
        agent,
        metrics,
        knowledge_provider,
        bridge_token_provider,
    } = context;
    let agent_id = agent.id.to_string();
    let mut watchdog = WatchdogStore::new();
    watchdog.register_down(&agent_id, &agent.name);

    loop {
        let event = watchdog.plan_restart(&agent_id, &agent.name)?;
        if event.action == "max_retries_exceeded" {
            metrics.inc_local_watchdog_exhausted();
            tracing::error!(
                agent_id = %agent.id,
                attempts = event.attempt,
                "local runtime watchdog exhausted restart attempts"
            );
            return None;
        }

        let retry_index = event
            .attempt
            .saturating_sub(AUTO_RETRY_MAX + 1)
            .min(config.watchdog_backoff.len() as i32 - 1) as usize;
        let delay = config.watchdog_backoff[retry_index];
        tracing::warn!(
            agent_id = %agent.id,
            attempt = event.attempt,
            delay_ms = delay.as_millis(),
            "local runtime exited; watchdog scheduled restart"
        );

        tokio::select! {
            _ = tokio::time::sleep(delay) => {}
            command = commands.recv() => {
                if reject_command_during_recovery(command) {
                    return None;
                }
                continue;
            }
        }

        match start_running_actor(
            registry,
            config,
            agent,
            knowledge_provider,
            bridge_token_provider,
        )
        .await
        {
            Ok(running) => {
                metrics.inc_local_watchdog_restart();
                metrics.inc_local_session_started();
                let _ = watchdog.mark_recovered(&agent_id);
                tracing::info!(
                    agent_id = %agent.id,
                    attempt = event.attempt,
                    "local runtime watchdog restart succeeded"
                );
                return Some(running);
            }
            Err(error) => {
                metrics.inc_local_watchdog_failure();
                let _ = watchdog.mark_restart_failed(&agent_id, error.to_string());
                tracing::warn!(
                    agent_id = %agent.id,
                    attempt = event.attempt,
                    %error,
                    "local runtime watchdog restart failed"
                );
            }
        }
    }
}

async fn recover_from_quota(
    context: RuntimeRestartContext<'_>,
    commands: &mut mpsc::Receiver<LocalSessionCommand>,
    recovery: &Mutex<RecoveryStore>,
    expected_at: Option<StdInstant>,
) -> Option<AgentActor<cocli_agent::state::Running>> {
    let RuntimeRestartContext {
        registry,
        config,
        agent,
        metrics,
        knowledge_provider,
        bridge_token_provider,
    } = context;
    let agent_id = agent.id.to_string();
    let mut delay = expected_at
        .map(|expected| expected.saturating_duration_since(StdInstant::now()))
        .unwrap_or(config.recovery_backoff[0]);

    for attempt in 0..config.recovery_backoff.len() {
        tracing::warn!(
            agent_id = %agent.id,
            attempt = attempt + 1,
            delay_ms = delay.as_millis(),
            "quota recovery probe scheduled"
        );
        tokio::select! {
            _ = tokio::time::sleep(delay) => {}
            command = commands.recv() => {
                if reject_command_during_recovery(command) {
                    return None;
                }
                continue;
            }
        }

        let due = recovery.lock().await.due_probes(StdInstant::now());
        if !due.iter().any(|probe| probe.agent_id == agent_id) {
            delay = config.recovery_backoff[(attempt + 1).min(config.recovery_backoff.len() - 1)];
            continue;
        }

        metrics.inc_recovery_probe_scheduled();
        match start_running_actor(
            registry,
            config,
            agent,
            knowledge_provider,
            bridge_token_provider,
        )
        .await
        {
            Ok(running) => {
                let mut recovery = recovery.lock().await;
                recovery.complete_probe(&agent_id, StdInstant::now(), ProbeResult::Recovered);
                metrics.inc_recovery_probe_recovered();
                metrics.set_recovery_tracked_agents(recovery.len());
                metrics.inc_local_session_started();
                tracing::info!(
                    agent_id = %agent.id,
                    attempt = attempt + 1,
                    "quota recovery probe succeeded"
                );
                return Some(running);
            }
            Err(error) => {
                let mut recovery = recovery.lock().await;
                recovery.complete_probe(&agent_id, StdInstant::now(), ProbeResult::Error);
                metrics.inc_recovery_probe_error();
                metrics.set_recovery_tracked_agents(recovery.len());
                tracing::warn!(
                    agent_id = %agent.id,
                    attempt = attempt + 1,
                    %error,
                    "quota recovery probe failed"
                );
                delay =
                    config.recovery_backoff[(attempt + 1).min(config.recovery_backoff.len() - 1)];
            }
        }
    }
    None
}

fn reject_command_during_recovery(command: Option<LocalSessionCommand>) -> bool {
    match command {
        Some(LocalSessionCommand::Reply { reply, .. }) => {
            let _ = reply.send(Err(RuntimeError::Busy(
                "agent runtime is recovering".to_owned(),
            )));
            false
        }
        Some(LocalSessionCommand::Cancel { reply })
        | Some(LocalSessionCommand::Steer { reply, .. }) => {
            let _ = reply.send(Err(RuntimeError::NotFound(
                "agent runtime is recovering".to_owned(),
            )));
            false
        }
        Some(LocalSessionCommand::Fork { reply }) => {
            let _ = reply.send(Err(RuntimeError::Busy(
                "agent runtime is recovering".to_owned(),
            )));
            false
        }
        Some(LocalSessionCommand::Stop { reply }) => {
            let _ = reply.send(Ok(()));
            true
        }
        None => true,
    }
}

fn rate_limit_is_limited(status: &str, overage_status: Option<&str>) -> bool {
    status.trim().eq_ignore_ascii_case("limited")
        || status.trim().eq_ignore_ascii_case("rejected")
        || overage_status.is_some_and(|value| value.trim().eq_ignore_ascii_case("limited"))
}

fn terminal_stop_reason(provider: &str) -> String {
    match provider.trim().to_lowercase().as_str() {
        "gemini" => "gemini_quota_terminal".to_owned(),
        "claude" => "claude_quota_terminal".to_owned(),
        "codex" => "codex_quota_terminal".to_owned(),
        other if !other.is_empty() => format!("terminal_driver_error:{other}"),
        _ => "terminal_driver_error".to_owned(),
    }
}

fn expected_recovery_time(resets_at: i64) -> (i64, Option<StdInstant>) {
    if resets_at <= 0 {
        return (0, None);
    }
    let expected_ms = resets_at.saturating_mul(1_000);
    let now_ms = chrono::Utc::now().timestamp_millis();
    let expected_at = if expected_ms <= now_ms {
        StdInstant::now()
    } else {
        StdInstant::now() + Duration::from_millis((expected_ms - now_ms) as u64)
    };
    (expected_ms, Some(expected_at))
}

async fn record_history(
    sink: &Option<Arc<dyn RuntimeHistorySink>>,
    event: RuntimeHistoryEvent,
) -> Result<(), RuntimeError> {
    let Some(sink) = sink else {
        return Ok(());
    };
    sink.record(event)
        .await
        .map_err(|error| RuntimeError::Delivery(format!("persist local runtime history: {error}")))
}

async fn record_session_start(
    sink: &Option<Arc<dyn RuntimeHistorySink>>,
    agent: &Agent,
    running: &AgentActor<cocli_agent::state::Running>,
) -> Result<(), RuntimeError> {
    record_history(
        sink,
        RuntimeHistoryEvent::SessionStarted {
            agent_id: agent.id,
            channel_id: None,
            session_id: running.state.session_id.clone(),
            launch_id: running.state.launch_id.clone(),
            started_at: chrono::Utc::now(),
        },
    )
    .await
}

async fn record_session_end(
    sink: &Option<Arc<dyn RuntimeHistorySink>>,
    agent: &Agent,
    running: &AgentActor<cocli_agent::state::Running>,
    totals: &SessionTotals,
    end_reason: &str,
) -> Result<(), RuntimeError> {
    record_history(
        sink,
        RuntimeHistoryEvent::SessionEnded {
            agent_id: agent.id,
            launch_id: running.state.launch_id.clone(),
            end_reason: end_reason.to_owned(),
            turn_count: totals.turn_count,
            input_tokens: totals.input_tokens,
            output_tokens: totals.output_tokens,
            cost_usd: totals.cost_usd,
            context_window: totals.context_window,
            ended_at: chrono::Utc::now(),
        },
    )
    .await
}

fn trajectory_entry(kind: &str, fields: serde_json::Value) -> serde_json::Value {
    let mut entry = serde_json::Map::new();
    entry.insert(
        "kind".to_owned(),
        serde_json::Value::String(kind.to_owned()),
    );
    entry.insert(
        "ts".to_owned(),
        serde_json::Value::Number(chrono::Utc::now().timestamp_millis().into()),
    );
    if let serde_json::Value::Object(fields) = fields {
        entry.extend(fields);
    }
    serde_json::Value::Object(entry)
}

fn tool_call_entry(id: String, name: String, input: serde_json::Value) -> serde_json::Value {
    let mut input = match input {
        serde_json::Value::Object(input) => input,
        other => {
            let mut input = serde_json::Map::new();
            input.insert("arguments".to_owned(), other);
            input
        }
    };
    input.insert("name".to_owned(), serde_json::Value::String(name));
    trajectory_entry(
        "tool_call",
        serde_json::json!({"id": id, "input": serde_json::Value::Object(input)}),
    )
}

async fn record_aborted_turn(
    sink: &Option<Arc<dyn RuntimeHistorySink>>,
    agent: &Agent,
    running: &AgentActor<cocli_agent::state::Running>,
    totals: &mut SessionTotals,
    turn: &mut ActiveReply,
    reason: &str,
) -> Result<(), RuntimeError> {
    totals.turn_count = totals.turn_count.saturating_add(1);
    turn.entries.push(trajectory_entry(
        "error",
        serde_json::json!({"text": reason}),
    ));
    record_history(
        sink,
        RuntimeHistoryEvent::TurnCompleted {
            agent_id: agent.id,
            channel_id: turn.channel_id,
            source_message_id: turn.source_message_id,
            session_id: running.state.session_id.clone(),
            launch_id: running.state.launch_id.clone(),
            turn_number: totals.turn_count,
            started_at: turn.started_at,
            ended_at: chrono::Utc::now(),
            input_tokens: 0,
            output_tokens: 0,
            cost_usd: 0.0,
            context_window: totals.context_window,
            entries: serde_json::Value::Array(turn.entries.clone()),
        },
    )
    .await
}

async fn emit_live_event(sink: &Option<Arc<dyn LiveEventSink>>, event: LiveEvent) {
    if let Some(sink) = sink {
        sink.emit(event).await;
    }
}

async fn run_local_session(
    mut running: AgentActor<cocli_agent::state::Running>,
    mut commands: mpsc::Receiver<LocalSessionCommand>,
    context: LocalSessionContext,
) -> Result<(), RuntimeError> {
    let LocalSessionContext {
        status,
        started_at,
        registry,
        config,
        agent,
        metrics,
        recovery,
        history_sink,
        knowledge_provider,
        bridge_token_provider,
        live_event_sink,
    } = context;
    let turn_timeout = config.turn_timeout;
    let mut active: Option<ActiveReply> = None;
    let mut event_rx_live = true;
    let mut quota_recovery: Option<QuotaRecovery> = None;
    let mut totals = SessionTotals::default();
    macro_rules! persist_history {
        ($operation:expr) => {
            match $operation.await {
                Ok(value) => value,
                Err(error) => {
                    if let Some(turn) = active.as_mut() {
                        if let Some(reply) = turn.reply.take() {
                            let _ = reply.send(Err(RuntimeError::Delivery(error.to_string())));
                        }
                    }
                    if let Err(stop_error) = running.stop(true).await {
                        tracing::error!(
                            agent_id = %agent.id,
                            %stop_error,
                            "failed to reap runtime after durable history failure"
                        );
                    }
                    return Err(error);
                }
            }
        };
    }
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
                    let channel_id = delivery.message.channel_id;
                    let source_message_id =
                        Uuid::parse_str(&delivery.message.message_id).ok();
                    let input_text = delivery.message.content.clone();
                    match running.deliver(delivery).await {
                        Ok(()) => {
                            if running.state.respawn_ctx.is_some() {
                                event_rx_live = true;
                            }
                            metrics.inc_local_turn_started();
                            metrics.inc_local_active_turns();
                            active = Some(ActiveReply {
                                reply: Some(reply),
                                text: String::new(),
                                last_error: None,
                                deadline: Some(Instant::now() + turn_timeout),
                                started_at: chrono::Utc::now(),
                                channel_id,
                                source_message_id,
                                entries: vec![trajectory_entry(
                                    "input",
                                    serde_json::json!({"text": input_text}),
                                )],
                            });
                            status.write().await.active_turn = true;
                            emit_live_event(
                                &live_event_sink,
                                LiveEvent::new(
                                    "turn_started",
                                    Some(channel_id),
                                    Some(agent.id),
                                    source_message_id,
                                    serde_json::json!({
                                        "runtime": agent.runtime.clone(),
                                        "model": agent.model.clone(),
                                    }),
                                ),
                            )
                            .await;
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
                        persist_history!(record_session_end(
                            &history_sink,
                            &agent,
                            &running,
                            &totals,
                            "context_reset",
                        ));
                        running.state.session_id.clone_from(session_id);
                        running.state.launch_id = Uuid::new_v4().to_string();
                        running.state.turn_count = 0;
                        totals = SessionTotals::default();
                        persist_history!(record_session_start(&history_sink, &agent, &running));
                        status.write().await.session_id.clone_from(session_id);
                    }
                    let _ = reply.send(result);
                }
                Some(LocalSessionCommand::Stop { reply }) => {
                    let had_active_turn = active.is_some();
                    if let Some(turn) = active.as_mut() {
                        persist_history!(record_aborted_turn(
                            &history_sink,
                            &agent,
                            &running,
                            &mut totals,
                            turn,
                            "agent runtime stopped during active turn",
                        ));
                        if let Some(reply) = turn.reply.take() {
                            let _ = reply.send(Err(RuntimeError::Delivery(
                                "agent runtime stopped during active turn".to_owned(),
                            )));
                        }
                        metrics.inc_local_turn_cancelled();
                    }
                    persist_history!(record_session_end(
                        &history_sink,
                        &agent,
                        &running,
                        &totals,
                        "manual_stop",
                    ));
                    running.stop(true).await.map_err(RuntimeError::Delivery)?;
                    if had_active_turn {
                        active.take();
                        metrics.dec_local_active_turns();
                    }
                    let mut state = status.write().await;
                    state.running = false;
                    state.active_turn = false;
                    let _ = reply.send(Ok(()));
                    break;
                }
                None => {
                    let had_active_turn = active.is_some();
                    if let Some(turn) = active.as_mut() {
                        persist_history!(record_aborted_turn(
                            &history_sink,
                            &agent,
                            &running,
                            &mut totals,
                            turn,
                            "runtime command channel closed during active turn",
                        ));
                    }
                    persist_history!(record_session_end(
                        &history_sink,
                        &agent,
                        &running,
                        &totals,
                        "shutdown",
                    ));
                    running.stop(true).await.map_err(RuntimeError::Delivery)?;
                    if had_active_turn {
                        active.take();
                        metrics.dec_local_active_turns();
                    }
                    let mut state = status.write().await;
                    state.running = false;
                    state.active_turn = false;
                    break;
                }
            },
            event = running.state.event_rx.recv(), if event_rx_live => match event {
                Some(DriverEvent::SessionStarted { session_id }) => {
                    running.state.session_id.clone_from(&session_id);
                    status.write().await.session_id = session_id;
                }
                Some(DriverEvent::ThinkingDelta { text }) => {
                    if let Some(turn) = active.as_mut() {
                        let live_text = text.clone();
                        turn.entries.push(trajectory_entry(
                            "thinking",
                            serde_json::json!({"text": text}),
                        ));
                        emit_live_event(
                            &live_event_sink,
                            LiveEvent::new(
                                "thinking_delta",
                                Some(turn.channel_id),
                                Some(agent.id),
                                turn.source_message_id,
                                serde_json::json!({ "text": live_text }),
                            ),
                        )
                        .await;
                    }
                }
                Some(DriverEvent::TextDelta { text }) => {
                    if let Some(turn) = active.as_mut() {
                        turn.text.push_str(&text);
                        let live_text = text.clone();
                        turn.entries.push(trajectory_entry(
                            "text",
                            serde_json::json!({"text": text}),
                        ));
                        emit_live_event(
                            &live_event_sink,
                            LiveEvent::new(
                                "text_delta",
                                Some(turn.channel_id),
                                Some(agent.id),
                                turn.source_message_id,
                                serde_json::json!({ "text": live_text }),
                            ),
                        )
                        .await;
                    }
                }
                Some(DriverEvent::Error { message, .. }) => {
                    if let Some(turn) = active.as_mut() {
                        turn.last_error = Some(message.clone());
                        turn.entries.push(trajectory_entry(
                            "error",
                            serde_json::json!({"text": message.clone()}),
                        ));
                        emit_live_event(
                            &live_event_sink,
                            LiveEvent::new(
                                "turn_error",
                                Some(turn.channel_id),
                                Some(agent.id),
                                turn.source_message_id,
                                serde_json::json!({ "message": message }),
                            ),
                        )
                        .await;
                    }
                }
                Some(DriverEvent::ToolCall { id, name, input }) => {
                    let tool_name = name.clone();
                    if let Some(turn) = active.as_mut() {
                        let live_id = id.clone();
                        let live_name = name.clone();
                        turn.entries.push(tool_call_entry(id, name, input));
                        emit_live_event(
                            &live_event_sink,
                            LiveEvent::new(
                                "tool_started",
                                Some(turn.channel_id),
                                Some(agent.id),
                                turn.source_message_id,
                                serde_json::json!({
                                    "id": live_id,
                                    "name": live_name,
                                }),
                            ),
                        )
                        .await;
                    }
                    persist_history!(record_history(
                        &history_sink,
                        RuntimeHistoryEvent::Activity {
                            agent_id: agent.id,
                            session_id: running.state.session_id.clone(),
                            launch_id: running.state.launch_id.clone(),
                            activity: "working".to_owned(),
                            detail: Some(tool_name.clone()),
                            trajectory: vec![tool_name],
                            created_at: chrono::Utc::now(),
                        },
                    ));
                }
                Some(DriverEvent::ToolResult { id, output, error })
                | Some(DriverEvent::ToolDone {
                    id,
                    result: output,
                    error,
                }) => {
                    if let Some(turn) = active.as_mut() {
                        let live_id = id.clone();
                        let live_error = error.clone();
                        turn.entries.push(trajectory_entry(
                            "tool_result",
                            serde_json::json!({
                                "id": id,
                                "result": output,
                                "error": error,
                            }),
                        ));
                        emit_live_event(
                            &live_event_sink,
                            LiveEvent::new(
                                "tool_finished",
                                Some(turn.channel_id),
                                Some(agent.id),
                                turn.source_message_id,
                                serde_json::json!({
                                    "id": live_id,
                                    "error": live_error,
                                }),
                            ),
                        )
                        .await;
                    }
                    persist_history!(record_history(
                        &history_sink,
                        RuntimeHistoryEvent::Activity {
                            agent_id: agent.id,
                            session_id: running.state.session_id.clone(),
                            launch_id: running.state.launch_id.clone(),
                            activity: "working".to_owned(),
                            detail: Some("tool_result".to_owned()),
                            trajectory: vec!["tool_result".to_owned()],
                            created_at: chrono::Utc::now(),
                        },
                    ));
                }
                Some(DriverEvent::RateLimit {
                    limit_type,
                    status: limit_status,
                    resets_at,
                    overage_status,
                    ..
                }) => {
                    if rate_limit_is_limited(&limit_status, overage_status.as_deref()) {
                        persist_history!(record_history(
                            &history_sink,
                            RuntimeHistoryEvent::Activity {
                                agent_id: agent.id,
                                session_id: running.state.session_id.clone(),
                                launch_id: running.state.launch_id.clone(),
                                activity: "rate_limit".to_owned(),
                                detail: Some(format!(
                                    "type={limit_type} status={limit_status}"
                                )),
                                trajectory: Vec::new(),
                                created_at: chrono::Utc::now(),
                            },
                        ));
                        let provider = running.state.driver.name().trim().to_owned();
                        let reason = terminal_stop_reason(&provider);
                        let (expected_recovery_at_ms, expected_at) =
                            expected_recovery_time(resets_at);
                        {
                            let mut recovery = recovery.lock().await;
                            recovery.register_with_expected_recovery_at(
                                agent.id.to_string(),
                                &provider,
                                &reason,
                                expected_at,
                            );
                            metrics.set_recovery_tracked_agents(recovery.len());
                        }
                        status.write().await.recovery = Some(RuntimeRecoveryStatus {
                            provider,
                            reason,
                            expected_recovery_at_ms,
                        });
                        quota_recovery = Some(QuotaRecovery { expected_at });
                        if let Some(turn) = active.as_mut() {
                            turn.last_error = Some(format!(
                                "rate limited: type={limit_type} status={limit_status}"
                            ));
                            emit_live_event(
                                &live_event_sink,
                                LiveEvent::new(
                                    "rate_limited",
                                    Some(turn.channel_id),
                                    Some(agent.id),
                                    turn.source_message_id,
                                    serde_json::json!({
                                        "limitType": limit_type,
                                        "status": limit_status,
                                    }),
                                ),
                            )
                            .await;
                        }
                    }
                }
                Some(DriverEvent::TurnEnd {
                    status: turn_status,
                    input_tokens,
                    output_tokens,
                    cost_usd,
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
                    if let Some(turn) = active.as_mut() {
                        let ended_at = chrono::Utc::now();
                        let context_window = if context_window_tokens > 0 {
                            context_window_tokens
                        } else {
                            running
                                .state
                                .driver
                                .context_window_tokens()
                                .unwrap_or_default() as u64
                        };
                        totals.turn_count = totals.turn_count.saturating_add(1);
                        totals.input_tokens = totals
                            .input_tokens
                            .saturating_add(input_tokens.min(i64::MAX as u64) as i64);
                        totals.output_tokens = totals
                            .output_tokens
                            .saturating_add(output_tokens.min(i64::MAX as u64) as i64);
                        totals.cost_usd += cost_usd;
                        totals.context_window = totals
                            .context_window
                            .max(context_window.min(i64::MAX as u64) as i64);
                        running.state.turn_count = running.state.turn_count.saturating_add(1);
                        turn.entries.push(trajectory_entry(
                            "status",
                            serde_json::json!({"text": format!("{turn_status:?}")}),
                        ));
                        persist_history!(record_history(
                            &history_sink,
                            RuntimeHistoryEvent::TurnCompleted {
                                agent_id: agent.id,
                                channel_id: turn.channel_id,
                                source_message_id: turn.source_message_id,
                                session_id: running.state.session_id.clone(),
                                launch_id: running.state.launch_id.clone(),
                                turn_number: totals.turn_count,
                                started_at: turn.started_at,
                                ended_at,
                                input_tokens: input_tokens.min(i64::MAX as u64) as i64,
                                output_tokens: output_tokens.min(i64::MAX as u64) as i64,
                                cost_usd,
                                context_window: context_window.min(i64::MAX as u64) as i64,
                                entries: serde_json::Value::Array(turn.entries.clone()),
                            },
                        ));
                        persist_history!(record_history(
                            &history_sink,
                            RuntimeHistoryEvent::Activity {
                                agent_id: agent.id,
                                session_id: running.state.session_id.clone(),
                                launch_id: running.state.launch_id.clone(),
                                activity: "online".to_owned(),
                                detail: Some(format!("turn {}", totals.turn_count)),
                                trajectory: Vec::new(),
                                created_at: ended_at,
                            },
                        ));
                        emit_live_event(
                            &live_event_sink,
                            LiveEvent::new(
                                "turn_finished",
                                Some(turn.channel_id),
                                Some(agent.id),
                                turn.source_message_id,
                                serde_json::json!({
                                    "status": format!("{turn_status:?}"),
                                    "inputTokens": input_tokens,
                                    "outputTokens": output_tokens,
                                    "costUsd": cost_usd,
                                }),
                            ),
                        )
                        .await;
                        let mut turn = active.take().expect("active turn remains after persistence");
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
                    if let Some(quota) = quota_recovery.take() {
                        persist_history!(record_session_end(
                            &history_sink,
                            &agent,
                            &running,
                            &totals,
                            "rate_limit",
                        ));
                        running.stop(true).await.map_err(RuntimeError::Delivery)?;
                        {
                            let mut state = status.write().await;
                            state.running = false;
                            state.recovering = true;
                        }
                        match recover_from_quota(
                            RuntimeRestartContext {
                                registry: &registry,
                                config: &config,
                                agent: &agent,
                                metrics: &metrics,
                                knowledge_provider: knowledge_provider.as_ref(),
                                bridge_token_provider: bridge_token_provider.as_ref(),
                            },
                            &mut commands,
                            &recovery,
                            quota.expected_at,
                        ).await {
                            Some(restarted) => {
                                running = restarted;
                                totals = SessionTotals::default();
                                persist_history!(record_session_start(
                                    &history_sink,
                                    &agent,
                                    &running,
                                ));
                                *status.write().await =
                                    runtime_status_for_running(&agent, &running);
                                *started_at.write().await = Instant::now();
                                event_rx_live = true;
                                continue;
                            }
                            None => break,
                        }
                    }
                }
                Some(DriverEvent::Write { data }) => {
                    if let Err(error) = running.write_driver_request(&data).await {
                        if let Some(turn) = active.as_mut() {
                            turn.last_error = Some(error);
                        }
                    }
                }
                Some(DriverEvent::CompactStarted) => {
                    persist_history!(record_history(
                        &history_sink,
                        RuntimeHistoryEvent::Activity {
                            agent_id: agent.id,
                            session_id: running.state.session_id.clone(),
                            launch_id: running.state.launch_id.clone(),
                            activity: "compact_started".to_owned(),
                            detail: None,
                            trajectory: Vec::new(),
                            created_at: chrono::Utc::now(),
                        },
                    ));
                }
                Some(DriverEvent::CompactFinished) => {
                    persist_history!(record_history(
                        &history_sink,
                        RuntimeHistoryEvent::Activity {
                            agent_id: agent.id,
                            session_id: running.state.session_id.clone(),
                            launch_id: running.state.launch_id.clone(),
                            activity: "compact_finished".to_owned(),
                            detail: None,
                            trajectory: Vec::new(),
                            created_at: chrono::Utc::now(),
                        },
                    ));
                }
                Some(_) => {}
                None => {
                    if let Some(quota) = quota_recovery.take() {
                        if let Some(turn) = active.as_mut() {
                            metrics.inc_local_turn_failed();
                            metrics.dec_local_active_turns();
                            let error = turn.last_error.clone().unwrap_or_else(|| {
                                "runtime exited while rate limited".to_owned()
                            });
                            persist_history!(record_aborted_turn(
                                &history_sink,
                                &agent,
                                &running,
                                &mut totals,
                                turn,
                                &error,
                            ));
                            let mut turn =
                                active.take().expect("active turn remains after persistence");
                            if let Some(reply) = turn.reply.take() {
                                let _ = reply.send(Err(RuntimeError::Delivery(error)));
                            }
                        }
                        persist_history!(record_session_end(
                            &history_sink,
                            &agent,
                            &running,
                            &totals,
                            "rate_limit",
                        ));
                        {
                            let mut state = status.write().await;
                            state.running = false;
                            state.recovering = true;
                            state.active_turn = false;
                        }
                        match recover_from_quota(
                            RuntimeRestartContext {
                                registry: &registry,
                                config: &config,
                                agent: &agent,
                                metrics: &metrics,
                                knowledge_provider: knowledge_provider.as_ref(),
                                bridge_token_provider: bridge_token_provider.as_ref(),
                            },
                            &mut commands,
                            &recovery,
                            quota.expected_at,
                        ).await {
                            Some(restarted) => {
                                running = restarted;
                                totals = SessionTotals::default();
                                persist_history!(record_session_start(
                                    &history_sink,
                                    &agent,
                                    &running,
                                ));
                                *status.write().await =
                                    runtime_status_for_running(&agent, &running);
                                *started_at.write().await = Instant::now();
                                event_rx_live = true;
                                continue;
                            }
                            None => break,
                        }
                    }
                    if running.state.respawn_ctx.is_some() {
                        event_rx_live = false;
                        continue;
                    }
                    if let Some(turn) = active.as_mut() {
                        metrics.inc_local_turn_failed();
                        metrics.dec_local_active_turns();
                        let error = turn
                            .last_error
                            .clone()
                            .unwrap_or_else(|| "runtime exited before completing the turn".to_owned());
                        persist_history!(record_aborted_turn(
                            &history_sink,
                            &agent,
                            &running,
                            &mut totals,
                            turn,
                            &error,
                        ));
                        let mut turn =
                            active.take().expect("active turn remains after persistence");
                        if let Some(reply) = turn.reply.take() {
                            let _ = reply.send(Err(RuntimeError::Delivery(error)));
                        }
                    }
                    {
                        let mut state = status.write().await;
                        state.running = false;
                        state.recovering = true;
                        state.active_turn = false;
                    }
                    persist_history!(record_session_end(
                        &history_sink,
                        &agent,
                        &running,
                        &totals,
                        "error",
                    ));
                    match restart_with_watchdog(
                        RuntimeRestartContext {
                            registry: &registry,
                            config: &config,
                            agent: &agent,
                            metrics: &metrics,
                            knowledge_provider: knowledge_provider.as_ref(),
                            bridge_token_provider: bridge_token_provider.as_ref(),
                        },
                        &mut commands,
                    ).await {
                        Some(restarted) => {
                            running = restarted;
                            totals = SessionTotals::default();
                            persist_history!(record_session_start(
                                &history_sink,
                                &agent,
                                &running,
                            ));
                            *status.write().await = runtime_status_for_running(&agent, &running);
                            *started_at.write().await = Instant::now();
                            event_rx_live = true;
                            continue;
                        }
                        None => break,
                    }
                }
            },
            _ = tokio::time::sleep_until(deadline), if deadline_live => {
                metrics.inc_local_turn_timed_out();
                if let Some(turn) = active.as_mut() {
                    turn.deadline = None;
                }
                let cancel_error = running.turn_cancel().await.err();
                if let Some(error) = &cancel_error {
                    tracing::warn!(
                        agent_id = %running.id,
                        %error,
                        "failed to request cancellation for timed-out turn"
                    );
                }
                let reason = cancel_error
                    .as_deref()
                    .map_or("runtime turn timed out", |_| {
                        "runtime turn timed out and cancellation failed"
                    });
                if let Some(turn) = active.as_mut() {
                    persist_history!(record_aborted_turn(
                        &history_sink,
                        &agent,
                        &running,
                        &mut totals,
                        turn,
                        reason,
                    ));
                }
                persist_history!(record_session_end(
                    &history_sink,
                    &agent,
                    &running,
                    &totals,
                    "timeout",
                ));
                running.stop(true).await.map_err(RuntimeError::Delivery)?;
                if let Some(mut turn) = active.take() {
                    if let Some(reply) = turn.reply.take() {
                        let _ = reply.send(Err(RuntimeError::Delivery(reason.to_owned())));
                    }
                    metrics.dec_local_active_turns();
                }
                let mut state = status.write().await;
                state.running = false;
                state.active_turn = false;
                break;
            }
        }
    }
    Ok(())
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
        recovering: false,
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
        recovery: None,
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

    fn encode_test_line(text: &str) -> String {
        text.replace('\n', "\\n")
    }

    struct FakeDriver;

    struct FailingHistorySink;

    struct BlockingSessionEndSink {
        release: Arc<tokio::sync::Notify>,
    }

    struct FailingTurnHistorySink;

    #[derive(Default)]
    struct RecordingLiveEventSink {
        events: tokio::sync::Mutex<Vec<LiveEvent>>,
    }

    #[async_trait]
    impl LiveEventSink for RecordingLiveEventSink {
        async fn emit(&self, event: LiveEvent) {
            self.events.lock().await.push(event);
        }
    }

    #[async_trait]
    impl RuntimeHistorySink for FailingHistorySink {
        async fn record(&self, _event: RuntimeHistoryEvent) -> Result<(), cocli_store::StoreError> {
            Err(cocli_store::StoreError::InvalidValue {
                kind: "history",
                value: "simulated failure".to_owned(),
            })
        }
    }

    #[async_trait]
    impl RuntimeHistorySink for BlockingSessionEndSink {
        async fn record(&self, event: RuntimeHistoryEvent) -> Result<(), cocli_store::StoreError> {
            if matches!(event, RuntimeHistoryEvent::SessionEnded { .. }) {
                self.release.notified().await;
            }
            Ok(())
        }
    }

    #[async_trait]
    impl RuntimeHistorySink for FailingTurnHistorySink {
        async fn record(&self, event: RuntimeHistoryEvent) -> Result<(), cocli_store::StoreError> {
            if matches!(event, RuntimeHistoryEvent::SessionStarted { .. }) {
                return Ok(());
            }
            Err(cocli_store::StoreError::InvalidValue {
                kind: "history",
                value: "simulated turn failure".to_owned(),
            })
        }
    }

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
            Some(encode_test_line(text))
        }

        fn supports_turn_cancel(&self) -> bool {
            true
        }

        fn skill_search_paths(&self, _workspace: &Path) -> Vec<PathBuf> {
            Vec::new()
        }
    }

    struct PromptCaptureDriver {
        prompts: Arc<StdMutex<Vec<String>>>,
    }

    #[async_trait]
    impl Driver for PromptCaptureDriver {
        fn name(&self) -> &str {
            "prompt-capture"
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
            system_prompt: &str,
        ) -> Result<(), DriverError> {
            self.prompts
                .lock()
                .expect("prompt capture mutex")
                .push(system_prompt.to_owned());
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
            FakeDriver.parse_event(line)
        }

        fn encode_stdin_message(
            &self,
            text: &str,
            _session_id: Option<&str>,
            _mode: MessageMode,
        ) -> Option<String> {
            Some(encode_test_line(text))
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
            Some(encode_test_line(text))
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
            Some(encode_test_line(text))
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

    struct TurnExitDriver {
        spawn_count: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl Driver for TurnExitDriver {
        fn name(&self) -> &str {
            "turn-exit"
        }

        fn mcp_tool_prefix(&self) -> &str {
            ""
        }

        fn busy_delivery_mode(&self) -> BusyDeliveryMode {
            BusyDeliveryMode::None
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

        fn spawn(&self, cfg: &SpawnConfig) -> Result<tokio::process::Child, DriverError> {
            self.spawn_count.fetch_add(1, Ordering::Relaxed);
            Command::new("/bin/sh")
                .arg("-c")
                .arg("printf 'session\\ntext:%s\\nturn\\n' \"$1\"")
                .arg("turn-exit")
                .arg(encode_test_line(cfg.initial_prompt))
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true)
                .spawn()
                .map_err(DriverError::Io)
        }

        fn parse_event(&self, line: &str) -> Vec<DriverEvent> {
            if line == "session" {
                vec![DriverEvent::SessionStarted {
                    session_id: "session-turn-exit".to_owned(),
                }]
            } else if let Some(text) = line.strip_prefix("text:") {
                vec![DriverEvent::TextDelta {
                    text: text.to_owned(),
                }]
            } else if line == "turn" {
                vec![DriverEvent::TurnEnd {
                    status: TurnStatus::Completed,
                    input_tokens: 10,
                    output_tokens: 2,
                    cost_usd: 0.0,
                    cache_creation_tokens: 0,
                    cache_read_tokens: 0,
                    context_window_tokens: 1_000,
                }]
            } else {
                Vec::new()
            }
        }

        fn encode_stdin_message(
            &self,
            _text: &str,
            _session_id: Option<&str>,
            _mode: MessageMode,
        ) -> Option<String> {
            None
        }

        fn supports_turn_cancel(&self) -> bool {
            true
        }

        fn is_turn_exit(&self) -> bool {
            true
        }

        fn skill_search_paths(&self, _workspace: &Path) -> Vec<PathBuf> {
            Vec::new()
        }
    }

    struct CrashOnceDriver {
        spawn_count: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl Driver for CrashOnceDriver {
        fn name(&self) -> &str {
            "crash-once"
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
            let spawn = self.spawn_count.fetch_add(1, Ordering::Relaxed) + 1;
            let script = if spawn == 1 {
                "printf 'session\\n'; n=0; while IFS= read -r line; do n=$((n+1)); if [ \"$n\" -eq 1 ]; then printf 'text:%s\\nturn\\n' \"$line\"; else exit 1; fi; done"
            } else {
                "printf 'session\\n'; while IFS= read -r line; do printf 'text:%s\\nturn\\n' \"$line\"; done"
            };
            Command::new("/bin/sh")
                .arg("-c")
                .arg(script)
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
                    session_id: "session-crash-once".to_owned(),
                }]
            } else if let Some(text) = line.strip_prefix("text:") {
                vec![DriverEvent::TextDelta {
                    text: text.to_owned(),
                }]
            } else if line == "turn" {
                vec![DriverEvent::TurnEnd {
                    status: TurnStatus::Completed,
                    input_tokens: 10,
                    output_tokens: 2,
                    cost_usd: 0.0,
                    cache_creation_tokens: 0,
                    cache_read_tokens: 0,
                    context_window_tokens: 1_000,
                }]
            } else {
                Vec::new()
            }
        }

        fn encode_stdin_message(
            &self,
            text: &str,
            _session_id: Option<&str>,
            _mode: MessageMode,
        ) -> Option<String> {
            Some(encode_test_line(text))
        }

        fn supports_turn_cancel(&self) -> bool {
            true
        }

        fn skill_search_paths(&self, _workspace: &Path) -> Vec<PathBuf> {
            Vec::new()
        }
    }

    struct QuotaOnceDriver {
        spawn_count: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl Driver for QuotaOnceDriver {
        fn name(&self) -> &str {
            "quota-once"
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
            let spawn = self.spawn_count.fetch_add(1, Ordering::Relaxed) + 1;
            let script = if spawn == 1 {
                "printf 'session\\n'; n=0; while IFS= read -r line; do n=$((n+1)); if [ \"$n\" -eq 1 ]; then printf 'text:%s\\nturn\\n' \"$line\"; else printf 'rate\\nturn-failed\\n'; while IFS= read -r _; do :; done; fi; done"
            } else {
                "printf 'session\\n'; while IFS= read -r line; do printf 'text:%s\\nturn\\n' \"$line\"; done"
            };
            Command::new("/bin/sh")
                .arg("-c")
                .arg(script)
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
                    session_id: "session-quota-once".to_owned(),
                }]
            } else if let Some(text) = line.strip_prefix("text:") {
                vec![DriverEvent::TextDelta {
                    text: text.to_owned(),
                }]
            } else if line == "rate" {
                vec![DriverEvent::RateLimit {
                    limit_type: "requests".to_owned(),
                    status: "limited".to_owned(),
                    resets_at: chrono::Utc::now().timestamp(),
                    overage_status: None,
                    overage_resets: None,
                    is_using_overage: false,
                }]
            } else if line == "turn-failed" {
                vec![DriverEvent::TurnEnd {
                    status: TurnStatus::Failed,
                    input_tokens: 10,
                    output_tokens: 0,
                    cost_usd: 0.0,
                    cache_creation_tokens: 0,
                    cache_read_tokens: 0,
                    context_window_tokens: 1_000,
                }]
            } else if line == "turn" {
                vec![DriverEvent::TurnEnd {
                    status: TurnStatus::Completed,
                    input_tokens: 10,
                    output_tokens: 2,
                    cost_usd: 0.0,
                    cache_creation_tokens: 0,
                    cache_read_tokens: 0,
                    context_window_tokens: 1_000,
                }]
            } else {
                Vec::new()
            }
        }

        fn encode_stdin_message(
            &self,
            text: &str,
            _session_id: Option<&str>,
            _mode: MessageMode,
        ) -> Option<String> {
            Some(encode_test_line(text))
        }

        fn supports_turn_cancel(&self) -> bool {
            true
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
            description: None,
            instructions: None,
            runtime: runtime.to_owned(),
            model: Some("test-model".to_owned()),
            status: cocli_store::AgentStatus::Running,
            lifecycle_status: cocli_store::AgentLifecycleStatus::Active,
            created_by_agent_id: None,
            created_by_channel_id: None,
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
                watchdog_backoff: [Duration::from_millis(1); 5],
                recovery_backoff: [Duration::from_millis(1); 5],
            },
        );
        let live_events = Arc::new(RecordingLiveEventSink::default());
        service.set_live_event_sink(live_events.clone());
        let agent = Agent {
            id: Uuid::new_v4(),
            channel_id: Uuid::new_v4(),
            name: "builder".to_owned(),
            description: None,
            instructions: None,
            runtime: "fake".to_owned(),
            model: Some("test-model".to_owned()),
            status: cocli_store::AgentStatus::Running,
            lifecycle_status: cocli_store::AgentLifecycleStatus::Active,
            created_by_agent_id: None,
            created_by_channel_id: None,
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
        let event_kinds = live_events
            .events
            .lock()
            .await
            .iter()
            .map(|event| event.kind.clone())
            .collect::<Vec<_>>();
        assert!(event_kinds.iter().any(|kind| kind == "turn_started"));
        assert!(event_kinds.iter().any(|kind| kind == "text_delta"));
        assert!(event_kinds.iter().any(|kind| kind == "turn_finished"));
    }

    #[tokio::test]
    async fn materializes_sqlite_memory_and_refreshes_live_runtime_mirror() {
        let temp = tempdir().expect("temp directory");
        let _pid_guard = TestPidDirGuard::new(&temp.path().join("pids"));
        let store = cocli_store::Store::in_memory()
            .await
            .expect("store should open");
        let channel = store
            .create_channel("knowledge")
            .await
            .expect("channel should persist");
        let agent = store
            .create_agent(
                channel.id,
                "builder",
                "prompt-capture",
                Some("test-model"),
                cocli_store::AgentStatus::Running,
            )
            .await
            .expect("agent should persist");
        store
            .write_memory_topic(
                cocli_store::MemoryNamespace::Agent(agent.id),
                "project",
                "alpha",
                "Alpha decisions",
                "Keep the local platform SQLite-first.",
                Some("test"),
                None,
            )
            .await
            .expect("agent memory should persist");
        store
            .write_memory_topic(
                cocli_store::MemoryNamespace::Channel(channel.id),
                "feedback",
                "beta",
                "Shared feedback",
                "Keep all agents aligned.",
                Some("test"),
                None,
            )
            .await
            .expect("channel memory should persist");

        let prompts = Arc::new(StdMutex::new(Vec::new()));
        let mut registry = RuntimeRegistry::new();
        registry.register(Arc::new(PromptCaptureDriver {
            prompts: Arc::clone(&prompts),
        }));
        let service = LocalRuntimeService::from_catalog(
            registry,
            RuntimeCatalog::default(),
            LocalRuntimeConfig {
                workspace_root: temp.path().join("workspaces"),
                server_url: "http://127.0.0.1:8090".to_owned(),
                auth_token: String::new(),
                turn_timeout: Duration::from_secs(2),
                watchdog_backoff: [Duration::from_millis(1); 5],
                recovery_backoff: [Duration::from_millis(1); 5],
            },
        );
        service.set_knowledge_provider(Arc::new(cocli_api::SqliteRuntimeKnowledgeProvider::new(
            store.clone(),
        )));

        service.start(&agent).await.expect("runtime should start");

        let workspace = temp.path().join("workspaces").join(agent.id.to_string());
        let agent_topic = workspace.join("memory/project_alpha.md");
        let channel_topic = workspace
            .join("memory/channels")
            .join(channel.id.to_string())
            .join("feedback_beta.md");
        assert!(tokio::fs::read_to_string(&agent_topic)
            .await
            .expect("agent mirror")
            .contains("SQLite-first"));
        assert!(tokio::fs::read_to_string(&channel_topic)
            .await
            .expect("channel mirror")
            .contains("all agents aligned"));

        let prompt = prompts
            .lock()
            .expect("prompt capture mutex")
            .last()
            .cloned()
            .expect("captured system prompt");
        assert!(prompt.contains("Alpha decisions"));
        assert!(prompt.contains("Shared feedback"));
        assert!(prompt.contains("memory/channels/<channel-id>"));
        assert!(prompt.contains(&channel.id.to_string()));

        store
            .write_memory_topic(
                cocli_store::MemoryNamespace::Agent(agent.id),
                "reference",
                "gamma",
                "Gamma reference",
                "Refresh this before the next live turn.",
                Some("test"),
                None,
            )
            .await
            .expect("new memory should persist");
        let message = test_message(&agent, 1, "refresh memory");
        service
            .reply(&agent, &message)
            .await
            .expect("live runtime reply");
        assert!(
            tokio::fs::read_to_string(workspace.join("memory/reference_gamma.md"))
                .await
                .expect("refreshed mirror")
                .contains("next live turn")
        );
        service.stop(agent.id).await.expect("runtime should stop");
    }

    #[tokio::test]
    async fn memory_manifest_removes_only_previously_managed_files() {
        let temp = tempdir().expect("temp directory");
        let agent = test_agent("fake");
        let config = LocalRuntimeConfig {
            workspace_root: temp.path().join("workspaces"),
            server_url: "http://127.0.0.1:8090".to_owned(),
            auth_token: String::new(),
            turn_timeout: Duration::from_secs(2),
            watchdog_backoff: [Duration::from_millis(1); 5],
            recovery_backoff: [Duration::from_millis(1); 5],
        };
        let first = RuntimeKnowledgeSnapshot {
            files: vec![cocli_api::RuntimeKnowledgeFile {
                relative_path: "memory/project_alpha.md".to_owned(),
                content: "managed".to_owned(),
            }],
            agent_index: String::new(),
            channel_index: String::new(),
        };
        materialize_runtime_knowledge(&config, &agent, &first)
            .await
            .expect("first materialization");
        let workspace = config.workspace_root.join(agent.id.to_string());
        tokio::fs::write(workspace.join("memory/manual.md"), "user-owned")
            .await
            .expect("manual file");

        materialize_runtime_knowledge(&config, &agent, &RuntimeKnowledgeSnapshot::default())
            .await
            .expect("second materialization");

        assert!(!workspace.join("memory/project_alpha.md").exists());
        assert_eq!(
            tokio::fs::read_to_string(workspace.join("memory/manual.md"))
                .await
                .expect("manual file should remain"),
            "user-owned"
        );
    }

    #[tokio::test]
    async fn runtime_actor_persists_session_turn_and_activity_history() {
        let temp = tempdir().expect("temp directory");
        let _pid_guard = TestPidDirGuard::new(&temp.path().join("pids"));
        let store = cocli_store::Store::in_memory()
            .await
            .expect("store should open");
        let channel = store
            .create_channel("history")
            .await
            .expect("channel should persist");
        let agent = store
            .create_agent(
                channel.id,
                "builder",
                "fake",
                Some("test-model"),
                cocli_store::AgentStatus::Running,
            )
            .await
            .expect("agent should persist");
        let message = store
            .append_message(
                channel.id,
                None,
                cocli_store::MessageRole::User,
                "record this",
            )
            .await
            .expect("message should persist");

        let mut registry = RuntimeRegistry::new();
        registry.register(Arc::new(FakeDriver));
        let service = LocalRuntimeService::from_catalog(
            registry,
            RuntimeCatalog::default(),
            LocalRuntimeConfig {
                workspace_root: temp.path().join("workspaces"),
                server_url: "http://127.0.0.1:8090".to_owned(),
                auth_token: String::new(),
                turn_timeout: Duration::from_secs(2),
                watchdog_backoff: [Duration::from_millis(1); 5],
                recovery_backoff: [Duration::from_millis(1); 5],
            },
        );
        service.set_history_sink(Arc::new(cocli_api::SqliteRuntimeHistorySink::new(
            store.clone(),
        )));

        let reply = service
            .reply(&agent, &message)
            .await
            .expect("runtime reply");
        assert!(reply.contains("record this"));
        service.stop(agent.id).await.expect("runtime should stop");

        let sessions = store
            .list_agent_sessions(agent.id, 20, Some("chat"))
            .await
            .expect("sessions should list");
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].end_reason.as_deref(), Some("manual_stop"));
        assert_eq!(sessions[0].turn_count, 1);
        assert_eq!(sessions[0].input_tokens, 1);
        assert_eq!(sessions[0].output_tokens, 1);

        let turns = store
            .list_agent_turns(agent.id, Some("session-1"), 50, 0)
            .await
            .expect("turns should list");
        assert_eq!(turns.len(), 1);
        assert_eq!(
            turns[0].message_ref.as_ref().map(|link| link.message_id),
            Some(message.id)
        );
        assert_eq!(turns[0].entries[0]["kind"], "input");
        assert!(turns[0]
            .entries
            .as_array()
            .expect("entries should be an array")
            .iter()
            .any(|entry| entry["kind"] == "text"));

        let activity = store
            .list_agent_activity(agent.id, 50, 0)
            .await
            .expect("activity should list");
        assert!(activity.iter().any(|entry| entry.activity == "online"));
    }

    #[tokio::test]
    async fn runtime_start_fails_when_session_history_is_not_durable() {
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
                watchdog_backoff: [Duration::from_millis(1); 5],
                recovery_backoff: [Duration::from_millis(1); 5],
            },
        );
        service.set_history_sink(Arc::new(FailingHistorySink));

        let error = service
            .start(&test_agent("fake"))
            .await
            .expect_err("history failure must abort runtime start");

        assert!(matches!(
            error,
            RuntimeError::Delivery(message)
                if message.contains("persist local runtime history")
        ));
    }

    #[tokio::test]
    async fn stop_timeout_retains_session_ownership_until_shutdown_finishes() {
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
                watchdog_backoff: [Duration::from_millis(1); 5],
                recovery_backoff: [Duration::from_millis(1); 5],
            },
        );
        let release = Arc::new(tokio::sync::Notify::new());
        service.set_history_sink(Arc::new(BlockingSessionEndSink {
            release: Arc::clone(&release),
        }));
        let agent = test_agent("fake");
        service.start(&agent).await.expect("runtime starts");

        let error = service
            .stop(agent.id)
            .await
            .expect_err("blocked durable session end should time out");
        assert!(error.to_string().contains("timed out"));
        assert!(
            service
                .status(&agent)
                .await
                .expect("status while stop is pending")
                .running,
            "timed-out stop must retain the live session handle"
        );

        release.notify_waiters();
        for _ in 0..100 {
            if !service
                .status(&agent)
                .await
                .expect("status after release")
                .running
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(
            !service
                .status(&agent)
                .await
                .expect("runtime should finish stopping")
                .running
        );
        service
            .stop(agent.id)
            .await
            .expect("repeated stop after completion is idempotent");
    }

    #[tokio::test]
    async fn stopping_an_active_turn_clears_the_active_turn_metric() {
        let temp = tempdir().expect("temp directory");
        let _pid_guard = TestPidDirGuard::new(&temp.path().join("pids"));
        let mut registry = RuntimeRegistry::new();
        registry.register(Arc::new(ControllableDriver {
            spawn_count: Arc::new(AtomicUsize::new(0)),
            fork_count: Arc::new(AtomicUsize::new(0)),
            steers: Arc::new(StdMutex::new(Vec::new())),
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
                watchdog_backoff: [Duration::from_millis(1); 5],
                recovery_backoff: [Duration::from_millis(1); 5],
            },
        ));
        let agent = test_agent("controllable");
        let reply_service = Arc::clone(&service);
        let reply_agent = agent.clone();
        let reply = tokio::spawn(async move {
            reply_service
                .reply(&reply_agent, &test_message(&reply_agent, 1, "stop me"))
                .await
        });

        for _ in 0..100 {
            if service
                .status(&agent)
                .await
                .expect("runtime status")
                .active_turn
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert!(
            service
                .status(&agent)
                .await
                .expect("active runtime status")
                .active_turn
        );

        service.stop(agent.id).await.expect("runtime should stop");
        assert!(reply
            .await
            .expect("reply task should join")
            .expect_err("active reply should be aborted")
            .to_string()
            .contains("stopped during active turn"));
        assert_eq!(
            service.metrics().await.gauges["local_agent_active_turns"],
            0.0
        );
    }

    #[tokio::test]
    async fn turn_history_failure_replies_with_error_and_reaps_runtime() {
        let temp = tempdir().expect("temp directory");
        let _pid_guard = TestPidDirGuard::new(&temp.path().join("pids"));
        let mut registry = RuntimeRegistry::new();
        registry.register(Arc::new(FakeDriver));
        let service = LocalRuntimeService::from_catalog(
            registry,
            RuntimeCatalog::default(),
            LocalRuntimeConfig {
                workspace_root: temp.path().join("workspaces"),
                server_url: "http://127.0.0.1:8090".to_owned(),
                auth_token: String::new(),
                turn_timeout: Duration::from_secs(2),
                watchdog_backoff: [Duration::from_millis(1); 5],
                recovery_backoff: [Duration::from_millis(1); 5],
            },
        );
        service.set_history_sink(Arc::new(FailingTurnHistorySink));
        let agent = test_agent("fake");

        let error = service
            .reply(&agent, &test_message(&agent, 1, "persist this"))
            .await
            .expect_err("turn must fail when durable history fails");
        assert!(error.to_string().contains("simulated turn failure"));
        for _ in 0..100 {
            if !service
                .status(&agent)
                .await
                .expect("runtime status")
                .running
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(
            !service
                .status(&agent)
                .await
                .expect("runtime should be reaped")
                .running
        );
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
                watchdog_backoff: [Duration::from_millis(1); 5],
                recovery_backoff: [Duration::from_millis(1); 5],
            },
        );
        let agent = Agent {
            id: Uuid::new_v4(),
            channel_id: Uuid::new_v4(),
            name: "builder".to_owned(),
            description: None,
            instructions: None,
            runtime: "bootstrap-write".to_owned(),
            model: Some("test-model".to_owned()),
            status: cocli_store::AgentStatus::Running,
            lifecycle_status: cocli_store::AgentLifecycleStatus::Active,
            created_by_agent_id: None,
            created_by_channel_id: None,
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
                watchdog_backoff: [Duration::from_millis(1); 5],
                recovery_backoff: [Duration::from_millis(1); 5],
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
                watchdog_backoff: [Duration::from_millis(1); 5],
                recovery_backoff: [Duration::from_millis(1); 5],
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
                watchdog_backoff: [Duration::from_millis(1); 5],
                recovery_backoff: [Duration::from_millis(1); 5],
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

    #[tokio::test]
    async fn turn_exit_runtime_parks_between_turns_without_watchdog_restart() {
        let temp = tempdir().expect("temp directory");
        let _pid_guard = TestPidDirGuard::new(&temp.path().join("pids"));
        let spawn_count = Arc::new(AtomicUsize::new(0));
        let mut registry = RuntimeRegistry::new();
        registry.register(Arc::new(TurnExitDriver {
            spawn_count: Arc::clone(&spawn_count),
        }));
        let service = LocalRuntimeService::from_catalog(
            registry,
            RuntimeCatalog::default(),
            LocalRuntimeConfig {
                workspace_root: temp.path().join("workspaces"),
                server_url: "http://127.0.0.1:8090".to_owned(),
                auth_token: String::new(),
                turn_timeout: Duration::from_secs(2),
                watchdog_backoff: [Duration::from_millis(1); 5],
                recovery_backoff: [Duration::from_millis(1); 5],
            },
        );
        let agent = test_agent("turn-exit");

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
        assert_eq!(spawn_count.load(Ordering::Relaxed), 3);
        let status = service.status(&agent).await.expect("runtime status");
        assert!(status.running);
        assert!(!status.recovering);
        let metrics = service.metrics().await;
        assert_eq!(metrics.counters["local_agent_watchdog_restart_total"], 0);
        assert_eq!(metrics.gauges["local_agent_active_sessions"], 1.0);
    }

    #[tokio::test]
    async fn watchdog_restarts_unexpectedly_exited_persistent_runtime() {
        let temp = tempdir().expect("temp directory");
        let _pid_guard = TestPidDirGuard::new(&temp.path().join("pids"));
        let spawn_count = Arc::new(AtomicUsize::new(0));
        let mut registry = RuntimeRegistry::new();
        registry.register(Arc::new(CrashOnceDriver {
            spawn_count: Arc::clone(&spawn_count),
        }));
        let service = LocalRuntimeService::from_catalog(
            registry,
            RuntimeCatalog::default(),
            LocalRuntimeConfig {
                workspace_root: temp.path().join("workspaces"),
                server_url: "http://127.0.0.1:8090".to_owned(),
                auth_token: String::new(),
                turn_timeout: Duration::from_secs(2),
                watchdog_backoff: [Duration::from_millis(1); 5],
                recovery_backoff: [Duration::from_millis(1); 5],
            },
        );
        let agent = test_agent("crash-once");
        service.start(&agent).await.expect("session starts");

        let first = service
            .reply(&agent, &test_message(&agent, 1, "crash now"))
            .await;
        assert!(matches!(first, Err(RuntimeError::Delivery(_))));

        for _ in 0..100 {
            let status = service.status(&agent).await.expect("runtime status");
            if status.running && !status.recovering && spawn_count.load(Ordering::Relaxed) >= 2 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let status = service.status(&agent).await.expect("runtime status");
        assert!(status.running);
        assert!(!status.recovering);
        assert_eq!(spawn_count.load(Ordering::Relaxed), 2);

        let reply = service
            .reply(&agent, &test_message(&agent, 2, "after restart"))
            .await
            .expect("reply after watchdog restart");
        assert!(reply.contains("after restart"));
        let metrics = service.metrics().await;
        assert_eq!(metrics.counters["local_agent_watchdog_restart_total"], 1);
        assert_eq!(metrics.counters["local_agent_watchdog_failure_total"], 0);
        assert_eq!(metrics.counters["local_agent_session_started_total"], 2);
    }

    #[tokio::test]
    async fn quota_limit_enters_recovery_and_restarts_after_expected_time() {
        let temp = tempdir().expect("temp directory");
        let _pid_guard = TestPidDirGuard::new(&temp.path().join("pids"));
        let spawn_count = Arc::new(AtomicUsize::new(0));
        let mut registry = RuntimeRegistry::new();
        registry.register(Arc::new(QuotaOnceDriver {
            spawn_count: Arc::clone(&spawn_count),
        }));
        let service = LocalRuntimeService::from_catalog(
            registry,
            RuntimeCatalog::default(),
            LocalRuntimeConfig {
                workspace_root: temp.path().join("workspaces"),
                server_url: "http://127.0.0.1:8090".to_owned(),
                auth_token: String::new(),
                turn_timeout: Duration::from_secs(2),
                watchdog_backoff: [Duration::from_millis(1); 5],
                recovery_backoff: [Duration::from_millis(1); 5],
            },
        );
        let agent = test_agent("quota-once");
        service.start(&agent).await.expect("session starts");

        let limited = service
            .reply(&agent, &test_message(&agent, 1, "hit quota"))
            .await;
        assert!(matches!(limited, Err(RuntimeError::Delivery(_))));

        for _ in 0..100 {
            let status = service.status(&agent).await.expect("runtime status");
            if status.running && !status.recovering && spawn_count.load(Ordering::Relaxed) >= 2 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let status = service.status(&agent).await.expect("runtime status");
        assert!(status.running);
        assert!(!status.recovering);
        assert!(status.recovery.is_none());
        assert_eq!(spawn_count.load(Ordering::Relaxed), 2);

        let reply = service
            .reply(&agent, &test_message(&agent, 2, "after quota"))
            .await
            .expect("reply after quota recovery");
        assert!(reply.contains("after quota"));

        let metrics = service.metrics().await;
        assert_eq!(metrics.counters["agent_recovery_probe_scheduled_total"], 1);
        assert_eq!(metrics.counters["agent_recovery_probe_recovered_total"], 1);
        assert_eq!(metrics.counters["agent_recovery_probe_error_total"], 0);
        assert_eq!(metrics.gauges["agent_recovery_tracked_agents"], 0.0);
        assert_eq!(metrics.counters["local_agent_session_started_total"], 2);

        let probe = service
            .probe_recovery(&agent)
            .await
            .expect("recovery probe");
        assert_eq!(probe.result, "no_state");
    }
}
