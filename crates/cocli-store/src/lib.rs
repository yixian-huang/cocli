//! SQLite-backed persistence for cocli local.

use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx_core::query::query;
use sqlx_core::query_scalar::query_scalar;
use sqlx_core::row::Row;
use sqlx_sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions, SqliteRow};
use uuid::Uuid;

/// Errors returned by the local SQLite store.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// A database query or connection failed.
    #[error(transparent)]
    Sqlx(#[from] sqlx_core::Error),
    /// A persisted enum value was not recognized.
    #[error("invalid persisted {kind} value: {value}")]
    InvalidValue {
        /// The domain field being decoded.
        kind: &'static str,
        /// The unexpected persisted value.
        value: String,
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

struct MessageRow {
    id: Uuid,
    channel_id: Uuid,
    seq: i64,
    agent_id: Option<Uuid>,
    role: String,
    content: String,
    created_at: DateTime<Utc>,
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
}

async fn apply_schema(pool: &SqlitePool) -> Result<(), sqlx_core::Error> {
    for statement in include_str!("../migrations/0001_local_loop.sql").split(';') {
        let statement = statement.trim();
        if !statement.is_empty() {
            query(statement).execute(pool).await?;
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
