//! Server → Daemon wire messages.
//!
//! Source-of-truth: `internal/protocol/daemon_msg.go` (S→D types in §"Server -> Daemon messages").
//!
//! Phase 0a coverage (spec §4.1):
//! - `agent:start`, `agent:stop`, `agent:deliver`, `agent:turn:cancel`,
//!   `agent:recover-sessions`, `ping`, `server:shutdown`
//!
//! All other wire types are routed to `ServerMsg::Unknown` and the conn
//! layer logs a `tracing::warn!` instead of erroring (spec §4.5).

use serde::{Deserialize, Serialize};

use crate::types::{i32_is_zero, i64_is_zero, AgentConfig, DeliveryMessage, RecoverSession};

/// Sum type for every server → daemon wire message. Variant is selected by
/// the JSON `type` field (`#[serde(tag = "type")]`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
#[allow(clippy::large_enum_variant)]
pub enum ServerMsg {
    #[serde(rename = "agent:start")]
    AgentStart(AgentStartMsg),
    #[serde(rename = "agent:stop")]
    AgentStop(AgentStopMsg),
    #[serde(rename = "agent:deliver")]
    AgentDeliver(AgentDeliverMsg),
    #[serde(rename = "agent:turn:cancel")]
    AgentTurnCancel(AgentTurnCancelMsg),
    #[serde(rename = "agent:recover-sessions")]
    AgentRecoverSessions(AgentRecoverSessionsMsg),
    #[serde(rename = "ping")]
    Ping(PingMsg),
    #[serde(rename = "server:shutdown")]
    ServerShutdown(ServerShutdownMsg),

    // Phase 0b: workspace ops (FPC #14 + #15)
    #[serde(rename = "agent:workspace:list")]
    AgentWorkspaceList(AgentWorkspaceListMsg),
    #[serde(rename = "agent:workspace:read")]
    AgentWorkspaceRead(AgentWorkspaceReadMsg),
    #[serde(rename = "agent:reset-workspace")]
    AgentResetWorkspace(AgentResetWorkspaceMsg),

    // Phase 0b: working-memory ops (FPC #16)
    #[serde(rename = "agent:working:set")]
    AgentWorkingSet(AgentWorkingSetMsg),
    #[serde(rename = "agent:working:get")]
    AgentWorkingGet(AgentWorkingGetMsg),
    #[serde(rename = "agent:working:clear")]
    AgentWorkingClear(AgentWorkingClearMsg),

    /// Catch-all for unimplemented wire types. The conn layer logs a
    /// warning instead of propagating an error (spec §4.5).
    #[serde(other)]
    Unknown,
}

// ----------------------------------------------------------------------------
// agent:start — `daemon_msg.go:93`
// ----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentStartMsg {
    #[serde(rename = "agentId")]
    pub agent_id: String,
    pub config: AgentConfig,
    #[serde(
        default,
        rename = "wakeMessage",
        skip_serializing_if = "Option::is_none"
    )]
    pub wake_message: Option<DeliveryMessage>,
    #[serde(
        default,
        rename = "unreadSummary",
        skip_serializing_if = "Option::is_none"
    )]
    pub unread_summary: Option<std::collections::HashMap<String, i64>>,
    #[serde(
        default,
        rename = "resumePrompt",
        skip_serializing_if = "String::is_empty"
    )]
    pub resume_prompt: String,
    #[serde(default, rename = "launchId", skip_serializing_if = "String::is_empty")]
    pub launch_id: String,
    #[serde(
        default,
        rename = "turnCount",
        skip_serializing_if = "i32_is_zero"
    )]
    pub turn_count: i32,
}

// ----------------------------------------------------------------------------
// agent:stop — `daemon_msg.go:104`
// ----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentStopMsg {
    #[serde(rename = "agentId")]
    pub agent_id: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub force: bool,
}

// ----------------------------------------------------------------------------
// agent:deliver — `daemon_msg.go:177`
// ----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentDeliverMsg {
    #[serde(rename = "agentId")]
    pub agent_id: String,
    pub seq: i64,
    #[serde(default, skip_serializing_if = "i64_is_zero")]
    pub attempt: i64,
    #[serde(
        default,
        rename = "priorityClass",
        skip_serializing_if = "String::is_empty"
    )]
    pub priority_class: String,
    #[serde(
        default,
        rename = "priorityReason",
        skip_serializing_if = "String::is_empty"
    )]
    pub priority_reason: String,
    #[serde(
        default,
        rename = "prioritySource",
        skip_serializing_if = "String::is_empty"
    )]
    pub priority_source: String,
    #[serde(
        default,
        rename = "responderRole",
        skip_serializing_if = "String::is_empty"
    )]
    pub responder_role: String,
    #[serde(
        default,
        rename = "deliveryTier",
        skip_serializing_if = "String::is_empty"
    )]
    pub delivery_tier: String,
    #[serde(
        default,
        rename = "routingReason",
        skip_serializing_if = "String::is_empty"
    )]
    pub routing_reason: String,
    pub message: DeliveryMessage,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context: Vec<DeliveryMessage>,
}

// ----------------------------------------------------------------------------
// agent:turn:cancel — `daemon_msg.go:110`
// ----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentTurnCancelMsg {
    #[serde(rename = "agentId")]
    pub agent_id: String,
}

// ----------------------------------------------------------------------------
// agent:recover-sessions — `daemon_msg.go:702`
// ----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentRecoverSessionsMsg {
    #[serde(default, deserialize_with = "crate::types::deserialize_null_default")]
    pub sessions: Vec<RecoverSession>,
}

// ----------------------------------------------------------------------------
// ping — `daemon_msg.go:211`
// ----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PingMsg {}

// ----------------------------------------------------------------------------
// server:shutdown — `daemon_msg.go:708`
// ----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ServerShutdownMsg {
    /// "deploy" | "restart" | …
    pub reason: String,
}

// ============================================================================
// Phase 0b — workspace ops (FPC #14 + #15)
// ============================================================================

/// `agent:workspace:list` — request a directory listing under the agent's workspace.
/// `dir_path` is relative to the workspace root; empty means root.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentWorkspaceListMsg {
    #[serde(rename = "agentId")]
    pub agent_id: String,
    #[serde(default, rename = "dirPath", skip_serializing_if = "String::is_empty")]
    pub dir_path: String,
    #[serde(rename = "requestId")]
    pub request_id: String,
}

/// `agent:workspace:read` — request the contents of a file under the agent's workspace.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentWorkspaceReadMsg {
    #[serde(rename = "agentId")]
    pub agent_id: String,
    pub path: String,
    #[serde(rename = "requestId")]
    pub request_id: String,
}

/// `agent:reset-workspace` — clear all files under the agent's workspace dir
/// (preserves the dir itself).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentResetWorkspaceMsg {
    #[serde(rename = "agentId")]
    pub agent_id: String,
}

// ============================================================================
// Phase 0b — working-memory ops (FPC #16)
// ============================================================================

/// `agent:working:set` — set the agent's CurrentWork anchor.
/// Mirrors Go `AgentWorkingSetMsg` in `internal/protocol/daemon_msg.go`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentWorkingSetMsg {
    #[serde(rename = "agentId")]
    pub agent_id: String,
    #[serde(rename = "requestId")]
    pub request_id: String,
    #[serde(default, rename = "taskId", skip_serializing_if = "Option::is_none")]
    pub task_id: Option<uuid::Uuid>,
    #[serde(default, rename = "taskNumber", skip_serializing_if = "crate::types::i64_is_zero")]
    pub task_number: i64,
    #[serde(default, rename = "channelName", skip_serializing_if = "String::is_empty")]
    pub channel_name: String,
    pub summary: String,
    #[serde(default, rename = "nextStepHint", skip_serializing_if = "String::is_empty")]
    pub next_step_hint: String,
}

/// `agent:working:get` — return the agent's CurrentWork anchor (or nil if unset).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentWorkingGetMsg {
    #[serde(rename = "agentId")]
    pub agent_id: String,
    #[serde(rename = "requestId")]
    pub request_id: String,
}

/// `agent:working:clear` — clear the agent's CurrentWork anchor (idempotent).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentWorkingClearMsg {
    #[serde(rename = "agentId")]
    pub agent_id: String,
    #[serde(rename = "requestId")]
    pub request_id: String,
}
