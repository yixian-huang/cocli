//! Shared domain types embedded in wire messages.
//!
//! Source-of-truth: `internal/types/types.go` + `internal/protocol/daemon_msg.go`.
//! Only Phase 0a-relevant fields are ported; lazily extend as roundtrip tests
//! against canonical Go JSON fixtures fail.

use serde::{Deserialize, Deserializer, Serialize};
use uuid::Uuid;

/// serde deserializer that treats explicit `null` as `Default::default()`.
/// Go marshals empty slices as `null` in some paths (e.g. `Sessions: nil`),
/// which Rust's default Vec deser rejects. Use with `#[serde(default, deserialize_with = "...")]`.
pub fn deserialize_null_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    T: Default + Deserialize<'de>,
    D: Deserializer<'de>,
{
    let opt = Option::<T>::deserialize(deserializer)?;
    Ok(opt.unwrap_or_default())
}

/// `true` when the value equals i64 zero. Used by `skip_serializing_if`
/// to mimic Go's `omitempty` on `int64` fields.
#[allow(clippy::trivially_copy_pass_by_ref)]
pub fn i64_is_zero(v: &i64) -> bool {
    *v == 0
}

/// `true` when the value equals i32 zero. Used by `skip_serializing_if`.
#[allow(clippy::trivially_copy_pass_by_ref)]
pub fn i32_is_zero(v: &i32) -> bool {
    *v == 0
}

/// `true` when the value equals f64 zero.
#[allow(clippy::trivially_copy_pass_by_ref)]
pub fn f64_is_zero(v: &f64) -> bool {
    *v == 0.0
}

/// `true` when the value equals u64 zero.
#[allow(clippy::trivially_copy_pass_by_ref)]
pub fn u64_is_zero(v: &u64) -> bool {
    *v == 0
}

// ----------------------------------------------------------------------------
// AgentConfig — `internal/types/types.go:536`
// ----------------------------------------------------------------------------

/// AgentConfig is the per-agent configuration embedded in `AgentStartMsg`
/// and `RecoverSession`. Mirrors Go `types.AgentConfig`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentConfig {
    pub runtime: String,
    pub model: String,
    #[serde(
        default,
        rename = "workingRuntime",
        skip_serializing_if = "String::is_empty"
    )]
    pub working_runtime: String,
    #[serde(
        default,
        rename = "workingModel",
        skip_serializing_if = "String::is_empty"
    )]
    pub working_model: String,
    #[serde(
        default,
        rename = "chatOnly",
        skip_serializing_if = "std::ops::Not::not"
    )]
    pub chat_only: bool,
    #[serde(
        default,
        rename = "sessionId",
        skip_serializing_if = "String::is_empty"
    )]
    pub session_id: String,
    #[serde(rename = "serverUrl")]
    pub server_url: String,
    #[serde(
        default,
        rename = "authToken",
        skip_serializing_if = "String::is_empty"
    )]
    pub auth_token: String,
    #[serde(default, rename = "envVars", skip_serializing_if = "Option::is_none")]
    pub env_vars: Option<std::collections::HashMap<String, String>>,
    pub name: String,
    #[serde(
        default,
        rename = "displayName",
        skip_serializing_if = "String::is_empty"
    )]
    pub display_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub onboarding: String,
}

// ----------------------------------------------------------------------------
// DeliveryMessage — `internal/types/types.go:553`
// ----------------------------------------------------------------------------

/// DeliveryMessage is the payload delivered to an agent. The Go struct
/// uses snake_case JSON tags throughout — preserved here verbatim.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeliveryMessage {
    #[serde(rename = "channel_id")]
    pub channel_id: Uuid,
    #[serde(rename = "sender_name")]
    pub sender_name: String,
    #[serde(rename = "sender_type")]
    pub sender_type: String,
    pub content: String,
    #[serde(rename = "channel_name")]
    pub channel_name: String,
    #[serde(
        default,
        rename = "channel_display_name",
        skip_serializing_if = "String::is_empty"
    )]
    pub channel_display_name: String,
    #[serde(rename = "channel_type")]
    pub channel_type: String,
    #[serde(
        default,
        rename = "parent_channel_name",
        skip_serializing_if = "String::is_empty"
    )]
    pub parent_channel_name: String,
    #[serde(
        default,
        rename = "parent_channel_type",
        skip_serializing_if = "String::is_empty"
    )]
    pub parent_channel_type: String,
    #[serde(default, skip_serializing_if = "i64_is_zero")]
    pub seq: i64,
    #[serde(rename = "message_id")]
    pub message_id: String,
    pub timestamp: String,
    #[serde(
        default,
        rename = "createdAtUnixNano",
        skip_serializing_if = "i64_is_zero"
    )]
    pub created_at_unix_nano: i64,
    #[serde(
        default,
        rename = "task_status",
        skip_serializing_if = "String::is_empty"
    )]
    pub task_status: String,
    #[serde(default, rename = "task_number", skip_serializing_if = "i32_is_zero")]
    pub task_number: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub blocks: String,
    #[serde(
        rename = "sourceMessageId",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub source_message_id: String,
    #[serde(
        rename = "expectedReplyTarget",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub expected_reply_target: String,
    #[serde(
        rename = "requiresThread",
        default,
        skip_serializing_if = "std::ops::Not::not"
    )]
    pub requires_thread: bool,
    #[serde(
        rename = "replyObligationId",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub reply_obligation_id: String,
    #[serde(
        rename = "replyRequired",
        default,
        skip_serializing_if = "std::ops::Not::not"
    )]
    pub reply_required: bool,
    #[serde(
        rename = "replyOwnerAgentId",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub reply_owner_agent_id: String,
    #[serde(
        rename = "replyDeadlineAt",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub reply_deadline_at: String,
    #[serde(
        rename = "recentContext",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub recent_context: Vec<DeliveryMessage>,
    #[serde(
        rename = "priorityClass",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub priority_class: String,
    #[serde(
        rename = "priorityReason",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub priority_reason: String,
    #[serde(
        rename = "relevanceClass",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub relevance_class: String,
}

// ----------------------------------------------------------------------------
// RecoverSession + ChannelSession — `internal/protocol/daemon_msg.go:683-697`
// ----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChannelSession {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "channelId")]
    pub channel_id: Uuid,
    #[serde(rename = "sessionType")]
    pub session_type: String,
    pub scope: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RecoverSession {
    #[serde(rename = "agentId")]
    pub agent_id: String,
    pub config: AgentConfig,
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(default, rename = "launchId", skip_serializing_if = "String::is_empty")]
    pub launch_id: String,
    #[serde(default, rename = "turnCount", skip_serializing_if = "i32_is_zero")]
    pub turn_count: i32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sessions: Vec<ChannelSession>,
}

// ----------------------------------------------------------------------------
// TrajectoryEntry — `internal/protocol/daemon_msg.go:306`
// ----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TrajectoryEntry {
    /// "thinking" | "text" | "tool_call" | "tool_result" | "status" | "warning" | "error"
    pub kind: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub result: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub error: String,
    #[serde(default, skip_serializing_if = "i64_is_zero")]
    pub ts: i64,
}

// ----------------------------------------------------------------------------
// FileTreeEntry — `internal/protocol/daemon_msg.go:433`
// ----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FileTreeEntry {
    pub name: String,
    #[serde(rename = "isDir")]
    pub is_dir: bool,
    #[serde(default, skip_serializing_if = "i64_is_zero")]
    pub size: i64,
}

// ----------------------------------------------------------------------------
// RuntimeModel — `internal/protocol/daemon_msg.go:217`
// ----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RuntimeModel {
    pub id: String,
    pub label: String,
}

// ----------------------------------------------------------------------------
// WorkingStatePayload — `internal/protocol/daemon_msg.go:481`
// ----------------------------------------------------------------------------

/// Over-the-wire shape of an agent's CurrentWork anchor (FPC #16). Timestamps
/// are RFC3339Nano strings on the wire (Go-side rationale: preserves sub-second
/// resolution after JSON round-trip).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkingStatePayload {
    #[serde(default, rename = "taskId", skip_serializing_if = "Option::is_none")]
    pub task_id: Option<Uuid>,
    #[serde(default, rename = "taskNumber", skip_serializing_if = "i64_is_zero")]
    pub task_number: i64,
    #[serde(
        default,
        rename = "channelName",
        skip_serializing_if = "String::is_empty"
    )]
    pub channel_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub summary: String,
    #[serde(
        default,
        rename = "nextStepHint",
        skip_serializing_if = "String::is_empty"
    )]
    pub next_step_hint: String,
    #[serde(
        default,
        rename = "startedAt",
        skip_serializing_if = "String::is_empty"
    )]
    pub started_at: String,
    #[serde(
        default,
        rename = "lastUpdatedAt",
        skip_serializing_if = "String::is_empty"
    )]
    pub last_updated_at: String,
}
