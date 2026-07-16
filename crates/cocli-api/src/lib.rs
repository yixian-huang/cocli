//! Local HTTP API and runtime-neutral application service.

use std::sync::Arc;

use async_trait::async_trait;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use cocli_store::{Agent, AgentStatus, Channel, Message, MessageRole, Store, StoreError};
use serde::{Deserialize, Serialize};
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

/// Runtime failures surfaced to the HTTP application layer.
#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    /// The runtime rejected or failed a message delivery.
    #[error("{0}")]
    Delivery(String),
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
}

/// Builds the local HTTP router.
pub fn router(store: Store, runtime: Arc<dyn RuntimeService>) -> Router {
    Router::new()
        .route("/healthz", get(health))
        .route("/api/runtimes", get(list_runtimes))
        .route("/api/channels", get(list_channels).post(create_channel))
        .route(
            "/api/channels/:channel_id/messages",
            get(list_messages).post(post_message),
        )
        .route("/api/agents", get(list_agents).post(create_agent))
        .route("/api/agents/:agent_id/start", post(start_agent))
        .route("/api/agents/:agent_id/stop", post(stop_agent))
        .with_state(AppState { store, runtime })
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
    set_agent_status(&state.store, agent_id, AgentStatus::Running).await
}

async fn stop_agent(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
) -> Result<Json<Agent>, ApiError> {
    set_agent_status(&state.store, agent_id, AgentStatus::Stopped).await
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
    let mut replies = Vec::new();
    for agent in agents
        .iter()
        .filter(|agent| agent.status == AgentStatus::Running)
    {
        let content = state.runtime.reply(agent, &message).await?;
        replies.push(
            state
                .store
                .append_message(channel_id, Some(agent.id), MessageRole::Assistant, &content)
                .await?,
        );
    }
    Ok((
        StatusCode::CREATED,
        Json(PostMessageResponse { message, replies }),
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
        Self {
            status: StatusCode::BAD_GATEWAY,
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
