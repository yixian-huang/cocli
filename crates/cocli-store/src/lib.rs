//! SQLite-backed persistence for cocli local.

use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx_core::query::query;
use sqlx_core::query_scalar::query_scalar;
use sqlx_core::row::Row;
use sqlx_sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions, SqliteRow};
use uuid::Uuid;

mod wiki;
pub use wiki::{WikiBacklink, WikiPage, WikiPageSummary, WikiRevision};

/// Errors returned by the local SQLite store.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// A database query or connection failed.
    #[error(transparent)]
    Sqlx(#[from] sqlx_core::Error),
    /// Persisted JSON could not be encoded or decoded.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// A persisted enum value was not recognized.
    #[error("invalid persisted {kind} value: {value}")]
    InvalidValue {
        /// The domain field being decoded.
        kind: &'static str,
        /// The unexpected persisted value.
        value: String,
    },
    /// A delivery completion was attempted after it was already finalized or released.
    #[error("delivery is not in flight: {0}")]
    DeliveryNotInFlight(Uuid),
    /// A requested channel task does not exist.
    #[error("task #{task_number} not found")]
    TaskNotFound {
        /// Channel-local task number.
        task_number: i64,
    },
    /// A task is already assigned to another agent.
    #[error("task #{task_number} is already claimed")]
    TaskAlreadyClaimed {
        /// Channel-local task number.
        task_number: i64,
    },
    /// A task cannot be claimed until its dependencies are complete.
    #[error("task #{task_number} has unmet dependencies")]
    TaskUnmetDependencies {
        /// Channel-local task number.
        task_number: i64,
    },
    /// The requested task status change is not permitted.
    #[error("invalid task transition: {from} -> {to}")]
    InvalidTaskTransition {
        /// Persisted source state.
        from: String,
        /// Requested destination state.
        to: String,
    },
    /// A task cannot depend on itself.
    #[error("a task cannot depend on itself")]
    TaskDependencySelf,
    /// A task dependency would introduce a cycle.
    #[error("circular task dependency detected")]
    TaskDependencyCycle,
    /// A wiki write used a stale optimistic-concurrency version.
    #[error(
        "wiki version conflict at path {path:?}: current={current_version}, ifVersion={attempted_version}"
    )]
    WikiVersionConflict {
        /// Canonical page path.
        path: String,
        /// Version currently stored.
        current_version: i64,
        /// Version supplied by the caller.
        attempted_version: i64,
    },
    /// A requested wiki page does not exist.
    #[error("wiki page not found: {0}")]
    WikiPageNotFound(String),
    /// A requested wiki revision does not exist.
    #[error("wiki revision {version} not found for page {path}")]
    WikiRevisionNotFound {
        /// Canonical page path.
        path: String,
        /// Requested historical version.
        version: i64,
    },
}

/// A local conversation channel.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Channel {
    /// Stable channel identifier.
    pub id: Uuid,
    /// User-visible channel name.
    pub name: String,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
}

/// Whether an agent should receive new channel messages.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    /// The agent is eligible for message delivery.
    Running,
    /// The agent remains configured but does not receive messages.
    Stopped,
}

impl AgentStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Stopped => "stopped",
        }
    }

    fn parse(value: String) -> Result<Self, StoreError> {
        match value.as_str() {
            "running" => Ok(Self::Running),
            "stopped" => Ok(Self::Stopped),
            _ => Err(StoreError::InvalidValue {
                kind: "agent status",
                value,
            }),
        }
    }
}

/// A configured local agent.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Agent {
    /// Stable agent identifier.
    pub id: Uuid,
    /// Channel receiving this agent's replies.
    pub channel_id: Uuid,
    /// User-visible agent name.
    pub name: String,
    /// Runtime registry key.
    pub runtime: String,
    /// Optional runtime model identifier.
    pub model: Option<String>,
    /// Current delivery status.
    pub status: AgentStatus,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
}

struct AgentRow {
    id: Uuid,
    channel_id: Uuid,
    name: String,
    runtime: String,
    model: Option<String>,
    status: String,
    created_at: DateTime<Utc>,
}

impl TryFrom<AgentRow> for Agent {
    type Error = StoreError;

    fn try_from(row: AgentRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            channel_id: row.channel_id,
            name: row.name,
            runtime: row.runtime,
            model: row.model,
            status: AgentStatus::parse(row.status)?,
            created_at: row.created_at,
        })
    }
}

/// The author role of a persisted message.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    /// A message submitted through the local API.
    User,
    /// A reply emitted by an agent runtime.
    Assistant,
}

impl MessageRole {
    fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
        }
    }

    fn parse(value: String) -> Result<Self, StoreError> {
        match value.as_str() {
            "user" => Ok(Self::User),
            "assistant" => Ok(Self::Assistant),
            _ => Err(StoreError::InvalidValue {
                kind: "message role",
                value,
            }),
        }
    }
}

/// A channel-local sequenced message.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Message {
    /// Stable message identifier.
    pub id: Uuid,
    /// Owning channel.
    pub channel_id: Uuid,
    /// Monotonic sequence number within the channel.
    pub seq: i64,
    /// Replying agent, when the role is [`MessageRole::Assistant`].
    pub agent_id: Option<Uuid>,
    /// Message author role.
    pub role: MessageRole,
    /// Plain-text message body.
    pub content: String,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
}

/// Durable state of one user-message delivery to one agent.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryState {
    /// Ready now or after `next_attempt_at`.
    Pending,
    /// Reserved by the local delivery worker.
    InFlight,
    /// Retry budget was exhausted and manual intervention is required.
    Exhausted,
}

impl DeliveryState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::InFlight => "in_flight",
            Self::Exhausted => "exhausted",
        }
    }

    fn parse(value: String) -> Result<Self, StoreError> {
        match value.as_str() {
            "pending" => Ok(Self::Pending),
            "in_flight" => Ok(Self::InFlight),
            "exhausted" => Ok(Self::Exhausted),
            _ => Err(StoreError::InvalidValue {
                kind: "delivery state",
                value,
            }),
        }
    }
}

/// Persisted delivery from one channel message to one configured agent.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Delivery {
    pub id: Uuid,
    pub agent_id: Uuid,
    pub channel_id: Uuid,
    pub message_id: Uuid,
    pub seq: i64,
    pub state: DeliveryState,
    pub attempts: i64,
    pub next_attempt_at: DateTime<Utc>,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Aggregate durable-delivery backlog state.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct DeliveryStats {
    pub pending: i64,
    pub in_flight: i64,
    pub exhausted: i64,
    pub ready: i64,
    pub max_attempts: i64,
}

/// Durable current-work anchor exposed through the local bridge.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct WorkingState {
    pub agent_id: Uuid,
    pub summary: String,
    pub channel_name: Option<String>,
    pub task_number: Option<i64>,
    pub next_step_hint: Option<String>,
    pub started_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Lifecycle state of a local channel task.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Todo,
    InProgress,
    InReview,
    Done,
}

impl TaskStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Todo => "todo",
            Self::InProgress => "in_progress",
            Self::InReview => "in_review",
            Self::Done => "done",
        }
    }

    fn parse(value: String) -> Result<Self, StoreError> {
        match value.as_str() {
            "todo" => Ok(Self::Todo),
            "in_progress" => Ok(Self::InProgress),
            "in_review" => Ok(Self::InReview),
            "done" => Ok(Self::Done),
            _ => Err(StoreError::InvalidValue {
                kind: "task status",
                value,
            }),
        }
    }

    fn can_transition_to(self, next: Self) -> bool {
        self == next
            || matches!(
                (self, next),
                (Self::Todo, Self::InProgress)
                    | (Self::InProgress, Self::InReview | Self::Done)
                    | (Self::InReview, Self::InProgress | Self::Done)
            )
    }
}

/// One channel-scoped task exposed to both the local UI and agent bridge.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: Uuid,
    pub channel_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<Uuid>,
    pub task_number: i64,
    pub title: String,
    pub status: TaskStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by_type: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// One persisted local runtime session.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSession {
    pub id: Uuid,
    pub agent_id: Uuid,
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub launch_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_session_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_reason: Option<String>,
    pub turn_count: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cost_usd: f64,
    pub context_window: i64,
    pub session_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_chat_session_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files_changed: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_success: Option<bool>,
    pub started_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<DateTime<Utc>>,
}

/// Final aggregate values recorded when a runtime session ends.
#[derive(Clone, Debug, PartialEq)]
pub struct AgentSessionFinish {
    pub end_reason: String,
    pub turn_count: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cost_usd: f64,
    pub context_window: i64,
    pub task_summary: Option<String>,
    pub files_changed: Option<Vec<String>>,
    pub task_success: Option<bool>,
    pub ended_at: DateTime<Utc>,
}

/// Direct link from a persisted turn to its source channel message.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnMessageRef {
    pub channel_id: Uuid,
    pub message_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seq: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
}

/// One completed runtime turn and its structured trajectory.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentTurn {
    pub id: Uuid,
    pub agent_id: Uuid,
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub launch_id: Option<String>,
    pub turn_number: i64,
    pub started_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<DateTime<Utc>>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cost_usd: f64,
    pub context_window: i64,
    pub entries: serde_json::Value,
    pub session_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_ref: Option<TurnMessageRef>,
}

/// Values used to insert or idempotently replace one completed turn.
#[derive(Clone, Debug, PartialEq)]
pub struct NewAgentTurn {
    pub agent_id: Uuid,
    pub session_id: String,
    pub launch_id: Option<String>,
    pub turn_number: i64,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cost_usd: f64,
    pub context_window: i64,
    pub entries: serde_json::Value,
    pub session_type: String,
    pub channel_id: Option<Uuid>,
    pub source_message_id: Option<Uuid>,
}

/// One durable runtime activity marker.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentActivity {
    pub id: Uuid,
    pub agent_id: Uuid,
    pub activity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub trajectory: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub launch_id: Option<String>,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_row_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

struct TaskRow {
    id: Uuid,
    channel_id: Uuid,
    message_id: Option<Uuid>,
    task_number: i64,
    title: String,
    status: String,
    progress: Option<String>,
    assignee_id: Option<Uuid>,
    assignee_name: Option<String>,
    created_by_agent_id: Option<Uuid>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

struct MessageRow {
    id: Uuid,
    channel_id: Uuid,
    seq: i64,
    agent_id: Option<Uuid>,
    role: String,
    content: String,
    created_at: DateTime<Utc>,
}

struct DeliveryRow {
    id: Uuid,
    agent_id: Uuid,
    channel_id: Uuid,
    message_id: Uuid,
    seq: i64,
    state: String,
    attempts: i64,
    next_attempt_at: DateTime<Utc>,
    last_error: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl TryFrom<MessageRow> for Message {
    type Error = StoreError;

    fn try_from(row: MessageRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            channel_id: row.channel_id,
            seq: row.seq,
            agent_id: row.agent_id,
            role: MessageRole::parse(row.role)?,
            content: row.content,
            created_at: row.created_at,
        })
    }
}

impl TryFrom<DeliveryRow> for Delivery {
    type Error = StoreError;

    fn try_from(row: DeliveryRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            agent_id: row.agent_id,
            channel_id: row.channel_id,
            message_id: row.message_id,
            seq: row.seq,
            state: DeliveryState::parse(row.state)?,
            attempts: row.attempts,
            next_attempt_at: row.next_attempt_at,
            last_error: row.last_error,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

impl TryFrom<TaskRow> for Task {
    type Error = StoreError;

    fn try_from(row: TaskRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            channel_id: row.channel_id,
            message_id: row.message_id,
            task_number: row.task_number,
            title: row.title,
            status: TaskStatus::parse(row.status)?,
            progress: row.progress,
            assignee_id: row.assignee_id,
            assignee_type: row.assignee_id.map(|_| "agent".to_owned()),
            assignee_name: row.assignee_name,
            created_by_id: row.created_by_agent_id,
            created_by_type: row.created_by_agent_id.map(|_| "agent".to_owned()),
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

/// Cloneable handle to the local SQLite database.
#[derive(Clone, Debug)]
pub struct Store {
    pool: SqlitePool,
}

impl Store {
    /// Opens or creates a file-backed SQLite database and applies migrations.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the database cannot be opened or migrated.
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .foreign_keys(true);
        Self::connect(options, 5).await
    }

    /// Creates a single-connection in-memory database for tests.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the database cannot be initialized.
    pub async fn in_memory() -> Result<Self, StoreError> {
        let options = SqliteConnectOptions::new()
            .filename(":memory:")
            .foreign_keys(true);
        Self::connect(options, 1).await
    }

    async fn connect(
        options: SqliteConnectOptions,
        max_connections: u32,
    ) -> Result<Self, StoreError> {
        let pool = SqlitePoolOptions::new()
            .max_connections(max_connections)
            .connect_with(options)
            .await?;
        apply_schema(&pool).await?;
        Ok(Self { pool })
    }

    /// Creates a channel.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the insert fails.
    pub async fn create_channel(&self, name: &str) -> Result<Channel, StoreError> {
        let channel = Channel {
            id: Uuid::new_v4(),
            name: name.to_owned(),
            created_at: Utc::now(),
        };
        query("INSERT INTO channels (id, name, created_at) VALUES (?, ?, ?)")
            .bind(channel.id)
            .bind(&channel.name)
            .bind(channel.created_at)
            .execute(&self.pool)
            .await?;
        Ok(channel)
    }

    /// Lists channels in creation order.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the query fails.
    pub async fn list_channels(&self) -> Result<Vec<Channel>, StoreError> {
        let rows = query("SELECT id, name, created_at FROM channels ORDER BY created_at, id")
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter().map(channel_from_row).collect()
    }

    /// Returns one channel by identifier.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the query fails.
    pub async fn get_channel(&self, channel_id: Uuid) -> Result<Option<Channel>, StoreError> {
        let row = query("SELECT id, name, created_at FROM channels WHERE id = ?")
            .bind(channel_id)
            .fetch_optional(&self.pool)
            .await?;
        row.map(channel_from_row).transpose()
    }

    /// Returns one channel by its exact local name.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the query fails.
    pub async fn get_channel_by_name(&self, name: &str) -> Result<Option<Channel>, StoreError> {
        let row = query(
            "SELECT id, name, created_at FROM channels \
             WHERE name = ? ORDER BY created_at, id LIMIT 1",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;
        row.map(channel_from_row).transpose()
    }

    /// Creates an agent attached to a channel.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the insert fails.
    pub async fn create_agent(
        &self,
        channel_id: Uuid,
        name: &str,
        runtime: &str,
        model: Option<&str>,
        status: AgentStatus,
    ) -> Result<Agent, StoreError> {
        let agent = Agent {
            id: Uuid::new_v4(),
            channel_id,
            name: name.to_owned(),
            runtime: runtime.to_owned(),
            model: model.map(str::to_owned),
            status,
            created_at: Utc::now(),
        };
        query(
            "INSERT INTO agents \
             (id, channel_id, name, runtime, model, status, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(agent.id)
        .bind(agent.channel_id)
        .bind(&agent.name)
        .bind(&agent.runtime)
        .bind(&agent.model)
        .bind(agent.status.as_str())
        .bind(agent.created_at)
        .execute(&self.pool)
        .await?;
        Ok(agent)
    }

    /// Lists all configured agents.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the query or persisted enum decoding fails.
    pub async fn list_agents(&self) -> Result<Vec<Agent>, StoreError> {
        let rows = query(
            "SELECT id, channel_id, name, runtime, model, status, created_at \
             FROM agents ORDER BY created_at, id",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(agent_row_from_sqlite)
            .map(|row| row.and_then(Agent::try_from))
            .collect()
    }

    /// Lists agents attached to one channel.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the query or persisted enum decoding fails.
    pub async fn list_channel_agents(&self, channel_id: Uuid) -> Result<Vec<Agent>, StoreError> {
        let rows = query(
            "SELECT id, channel_id, name, runtime, model, status, created_at \
             FROM agents WHERE channel_id = ? ORDER BY created_at, id",
        )
        .bind(channel_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(agent_row_from_sqlite)
            .map(|row| row.and_then(Agent::try_from))
            .collect()
    }

    /// Updates an agent's delivery status.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the update or follow-up query fails.
    pub async fn set_agent_status(
        &self,
        agent_id: Uuid,
        status: AgentStatus,
    ) -> Result<Option<Agent>, StoreError> {
        query("UPDATE agents SET status = ? WHERE id = ?")
            .bind(status.as_str())
            .bind(agent_id)
            .execute(&self.pool)
            .await?;
        self.get_agent(agent_id).await
    }

    /// Returns one agent by identifier.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the query or persisted enum decoding fails.
    pub async fn get_agent(&self, agent_id: Uuid) -> Result<Option<Agent>, StoreError> {
        let row = query(
            "SELECT id, channel_id, name, runtime, model, status, created_at \
             FROM agents WHERE id = ?",
        )
        .bind(agent_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(agent_row_from_sqlite)
            .transpose()?
            .map(Agent::try_from)
            .transpose()
    }

    /// Appends a message with the next sequence number in its channel.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the transaction fails.
    pub async fn append_message(
        &self,
        channel_id: Uuid,
        agent_id: Option<Uuid>,
        role: MessageRole,
        content: &str,
    ) -> Result<Message, StoreError> {
        let mut transaction = self.pool.begin().await?;
        let seq: i64 =
            query_scalar("SELECT COALESCE(MAX(seq), 0) + 1 FROM messages WHERE channel_id = ?")
                .bind(channel_id)
                .fetch_one(&mut *transaction)
                .await?;
        let row = query(
            "INSERT INTO messages \
             (id, channel_id, seq, agent_id, role, content, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?) \
             RETURNING id, channel_id, seq, agent_id, role, content, created_at",
        )
        .bind(Uuid::new_v4())
        .bind(channel_id)
        .bind(seq)
        .bind(agent_id)
        .bind(role.as_str())
        .bind(content)
        .bind(Utc::now())
        .fetch_one(&mut *transaction)
        .await?;
        transaction.commit().await?;
        message_row_from_sqlite(row)?.try_into()
    }

    /// Lists a channel's messages in sequence order.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the query or persisted enum decoding fails.
    pub async fn list_messages(&self, channel_id: Uuid) -> Result<Vec<Message>, StoreError> {
        let rows = query(
            "SELECT id, channel_id, seq, agent_id, role, content, created_at \
             FROM messages WHERE channel_id = ? ORDER BY seq",
        )
        .bind(channel_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(message_row_from_sqlite)
            .map(|row| row.and_then(Message::try_from))
            .collect()
    }

    /// Lists a bounded message page in ascending sequence order.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the query or persisted enum decoding fails.
    pub async fn list_message_page(
        &self,
        channel_id: Uuid,
        limit: i64,
        before: Option<i64>,
        after: Option<i64>,
    ) -> Result<Vec<Message>, StoreError> {
        let rows = query(
            "SELECT id, channel_id, seq, agent_id, role, content, created_at \
             FROM messages \
             WHERE channel_id = ? \
               AND (? IS NULL OR seq < ?) \
               AND (? IS NULL OR seq > ?) \
             ORDER BY seq DESC LIMIT ?",
        )
        .bind(channel_id)
        .bind(before)
        .bind(before)
        .bind(after)
        .bind(after)
        .bind(limit.clamp(1, 200))
        .fetch_all(&self.pool)
        .await?;
        let mut messages: Vec<Message> = rows
            .into_iter()
            .map(message_row_from_sqlite)
            .map(|row| row.and_then(Message::try_from))
            .collect::<Result<_, _>>()?;
        messages.reverse();
        Ok(messages)
    }

    /// Consumes unread channel messages for one agent and advances its cursor.
    ///
    /// Messages authored by the same agent are skipped. The cursor is also
    /// advanced when a durable delivery completes, so the source message does
    /// not reappear through `check_messages`.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the transaction fails.
    pub async fn consume_agent_inbox(
        &self,
        agent_id: Uuid,
        limit: i64,
    ) -> Result<Vec<Message>, StoreError> {
        let agent = match self.get_agent(agent_id).await? {
            Some(agent) => agent,
            None => return Ok(Vec::new()),
        };
        let mut transaction = self.pool.begin().await?;
        let cursor: i64 = query_scalar(
            "SELECT COALESCE( \
                 (SELECT last_read_seq FROM agent_inbox_state WHERE agent_id = ?), 0)",
        )
        .bind(agent_id)
        .fetch_one(&mut *transaction)
        .await?;
        let rows = query(
            "SELECT id, channel_id, seq, agent_id, role, content, created_at \
             FROM messages \
             WHERE channel_id = ? AND seq > ? \
               AND (agent_id IS NULL OR agent_id != ?) \
             ORDER BY seq LIMIT ?",
        )
        .bind(agent.channel_id)
        .bind(cursor)
        .bind(agent_id)
        .bind(limit.clamp(1, 200))
        .fetch_all(&mut *transaction)
        .await?;
        let messages: Vec<Message> = rows
            .into_iter()
            .map(message_row_from_sqlite)
            .map(|row| row.and_then(Message::try_from))
            .collect::<Result<_, _>>()?;
        if let Some(last) = messages.last() {
            upsert_inbox_cursor(&mut transaction, agent_id, last.seq, Utc::now()).await?;
        }
        transaction.commit().await?;
        Ok(messages)
    }

    /// Stores or updates one agent's current-work anchor.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the query fails.
    pub async fn set_working_state(
        &self,
        agent_id: Uuid,
        summary: &str,
        channel_name: Option<&str>,
        task_number: Option<i64>,
        next_step_hint: Option<&str>,
    ) -> Result<WorkingState, StoreError> {
        let now = Utc::now();
        query(
            "INSERT INTO agent_working_state \
             (agent_id, summary, channel_name, task_number, next_step_hint, started_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(agent_id) DO UPDATE SET \
               summary = excluded.summary, channel_name = excluded.channel_name, \
               task_number = excluded.task_number, next_step_hint = excluded.next_step_hint, \
               updated_at = excluded.updated_at",
        )
        .bind(agent_id)
        .bind(summary)
        .bind(channel_name)
        .bind(task_number)
        .bind(next_step_hint)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;
        self.get_working_state(agent_id)
            .await?
            .ok_or(StoreError::InvalidValue {
                kind: "working state",
                value: agent_id.to_string(),
            })
    }

    /// Returns one agent's current-work anchor.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the query fails.
    pub async fn get_working_state(
        &self,
        agent_id: Uuid,
    ) -> Result<Option<WorkingState>, StoreError> {
        let row = query(
            "SELECT agent_id, summary, channel_name, task_number, next_step_hint, \
             started_at, updated_at FROM agent_working_state WHERE agent_id = ?",
        )
        .bind(agent_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(working_state_from_row).transpose()
    }

    /// Clears one agent's current-work anchor.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the query fails.
    pub async fn clear_working_state(&self, agent_id: Uuid) -> Result<bool, StoreError> {
        let result = query("DELETE FROM agent_working_state WHERE agent_id = ?")
            .bind(agent_id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Creates a task with the next channel-local task number.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the insert or row decoding fails.
    pub async fn create_task(
        &self,
        channel_id: Uuid,
        title: &str,
        message_id: Option<Uuid>,
        created_by_agent_id: Option<Uuid>,
    ) -> Result<Task, StoreError> {
        let title = title.chars().take(100).collect::<String>();
        let now = Utc::now();
        let row = query(
            "INSERT INTO tasks \
             (id, channel_id, message_id, task_number, title, status, \
              created_by_agent_id, created_at, updated_at) \
             SELECT ?, ?, ?, COALESCE(MAX(task_number), 0) + 1, ?, 'todo', ?, ?, ? \
             FROM tasks WHERE channel_id = ? \
             RETURNING id",
        )
        .bind(Uuid::new_v4())
        .bind(channel_id)
        .bind(message_id)
        .bind(title)
        .bind(created_by_agent_id)
        .bind(now)
        .bind(now)
        .bind(channel_id)
        .fetch_one(&self.pool)
        .await?;
        let task_id: Uuid = row.try_get("id")?;
        self.get_task_by_id(task_id)
            .await?
            .ok_or(StoreError::InvalidValue {
                kind: "task",
                value: task_id.to_string(),
            })
    }

    /// Lists tasks for one channel in task-number order.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the query or row decoding fails.
    pub async fn list_tasks(
        &self,
        channel_id: Uuid,
        status: Option<TaskStatus>,
    ) -> Result<Vec<Task>, StoreError> {
        let rows = if let Some(status) = status {
            query(
                "SELECT t.id, t.channel_id, t.message_id, t.task_number, t.title, \
                 t.status, t.progress, t.assignee_id, a.name AS assignee_name, \
                 t.created_by_agent_id, t.created_at, t.updated_at \
                 FROM tasks t LEFT JOIN agents a ON a.id = t.assignee_id \
                 WHERE t.channel_id = ? AND t.status = ? ORDER BY t.task_number",
            )
            .bind(channel_id)
            .bind(status.as_str())
            .fetch_all(&self.pool)
            .await?
        } else {
            query(
                "SELECT t.id, t.channel_id, t.message_id, t.task_number, t.title, \
                 t.status, t.progress, t.assignee_id, a.name AS assignee_name, \
                 t.created_by_agent_id, t.created_at, t.updated_at \
                 FROM tasks t LEFT JOIN agents a ON a.id = t.assignee_id \
                 WHERE t.channel_id = ? ORDER BY t.task_number",
            )
            .bind(channel_id)
            .fetch_all(&self.pool)
            .await?
        };
        rows.into_iter()
            .map(task_row_from_sqlite)
            .map(|row| row.and_then(Task::try_from))
            .collect()
    }

    /// Returns one task by channel-local number.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the query or row decoding fails.
    pub async fn get_task(
        &self,
        channel_id: Uuid,
        task_number: i64,
    ) -> Result<Option<Task>, StoreError> {
        let row = query(
            "SELECT t.id, t.channel_id, t.message_id, t.task_number, t.title, \
             t.status, t.progress, t.assignee_id, a.name AS assignee_name, \
             t.created_by_agent_id, t.created_at, t.updated_at \
             FROM tasks t LEFT JOIN agents a ON a.id = t.assignee_id \
             WHERE t.channel_id = ? AND t.task_number = ?",
        )
        .bind(channel_id)
        .bind(task_number)
        .fetch_optional(&self.pool)
        .await?;
        row.map(task_row_from_sqlite)
            .transpose()?
            .map(Task::try_from)
            .transpose()
    }

    /// Returns the task linked to one source message.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the query or row decoding fails.
    pub async fn get_task_by_message(
        &self,
        channel_id: Uuid,
        message_id: Uuid,
    ) -> Result<Option<Task>, StoreError> {
        let row = query(
            "SELECT t.id, t.channel_id, t.message_id, t.task_number, t.title, \
             t.status, t.progress, t.assignee_id, a.name AS assignee_name, \
             t.created_by_agent_id, t.created_at, t.updated_at \
             FROM tasks t LEFT JOIN agents a ON a.id = t.assignee_id \
             WHERE t.channel_id = ? AND t.message_id = ?",
        )
        .bind(channel_id)
        .bind(message_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(task_row_from_sqlite)
            .transpose()?
            .map(Task::try_from)
            .transpose()
    }

    /// Claims an unassigned task for one local agent.
    ///
    /// # Errors
    ///
    /// Returns a typed task error when the task is absent, assigned, or blocked.
    pub async fn claim_task(
        &self,
        channel_id: Uuid,
        task_number: i64,
        agent_id: Uuid,
    ) -> Result<Task, StoreError> {
        let existing = self
            .get_task(channel_id, task_number)
            .await?
            .ok_or(StoreError::TaskNotFound { task_number })?;
        if existing.assignee_id == Some(agent_id) {
            return Ok(existing);
        }
        if existing.assignee_id.is_some() {
            return Err(StoreError::TaskAlreadyClaimed { task_number });
        }
        if !self.dependencies_met(channel_id, task_number).await? {
            return Err(StoreError::TaskUnmetDependencies { task_number });
        }
        let now = Utc::now();
        let result = query(
            "UPDATE tasks SET assignee_id = ?, \
             status = CASE WHEN status = 'todo' THEN 'in_progress' ELSE status END, \
             updated_at = ? \
             WHERE channel_id = ? AND task_number = ? AND assignee_id IS NULL",
        )
        .bind(agent_id)
        .bind(now)
        .bind(channel_id)
        .bind(task_number)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() != 1 {
            return Err(StoreError::TaskAlreadyClaimed { task_number });
        }
        self.get_task(channel_id, task_number)
            .await?
            .ok_or(StoreError::TaskNotFound { task_number })
    }

    /// Clears a task's assignee, returning active work to `todo`.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::TaskNotFound`] when the task is absent.
    pub async fn unclaim_task(
        &self,
        channel_id: Uuid,
        task_number: i64,
    ) -> Result<Task, StoreError> {
        let now = Utc::now();
        let result = query(
            "UPDATE tasks SET assignee_id = NULL, \
             status = CASE WHEN status = 'in_progress' THEN 'todo' ELSE status END, \
             updated_at = ? WHERE channel_id = ? AND task_number = ?",
        )
        .bind(now)
        .bind(channel_id)
        .bind(task_number)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() != 1 {
            return Err(StoreError::TaskNotFound { task_number });
        }
        self.get_task(channel_id, task_number)
            .await?
            .ok_or(StoreError::TaskNotFound { task_number })
    }

    /// Advances a task through the local lifecycle.
    ///
    /// # Errors
    ///
    /// Returns a typed error for absent tasks or invalid transitions.
    pub async fn update_task_status(
        &self,
        channel_id: Uuid,
        task_number: i64,
        status: TaskStatus,
        progress: Option<&str>,
    ) -> Result<Task, StoreError> {
        let existing = self
            .get_task(channel_id, task_number)
            .await?
            .ok_or(StoreError::TaskNotFound { task_number })?;
        if !existing.status.can_transition_to(status) {
            return Err(StoreError::InvalidTaskTransition {
                from: existing.status.as_str().to_owned(),
                to: status.as_str().to_owned(),
            });
        }
        if existing.status == status && progress.is_none() {
            return Ok(existing);
        }
        let now = Utc::now();
        if let Some(progress) = progress {
            query(
                "UPDATE tasks SET status = ?, progress = ?, updated_at = ? \
                 WHERE channel_id = ? AND task_number = ?",
            )
            .bind(status.as_str())
            .bind(progress)
            .bind(now)
            .bind(channel_id)
            .bind(task_number)
            .execute(&self.pool)
            .await?;
        } else {
            query(
                "UPDATE tasks SET status = ?, updated_at = ? \
                 WHERE channel_id = ? AND task_number = ?",
            )
            .bind(status.as_str())
            .bind(now)
            .bind(channel_id)
            .bind(task_number)
            .execute(&self.pool)
            .await?;
        }
        self.get_task(channel_id, task_number)
            .await?
            .ok_or(StoreError::TaskNotFound { task_number })
    }

    /// Adds an acyclic dependency between two tasks in the same channel.
    ///
    /// # Errors
    ///
    /// Returns a typed error for missing tasks, self-dependency, or a cycle.
    pub async fn add_task_dependency(
        &self,
        channel_id: Uuid,
        task_number: i64,
        depends_on: i64,
    ) -> Result<(), StoreError> {
        if task_number == depends_on {
            return Err(StoreError::TaskDependencySelf);
        }
        for number in [task_number, depends_on] {
            if self.get_task(channel_id, number).await?.is_none() {
                return Err(StoreError::TaskNotFound {
                    task_number: number,
                });
            }
        }
        let creates_cycle: i64 = query_scalar(
            "WITH RECURSIVE reachable(task_number) AS ( \
               SELECT depends_on FROM task_dependencies \
               WHERE channel_id = ? AND task_number = ? \
               UNION \
               SELECT dependency.depends_on FROM task_dependencies dependency \
               JOIN reachable ON dependency.task_number = reachable.task_number \
               WHERE dependency.channel_id = ? \
             ) \
             SELECT EXISTS(SELECT 1 FROM reachable WHERE task_number = ?)",
        )
        .bind(channel_id)
        .bind(depends_on)
        .bind(channel_id)
        .bind(task_number)
        .fetch_one(&self.pool)
        .await?;
        if creates_cycle != 0 {
            return Err(StoreError::TaskDependencyCycle);
        }
        query(
            "INSERT OR IGNORE INTO task_dependencies \
             (channel_id, task_number, depends_on, created_at) VALUES (?, ?, ?, ?)",
        )
        .bind(channel_id)
        .bind(task_number)
        .bind(depends_on)
        .bind(Utc::now())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Removes one dependency edge.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the delete fails.
    pub async fn remove_task_dependency(
        &self,
        channel_id: Uuid,
        task_number: i64,
        depends_on: i64,
    ) -> Result<bool, StoreError> {
        let result = query(
            "DELETE FROM task_dependencies \
             WHERE channel_id = ? AND task_number = ? AND depends_on = ?",
        )
        .bind(channel_id)
        .bind(task_number)
        .bind(depends_on)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Returns the task numbers this task depends on.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the query fails.
    pub async fn get_task_dependencies(
        &self,
        channel_id: Uuid,
        task_number: i64,
    ) -> Result<Vec<i64>, StoreError> {
        let rows = query(
            "SELECT depends_on FROM task_dependencies \
             WHERE channel_id = ? AND task_number = ? ORDER BY depends_on",
        )
        .bind(channel_id)
        .bind(task_number)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| row.try_get("depends_on").map_err(StoreError::from))
            .collect()
    }

    async fn dependencies_met(
        &self,
        channel_id: Uuid,
        task_number: i64,
    ) -> Result<bool, StoreError> {
        let count: i64 = query_scalar(
            "SELECT COUNT(*) FROM task_dependencies dependency \
             JOIN tasks prerequisite \
               ON prerequisite.channel_id = dependency.channel_id \
              AND prerequisite.task_number = dependency.depends_on \
             WHERE dependency.channel_id = ? AND dependency.task_number = ? \
               AND prerequisite.status != 'done'",
        )
        .bind(channel_id)
        .bind(task_number)
        .fetch_one(&self.pool)
        .await?;
        Ok(count == 0)
    }

    /// Returns one persisted message by identifier.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the query or persisted enum decoding fails.
    pub async fn get_message(&self, message_id: Uuid) -> Result<Option<Message>, StoreError> {
        let row = query(
            "SELECT id, channel_id, seq, agent_id, role, content, created_at \
             FROM messages WHERE id = ?",
        )
        .bind(message_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(message_row_from_sqlite)
            .transpose()?
            .map(Message::try_from)
            .transpose()
    }

    /// Enqueues one durable delivery per agent, ignoring duplicate message-agent pairs.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the transaction fails.
    pub async fn enqueue_deliveries(
        &self,
        message: &Message,
        agent_ids: &[Uuid],
    ) -> Result<Vec<Delivery>, StoreError> {
        let now = Utc::now();
        let mut transaction = self.pool.begin().await?;
        for agent_id in agent_ids {
            query(
                "INSERT OR IGNORE INTO delivery_queue \
                 (id, agent_id, channel_id, message_id, seq, state, attempts, \
                  next_attempt_at, last_error, created_at, updated_at) \
                 VALUES (?, ?, ?, ?, ?, ?, 0, ?, NULL, ?, ?)",
            )
            .bind(Uuid::new_v4())
            .bind(agent_id)
            .bind(message.channel_id)
            .bind(message.id)
            .bind(message.seq)
            .bind(DeliveryState::Pending.as_str())
            .bind(now)
            .bind(now)
            .bind(now)
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        self.list_message_deliveries(message.id).await
    }

    /// Lists durable deliveries for one source message.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the query or persisted enum decoding fails.
    pub async fn list_message_deliveries(
        &self,
        message_id: Uuid,
    ) -> Result<Vec<Delivery>, StoreError> {
        let rows = query(
            "SELECT id, agent_id, channel_id, message_id, seq, state, attempts, \
             next_attempt_at, last_error, created_at, updated_at \
             FROM delivery_queue WHERE message_id = ? ORDER BY created_at, agent_id",
        )
        .bind(message_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(delivery_row_from_sqlite)
            .map(|row| row.and_then(Delivery::try_from))
            .collect()
    }

    /// Atomically reserves due deliveries and increments their attempt generation.
    ///
    /// This local implementation has one worker, so a transaction with guarded
    /// updates provides the same no-double-reserve invariant without
    /// `FOR UPDATE SKIP LOCKED`.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the transaction fails.
    pub async fn reserve_due_deliveries(
        &self,
        limit: i64,
        max_attempts: i64,
        now: DateTime<Utc>,
    ) -> Result<Vec<Delivery>, StoreError> {
        let limit = limit.max(1);
        let max_attempts = max_attempts.max(1);
        let mut transaction = self.pool.begin().await?;
        let rows = query(
            "SELECT id, agent_id, channel_id, message_id, seq, state, attempts, \
             next_attempt_at, last_error, created_at, updated_at \
             FROM delivery_queue \
             WHERE state = 'pending' AND next_attempt_at <= ? AND attempts < ? \
               AND EXISTS (SELECT 1 FROM agents \
                           WHERE agents.id = delivery_queue.agent_id \
                             AND agents.status = 'running') \
             ORDER BY next_attempt_at, created_at, id LIMIT ?",
        )
        .bind(now)
        .bind(max_attempts)
        .bind(limit)
        .fetch_all(&mut *transaction)
        .await?;
        let candidates: Vec<Delivery> = rows
            .into_iter()
            .map(delivery_row_from_sqlite)
            .map(|row| row.and_then(Delivery::try_from))
            .collect::<Result<_, _>>()?;
        let mut reserved = Vec::with_capacity(candidates.len());
        for mut delivery in candidates {
            let result = query(
                "UPDATE delivery_queue \
                 SET state = 'in_flight', attempts = attempts + 1, \
                     last_error = NULL, updated_at = ? \
                 WHERE id = ? AND state = 'pending'",
            )
            .bind(now)
            .bind(delivery.id)
            .execute(&mut *transaction)
            .await?;
            if result.rows_affected() == 1 {
                delivery.state = DeliveryState::InFlight;
                delivery.attempts = delivery.attempts.saturating_add(1);
                delivery.last_error = None;
                delivery.updated_at = now;
                reserved.push(delivery);
            }
        }
        transaction.commit().await?;
        Ok(reserved)
    }

    /// Reserves one specific pending delivery when it is due.
    ///
    /// Returns `None` when another worker already reserved it, it is delayed,
    /// or its retry budget is exhausted.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the update or follow-up query fails.
    pub async fn reserve_delivery(
        &self,
        delivery_id: Uuid,
        max_attempts: i64,
        now: DateTime<Utc>,
    ) -> Result<Option<Delivery>, StoreError> {
        let result = query(
            "UPDATE delivery_queue \
             SET state = 'in_flight', attempts = attempts + 1, \
                 last_error = NULL, updated_at = ? \
             WHERE id = ? AND state = 'pending' \
               AND next_attempt_at <= ? AND attempts < ? \
               AND EXISTS (SELECT 1 FROM agents \
                           WHERE agents.id = delivery_queue.agent_id \
                             AND agents.status = 'running')",
        )
        .bind(now)
        .bind(delivery_id)
        .bind(now)
        .bind(max_attempts.max(1))
        .execute(&self.pool)
        .await?;
        if result.rows_affected() != 1 {
            return Ok(None);
        }
        self.get_delivery(delivery_id).await
    }

    /// Makes pending deliveries for a started agent immediately retryable.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the update fails.
    pub async fn nudge_agent_deliveries(&self, agent_id: Uuid) -> Result<u64, StoreError> {
        let now = Utc::now();
        let result = query(
            "UPDATE delivery_queue \
             SET next_attempt_at = ?, updated_at = ? \
             WHERE agent_id = ? AND state = 'pending'",
        )
        .bind(now)
        .bind(now)
        .bind(agent_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    /// Returns an in-flight delivery to the retry queue or exhausts it.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the update fails.
    pub async fn defer_delivery(
        &self,
        delivery_id: Uuid,
        error: &str,
        next_attempt_at: DateTime<Utc>,
        max_attempts: i64,
    ) -> Result<Option<Delivery>, StoreError> {
        let now = Utc::now();
        query(
            "UPDATE delivery_queue \
             SET state = CASE WHEN attempts >= ? THEN 'exhausted' ELSE 'pending' END, \
                 next_attempt_at = ?, last_error = ?, updated_at = ? \
             WHERE id = ? AND state = 'in_flight'",
        )
        .bind(max_attempts.max(1))
        .bind(next_attempt_at)
        .bind(error)
        .bind(now)
        .bind(delivery_id)
        .execute(&self.pool)
        .await?;
        self.get_delivery(delivery_id).await
    }

    /// Releases all in-flight rows after a process restart.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the update fails.
    pub async fn release_in_flight_deliveries(&self) -> Result<u64, StoreError> {
        let now = Utc::now();
        let result = query(
            "UPDATE delivery_queue \
             SET state = 'pending', next_attempt_at = ?, updated_at = ? \
             WHERE state = 'in_flight'",
        )
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    /// Persists an assistant reply and removes its delivery in one transaction.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the transaction fails.
    pub async fn complete_delivery(
        &self,
        delivery: &Delivery,
        content: &str,
    ) -> Result<Message, StoreError> {
        let mut transaction = self.pool.begin().await?;
        let deleted = query("DELETE FROM delivery_queue WHERE id = ? AND state = 'in_flight'")
            .bind(delivery.id)
            .execute(&mut *transaction)
            .await?;
        if deleted.rows_affected() != 1 {
            return Err(StoreError::DeliveryNotInFlight(delivery.id));
        }
        upsert_inbox_cursor(
            &mut transaction,
            delivery.agent_id,
            delivery.seq,
            Utc::now(),
        )
        .await?;
        let seq: i64 =
            query_scalar("SELECT COALESCE(MAX(seq), 0) + 1 FROM messages WHERE channel_id = ?")
                .bind(delivery.channel_id)
                .fetch_one(&mut *transaction)
                .await?;
        let row = query(
            "INSERT INTO messages \
             (id, channel_id, seq, agent_id, role, content, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?) \
             RETURNING id, channel_id, seq, agent_id, role, content, created_at",
        )
        .bind(Uuid::new_v4())
        .bind(delivery.channel_id)
        .bind(seq)
        .bind(delivery.agent_id)
        .bind(MessageRole::Assistant.as_str())
        .bind(content)
        .bind(Utc::now())
        .fetch_one(&mut *transaction)
        .await?;
        transaction.commit().await?;
        message_row_from_sqlite(row)?.try_into()
    }

    /// Returns aggregate durable-delivery backlog state.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the query fails.
    pub async fn delivery_stats(&self, now: DateTime<Utc>) -> Result<DeliveryStats, StoreError> {
        let row = query(
            "SELECT \
             SUM(CASE WHEN state = 'pending' THEN 1 ELSE 0 END) AS pending, \
             SUM(CASE WHEN state = 'in_flight' THEN 1 ELSE 0 END) AS in_flight, \
             SUM(CASE WHEN state = 'exhausted' THEN 1 ELSE 0 END) AS exhausted, \
             SUM(CASE WHEN state = 'pending' AND next_attempt_at <= ? THEN 1 ELSE 0 END) AS ready, \
             COALESCE(MAX(attempts), 0) AS max_attempts \
             FROM delivery_queue",
        )
        .bind(now)
        .fetch_one(&self.pool)
        .await?;
        Ok(DeliveryStats {
            pending: row
                .try_get::<Option<i64>, _>("pending")?
                .unwrap_or_default(),
            in_flight: row
                .try_get::<Option<i64>, _>("in_flight")?
                .unwrap_or_default(),
            exhausted: row
                .try_get::<Option<i64>, _>("exhausted")?
                .unwrap_or_default(),
            ready: row.try_get::<Option<i64>, _>("ready")?.unwrap_or_default(),
            max_attempts: row.try_get("max_attempts")?,
        })
    }

    /// Records one runtime session start.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the insert or row decoding fails.
    #[allow(clippy::too_many_arguments)]
    pub async fn create_agent_session(
        &self,
        agent_id: Uuid,
        channel_id: Option<Uuid>,
        session_id: &str,
        launch_id: Option<&str>,
        parent_session_id: Option<Uuid>,
        session_type: &str,
        started_at: DateTime<Utc>,
    ) -> Result<AgentSession, StoreError> {
        let row = query(
            "INSERT INTO agent_sessions \
             (id, agent_id, session_id, launch_id, channel_id, parent_session_id, \
              session_type, started_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?) \
             RETURNING id, agent_id, session_id, launch_id, channel_id, parent_session_id, \
               end_reason, turn_count, input_tokens, output_tokens, cost_usd, context_window, \
               session_type, scope, parent_chat_session_id, task_summary, files_changed, \
               task_success, started_at, ended_at",
        )
        .bind(Uuid::new_v4())
        .bind(agent_id)
        .bind(session_id)
        .bind(launch_id)
        .bind(channel_id)
        .bind(parent_session_id)
        .bind(normalized_session_type(session_type))
        .bind(started_at)
        .fetch_one(&self.pool)
        .await?;
        agent_session_from_row(row)
    }

    /// Marks one launch's newest open session as ended.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when serialization or the update fails.
    pub async fn finish_agent_session(
        &self,
        agent_id: Uuid,
        launch_id: &str,
        finish: &AgentSessionFinish,
    ) -> Result<bool, StoreError> {
        let files_changed = finish
            .files_changed
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        let result = query(
            "UPDATE agent_sessions SET end_reason = ?, turn_count = ?, input_tokens = ?, \
               output_tokens = ?, cost_usd = ?, context_window = ?, task_summary = ?, \
               files_changed = ?, task_success = ?, ended_at = ? \
             WHERE id = (SELECT id FROM agent_sessions \
               WHERE agent_id = ? AND launch_id = ? AND ended_at IS NULL \
               ORDER BY started_at DESC LIMIT 1)",
        )
        .bind(&finish.end_reason)
        .bind(finish.turn_count)
        .bind(finish.input_tokens)
        .bind(finish.output_tokens)
        .bind(finish.cost_usd)
        .bind(finish.context_window)
        .bind(&finish.task_summary)
        .bind(files_changed)
        .bind(finish.task_success.map(i64::from))
        .bind(finish.ended_at)
        .bind(agent_id)
        .bind(launch_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    /// Lists recent sessions for one agent, optionally filtered by type.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the query or row decoding fails.
    pub async fn list_agent_sessions(
        &self,
        agent_id: Uuid,
        limit: i64,
        session_type: Option<&str>,
    ) -> Result<Vec<AgentSession>, StoreError> {
        let rows = if let Some(session_type) = session_type.filter(|value| !value.is_empty()) {
            query(
                "SELECT id, agent_id, session_id, launch_id, channel_id, parent_session_id, \
                   end_reason, turn_count, input_tokens, output_tokens, cost_usd, context_window, \
                   session_type, scope, parent_chat_session_id, task_summary, files_changed, \
                   task_success, started_at, ended_at \
                 FROM agent_sessions WHERE agent_id = ? AND session_type = ? \
                 ORDER BY started_at DESC LIMIT ?",
            )
            .bind(agent_id)
            .bind(session_type)
            .bind(limit.clamp(1, 100))
            .fetch_all(&self.pool)
            .await?
        } else {
            query(
                "SELECT id, agent_id, session_id, launch_id, channel_id, parent_session_id, \
                   end_reason, turn_count, input_tokens, output_tokens, cost_usd, context_window, \
                   session_type, scope, parent_chat_session_id, task_summary, files_changed, \
                   task_success, started_at, ended_at \
                 FROM agent_sessions WHERE agent_id = ? \
                 ORDER BY started_at DESC LIMIT ?",
            )
            .bind(agent_id)
            .bind(limit.clamp(1, 100))
            .fetch_all(&self.pool)
            .await?
        };
        rows.into_iter().map(agent_session_from_row).collect()
    }

    /// Returns the newest active runtime session for one agent.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the query or row decoding fails.
    pub async fn current_agent_session(
        &self,
        agent_id: Uuid,
    ) -> Result<Option<AgentSession>, StoreError> {
        let row = query(
            "SELECT id, agent_id, session_id, launch_id, channel_id, parent_session_id, \
               end_reason, turn_count, input_tokens, output_tokens, cost_usd, context_window, \
               session_type, scope, parent_chat_session_id, task_summary, files_changed, \
               task_success, started_at, ended_at \
             FROM agent_sessions WHERE agent_id = ? AND ended_at IS NULL \
             ORDER BY started_at DESC LIMIT 1",
        )
        .bind(agent_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(agent_session_from_row).transpose()
    }

    /// Inserts or replaces one completed turn and refreshes its session aggregates.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when serialization or the transaction fails.
    pub async fn upsert_agent_turn(&self, turn: &NewAgentTurn) -> Result<AgentTurn, StoreError> {
        let id = Uuid::new_v4();
        let entries = serde_json::to_string(&turn.entries)?;
        let mut transaction = self.pool.begin().await?;
        query(
            "INSERT INTO agent_turns \
             (id, agent_id, session_id, launch_id, turn_number, started_at, ended_at, \
              input_tokens, output_tokens, cost_usd, context_window, entries, session_type, \
              channel_id, source_message_id) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(agent_id, launch_id, turn_number) DO UPDATE SET \
               session_id = excluded.session_id, started_at = excluded.started_at, \
               ended_at = excluded.ended_at, input_tokens = excluded.input_tokens, \
               output_tokens = excluded.output_tokens, cost_usd = excluded.cost_usd, \
               context_window = excluded.context_window, entries = excluded.entries, \
               session_type = excluded.session_type, channel_id = excluded.channel_id, \
               source_message_id = excluded.source_message_id",
        )
        .bind(id)
        .bind(turn.agent_id)
        .bind(&turn.session_id)
        .bind(&turn.launch_id)
        .bind(turn.turn_number)
        .bind(turn.started_at)
        .bind(turn.ended_at)
        .bind(turn.input_tokens)
        .bind(turn.output_tokens)
        .bind(turn.cost_usd)
        .bind(turn.context_window)
        .bind(entries)
        .bind(normalized_session_type(&turn.session_type))
        .bind(turn.channel_id)
        .bind(turn.source_message_id)
        .execute(&mut *transaction)
        .await?;
        if let Some(launch_id) = turn.launch_id.as_deref() {
            query(
                "UPDATE agent_sessions SET \
                   turn_count = (SELECT COUNT(*) FROM agent_turns \
                     WHERE agent_id = ? AND launch_id = ?), \
                   input_tokens = (SELECT COALESCE(SUM(input_tokens), 0) FROM agent_turns \
                     WHERE agent_id = ? AND launch_id = ?), \
                   output_tokens = (SELECT COALESCE(SUM(output_tokens), 0) FROM agent_turns \
                     WHERE agent_id = ? AND launch_id = ?), \
                   cost_usd = (SELECT COALESCE(SUM(cost_usd), 0) FROM agent_turns \
                     WHERE agent_id = ? AND launch_id = ?), \
                   context_window = (SELECT COALESCE(MAX(context_window), 0) FROM agent_turns \
                     WHERE agent_id = ? AND launch_id = ?) \
                 WHERE id = (SELECT id FROM agent_sessions \
                   WHERE agent_id = ? AND launch_id = ? ORDER BY started_at DESC LIMIT 1)",
            )
            .bind(turn.agent_id)
            .bind(launch_id)
            .bind(turn.agent_id)
            .bind(launch_id)
            .bind(turn.agent_id)
            .bind(launch_id)
            .bind(turn.agent_id)
            .bind(launch_id)
            .bind(turn.agent_id)
            .bind(launch_id)
            .bind(turn.agent_id)
            .bind(launch_id)
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        self.get_agent_turn_by_generation(
            turn.agent_id,
            turn.launch_id.as_deref(),
            turn.turn_number,
        )
        .await?
        .ok_or(StoreError::InvalidValue {
            kind: "agent turn",
            value: format!("{}:{}", turn.agent_id, turn.turn_number),
        })
    }

    /// Lists an agent's turns, optionally filtered by runtime session id.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the query or row decoding fails.
    pub async fn list_agent_turns(
        &self,
        agent_id: Uuid,
        session_id: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<AgentTurn>, StoreError> {
        let rows = if let Some(session_id) = session_id.filter(|value| !value.is_empty()) {
            let statement = agent_turn_select(
                "WHERE turn.agent_id = ? AND turn.session_id = ? \
                 ORDER BY turn.turn_number ASC LIMIT ? OFFSET ?",
            );
            query(&statement)
                .bind(agent_id)
                .bind(session_id)
                .bind(limit.clamp(1, 200))
                .bind(offset.max(0))
                .fetch_all(&self.pool)
                .await?
        } else {
            let statement = agent_turn_select(
                "WHERE turn.agent_id = ? ORDER BY turn.started_at DESC LIMIT ? OFFSET ?",
            );
            query(&statement)
                .bind(agent_id)
                .bind(limit.clamp(1, 200))
                .bind(offset.max(0))
                .fetch_all(&self.pool)
                .await?
        };
        rows.into_iter().map(agent_turn_from_row).collect()
    }

    /// Returns one turn owned by an agent.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the query or row decoding fails.
    pub async fn get_agent_turn(
        &self,
        agent_id: Uuid,
        turn_id: Uuid,
    ) -> Result<Option<AgentTurn>, StoreError> {
        let statement = agent_turn_select("WHERE turn.agent_id = ? AND turn.id = ? LIMIT 1");
        let row = query(&statement)
            .bind(agent_id)
            .bind(turn_id)
            .fetch_optional(&self.pool)
            .await?;
        row.map(agent_turn_from_row).transpose()
    }

    /// Inserts one runtime activity marker.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when serialization or insertion fails.
    #[allow(clippy::too_many_arguments)]
    pub async fn insert_agent_activity(
        &self,
        agent_id: Uuid,
        session_row_id: Option<Uuid>,
        session_id: Option<&str>,
        activity: &str,
        detail: Option<&str>,
        trajectory: &[String],
        launch_id: Option<&str>,
        created_at: DateTime<Utc>,
    ) -> Result<AgentActivity, StoreError> {
        let id = Uuid::new_v4();
        query(
            "INSERT INTO agent_activity \
             (id, agent_id, session_row_id, session_id, activity, detail, trajectory, \
              launch_id, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(agent_id)
        .bind(session_row_id)
        .bind(session_id)
        .bind(activity)
        .bind(detail)
        .bind(serde_json::to_string(trajectory)?)
        .bind(launch_id)
        .bind(created_at)
        .execute(&self.pool)
        .await?;
        Ok(AgentActivity {
            id,
            agent_id,
            activity: activity.to_owned(),
            detail: detail.map(str::to_owned),
            trajectory: trajectory.to_vec(),
            launch_id: launch_id.map(str::to_owned),
            created_at,
            session_row_id,
            session_id: session_id.map(str::to_owned),
        })
    }

    /// Lists recent activity for one agent.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the query or row decoding fails.
    pub async fn list_agent_activity(
        &self,
        agent_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<AgentActivity>, StoreError> {
        let rows = query(
            "SELECT id, agent_id, activity, detail, trajectory, launch_id, created_at, \
               session_row_id, session_id FROM agent_activity \
             WHERE agent_id = ? ORDER BY created_at DESC LIMIT ? OFFSET ?",
        )
        .bind(agent_id)
        .bind(limit.clamp(1, 100))
        .bind(offset.max(0))
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(agent_activity_from_row).collect()
    }

    async fn get_delivery(&self, delivery_id: Uuid) -> Result<Option<Delivery>, StoreError> {
        let row = query(
            "SELECT id, agent_id, channel_id, message_id, seq, state, attempts, \
             next_attempt_at, last_error, created_at, updated_at \
             FROM delivery_queue WHERE id = ?",
        )
        .bind(delivery_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(delivery_row_from_sqlite)
            .transpose()?
            .map(Delivery::try_from)
            .transpose()
    }

    async fn get_task_by_id(&self, task_id: Uuid) -> Result<Option<Task>, StoreError> {
        let row = query(
            "SELECT t.id, t.channel_id, t.message_id, t.task_number, t.title, \
             t.status, t.progress, t.assignee_id, a.name AS assignee_name, \
             t.created_by_agent_id, t.created_at, t.updated_at \
             FROM tasks t LEFT JOIN agents a ON a.id = t.assignee_id \
             WHERE t.id = ?",
        )
        .bind(task_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(task_row_from_sqlite)
            .transpose()?
            .map(Task::try_from)
            .transpose()
    }

    async fn get_agent_turn_by_generation(
        &self,
        agent_id: Uuid,
        launch_id: Option<&str>,
        turn_number: i64,
    ) -> Result<Option<AgentTurn>, StoreError> {
        let statement = agent_turn_select(
            "WHERE turn.agent_id = ? AND turn.launch_id IS ? AND turn.turn_number = ? LIMIT 1",
        );
        let row = query(&statement)
            .bind(agent_id)
            .bind(launch_id)
            .bind(turn_number)
            .fetch_optional(&self.pool)
            .await?;
        row.map(agent_turn_from_row).transpose()
    }
}

async fn apply_schema(pool: &SqlitePool) -> Result<(), sqlx_core::Error> {
    for migration in [
        include_str!("../migrations/0001_local_loop.sql"),
        include_str!("../migrations/0002_delivery_queue.sql"),
        include_str!("../migrations/0003_agent_bridge_state.sql"),
        include_str!("../migrations/0004_tasks.sql"),
        include_str!("../migrations/0005_runtime_history.sql"),
        include_str!("../migrations/0006_wiki.sql"),
    ] {
        for statement in migration.split(';') {
            let statement = statement.trim();
            if !statement.is_empty() {
                query(statement).execute(pool).await?;
            }
        }
    }
    Ok(())
}

fn channel_from_row(row: SqliteRow) -> Result<Channel, StoreError> {
    Ok(Channel {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        created_at: row.try_get("created_at")?,
    })
}

fn agent_row_from_sqlite(row: SqliteRow) -> Result<AgentRow, StoreError> {
    Ok(AgentRow {
        id: row.try_get("id")?,
        channel_id: row.try_get("channel_id")?,
        name: row.try_get("name")?,
        runtime: row.try_get("runtime")?,
        model: row.try_get("model")?,
        status: row.try_get("status")?,
        created_at: row.try_get("created_at")?,
    })
}

fn message_row_from_sqlite(row: SqliteRow) -> Result<MessageRow, StoreError> {
    Ok(MessageRow {
        id: row.try_get("id")?,
        channel_id: row.try_get("channel_id")?,
        seq: row.try_get("seq")?,
        agent_id: row.try_get("agent_id")?,
        role: row.try_get("role")?,
        content: row.try_get("content")?,
        created_at: row.try_get("created_at")?,
    })
}

fn delivery_row_from_sqlite(row: SqliteRow) -> Result<DeliveryRow, StoreError> {
    Ok(DeliveryRow {
        id: row.try_get("id")?,
        agent_id: row.try_get("agent_id")?,
        channel_id: row.try_get("channel_id")?,
        message_id: row.try_get("message_id")?,
        seq: row.try_get("seq")?,
        state: row.try_get("state")?,
        attempts: row.try_get("attempts")?,
        next_attempt_at: row.try_get("next_attempt_at")?,
        last_error: row.try_get("last_error")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn working_state_from_row(row: SqliteRow) -> Result<WorkingState, StoreError> {
    Ok(WorkingState {
        agent_id: row.try_get("agent_id")?,
        summary: row.try_get("summary")?,
        channel_name: row.try_get("channel_name")?,
        task_number: row.try_get("task_number")?,
        next_step_hint: row.try_get("next_step_hint")?,
        started_at: row.try_get("started_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn task_row_from_sqlite(row: SqliteRow) -> Result<TaskRow, StoreError> {
    Ok(TaskRow {
        id: row.try_get("id")?,
        channel_id: row.try_get("channel_id")?,
        message_id: row.try_get("message_id")?,
        task_number: row.try_get("task_number")?,
        title: row.try_get("title")?,
        status: row.try_get("status")?,
        progress: row.try_get("progress")?,
        assignee_id: row.try_get("assignee_id")?,
        assignee_name: row.try_get("assignee_name")?,
        created_by_agent_id: row.try_get("created_by_agent_id")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn agent_session_from_row(row: SqliteRow) -> Result<AgentSession, StoreError> {
    let files_changed = row
        .try_get::<Option<String>, _>("files_changed")?
        .map(|value| serde_json::from_str(&value))
        .transpose()?;
    Ok(AgentSession {
        id: row.try_get("id")?,
        agent_id: row.try_get("agent_id")?,
        session_id: row.try_get("session_id")?,
        launch_id: row.try_get("launch_id")?,
        channel_id: row.try_get("channel_id")?,
        parent_session_id: row.try_get("parent_session_id")?,
        end_reason: row.try_get("end_reason")?,
        turn_count: row.try_get("turn_count")?,
        input_tokens: row.try_get("input_tokens")?,
        output_tokens: row.try_get("output_tokens")?,
        cost_usd: row.try_get("cost_usd")?,
        context_window: row.try_get("context_window")?,
        session_type: row.try_get("session_type")?,
        scope: row.try_get("scope")?,
        parent_chat_session_id: row.try_get("parent_chat_session_id")?,
        task_summary: row.try_get("task_summary")?,
        files_changed,
        task_success: row
            .try_get::<Option<i64>, _>("task_success")?
            .map(|value| value != 0),
        started_at: row.try_get("started_at")?,
        ended_at: row.try_get("ended_at")?,
    })
}

fn agent_turn_select(suffix: &str) -> String {
    format!(
        "SELECT turn.id, turn.agent_id, turn.session_id, turn.launch_id, turn.turn_number, \
           turn.started_at, turn.ended_at, turn.input_tokens, turn.output_tokens, turn.cost_usd, \
           turn.context_window, turn.entries, turn.session_type, turn.channel_id, \
           turn.source_message_id, message.seq AS message_seq, message.created_at AS message_created_at \
         FROM agent_turns turn LEFT JOIN messages message ON message.id = turn.source_message_id \
         {suffix}"
    )
}

fn agent_turn_from_row(row: SqliteRow) -> Result<AgentTurn, StoreError> {
    let started_at: DateTime<Utc> = row.try_get("started_at")?;
    let ended_at: Option<DateTime<Utc>> = row.try_get("ended_at")?;
    let channel_id: Option<Uuid> = row.try_get("channel_id")?;
    let source_message_id: Option<Uuid> = row.try_get("source_message_id")?;
    let message_ref = channel_id
        .zip(source_message_id)
        .map(|(channel_id, message_id)| TurnMessageRef {
            channel_id,
            message_id,
            seq: row.try_get("message_seq").ok().flatten(),
            created_at: row.try_get("message_created_at").ok().flatten(),
        });
    Ok(AgentTurn {
        id: row.try_get("id")?,
        agent_id: row.try_get("agent_id")?,
        session_id: row.try_get("session_id")?,
        launch_id: row.try_get("launch_id")?,
        turn_number: row.try_get("turn_number")?,
        started_at,
        ended_at,
        input_tokens: row.try_get("input_tokens")?,
        output_tokens: row.try_get("output_tokens")?,
        cost_usd: row.try_get("cost_usd")?,
        context_window: row.try_get("context_window")?,
        entries: serde_json::from_str(&row.try_get::<String, _>("entries")?)?,
        session_type: row.try_get("session_type")?,
        duration_ms: ended_at.map(|ended_at| {
            ended_at
                .signed_duration_since(started_at)
                .num_milliseconds()
                .max(0)
        }),
        message_ref,
    })
}

fn agent_activity_from_row(row: SqliteRow) -> Result<AgentActivity, StoreError> {
    Ok(AgentActivity {
        id: row.try_get("id")?,
        agent_id: row.try_get("agent_id")?,
        activity: row.try_get("activity")?,
        detail: row.try_get("detail")?,
        trajectory: serde_json::from_str(&row.try_get::<String, _>("trajectory")?)?,
        launch_id: row.try_get("launch_id")?,
        created_at: row.try_get("created_at")?,
        session_row_id: row.try_get("session_row_id")?,
        session_id: row.try_get("session_id")?,
    })
}

fn normalized_session_type(session_type: &str) -> &str {
    let session_type = session_type.trim();
    if session_type.is_empty() {
        "chat"
    } else {
        session_type
    }
}

async fn upsert_inbox_cursor(
    transaction: &mut sqlx_core::transaction::Transaction<'_, sqlx_sqlite::Sqlite>,
    agent_id: Uuid,
    seq: i64,
    now: DateTime<Utc>,
) -> Result<(), sqlx_core::Error> {
    query(
        "INSERT INTO agent_inbox_state (agent_id, last_read_seq, updated_at) \
         VALUES (?, ?, ?) \
         ON CONFLICT(agent_id) DO UPDATE SET \
           last_read_seq = MAX(agent_inbox_state.last_read_seq, excluded.last_read_seq), \
           updated_at = excluded.updated_at",
    )
    .bind(agent_id)
    .bind(seq)
    .bind(now)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}
