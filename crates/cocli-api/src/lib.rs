//! Local HTTP API and runtime-neutral application service.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use cocli_store::{
    Agent, AgentStatus, Channel, Delivery, DeliveryStats, Message, MessageRole, Store, StoreError,
};
use serde::{Deserialize, Serialize};
use tokio::sync::Notify;
use uuid::Uuid;

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
struct AppState {
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
        .route("/healthz", get(health))
        .route("/api/metrics", get(runtime_metrics))
        .route("/api/deliveries/stats", get(delivery_stats))
        .route("/api/runtimes", get(list_runtimes))
        .route("/api/channels", get(list_channels).post(create_channel))
        .route(
            "/api/channels/:channel_id/messages",
            get(list_messages).post(post_message),
        )
        .route("/api/agents", get(list_agents).post(create_agent))
        .route("/api/agents/:agent_id/start", post(start_agent))
        .route("/api/agents/:agent_id/stop", post(stop_agent))
        .route("/api/agents/:agent_id/runtime", get(runtime_status))
        .route("/api/agents/:agent_id/turn/cancel", post(cancel_turn))
        .route("/api/agents/:agent_id/turn/steer", post(steer_turn))
        .route("/api/agents/:agent_id/thread/fork", post(fork_thread))
        .route("/api/agents/:agent_id/recovery/probe", post(probe_recovery))
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

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
        }
    }
}

impl From<StoreError> for ApiError {
    fn from(error: StoreError) -> Self {
        tracing::error!(%error, "local store request failed");
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: "local store request failed".to_owned(),
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
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorResponse {
                error: self.message,
            }),
        )
            .into_response()
    }
}
