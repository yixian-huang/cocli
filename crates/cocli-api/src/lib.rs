//! Local HTTP API and runtime-neutral application service.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use cocli_store::{
    Agent, AgentActivity, AgentSession, AgentSessionFinish, AgentStatus, AgentTurn, Channel,
    Delivery, DeliveryStats, MemoryDocument, MemoryDocumentEntry, MemoryMoveResult,
    MemoryNamespace, MemoryScope, MemoryTopic, Message, MessageRole, NewAgentTurn,
    SkillLibraryFile, Store, StoreError, Task, TaskStatus, WikiBacklink, WikiPage, WikiPageSummary,
    WikiRevision,
};
use serde::{Deserialize, Serialize};
use tokio::sync::Notify;
use uuid::Uuid;

mod skill_http;
mod skill_import;

/// Runtime discovery information consumed by the local product surface.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RuntimeInfo {
    /// Stable runtime registry key.
    pub name: String,
    /// Whether the runtime can be started on this machine.
    pub installed: bool,
    /// Discovered executable path.
    pub binary: Option<String>,
    /// Discovered runtime version.
    pub version: Option<String>,
    /// Models offered by the runtime.
    pub models: Vec<String>,
    /// Runtime-neutral capability names.
    pub capabilities: Vec<String>,
    /// Structured human-readable reason for an unavailable runtime.
    pub unavailable_reason: Option<String>,
}

/// Live local runtime session state for one agent.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RuntimeSessionStatus {
    pub agent_id: Uuid,
    pub session_id: String,
    pub runtime: String,
    pub model: Option<String>,
    pub running: bool,
    pub recovering: bool,
    pub active_turn: bool,
    pub supports_turn_cancel: bool,
    pub supports_turn_steer: bool,
    pub supports_thread_fork: bool,
    pub input_tokens: u64,
    pub context_window_tokens: u64,
    pub context_util_pct: f64,
    pub tier: String,
    pub fork_suggested: bool,
    pub session_age_seconds: u64,
    pub recovery: Option<RuntimeRecoveryStatus>,
}

impl RuntimeSessionStatus {
    fn stateless(agent: &Agent, running: bool) -> Self {
        Self {
            agent_id: agent.id,
            session_id: String::new(),
            runtime: agent.runtime.clone(),
            model: agent.model.clone(),
            running,
            recovering: false,
            active_turn: false,
            supports_turn_cancel: false,
            supports_turn_steer: false,
            supports_thread_fork: false,
            input_tokens: 0,
            context_window_tokens: 0,
            context_util_pct: 0.0,
            tier: "healthy".to_owned(),
            fork_suggested: false,
            session_age_seconds: 0,
            recovery: None,
        }
    }
}

/// Quota/rate-limit recovery state for one local runtime session.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RuntimeRecoveryStatus {
    pub provider: String,
    pub reason: String,
    pub expected_recovery_at_ms: i64,
}

/// Result returned by an explicit local recovery probe.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RuntimeRecoveryProbeResult {
    pub result: String,
    pub detail: String,
}

/// Result of a native or restart-backed local thread fork.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RuntimeForkResult {
    pub fork_id: String,
    pub native: bool,
}

/// Process-local runtime metrics exposed by the local server.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct RuntimeMetricsSnapshot {
    pub counters: BTreeMap<String, i64>,
    pub gauges: BTreeMap<String, f64>,
}

/// Runtime skill-loading confidence shown by the local management UI.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeSkillCompatibility {
    Supported,
    Uncertain,
    Unsupported,
    Unknown,
}

/// Skill discovered in one runtime's global or workspace search roots.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSkill {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub user_invocable: bool,
    #[serde(rename = "type")]
    pub skill_type: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install_path: Option<String>,
}

/// One file entry exposed by the installed-skill browser.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSkillFileEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: i64,
}

/// File content returned by the installed-skill browser.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RuntimeSkillFileContent {
    pub content: String,
    pub binary: bool,
}

/// Durable history events emitted by a local runtime service.
#[derive(Clone, Debug, PartialEq)]
pub enum RuntimeHistoryEvent {
    SessionStarted {
        agent_id: Uuid,
        channel_id: Uuid,
        session_id: String,
        launch_id: String,
        started_at: DateTime<Utc>,
    },
    SessionEnded {
        agent_id: Uuid,
        launch_id: String,
        end_reason: String,
        turn_count: i64,
        input_tokens: i64,
        output_tokens: i64,
        cost_usd: f64,
        context_window: i64,
        ended_at: DateTime<Utc>,
    },
    TurnCompleted {
        agent_id: Uuid,
        channel_id: Uuid,
        source_message_id: Option<Uuid>,
        session_id: String,
        launch_id: String,
        turn_number: i64,
        started_at: DateTime<Utc>,
        ended_at: DateTime<Utc>,
        input_tokens: i64,
        output_tokens: i64,
        cost_usd: f64,
        context_window: i64,
        entries: serde_json::Value,
    },
    Activity {
        agent_id: Uuid,
        session_id: String,
        launch_id: String,
        activity: String,
        detail: Option<String>,
        trajectory: Vec<String>,
        created_at: DateTime<Utc>,
    },
}

/// Runtime-neutral sink for durable session, turn, and activity history.
#[async_trait]
pub trait RuntimeHistorySink: Send + Sync {
    async fn record(&self, event: RuntimeHistoryEvent) -> Result<(), StoreError>;
}

/// One SQLite-backed memory document materialized into a runtime workspace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeKnowledgeFile {
    /// Safe path relative to the per-agent workspace.
    pub relative_path: String,
    /// Complete Markdown document body.
    pub content: String,
}

/// Durable knowledge loaded before a runtime starts or handles another turn.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RuntimeKnowledgeSnapshot {
    /// Files managed by cocli inside the runtime workspace.
    pub files: Vec<RuntimeKnowledgeFile>,
    /// Agent-private L1 memory index.
    pub agent_index: String,
    /// Current-channel L2 memory index.
    pub channel_index: String,
}

/// Runtime-neutral source for durable local knowledge.
#[async_trait]
pub trait RuntimeKnowledgeProvider: Send + Sync {
    async fn snapshot(&self, agent: &Agent) -> Result<RuntimeKnowledgeSnapshot, RuntimeError>;
}

/// SQLite knowledge provider installed by the local server.
#[derive(Clone, Debug)]
pub struct SqliteRuntimeKnowledgeProvider {
    store: Store,
}

impl SqliteRuntimeKnowledgeProvider {
    pub fn new(store: Store) -> Self {
        Self { store }
    }
}

#[async_trait]
impl RuntimeKnowledgeProvider for SqliteRuntimeKnowledgeProvider {
    async fn snapshot(&self, agent: &Agent) -> Result<RuntimeKnowledgeSnapshot, RuntimeError> {
        let agent_namespace = MemoryNamespace::Agent(agent.id);
        let channel_namespace = MemoryNamespace::Channel(agent.channel_id);
        let (agent_entries, channel_entries) = tokio::try_join!(
            self.store.list_memory_namespace(agent_namespace),
            self.store.list_memory_namespace(channel_namespace),
        )
        .map_err(|error| {
            RuntimeError::Delivery(format!("failed to load durable runtime memory: {error}"))
        })?;

        let agent_index = memory_index_from_entries(&agent_entries, &agent_namespace.index_path());
        let channel_index =
            memory_index_from_entries(&channel_entries, &channel_namespace.index_path());
        let mut files = Vec::with_capacity(agent_entries.len() + channel_entries.len());
        append_runtime_knowledge_files(
            &mut files,
            agent_entries,
            &agent_namespace.prefix(),
            "memory",
        )?;
        append_runtime_knowledge_files(
            &mut files,
            channel_entries,
            &channel_namespace.prefix(),
            &format!("memory/channels/{}", agent.channel_id),
        )?;
        files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));

        Ok(RuntimeKnowledgeSnapshot {
            files,
            agent_index,
            channel_index,
        })
    }
}

fn memory_index_from_entries(entries: &[MemoryDocumentEntry], index_path: &str) -> String {
    entries
        .iter()
        .find(|entry| entry.path == index_path)
        .map(|entry| entry.body.clone())
        .unwrap_or_default()
}

fn append_runtime_knowledge_files(
    output: &mut Vec<RuntimeKnowledgeFile>,
    entries: Vec<MemoryDocumentEntry>,
    source_prefix: &str,
    target_prefix: &str,
) -> Result<(), RuntimeError> {
    for entry in entries {
        let relative = entry.path.strip_prefix(source_prefix).ok_or_else(|| {
            RuntimeError::Delivery(format!(
                "invalid durable memory path outside namespace: {}",
                entry.path
            ))
        })?;
        if relative.is_empty()
            || relative.starts_with('/')
            || relative
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..")
        {
            return Err(RuntimeError::Delivery(format!(
                "invalid durable memory path: {}",
                entry.path
            )));
        }
        output.push(RuntimeKnowledgeFile {
            relative_path: format!("{target_prefix}/{relative}"),
            content: entry.body,
        });
    }
    Ok(())
}

/// SQLite implementation installed by the local server after it opens the store.
#[derive(Clone, Debug)]
pub struct SqliteRuntimeHistorySink {
    store: Store,
}

impl SqliteRuntimeHistorySink {
    pub fn new(store: Store) -> Self {
        Self { store }
    }
}

#[async_trait]
impl RuntimeHistorySink for SqliteRuntimeHistorySink {
    async fn record(&self, event: RuntimeHistoryEvent) -> Result<(), StoreError> {
        match event {
            RuntimeHistoryEvent::SessionStarted {
                agent_id,
                channel_id,
                session_id,
                launch_id,
                started_at,
            } => {
                self.store
                    .create_agent_session(
                        agent_id,
                        Some(channel_id),
                        &session_id,
                        Some(&launch_id),
                        None,
                        "chat",
                        started_at,
                    )
                    .await?;
            }
            RuntimeHistoryEvent::SessionEnded {
                agent_id,
                launch_id,
                end_reason,
                turn_count,
                input_tokens,
                output_tokens,
                cost_usd,
                context_window,
                ended_at,
            } => {
                self.store
                    .finish_agent_session(
                        agent_id,
                        &launch_id,
                        &AgentSessionFinish {
                            end_reason,
                            turn_count,
                            input_tokens,
                            output_tokens,
                            cost_usd,
                            context_window,
                            task_summary: None,
                            files_changed: None,
                            task_success: None,
                            ended_at,
                        },
                    )
                    .await?;
            }
            RuntimeHistoryEvent::TurnCompleted {
                agent_id,
                channel_id,
                source_message_id,
                session_id,
                launch_id,
                turn_number,
                started_at,
                ended_at,
                input_tokens,
                output_tokens,
                cost_usd,
                context_window,
                entries,
            } => {
                self.store
                    .upsert_agent_turn(&NewAgentTurn {
                        agent_id,
                        session_id,
                        launch_id: Some(launch_id),
                        turn_number,
                        started_at,
                        ended_at: Some(ended_at),
                        input_tokens,
                        output_tokens,
                        cost_usd,
                        context_window,
                        entries,
                        session_type: "chat".to_owned(),
                        channel_id: Some(channel_id),
                        source_message_id,
                    })
                    .await?;
            }
            RuntimeHistoryEvent::Activity {
                agent_id,
                session_id,
                launch_id,
                activity,
                detail,
                trajectory,
                created_at,
            } => {
                let session_row_id = self
                    .store
                    .current_agent_session(agent_id)
                    .await?
                    .filter(|session| session.launch_id.as_deref() == Some(launch_id.as_str()))
                    .map(|session| session.id);
                self.store
                    .insert_agent_activity(
                        agent_id,
                        session_row_id,
                        Some(&session_id),
                        &activity,
                        detail.as_deref(),
                        &trajectory,
                        Some(&launch_id),
                        created_at,
                    )
                    .await?;
            }
        }
        Ok(())
    }
}

/// Runtime failures surfaced to the HTTP application layer.
#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    /// The runtime rejected or failed a message delivery.
    #[error("{0}")]
    Delivery(String),
    /// The requested control is not supported by the selected runtime.
    #[error("{0}")]
    Unsupported(String),
    /// The runtime is already processing another turn.
    #[error("{0}")]
    Busy(String),
    /// No live runtime session exists for the requested agent.
    #[error("{0}")]
    NotFound(String),
}

/// Runtime-neutral boundary used by the local product loop.
#[async_trait]
pub trait RuntimeService: Send + Sync {
    /// Installs the durable history sink after the local store is available.
    fn set_history_sink(&self, _sink: Arc<dyn RuntimeHistorySink>) {}

    /// Installs the durable knowledge provider after the local store is available.
    fn set_knowledge_provider(&self, _provider: Arc<dyn RuntimeKnowledgeProvider>) {}

    /// Returns the current runtime catalog.
    async fn list(&self) -> Vec<RuntimeInfo>;

    /// Delivers a user message and returns one completed reply.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError`] when the runtime cannot complete delivery.
    async fn reply(&self, agent: &Agent, message: &Message) -> Result<String, RuntimeError>;

    /// Ensures a live local session exists for an agent.
    async fn start(&self, agent: &Agent) -> Result<RuntimeSessionStatus, RuntimeError> {
        Ok(RuntimeSessionStatus::stateless(agent, true))
    }

    /// Stops and forgets a live local session.
    async fn stop(&self, _agent_id: Uuid) -> Result<(), RuntimeError> {
        Ok(())
    }

    /// Cancels the active turn.
    async fn cancel(&self, _agent_id: Uuid) -> Result<(), RuntimeError> {
        Err(RuntimeError::Unsupported(
            "turn cancellation is not supported".to_owned(),
        ))
    }

    /// Steers the active turn.
    async fn steer(&self, _agent_id: Uuid, _input: &str) -> Result<(), RuntimeError> {
        Err(RuntimeError::Unsupported(
            "turn steering is not supported".to_owned(),
        ))
    }

    /// Forks the current thread, natively or by restarting with fresh context.
    async fn fork(&self, _agent: &Agent, _reason: &str) -> Result<RuntimeForkResult, RuntimeError> {
        Err(RuntimeError::Unsupported(
            "thread fork is not supported".to_owned(),
        ))
    }

    /// Returns the current live-session status.
    async fn status(&self, agent: &Agent) -> Result<RuntimeSessionStatus, RuntimeError> {
        Ok(RuntimeSessionStatus::stateless(agent, false))
    }

    /// Returns process-local runtime metrics.
    async fn metrics(&self) -> RuntimeMetricsSnapshot {
        RuntimeMetricsSnapshot::default()
    }

    /// Returns skill compatibility for one runtime name.
    fn skill_compatibility(&self, _runtime: &str) -> RuntimeSkillCompatibility {
        RuntimeSkillCompatibility::Unknown
    }

    /// Scans runtime-specific global and workspace skill roots.
    async fn list_skills(&self, _agent: &Agent) -> Result<Vec<RuntimeSkill>, RuntimeError> {
        Ok(Vec::new())
    }

    /// Atomically installs or refreshes one library skill in the agent workspace.
    async fn install_skill(
        &self,
        _agent: &Agent,
        _skill_name: &str,
        _files: &[SkillLibraryFile],
    ) -> Result<String, RuntimeError> {
        Err(RuntimeError::Unsupported(
            "runtime skill installation is not supported".to_owned(),
        ))
    }

    /// Removes one managed skill from the agent workspace.
    async fn uninstall_skill(
        &self,
        _agent: &Agent,
        _install_path: &str,
    ) -> Result<(), RuntimeError> {
        Err(RuntimeError::Unsupported(
            "runtime skill installation is not supported".to_owned(),
        ))
    }

    /// Lists files below one managed skill install.
    async fn list_skill_files(
        &self,
        _agent: &Agent,
        _install_path: &str,
    ) -> Result<Vec<RuntimeSkillFileEntry>, RuntimeError> {
        Err(RuntimeError::Unsupported(
            "runtime skill browsing is not supported".to_owned(),
        ))
    }

    /// Reads one file below a managed skill install.
    async fn read_skill_file(
        &self,
        _agent: &Agent,
        _install_path: &str,
        _relative_path: &str,
    ) -> Result<RuntimeSkillFileContent, RuntimeError> {
        Err(RuntimeError::Unsupported(
            "runtime skill browsing is not supported".to_owned(),
        ))
    }

    /// Returns or advances quota-recovery state for one agent.
    async fn probe_recovery(
        &self,
        _agent: &Agent,
    ) -> Result<RuntimeRecoveryProbeResult, RuntimeError> {
        Err(RuntimeError::Unsupported(
            "runtime recovery probes are not supported".to_owned(),
        ))
    }
}

/// Runtime service used when no local runtime registry is available.
#[derive(Debug, Default)]
pub struct NoRuntimeService;

#[async_trait]
impl RuntimeService for NoRuntimeService {
    async fn list(&self) -> Vec<RuntimeInfo> {
        Vec::new()
    }

    async fn reply(&self, _agent: &Agent, _message: &Message) -> Result<String, RuntimeError> {
        Err(RuntimeError::Delivery(
            "no runtime service is configured".to_owned(),
        ))
    }
}

/// Deterministic fake runtime used to validate the local product loop.
#[derive(Debug, Default)]
pub struct EchoRuntimeService;

#[async_trait]
impl RuntimeService for EchoRuntimeService {
    async fn list(&self) -> Vec<RuntimeInfo> {
        vec![RuntimeInfo {
            name: "fake".to_owned(),
            installed: true,
            binary: None,
            version: Some("local-loop".to_owned()),
            models: vec!["test-model".to_owned()],
            capabilities: vec!["reply".to_owned()],
            unavailable_reason: None,
        }]
    }

    async fn reply(&self, _agent: &Agent, message: &Message) -> Result<String, RuntimeError> {
        Ok(format!("echo: {}", message.content))
    }
}

#[derive(Clone)]
pub(crate) struct AppState {
    store: Store,
    runtime: Arc<dyn RuntimeService>,
    deliveries: Arc<DeliveryCoordinator>,
}

/// Tuning for the local SQLite-backed delivery worker.
#[derive(Clone, Copy, Debug)]
pub struct DeliveryConfig {
    pub batch_size: i64,
    pub max_attempts: i64,
    pub poll_interval: Duration,
    pub attempt_timeout: Duration,
    pub base_backoff: Duration,
    pub max_backoff: Duration,
}

impl Default for DeliveryConfig {
    fn default() -> Self {
        Self {
            batch_size: 32,
            max_attempts: 10,
            poll_interval: Duration::from_secs(3),
            attempt_timeout: Duration::from_secs(3 * 60),
            base_backoff: Duration::from_secs(2),
            max_backoff: Duration::from_secs(5 * 60),
        }
    }
}

/// Coordinates durable SQLite deliveries with the runtime service.
pub struct DeliveryCoordinator {
    store: Store,
    runtime: Arc<dyn RuntimeService>,
    config: DeliveryConfig,
    wake: Notify,
    ready: AtomicBool,
    ready_notify: Notify,
}

#[derive(Default)]
struct DeliveryBatchResult {
    replies: Vec<Message>,
}

impl DeliveryCoordinator {
    fn new(store: Store, runtime: Arc<dyn RuntimeService>, config: DeliveryConfig) -> Arc<Self> {
        Arc::new(Self {
            store,
            runtime,
            config,
            wake: Notify::new(),
            ready: AtomicBool::new(false),
            ready_notify: Notify::new(),
        })
    }

    fn spawn(self: &Arc<Self>) {
        let coordinator = Arc::clone(self);
        tokio::spawn(async move {
            if let Err(error) = coordinator.store.release_in_flight_deliveries().await {
                tracing::error!(%error, "failed to release in-flight deliveries at startup");
            }
            coordinator.ready.store(true, Ordering::Release);
            coordinator.ready_notify.notify_waiters();
            coordinator.run().await;
        });
    }

    async fn wait_until_ready(&self) {
        while !self.ready.load(Ordering::Acquire) {
            let notified = self.ready_notify.notified();
            if self.ready.load(Ordering::Acquire) {
                return;
            }
            notified.await;
        }
    }

    async fn run(self: Arc<Self>) {
        let mut interval = tokio::time::interval(self.config.poll_interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        interval.tick().await;
        loop {
            tokio::select! {
                _ = interval.tick() => {}
                _ = self.wake.notified() => {}
            }
            if let Err(error) = self.dispatch_due().await {
                tracing::error!(%error, "durable delivery dispatch failed");
            }
        }
    }

    async fn dispatch_message(&self, message_id: Uuid) -> Result<DeliveryBatchResult, StoreError> {
        self.wait_until_ready().await;
        let queued = self.store.list_message_deliveries(message_id).await?;
        let mut reserved = Vec::with_capacity(queued.len());
        let now = Utc::now();
        for delivery in queued {
            if let Some(delivery) = self
                .store
                .reserve_delivery(delivery.id, self.config.max_attempts, now)
                .await?
            {
                reserved.push(delivery);
            }
        }
        let result = self.process(reserved).await?;
        self.wake.notify_one();
        Ok(result)
    }

    async fn dispatch_due(&self) -> Result<(), StoreError> {
        let reserved = self
            .store
            .reserve_due_deliveries(self.config.batch_size, self.config.max_attempts, Utc::now())
            .await?;
        let _ = self.process(reserved).await?;
        Ok(())
    }

    async fn process(&self, deliveries: Vec<Delivery>) -> Result<DeliveryBatchResult, StoreError> {
        let mut tasks = tokio::task::JoinSet::new();
        let mut task_deliveries = HashMap::new();
        for (index, delivery) in deliveries.into_iter().enumerate() {
            let Some(agent) = self.store.get_agent(delivery.agent_id).await? else {
                continue;
            };
            let Some(message) = self.store.get_message(delivery.message_id).await? else {
                continue;
            };
            let runtime = Arc::clone(&self.runtime);
            let attempt_timeout = self.config.attempt_timeout;
            let task_delivery = delivery.clone();
            let handle = tasks.spawn(async move {
                tokio::time::timeout(attempt_timeout, runtime.reply(&agent, &message))
                    .await
                    .unwrap_or_else(|_| {
                        Err(RuntimeError::Delivery(
                            "durable delivery attempt timed out".to_owned(),
                        ))
                    })
            });
            task_deliveries.insert(handle.id(), (index, task_delivery));
        }

        let mut completed = Vec::new();
        while let Some(result) = tasks.join_next_with_id().await {
            match result {
                Ok((task_id, result)) => {
                    if let Some((index, delivery)) = task_deliveries.remove(&task_id) {
                        completed.push((index, delivery, result));
                    }
                }
                Err(error) => {
                    if let Some((index, delivery)) = task_deliveries.remove(&error.id()) {
                        completed.push((
                            index,
                            delivery,
                            Err(RuntimeError::Delivery(format!(
                                "durable delivery runtime task failed: {error}"
                            ))),
                        ));
                    }
                    tracing::error!(%error, "durable delivery runtime task failed");
                }
            }
        }
        completed.sort_by_key(|(index, _, _)| *index);

        let mut batch = DeliveryBatchResult::default();
        for (_, delivery, result) in completed {
            match result {
                Ok(content) => {
                    batch
                        .replies
                        .push(self.store.complete_delivery(&delivery, &content).await?);
                }
                Err(error) => {
                    let next_attempt_at = Utc::now()
                        + chrono::Duration::from_std(self.backoff(delivery.attempts))
                            .unwrap_or_else(|_| chrono::Duration::minutes(5));
                    let _ = self
                        .store
                        .defer_delivery(
                            delivery.id,
                            &error.to_string(),
                            next_attempt_at,
                            self.config.max_attempts,
                        )
                        .await?;
                }
            }
        }
        Ok(batch)
    }

    fn backoff(&self, attempts: i64) -> Duration {
        let exponent = attempts.saturating_sub(1).min(8) as u32;
        self.config
            .base_backoff
            .saturating_mul(1_u32 << exponent)
            .min(self.config.max_backoff)
    }
}

/// Builds the local HTTP router.
pub fn router(store: Store, runtime: Arc<dyn RuntimeService>) -> Router {
    router_with_delivery_config(store, runtime, DeliveryConfig::default())
}

/// Builds the local HTTP router with explicit durable-delivery tuning.
pub fn router_with_delivery_config(
    store: Store,
    runtime: Arc<dyn RuntimeService>,
    delivery_config: DeliveryConfig,
) -> Router {
    let deliveries = DeliveryCoordinator::new(store.clone(), Arc::clone(&runtime), delivery_config);
    deliveries.spawn();
    Router::new()
        .merge(skill_http::router())
        .route("/healthz", get(health))
        .route("/api/metrics", get(runtime_metrics))
        .route("/api/deliveries/stats", get(delivery_stats))
        .route("/api/runtimes", get(list_runtimes))
        .route("/api/channels", get(list_channels).post(create_channel))
        .route(
            "/api/channels/:channel_id/messages",
            get(list_messages).post(post_message),
        )
        .route(
            "/api/channels/:channel_id/tasks",
            get(list_tasks).post(create_task),
        )
        .route(
            "/api/channels/:channel_id/tasks/:task_number/claim",
            post(claim_task),
        )
        .route(
            "/api/channels/:channel_id/tasks/:task_number/unclaim",
            post(unclaim_task),
        )
        .route(
            "/api/channels/:channel_id/tasks/:task_number/status",
            post(update_task_status),
        )
        .route(
            "/api/channels/:channel_id/tasks/:task_number/dependencies",
            get(get_task_dependencies)
                .post(add_task_dependency)
                .delete(remove_task_dependency),
        )
        .route("/api/agents", get(list_agents).post(create_agent))
        .route("/api/agents/:agent_id/start", post(start_agent))
        .route("/api/agents/:agent_id/stop", post(stop_agent))
        .route("/api/agents/:agent_id/runtime", get(runtime_status))
        .route("/api/agents/:agent_id/sessions", get(list_agent_sessions))
        .route(
            "/api/agents/:agent_id/sessions/current",
            get(current_agent_session),
        )
        .route(
            "/api/agents/:agent_id/sessions/:session_id/turns",
            get(list_session_turns),
        )
        .route("/api/agents/:agent_id/turns", get(list_agent_turns))
        .route("/api/agents/:agent_id/turns/:turn_id", get(get_agent_turn))
        .route("/api/agents/:agent_id/activity", get(list_agent_activity))
        .route(
            "/api/agents/:agent_id/memory/index",
            get(get_agent_memory_index),
        )
        .route(
            "/api/agents/:agent_id/memory/topic",
            get(get_agent_memory_topic),
        )
        .route(
            "/api/channels/:channel_id/memory/index",
            get(get_channel_memory_index),
        )
        .route(
            "/api/channels/:channel_id/memory/topic",
            get(get_channel_memory_topic),
        )
        .route("/api/agents/:agent_id/turn/cancel", post(cancel_turn))
        .route("/api/agents/:agent_id/turn/steer", post(steer_turn))
        .route("/api/agents/:agent_id/thread/fork", post(fork_thread))
        .route("/api/agents/:agent_id/recovery/probe", post(probe_recovery))
        .route("/api/wiki/pages", get(list_wiki_pages))
        .route(
            "/api/wiki/pages/:path",
            get(get_wiki_page).put(upsert_wiki_page),
        )
        .route("/api/wiki/pages/:path/revisions", get(list_wiki_revisions))
        .route("/api/wiki/pages/:path/backlinks", get(list_wiki_backlinks))
        .route("/api/wiki/pages/:path/revert", post(revert_wiki_page))
        .route(
            "/api/bridge/agents/:agent_id/messages",
            post(bridge_send_message),
        )
        .route(
            "/api/bridge/agents/:agent_id/inbox",
            get(bridge_check_messages),
        )
        .route(
            "/api/bridge/agents/:agent_id/history",
            get(bridge_read_history),
        )
        .route(
            "/api/bridge/agents/:agent_id/tasks",
            get(bridge_list_tasks).post(bridge_create_tasks),
        )
        .route(
            "/api/bridge/agents/:agent_id/tasks/claim",
            post(bridge_claim_tasks),
        )
        .route(
            "/api/bridge/agents/:agent_id/tasks/unclaim",
            post(bridge_unclaim_task),
        )
        .route(
            "/api/bridge/agents/:agent_id/tasks/update-status",
            post(bridge_update_task_status),
        )
        .route(
            "/api/bridge/agents/:agent_id/tasks/dependencies",
            get(bridge_get_task_dependencies).post(bridge_add_task_dependency),
        )
        .route(
            "/api/bridge/agents/:agent_id/working",
            get(bridge_get_working_state).post(bridge_set_working_state),
        )
        .route(
            "/api/bridge/agents/:agent_id/working/clear",
            post(bridge_clear_working_state),
        )
        .route(
            "/api/bridge/agents/:agent_id/memory/index",
            get(bridge_get_memory_index),
        )
        .route(
            "/api/bridge/agents/:agent_id/memory/list",
            get(bridge_list_memory_namespace),
        )
        .route(
            "/api/bridge/agents/:agent_id/memory/topic",
            get(bridge_get_memory_topic).post(bridge_write_memory_topic),
        )
        .route(
            "/api/bridge/agents/:agent_id/memory/move",
            post(bridge_move_memory_topic),
        )
        .route(
            "/api/bridge/agents/:agent_id/wiki/pages",
            get(bridge_list_wiki_pages),
        )
        .route(
            "/api/bridge/agents/:agent_id/wiki/pages/:path",
            get(bridge_get_wiki_page).put(bridge_upsert_wiki_page),
        )
        .with_state(AppState {
            store,
            runtime,
            deliveries,
        })
}

#[derive(Serialize)]
struct HealthResponse {
    ok: bool,
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { ok: true })
}

async fn list_runtimes(State(state): State<AppState>) -> Json<Vec<RuntimeInfo>> {
    Json(state.runtime.list().await)
}

async fn runtime_metrics(State(state): State<AppState>) -> Json<RuntimeMetricsSnapshot> {
    Json(state.runtime.metrics().await)
}

async fn delivery_stats(State(state): State<AppState>) -> Result<Json<DeliveryStats>, ApiError> {
    Ok(Json(state.store.delivery_stats(Utc::now()).await?))
}

async fn list_channels(State(state): State<AppState>) -> Result<Json<Vec<Channel>>, ApiError> {
    Ok(Json(state.store.list_channels().await?))
}

#[derive(Deserialize)]
struct CreateChannelRequest {
    name: String,
}

async fn create_channel(
    State(state): State<AppState>,
    Json(request): Json<CreateChannelRequest>,
) -> Result<(StatusCode, Json<Channel>), ApiError> {
    let name = non_empty("channel name", &request.name)?;
    let channel = state.store.create_channel(name).await?;
    Ok((StatusCode::CREATED, Json(channel)))
}

async fn list_agents(State(state): State<AppState>) -> Result<Json<Vec<Agent>>, ApiError> {
    Ok(Json(state.store.list_agents().await?))
}

#[derive(Deserialize)]
struct CreateAgentRequest {
    channel_id: Uuid,
    name: String,
    runtime: String,
    model: Option<String>,
}

async fn create_agent(
    State(state): State<AppState>,
    Json(request): Json<CreateAgentRequest>,
) -> Result<(StatusCode, Json<Agent>), ApiError> {
    let name = non_empty("agent name", &request.name)?;
    let runtime_name = non_empty("runtime", &request.runtime)?;
    let runtimes = state.runtime.list().await;
    let runtime = runtimes
        .iter()
        .find(|runtime| runtime.name == runtime_name)
        .ok_or_else(|| ApiError::bad_request(format!("unknown runtime: {runtime_name}")))?;
    if !runtime.installed {
        return Err(ApiError::bad_request(
            runtime
                .unavailable_reason
                .clone()
                .unwrap_or_else(|| format!("runtime is unavailable: {runtime_name}")),
        ));
    }
    let agent = state
        .store
        .create_agent(
            request.channel_id,
            name,
            runtime_name,
            request.model.as_deref(),
            AgentStatus::Running,
        )
        .await?;
    Ok((StatusCode::CREATED, Json(agent)))
}

async fn start_agent(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
) -> Result<Json<Agent>, ApiError> {
    let agent = require_agent(&state.store, agent_id).await?;
    state.runtime.start(&agent).await?;
    let result = set_agent_status(&state.store, agent_id, AgentStatus::Running).await?;
    state.store.nudge_agent_deliveries(agent_id).await?;
    state.deliveries.wake.notify_one();
    Ok(result)
}

async fn stop_agent(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
) -> Result<Json<Agent>, ApiError> {
    state.runtime.stop(agent_id).await?;
    set_agent_status(&state.store, agent_id, AgentStatus::Stopped).await
}

async fn runtime_status(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
) -> Result<Json<RuntimeSessionStatus>, ApiError> {
    let agent = require_agent(&state.store, agent_id).await?;
    Ok(Json(state.runtime.status(&agent).await?))
}

#[derive(Deserialize)]
struct AgentSessionsQuery {
    #[serde(default = "default_session_limit")]
    limit: i64,
    #[serde(rename = "type")]
    session_type: Option<String>,
}

async fn list_agent_sessions(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Query(query): Query<AgentSessionsQuery>,
) -> Result<Json<Vec<AgentSession>>, ApiError> {
    require_agent(&state.store, agent_id).await?;
    Ok(Json(
        state
            .store
            .list_agent_sessions(agent_id, query.limit, query.session_type.as_deref())
            .await?,
    ))
}

async fn current_agent_session(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
) -> Result<Json<Option<AgentSession>>, ApiError> {
    require_agent(&state.store, agent_id).await?;
    Ok(Json(state.store.current_agent_session(agent_id).await?))
}

#[derive(Deserialize)]
struct AgentTurnsQuery {
    #[serde(default, rename = "sessionId")]
    session_id: Option<String>,
    #[serde(default = "default_turn_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
}

async fn list_agent_turns(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Query(query): Query<AgentTurnsQuery>,
) -> Result<Json<Vec<AgentTurn>>, ApiError> {
    require_agent(&state.store, agent_id).await?;
    Ok(Json(
        state
            .store
            .list_agent_turns(
                agent_id,
                query.session_id.as_deref(),
                query.limit,
                query.offset,
            )
            .await?,
    ))
}

#[derive(Deserialize)]
struct TurnPageQuery {
    #[serde(default = "default_session_turn_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
}

async fn list_session_turns(
    State(state): State<AppState>,
    Path((agent_id, session_id)): Path<(Uuid, String)>,
    Query(query): Query<TurnPageQuery>,
) -> Result<Json<Vec<AgentTurn>>, ApiError> {
    require_agent(&state.store, agent_id).await?;
    Ok(Json(
        state
            .store
            .list_agent_turns(agent_id, Some(&session_id), query.limit, query.offset)
            .await?,
    ))
}

async fn get_agent_turn(
    State(state): State<AppState>,
    Path((agent_id, turn_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<AgentTurn>, ApiError> {
    require_agent(&state.store, agent_id).await?;
    state
        .store
        .get_agent_turn(agent_id, turn_id)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::not_found("turn not found"))
}

#[derive(Deserialize)]
struct AgentActivityQuery {
    #[serde(default = "default_activity_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
}

async fn list_agent_activity(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Query(query): Query<AgentActivityQuery>,
) -> Result<Json<Vec<AgentActivity>>, ApiError> {
    require_agent(&state.store, agent_id).await?;
    Ok(Json(
        state
            .store
            .list_agent_activity(agent_id, query.limit, query.offset)
            .await?,
    ))
}

fn default_session_limit() -> i64 {
    20
}

fn default_turn_limit() -> i64 {
    50
}

fn default_session_turn_limit() -> i64 {
    100
}

fn default_activity_limit() -> i64 {
    50
}

#[derive(Deserialize)]
struct MemoryTopicQuery {
    #[serde(rename = "type")]
    memory_type: String,
    topic: String,
}

async fn get_agent_memory_index(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
) -> Result<Json<MemoryDocument>, ApiError> {
    require_agent(&state.store, agent_id).await?;
    Ok(Json(
        state
            .store
            .get_memory_index(MemoryNamespace::Agent(agent_id))
            .await?,
    ))
}

async fn get_agent_memory_topic(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Query(query): Query<MemoryTopicQuery>,
) -> Result<Json<MemoryDocument>, ApiError> {
    require_agent(&state.store, agent_id).await?;
    let topic = state
        .store
        .get_memory_topic(
            MemoryNamespace::Agent(agent_id),
            &query.memory_type,
            &query.topic,
        )
        .await?
        .ok_or_else(|| ApiError::not_found("memory topic not found"))?;
    Ok(Json(MemoryDocument {
        body: topic.body,
        version: topic.version,
    }))
}

async fn get_channel_memory_index(
    State(state): State<AppState>,
    Path(channel_id): Path<Uuid>,
) -> Result<Json<MemoryDocument>, ApiError> {
    require_channel(&state.store, channel_id).await?;
    Ok(Json(
        state
            .store
            .get_memory_index(MemoryNamespace::Channel(channel_id))
            .await?,
    ))
}

async fn get_channel_memory_topic(
    State(state): State<AppState>,
    Path(channel_id): Path<Uuid>,
    Query(query): Query<MemoryTopicQuery>,
) -> Result<Json<MemoryDocument>, ApiError> {
    require_channel(&state.store, channel_id).await?;
    let topic = state
        .store
        .get_memory_topic(
            MemoryNamespace::Channel(channel_id),
            &query.memory_type,
            &query.topic,
        )
        .await?
        .ok_or_else(|| ApiError::not_found("memory topic not found"))?;
    Ok(Json(MemoryDocument {
        body: topic.body,
        version: topic.version,
    }))
}

#[derive(Serialize)]
struct ControlResponse {
    ok: bool,
}

async fn cancel_turn(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
) -> Result<Json<ControlResponse>, ApiError> {
    require_agent(&state.store, agent_id).await?;
    state.runtime.cancel(agent_id).await?;
    Ok(Json(ControlResponse { ok: true }))
}

#[derive(Deserialize)]
struct SteerTurnRequest {
    input: String,
}

async fn steer_turn(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Json(request): Json<SteerTurnRequest>,
) -> Result<Json<ControlResponse>, ApiError> {
    require_agent(&state.store, agent_id).await?;
    let input = non_empty("steer input", &request.input)?;
    state.runtime.steer(agent_id, input).await?;
    Ok(Json(ControlResponse { ok: true }))
}

#[derive(Deserialize)]
struct ForkThreadRequest {
    #[serde(default)]
    reason: String,
}

async fn fork_thread(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    request: Option<Json<ForkThreadRequest>>,
) -> Result<Json<RuntimeForkResult>, ApiError> {
    let agent = require_agent(&state.store, agent_id).await?;
    let reason = request
        .as_ref()
        .map(|Json(request)| request.reason.trim())
        .unwrap_or_default();
    Ok(Json(state.runtime.fork(&agent, reason).await?))
}

async fn probe_recovery(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
) -> Result<Json<RuntimeRecoveryProbeResult>, ApiError> {
    let agent = require_agent(&state.store, agent_id).await?;
    Ok(Json(state.runtime.probe_recovery(&agent).await?))
}

#[derive(Deserialize)]
struct WikiListQuery {
    q: Option<String>,
    tag: Option<String>,
    #[serde(default = "default_wiki_limit")]
    limit: i64,
}

#[derive(Serialize)]
struct WikiPagesResponse {
    pages: Vec<WikiPageSummary>,
}

async fn list_wiki_pages(
    State(state): State<AppState>,
    Query(query): Query<WikiListQuery>,
) -> Result<Json<WikiPagesResponse>, ApiError> {
    Ok(Json(WikiPagesResponse {
        pages: state
            .store
            .list_wiki_pages(query.q.as_deref(), query.tag.as_deref(), query.limit)
            .await?,
    }))
}

async fn get_wiki_page(
    State(state): State<AppState>,
    Path(path): Path<String>,
) -> Result<Json<WikiPage>, ApiError> {
    let path = validate_wiki_path(&path)?;
    state
        .store
        .get_wiki_page(path)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::not_found("wiki page not found"))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpsertWikiPageRequest {
    title: String,
    content: String,
    #[serde(default)]
    tags: Vec<String>,
    updated_by: Option<String>,
    reason: Option<String>,
    if_version: Option<i64>,
}

async fn upsert_wiki_page(
    State(state): State<AppState>,
    Path(path): Path<String>,
    Json(request): Json<UpsertWikiPageRequest>,
) -> Result<Json<WikiPage>, ApiError> {
    let path = validate_wiki_path(&path)?;
    let title = non_empty("wiki title", &request.title)?;
    Ok(Json(
        state
            .store
            .upsert_wiki_page(
                path,
                title,
                &request.content,
                &request.tags,
                request.updated_by.as_deref(),
                request.reason.as_deref(),
                request.if_version,
            )
            .await?,
    ))
}

#[derive(Serialize)]
struct WikiRevisionsResponse {
    revisions: Vec<WikiRevision>,
}

async fn list_wiki_revisions(
    State(state): State<AppState>,
    Path(path): Path<String>,
) -> Result<Json<WikiRevisionsResponse>, ApiError> {
    let path = validate_wiki_path(&path)?;
    if state.store.get_wiki_page(path).await?.is_none() {
        return Err(ApiError::not_found("wiki page not found"));
    }
    Ok(Json(WikiRevisionsResponse {
        revisions: state.store.list_wiki_revisions(path, 200).await?,
    }))
}

#[derive(Serialize)]
struct WikiBacklinksResponse {
    backlinks: Vec<WikiBacklink>,
}

async fn list_wiki_backlinks(
    State(state): State<AppState>,
    Path(path): Path<String>,
) -> Result<Json<WikiBacklinksResponse>, ApiError> {
    let path = validate_wiki_path(&path)?;
    if state.store.get_wiki_page(path).await?.is_none() {
        return Err(ApiError::not_found("wiki page not found"));
    }
    Ok(Json(WikiBacklinksResponse {
        backlinks: state.store.list_wiki_backlinks(path).await?,
    }))
}

#[derive(Deserialize)]
struct RevertWikiPageRequest {
    version: i64,
    #[serde(default, rename = "updatedBy")]
    updated_by: Option<String>,
}

#[derive(Serialize)]
struct WikiPageResponse {
    page: WikiPage,
}

async fn revert_wiki_page(
    State(state): State<AppState>,
    Path(path): Path<String>,
    Json(request): Json<RevertWikiPageRequest>,
) -> Result<Json<WikiPageResponse>, ApiError> {
    let path = validate_wiki_path(&path)?;
    if request.version <= 0 {
        return Err(ApiError::bad_request("wiki version must be positive"));
    }
    Ok(Json(WikiPageResponse {
        page: state
            .store
            .revert_wiki_page(path, request.version, request.updated_by.as_deref())
            .await?,
    }))
}

fn default_wiki_limit() -> i64 {
    50
}

#[derive(Deserialize)]
struct BridgeSendMessageRequest {
    #[serde(default)]
    target: String,
    content: String,
}

async fn bridge_send_message(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Json(request): Json<BridgeSendMessageRequest>,
) -> Result<(StatusCode, Json<Message>), ApiError> {
    let agent = require_agent(&state.store, agent_id).await?;
    let content = non_empty("message content", &request.content)?;
    let channel_id = resolve_bridge_channel(&state.store, &agent, &request.target).await?;
    let message = state
        .store
        .append_message(channel_id, Some(agent.id), MessageRole::Assistant, content)
        .await?;
    Ok((StatusCode::CREATED, Json(message)))
}

#[derive(Deserialize)]
struct BridgeInboxQuery {
    #[serde(default = "default_bridge_message_limit")]
    limit: i64,
}

#[derive(Serialize)]
struct BridgeInboxResponse {
    messages: Vec<Message>,
}

async fn bridge_check_messages(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Query(query): Query<BridgeInboxQuery>,
) -> Result<Json<BridgeInboxResponse>, ApiError> {
    require_agent(&state.store, agent_id).await?;
    let messages = state
        .store
        .consume_agent_inbox(agent_id, query.limit)
        .await?;
    Ok(Json(BridgeInboxResponse { messages }))
}

#[derive(Deserialize)]
struct BridgeHistoryQuery {
    #[serde(default)]
    channel: String,
    #[serde(default = "default_bridge_message_limit")]
    limit: i64,
    before: Option<i64>,
    after: Option<i64>,
}

#[derive(Serialize)]
struct BridgeHistoryResponse {
    channel: Channel,
    messages: Vec<Message>,
}

async fn bridge_read_history(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Query(query): Query<BridgeHistoryQuery>,
) -> Result<Json<BridgeHistoryResponse>, ApiError> {
    let agent = require_agent(&state.store, agent_id).await?;
    let channel_id = resolve_bridge_channel(&state.store, &agent, &query.channel).await?;
    let channel = state
        .store
        .get_channel(channel_id)
        .await?
        .ok_or_else(|| ApiError::not_found("channel not found"))?;
    let messages = state
        .store
        .list_message_page(channel_id, query.limit, query.before, query.after)
        .await?;
    Ok(Json(BridgeHistoryResponse { channel, messages }))
}

#[derive(Deserialize)]
struct BridgeTaskListQuery {
    #[serde(default)]
    channel: String,
    status: Option<String>,
}

#[derive(Serialize)]
struct BridgeTasksResponse {
    tasks: Vec<Task>,
}

async fn bridge_list_tasks(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Query(query): Query<BridgeTaskListQuery>,
) -> Result<Json<BridgeTasksResponse>, ApiError> {
    let agent = require_agent(&state.store, agent_id).await?;
    let channel_id = resolve_bridge_channel(&state.store, &agent, &query.channel).await?;
    let status = parse_task_status_filter(query.status.as_deref())?;
    Ok(Json(BridgeTasksResponse {
        tasks: state.store.list_tasks(channel_id, status).await?,
    }))
}

#[derive(Deserialize)]
struct BridgeTaskInput {
    title: String,
}

#[derive(Deserialize)]
struct BridgeCreateTasksRequest {
    #[serde(default)]
    channel: String,
    tasks: Vec<BridgeTaskInput>,
}

async fn bridge_create_tasks(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Json(request): Json<BridgeCreateTasksRequest>,
) -> Result<(StatusCode, Json<BridgeTasksResponse>), ApiError> {
    let agent = require_agent(&state.store, agent_id).await?;
    let channel_id = resolve_bridge_channel(&state.store, &agent, &request.channel).await?;
    if request.tasks.is_empty() {
        return Err(ApiError::bad_request("at least one task is required"));
    }
    let mut tasks = Vec::with_capacity(request.tasks.len());
    for input in request.tasks {
        let title = non_empty("task title", &input.title)?;
        tasks.push(
            state
                .store
                .create_task(channel_id, title, None, Some(agent.id))
                .await?,
        );
    }
    Ok((StatusCode::CREATED, Json(BridgeTasksResponse { tasks })))
}

#[derive(Deserialize)]
struct BridgeClaimTasksRequest {
    #[serde(default)]
    channel: String,
    #[serde(default, alias = "taskNumbers")]
    task_numbers: Vec<i64>,
    #[serde(default, alias = "messageIds")]
    message_ids: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BridgeTaskMutationResult {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    task_number: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    created: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    task: Option<Task>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

#[derive(Serialize)]
struct BridgeClaimTasksResponse {
    results: Vec<BridgeTaskMutationResult>,
}

async fn bridge_claim_tasks(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Json(request): Json<BridgeClaimTasksRequest>,
) -> Result<Json<BridgeClaimTasksResponse>, ApiError> {
    let agent = require_agent(&state.store, agent_id).await?;
    let channel_id = resolve_bridge_channel(&state.store, &agent, &request.channel).await?;
    if request.task_numbers.is_empty() && request.message_ids.is_empty() {
        return Err(ApiError::bad_request(
            "task_numbers or message_ids is required",
        ));
    }
    let mut results = Vec::with_capacity(request.task_numbers.len() + request.message_ids.len());
    for task_number in request.task_numbers {
        match state
            .store
            .claim_task(channel_id, task_number, agent.id)
            .await
        {
            Ok(task) => results.push(BridgeTaskMutationResult {
                success: true,
                task_number: Some(task.task_number),
                message_id: None,
                created: None,
                task: Some(task),
                reason: None,
            }),
            Err(error) => results.push(BridgeTaskMutationResult {
                success: false,
                task_number: Some(task_number),
                message_id: None,
                created: None,
                task: None,
                reason: Some(error.to_string()),
            }),
        }
    }
    for message_id in request.message_ids {
        let result = claim_message_as_task(&state.store, &agent, channel_id, &message_id).await;
        match result {
            Ok((task, created)) => results.push(BridgeTaskMutationResult {
                success: true,
                task_number: Some(task.task_number),
                message_id: Some(message_id),
                created: created.then_some(true),
                task: Some(task),
                reason: None,
            }),
            Err(error) => results.push(BridgeTaskMutationResult {
                success: false,
                task_number: None,
                message_id: Some(message_id),
                created: None,
                task: None,
                reason: Some(error.message),
            }),
        }
    }
    Ok(Json(BridgeClaimTasksResponse { results }))
}

async fn claim_message_as_task(
    store: &Store,
    agent: &Agent,
    channel_id: Uuid,
    message_id: &str,
) -> Result<(Task, bool), ApiError> {
    let message_id = Uuid::parse_str(message_id)
        .map_err(|_| ApiError::bad_request("message_ids must contain full UUIDs"))?;
    let message = store
        .get_message(message_id)
        .await?
        .filter(|message| message.channel_id == channel_id)
        .ok_or_else(|| ApiError::not_found("message not found in task channel"))?;
    if let Some(task) = store.get_task_by_message(channel_id, message.id).await? {
        return Ok((
            store
                .claim_task(channel_id, task.task_number, agent.id)
                .await?,
            false,
        ));
    }
    let task = store
        .create_task(
            channel_id,
            &message.content,
            Some(message.id),
            Some(agent.id),
        )
        .await?;
    Ok((
        store
            .claim_task(channel_id, task.task_number, agent.id)
            .await?,
        true,
    ))
}

#[derive(Deserialize)]
struct BridgeTaskNumberRequest {
    #[serde(default)]
    channel: String,
    #[serde(alias = "taskNumber")]
    task_number: i64,
}

async fn bridge_unclaim_task(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Json(request): Json<BridgeTaskNumberRequest>,
) -> Result<Json<Task>, ApiError> {
    let agent = require_agent(&state.store, agent_id).await?;
    let channel_id = resolve_bridge_channel(&state.store, &agent, &request.channel).await?;
    let task = state
        .store
        .get_task(channel_id, request.task_number)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("task #{} not found", request.task_number)))?;
    if task.assignee_id != Some(agent.id) {
        return Err(ApiError::conflict("task is not claimed by this agent"));
    }
    Ok(Json(
        state
            .store
            .unclaim_task(channel_id, request.task_number)
            .await?,
    ))
}

#[derive(Deserialize)]
struct BridgeUpdateTaskStatusRequest {
    #[serde(default)]
    channel: String,
    #[serde(alias = "taskNumber")]
    task_number: i64,
    status: TaskStatus,
    progress: Option<String>,
}

async fn bridge_update_task_status(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Json(request): Json<BridgeUpdateTaskStatusRequest>,
) -> Result<Json<Task>, ApiError> {
    let agent = require_agent(&state.store, agent_id).await?;
    let channel_id = resolve_bridge_channel(&state.store, &agent, &request.channel).await?;
    let task = state
        .store
        .get_task(channel_id, request.task_number)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("task #{} not found", request.task_number)))?;
    if task.assignee_id.is_some() && task.assignee_id != Some(agent.id) {
        return Err(ApiError::conflict("task is claimed by another agent"));
    }
    Ok(Json(
        state
            .store
            .update_task_status(
                channel_id,
                request.task_number,
                request.status,
                request.progress.as_deref(),
            )
            .await?,
    ))
}

#[derive(Deserialize)]
struct BridgeTaskDependenciesQuery {
    #[serde(default)]
    channel: String,
    task_number: i64,
}

async fn bridge_get_task_dependencies(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Query(query): Query<BridgeTaskDependenciesQuery>,
) -> Result<Json<TaskDependenciesResponse>, ApiError> {
    let agent = require_agent(&state.store, agent_id).await?;
    let channel_id = resolve_bridge_channel(&state.store, &agent, &query.channel).await?;
    if state
        .store
        .get_task(channel_id, query.task_number)
        .await?
        .is_none()
    {
        return Err(ApiError::not_found(format!(
            "task #{} not found",
            query.task_number
        )));
    }
    Ok(Json(TaskDependenciesResponse {
        task_number: query.task_number,
        depends_on: state
            .store
            .get_task_dependencies(channel_id, query.task_number)
            .await?,
    }))
}

#[derive(Deserialize)]
struct BridgeAddTaskDependencyRequest {
    #[serde(default)]
    channel: String,
    #[serde(alias = "taskNumber")]
    task_number: i64,
    #[serde(alias = "dependsOn")]
    depends_on: i64,
}

async fn bridge_add_task_dependency(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Json(request): Json<BridgeAddTaskDependencyRequest>,
) -> Result<Json<TaskDependenciesResponse>, ApiError> {
    let agent = require_agent(&state.store, agent_id).await?;
    let channel_id = resolve_bridge_channel(&state.store, &agent, &request.channel).await?;
    state
        .store
        .add_task_dependency(channel_id, request.task_number, request.depends_on)
        .await?;
    Ok(Json(TaskDependenciesResponse {
        task_number: request.task_number,
        depends_on: state
            .store
            .get_task_dependencies(channel_id, request.task_number)
            .await?,
    }))
}

#[derive(Deserialize)]
struct BridgeSetWorkingStateRequest {
    summary: String,
    #[serde(default, alias = "channelName")]
    channel_name: Option<String>,
    #[serde(default, alias = "taskNumber")]
    task_number: Option<i64>,
    #[serde(default, alias = "nextStepHint")]
    next_step_hint: Option<String>,
}

#[derive(Serialize)]
struct BridgeWorkingStateResponse {
    state: Option<cocli_store::WorkingState>,
}

async fn bridge_get_working_state(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
) -> Result<Json<BridgeWorkingStateResponse>, ApiError> {
    require_agent(&state.store, agent_id).await?;
    Ok(Json(BridgeWorkingStateResponse {
        state: state.store.get_working_state(agent_id).await?,
    }))
}

async fn bridge_set_working_state(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Json(request): Json<BridgeSetWorkingStateRequest>,
) -> Result<Json<BridgeWorkingStateResponse>, ApiError> {
    require_agent(&state.store, agent_id).await?;
    let summary = non_empty("working summary", &request.summary)?;
    let working = state
        .store
        .set_working_state(
            agent_id,
            summary,
            request.channel_name.as_deref(),
            request.task_number,
            request.next_step_hint.as_deref(),
        )
        .await?;
    Ok(Json(BridgeWorkingStateResponse {
        state: Some(working),
    }))
}

#[derive(Serialize)]
struct BridgeClearWorkingStateResponse {
    cleared: bool,
}

async fn bridge_clear_working_state(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
) -> Result<Json<BridgeClearWorkingStateResponse>, ApiError> {
    require_agent(&state.store, agent_id).await?;
    Ok(Json(BridgeClearWorkingStateResponse {
        cleared: state.store.clear_working_state(agent_id).await?,
    }))
}

#[derive(Deserialize)]
struct BridgeMemoryScopeQuery {
    scope: MemoryScope,
    #[serde(default)]
    channel_id: Option<Uuid>,
}

#[derive(Deserialize)]
struct BridgeMemoryTopicQuery {
    scope: MemoryScope,
    #[serde(default)]
    channel_id: Option<Uuid>,
    #[serde(rename = "type")]
    memory_type: String,
    topic: String,
}

#[derive(Serialize)]
struct BridgeMemoryNamespaceResponse {
    entries: Vec<MemoryDocumentEntry>,
}

async fn bridge_get_memory_index(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Query(query): Query<BridgeMemoryScopeQuery>,
) -> Result<Json<MemoryDocument>, ApiError> {
    let agent = require_agent(&state.store, agent_id).await?;
    let namespace =
        resolve_bridge_memory_namespace(&state.store, &agent, query.scope, query.channel_id)
            .await?;
    Ok(Json(state.store.get_memory_index(namespace).await?))
}

async fn bridge_list_memory_namespace(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Query(query): Query<BridgeMemoryScopeQuery>,
) -> Result<Json<BridgeMemoryNamespaceResponse>, ApiError> {
    let agent = require_agent(&state.store, agent_id).await?;
    let namespace =
        resolve_bridge_memory_namespace(&state.store, &agent, query.scope, query.channel_id)
            .await?;
    Ok(Json(BridgeMemoryNamespaceResponse {
        entries: state.store.list_memory_namespace(namespace).await?,
    }))
}

async fn bridge_get_memory_topic(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Query(query): Query<BridgeMemoryTopicQuery>,
) -> Result<Json<MemoryTopic>, ApiError> {
    let agent = require_agent(&state.store, agent_id).await?;
    let namespace =
        resolve_bridge_memory_namespace(&state.store, &agent, query.scope, query.channel_id)
            .await?;
    state
        .store
        .get_memory_topic(namespace, &query.memory_type, &query.topic)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::not_found("memory topic not found"))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BridgeWriteMemoryTopicRequest {
    scope: MemoryScope,
    #[serde(default, alias = "channel_id")]
    channel_id: Option<Uuid>,
    #[serde(rename = "type")]
    memory_type: String,
    topic: String,
    #[serde(default)]
    description: String,
    body: String,
    #[serde(default, alias = "if_version")]
    if_version: Option<i64>,
}

async fn bridge_write_memory_topic(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Json(request): Json<BridgeWriteMemoryTopicRequest>,
) -> Result<Json<MemoryTopic>, ApiError> {
    let agent = require_agent(&state.store, agent_id).await?;
    let namespace =
        resolve_bridge_memory_namespace(&state.store, &agent, request.scope, request.channel_id)
            .await?;
    Ok(Json(
        state
            .store
            .write_memory_topic(
                namespace,
                &request.memory_type,
                &request.topic,
                &request.description,
                &request.body,
                Some(&agent.name),
                request.if_version,
            )
            .await?,
    ))
}

#[derive(Deserialize)]
struct BridgeMoveMemoryTopicRequest {
    from_scope: MemoryScope,
    #[serde(default)]
    from_channel_id: Option<Uuid>,
    to_scope: MemoryScope,
    #[serde(default)]
    to_channel_id: Option<Uuid>,
    #[serde(rename = "type")]
    memory_type: String,
    topic: String,
}

async fn bridge_move_memory_topic(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Json(request): Json<BridgeMoveMemoryTopicRequest>,
) -> Result<Json<MemoryMoveResult>, ApiError> {
    let agent = require_agent(&state.store, agent_id).await?;
    let from = resolve_bridge_memory_namespace(
        &state.store,
        &agent,
        request.from_scope,
        request.from_channel_id,
    )
    .await?;
    let to = resolve_bridge_memory_namespace(
        &state.store,
        &agent,
        request.to_scope,
        request.to_channel_id,
    )
    .await?;
    Ok(Json(
        state
            .store
            .move_memory_topic(
                from,
                to,
                &request.memory_type,
                &request.topic,
                Some(&agent.name),
            )
            .await?,
    ))
}

async fn resolve_bridge_memory_namespace(
    store: &Store,
    agent: &Agent,
    scope: MemoryScope,
    channel_id: Option<Uuid>,
) -> Result<MemoryNamespace, ApiError> {
    match scope {
        MemoryScope::Agent => {
            if channel_id.is_some() {
                return Err(ApiError::bad_request(
                    "channel_id is forbidden for agent memory",
                ));
            }
            Ok(MemoryNamespace::Agent(agent.id))
        }
        MemoryScope::Channel => {
            let channel_id = channel_id.ok_or_else(|| {
                ApiError::bad_request("channel_id is required for channel memory")
            })?;
            if channel_id != agent.channel_id {
                return Err(ApiError::forbidden(
                    "agent does not belong to the requested channel",
                ));
            }
            require_channel(store, channel_id).await?;
            Ok(MemoryNamespace::Channel(channel_id))
        }
    }
}

async fn bridge_list_wiki_pages(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Query(query): Query<WikiListQuery>,
) -> Result<Json<WikiPagesResponse>, ApiError> {
    require_agent(&state.store, agent_id).await?;
    Ok(Json(WikiPagesResponse {
        pages: state
            .store
            .list_wiki_pages(query.q.as_deref(), query.tag.as_deref(), query.limit)
            .await?,
    }))
}

async fn bridge_get_wiki_page(
    State(state): State<AppState>,
    Path((agent_id, path)): Path<(Uuid, String)>,
) -> Result<Json<WikiPage>, ApiError> {
    require_agent(&state.store, agent_id).await?;
    let path = validate_wiki_path(&path)?;
    state
        .store
        .get_wiki_page(path)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::not_found("wiki page not found"))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BridgeUpsertWikiPageRequest {
    title: String,
    #[serde(alias = "content_md", alias = "content")]
    content_md: String,
    #[serde(default)]
    tags: Vec<String>,
    reason: Option<String>,
    #[serde(default, alias = "if_version")]
    if_version: Option<i64>,
}

async fn bridge_upsert_wiki_page(
    State(state): State<AppState>,
    Path((agent_id, path)): Path<(Uuid, String)>,
    Json(request): Json<BridgeUpsertWikiPageRequest>,
) -> Result<Json<WikiPage>, ApiError> {
    let agent = require_agent(&state.store, agent_id).await?;
    let path = validate_wiki_path(&path)?;
    let title = non_empty("wiki title", &request.title)?;
    Ok(Json(
        state
            .store
            .upsert_wiki_page(
                path,
                title,
                &request.content_md,
                &request.tags,
                Some(&agent.name),
                request.reason.as_deref(),
                request.if_version,
            )
            .await?,
    ))
}

fn default_bridge_message_limit() -> i64 {
    50
}

async fn resolve_bridge_channel(
    store: &Store,
    agent: &Agent,
    target: &str,
) -> Result<Uuid, ApiError> {
    let target = target.trim();
    if target.is_empty() {
        return Ok(agent.channel_id);
    }
    if let Ok(channel_id) = Uuid::parse_str(target) {
        return store
            .get_channel(channel_id)
            .await?
            .map(|channel| channel.id)
            .ok_or_else(|| ApiError::not_found("channel not found"));
    }
    let name = target.strip_prefix('#').unwrap_or(target);
    store
        .get_channel_by_name(name)
        .await?
        .map(|channel| channel.id)
        .ok_or_else(|| ApiError::not_found("channel not found"))
}

async fn require_agent(store: &Store, agent_id: Uuid) -> Result<Agent, ApiError> {
    store
        .get_agent(agent_id)
        .await?
        .ok_or_else(|| ApiError::not_found("agent not found"))
}

async fn set_agent_status(
    store: &Store,
    agent_id: Uuid,
    status: AgentStatus,
) -> Result<Json<Agent>, ApiError> {
    let agent = store
        .set_agent_status(agent_id, status)
        .await?
        .ok_or_else(|| ApiError::not_found("agent not found"))?;
    Ok(Json(agent))
}

async fn list_messages(
    State(state): State<AppState>,
    Path(channel_id): Path<Uuid>,
) -> Result<Json<Vec<Message>>, ApiError> {
    Ok(Json(state.store.list_messages(channel_id).await?))
}

#[derive(Deserialize)]
struct TaskListQuery {
    status: Option<String>,
}

async fn list_tasks(
    State(state): State<AppState>,
    Path(channel_id): Path<Uuid>,
    Query(query): Query<TaskListQuery>,
) -> Result<Json<Vec<Task>>, ApiError> {
    require_channel(&state.store, channel_id).await?;
    let status = parse_task_status_filter(query.status.as_deref())?;
    Ok(Json(state.store.list_tasks(channel_id, status).await?))
}

#[derive(Deserialize)]
struct CreateTaskRequest {
    title: String,
    #[serde(default, alias = "messageId")]
    message_id: Option<Uuid>,
    #[serde(default, alias = "createdByAgentId")]
    created_by_agent_id: Option<Uuid>,
}

async fn create_task(
    State(state): State<AppState>,
    Path(channel_id): Path<Uuid>,
    Json(request): Json<CreateTaskRequest>,
) -> Result<(StatusCode, Json<Task>), ApiError> {
    require_channel(&state.store, channel_id).await?;
    let title = non_empty("task title", &request.title)?;
    Ok((
        StatusCode::CREATED,
        Json(
            state
                .store
                .create_task(
                    channel_id,
                    title,
                    request.message_id,
                    request.created_by_agent_id,
                )
                .await?,
        ),
    ))
}

#[derive(Deserialize)]
struct ClaimTaskRequest {
    #[serde(alias = "agentId")]
    agent_id: Uuid,
}

async fn claim_task(
    State(state): State<AppState>,
    Path((channel_id, task_number)): Path<(Uuid, i64)>,
    Json(request): Json<ClaimTaskRequest>,
) -> Result<Json<Task>, ApiError> {
    let agent = require_agent(&state.store, request.agent_id).await?;
    if agent.channel_id != channel_id {
        return Err(ApiError::bad_request(
            "task assignee must belong to the task channel",
        ));
    }
    Ok(Json(
        state
            .store
            .claim_task(channel_id, task_number, agent.id)
            .await?,
    ))
}

async fn unclaim_task(
    State(state): State<AppState>,
    Path((channel_id, task_number)): Path<(Uuid, i64)>,
) -> Result<Json<Task>, ApiError> {
    Ok(Json(
        state.store.unclaim_task(channel_id, task_number).await?,
    ))
}

#[derive(Deserialize)]
struct UpdateTaskStatusRequest {
    status: TaskStatus,
    progress: Option<String>,
}

async fn update_task_status(
    State(state): State<AppState>,
    Path((channel_id, task_number)): Path<(Uuid, i64)>,
    Json(request): Json<UpdateTaskStatusRequest>,
) -> Result<Json<Task>, ApiError> {
    Ok(Json(
        state
            .store
            .update_task_status(
                channel_id,
                task_number,
                request.status,
                request.progress.as_deref(),
            )
            .await?,
    ))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TaskDependenciesResponse {
    task_number: i64,
    depends_on: Vec<i64>,
}

async fn get_task_dependencies(
    State(state): State<AppState>,
    Path((channel_id, task_number)): Path<(Uuid, i64)>,
) -> Result<Json<TaskDependenciesResponse>, ApiError> {
    if state
        .store
        .get_task(channel_id, task_number)
        .await?
        .is_none()
    {
        return Err(ApiError::not_found(format!(
            "task #{task_number} not found"
        )));
    }
    Ok(Json(TaskDependenciesResponse {
        task_number,
        depends_on: state
            .store
            .get_task_dependencies(channel_id, task_number)
            .await?,
    }))
}

#[derive(Deserialize)]
struct TaskDependencyRequest {
    #[serde(alias = "dependsOn")]
    depends_on: i64,
}

async fn add_task_dependency(
    State(state): State<AppState>,
    Path((channel_id, task_number)): Path<(Uuid, i64)>,
    Json(request): Json<TaskDependencyRequest>,
) -> Result<(StatusCode, Json<TaskDependenciesResponse>), ApiError> {
    state
        .store
        .add_task_dependency(channel_id, task_number, request.depends_on)
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(TaskDependenciesResponse {
            task_number,
            depends_on: state
                .store
                .get_task_dependencies(channel_id, task_number)
                .await?,
        }),
    ))
}

async fn remove_task_dependency(
    State(state): State<AppState>,
    Path((channel_id, task_number)): Path<(Uuid, i64)>,
    Json(request): Json<TaskDependencyRequest>,
) -> Result<Json<TaskDependenciesResponse>, ApiError> {
    state
        .store
        .remove_task_dependency(channel_id, task_number, request.depends_on)
        .await?;
    Ok(Json(TaskDependenciesResponse {
        task_number,
        depends_on: state
            .store
            .get_task_dependencies(channel_id, task_number)
            .await?,
    }))
}

#[derive(Deserialize)]
struct PostMessageRequest {
    content: String,
}

#[derive(Serialize)]
struct PostMessageResponse {
    message: Message,
    replies: Vec<Message>,
    pending_deliveries: Vec<Delivery>,
}

async fn post_message(
    State(state): State<AppState>,
    Path(channel_id): Path<Uuid>,
    Json(request): Json<PostMessageRequest>,
) -> Result<(StatusCode, Json<PostMessageResponse>), ApiError> {
    let content = non_empty("message content", &request.content)?;
    let message = state
        .store
        .append_message(channel_id, None, MessageRole::User, content)
        .await?;
    let agents = state.store.list_channel_agents(channel_id).await?;
    let agent_ids: Vec<Uuid> = agents
        .into_iter()
        .filter(|agent| agent.status == AgentStatus::Running)
        .map(|agent| agent.id)
        .collect();
    state.store.enqueue_deliveries(&message, &agent_ids).await?;
    let replies = state.deliveries.dispatch_message(message.id).await?.replies;
    let pending_deliveries = state.store.list_message_deliveries(message.id).await?;
    let status = if pending_deliveries.is_empty() {
        StatusCode::CREATED
    } else {
        StatusCode::ACCEPTED
    };
    Ok((
        status,
        Json(PostMessageResponse {
            message,
            replies,
            pending_deliveries,
        }),
    ))
}

fn non_empty<'a>(field: &str, value: &'a str) -> Result<&'a str, ApiError> {
    let value = value.trim();
    if value.is_empty() {
        Err(ApiError::bad_request(format!("{field} must not be empty")))
    } else {
        Ok(value)
    }
}

fn validate_wiki_path(path: &str) -> Result<&str, ApiError> {
    if path.is_empty()
        || path.starts_with('/')
        || path.ends_with('/')
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        || !path
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b'/'))
    {
        return Err(ApiError::bad_request(
            "wiki path must be a safe relative path",
        ));
    }
    Ok(path)
}

fn parse_task_status_filter(status: Option<&str>) -> Result<Option<TaskStatus>, ApiError> {
    match status.map(str::trim).filter(|value| !value.is_empty()) {
        None | Some("all") => Ok(None),
        Some("todo") => Ok(Some(TaskStatus::Todo)),
        Some("in_progress") => Ok(Some(TaskStatus::InProgress)),
        Some("in_review") => Ok(Some(TaskStatus::InReview)),
        Some("done") => Ok(Some(TaskStatus::Done)),
        Some(value) => Err(ApiError::bad_request(format!(
            "unsupported task status: {value}"
        ))),
    }
}

async fn require_channel(store: &Store, channel_id: Uuid) -> Result<Channel, ApiError> {
    store
        .get_channel(channel_id)
        .await?
        .ok_or_else(|| ApiError::not_found("channel not found"))
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
    body: Option<serde_json::Value>,
}

impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
            body: None,
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
            body: None,
        }
    }

    fn conflict(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            message: message.into(),
            body: None,
        }
    }

    fn forbidden(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            message: message.into(),
            body: None,
        }
    }

    fn json(status: StatusCode, body: serde_json::Value) -> Self {
        let message = body
            .get("error")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("request failed")
            .to_owned();
        Self {
            status,
            message,
            body: Some(body),
        }
    }
}

impl From<StoreError> for ApiError {
    fn from(error: StoreError) -> Self {
        let status = match &error {
            StoreError::TaskNotFound { .. } => Some(StatusCode::NOT_FOUND),
            StoreError::TaskAlreadyClaimed { .. }
            | StoreError::TaskUnmetDependencies { .. }
            | StoreError::InvalidTaskTransition { .. }
            | StoreError::TaskDependencyCycle => Some(StatusCode::CONFLICT),
            StoreError::TaskDependencySelf => Some(StatusCode::BAD_REQUEST),
            StoreError::WikiPageNotFound(_) | StoreError::WikiRevisionNotFound { .. } => {
                Some(StatusCode::NOT_FOUND)
            }
            StoreError::WikiVersionConflict { .. } => Some(StatusCode::CONFLICT),
            StoreError::InvalidMemoryType(_)
            | StoreError::InvalidMemoryTopic(_)
            | StoreError::InvalidMemoryDescription(_)
            | StoreError::MemoryMoveSameNamespace => Some(StatusCode::BAD_REQUEST),
            StoreError::MemoryTopicTooLarge { .. }
            | StoreError::MemoryNamespaceFull { .. }
            | StoreError::MemoryIndexFull { .. } => Some(StatusCode::UNPROCESSABLE_ENTITY),
            StoreError::SkillLibraryNotFound(_) | StoreError::SkillInstallNotFound(_) => {
                Some(StatusCode::NOT_FOUND)
            }
            StoreError::SkillNameConflict(_) | StoreError::SkillAlreadyInstalled { .. } => {
                Some(StatusCode::CONFLICT)
            }
            StoreError::InvalidSkillName(_)
            | StoreError::InvalidSkillFilePath(_)
            | StoreError::InvalidSkillFileSize { .. } => Some(StatusCode::BAD_REQUEST),
            StoreError::SkillLibraryTooLarge { .. } => Some(StatusCode::UNPROCESSABLE_ENTITY),
            _ => None,
        };
        if let Some(status) = status {
            return Self {
                status,
                message: error.to_string(),
                body: None,
            };
        }
        tracing::error!(%error, "local store request failed");
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: "local store request failed".to_owned(),
            body: None,
        }
    }
}

impl From<RuntimeError> for ApiError {
    fn from(error: RuntimeError) -> Self {
        tracing::error!(%error, "runtime delivery failed");
        let status = match error {
            RuntimeError::Delivery(_) => StatusCode::BAD_GATEWAY,
            RuntimeError::Unsupported(_) => StatusCode::UNPROCESSABLE_ENTITY,
            RuntimeError::Busy(_) => StatusCode::CONFLICT,
            RuntimeError::NotFound(_) => StatusCode::NOT_FOUND,
        };
        Self {
            status,
            message: error.to_string(),
            body: None,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        if let Some(body) = self.body {
            return (self.status, Json(body)).into_response();
        }
        (
            self.status,
            Json(ErrorResponse {
                error: self.message,
            }),
        )
            .into_response()
    }
}
