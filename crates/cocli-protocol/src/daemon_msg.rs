//! Daemon → Server wire messages.
//!
//! Source-of-truth: `internal/protocol/daemon_msg.go` (D→S types in §"Daemon -> Server messages").
//!
//! Phase 0a coverage (spec §4.1):
//! - `ready`, `pong`, `daemon:recover`, `agent:status`, `agent:activity`,
//!   `agent:session`, `agent:session:end`, `agent:session:idle`,
//!   `agent:deliver:ack`, `agent:deliver:accepted`, `agent:stop:error`,
//!   `agent:turn`, `agent:recovery:record`

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::types::{
    f64_is_zero, i32_is_zero, i64_is_zero, u64_is_zero, RuntimeModel, TrajectoryEntry,
};

/// Sum type for every daemon → server wire message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
#[allow(clippy::large_enum_variant)]
pub enum DaemonMsg {
    #[serde(rename = "ready")]
    Ready(ReadyMsg),
    #[serde(rename = "pong")]
    Pong(PongMsg),
    #[serde(rename = "daemon:recover")]
    DaemonRecover(DaemonRecoverMsg),
    #[serde(rename = "agent:status")]
    AgentStatus(AgentStatusMsg),
    #[serde(rename = "agent:activity")]
    AgentActivity(AgentActivityMsg),
    #[serde(rename = "agent:turn")]
    AgentTurn(AgentTurnMsg),
    #[serde(rename = "agent:session")]
    AgentSession(AgentSessionMsg),
    #[serde(rename = "agent:session:end")]
    AgentSessionEnd(AgentSessionEndMsg),
    #[serde(rename = "agent:session:idle")]
    AgentSessionIdle(AgentSessionIdleMsg),
    #[serde(rename = "agent:deliver:ack")]
    AgentDeliverAck(AgentDeliverAckMsg),
    #[serde(rename = "agent:deliver:accepted")]
    AgentDeliverAccepted(AgentDeliverAcceptedMsg),
    #[serde(rename = "agent:stop:error")]
    AgentStopError(AgentStopErrorMsg),
    #[serde(rename = "agent:recovery:record")]
    AgentRecoveryRecord(AgentRecoveryRecordMsg),

    // Phase 0b: workspace ops responses (FPC #14)
    #[serde(rename = "agent:workspace:file_tree")]
    AgentWorkspaceFileTree(AgentWorkspaceFileTreeMsg),
    #[serde(rename = "agent:workspace:file_content")]
    AgentWorkspaceFileContent(AgentWorkspaceFileContentMsg),

    // Phase 0b: working-memory op response (FPC #16)
    #[serde(rename = "agent:working:result")]
    AgentWorkingResult(AgentWorkingResultMsg),
}

// ----------------------------------------------------------------------------
// ready — `daemon_msg.go:222`
// ----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReadyMsg {
    pub capabilities: Vec<String>,
    pub runtimes: Vec<String>,
    /// runtime -> models
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub models: Option<std::collections::HashMap<String, Vec<RuntimeModel>>>,
    #[serde(rename = "runningAgents")]
    pub running_agents: Vec<String>,
    pub hostname: String,
    pub os: String,
    #[serde(rename = "daemonVersion")]
    pub daemon_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<serde_json::Value>,
}

// ----------------------------------------------------------------------------
// pong — `daemon_msg.go:578`
// ----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PongMsg {}

// ----------------------------------------------------------------------------
// daemon:recover — `daemon_msg.go:631`
// ----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DaemonRecoverMsg {}

// ----------------------------------------------------------------------------
// agent:status — `daemon_msg.go:245`
// Status enum: "active" | "inactive" | "error" (NOT "crashed" / "stopped")
// ----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentStatusMsg {
    #[serde(rename = "agentId")]
    pub agent_id: String,
    /// "active" | "inactive" | "error"
    pub status: String,
    #[serde(default, rename = "launchId", skip_serializing_if = "String::is_empty")]
    pub launch_id: String,
    #[serde(
        default,
        rename = "errorDetail",
        skip_serializing_if = "String::is_empty"
    )]
    pub error_detail: String,
}

// ----------------------------------------------------------------------------
// agent:stop:error — `daemon_msg.go:171`
// ----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentStopErrorMsg {
    #[serde(rename = "agentId")]
    pub agent_id: String,
    pub error: String,
}

// ----------------------------------------------------------------------------
// agent:deliver:ack — `daemon_msg.go:413`
// RouteAction enum: "inbox" | "tierDigest" | "tierDelayed" | "checkMessages"
// ----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentDeliverAckMsg {
    #[serde(rename = "agentId")]
    pub agent_id: String,
    pub seq: i64,
    #[serde(default, skip_serializing_if = "i64_is_zero")]
    pub attempt: i64,
    #[serde(rename = "channelId")]
    pub channel_id: Uuid,
    /// "inbox" | "tierDigest" | "tierDelayed" | "checkMessages"
    #[serde(rename = "routeAction")]
    pub route_action: String,
}

// ----------------------------------------------------------------------------
// agent:deliver:accepted — `daemon_msg.go:424`
//
// Shape is identical to AgentDeliverAckMsg, but keep as a separate struct
// so the `#[serde(tag = "type")]` enum dispatch in DaemonMsg stays clean.
// ----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentDeliverAcceptedMsg {
    #[serde(rename = "agentId")]
    pub agent_id: String,
    pub seq: i64,
    #[serde(default, skip_serializing_if = "i64_is_zero")]
    pub attempt: i64,
    #[serde(rename = "channelId")]
    pub channel_id: Uuid,
    #[serde(rename = "routeAction")]
    pub route_action: String,
}

// ----------------------------------------------------------------------------
// agent:session — `daemon_msg.go:316`
//
// NOTE: ChannelID field uses snake_case "channel_id" on the wire — this is
// an anomaly vs every other camelCase field in this struct; the Go tag is
// explicitly `json:"channel_id"`. Do not "fix" it.
// ----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentSessionMsg {
    #[serde(rename = "agentId")]
    pub agent_id: String,
    #[serde(rename = "sessionId")]
    pub session_id: String,
    /// Snake_case anomaly — preserved verbatim from Go.
    #[serde(rename = "channel_id")]
    pub channel_id: Uuid,
    #[serde(rename = "isNew")]
    pub is_new: bool,
    #[serde(rename = "resumedFrom")]
    pub resumed_from: String,
    #[serde(rename = "activeSessions")]
    pub active_sessions: i32,
    #[serde(rename = "queueDepth")]
    pub queue_depth: i32,
    #[serde(rename = "promptLayer")]
    pub prompt_layer: String,
    #[serde(rename = "promptTokens")]
    pub prompt_tokens: i32,
    #[serde(default, rename = "launchId", skip_serializing_if = "String::is_empty")]
    pub launch_id: String,
    #[serde(
        default,
        rename = "launchGeneration",
        skip_serializing_if = "u64_is_zero"
    )]
    pub launch_generation: u64,
    #[serde(
        default,
        rename = "sessionType",
        skip_serializing_if = "String::is_empty"
    )]
    pub session_type: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub scope: String,
    #[serde(
        default,
        rename = "parentChatSessionId",
        skip_serializing_if = "String::is_empty"
    )]
    pub parent_chat_session_id: String,
    #[serde(
        default,
        rename = "executionIntentId",
        skip_serializing_if = "String::is_empty"
    )]
    pub execution_intent_id: String,
    #[serde(
        default,
        rename = "executionRunId",
        skip_serializing_if = "String::is_empty"
    )]
    pub execution_run_id: String,
}

// ----------------------------------------------------------------------------
// agent:session:end — `daemon_msg.go:336`
//
// EndReason enum: "idle" | "context_reset" | "error" | "manual_stop"
// Field tag is `endReason` (NOT `reason`).
// ----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentSessionEndMsg {
    #[serde(rename = "agentId")]
    pub agent_id: String,
    #[serde(rename = "sessionId")]
    pub session_id: String,
    /// "idle" | "context_reset" | "error" | "manual_stop"
    #[serde(rename = "endReason")]
    pub end_reason: String,
    #[serde(rename = "turnCount")]
    pub turn_count: i32,
    #[serde(default, rename = "launchId", skip_serializing_if = "String::is_empty")]
    pub launch_id: String,
    #[serde(
        default,
        rename = "launchGeneration",
        skip_serializing_if = "u64_is_zero"
    )]
    pub launch_generation: u64,
    #[serde(
        default,
        rename = "parentSessionId",
        skip_serializing_if = "String::is_empty"
    )]
    pub parent_session_id: String,
    #[serde(default, rename = "inputTokens", skip_serializing_if = "i32_is_zero")]
    pub input_tokens: i32,
    #[serde(default, rename = "outputTokens", skip_serializing_if = "i32_is_zero")]
    pub output_tokens: i32,
    #[serde(default, rename = "costUsd", skip_serializing_if = "f64_is_zero")]
    pub cost_usd: f64,
    #[serde(default, rename = "contextWindow", skip_serializing_if = "i32_is_zero")]
    pub context_window: i32,
    #[serde(
        default,
        rename = "sessionType",
        skip_serializing_if = "String::is_empty"
    )]
    pub session_type: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub scope: String,
    #[serde(
        default,
        rename = "taskSummary",
        skip_serializing_if = "String::is_empty"
    )]
    pub task_summary: String,
    #[serde(
        default,
        rename = "filesChanged",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub files_changed: Vec<String>,
    #[serde(
        default,
        rename = "taskSuccess",
        skip_serializing_if = "Option::is_none"
    )]
    pub task_success: Option<bool>,
    #[serde(
        default,
        rename = "executionIntentId",
        skip_serializing_if = "String::is_empty"
    )]
    pub execution_intent_id: String,
    #[serde(
        default,
        rename = "executionRunId",
        skip_serializing_if = "String::is_empty"
    )]
    pub execution_run_id: String,
}

// ----------------------------------------------------------------------------
// agent:session:idle — `daemon_msg.go:361`
// ----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentSessionIdleMsg {
    #[serde(rename = "agentId")]
    pub agent_id: String,
    #[serde(rename = "channelId")]
    pub channel_id: Uuid,
    #[serde(rename = "channelName")]
    pub channel_name: String,
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "turnCount")]
    pub turn_count: i32,
    #[serde(rename = "totalCostUsd")]
    pub total_cost_usd: f64,
    /// Go field is `CacheTTL` but JSON tag is `cacheTtlSeconds`.
    #[serde(rename = "cacheTtlSeconds")]
    pub cache_ttl_seconds: i32,
    #[serde(rename = "activeSessions")]
    pub active_sessions: i32,
}

// ----------------------------------------------------------------------------
// agent:activity — `daemon_msg.go:253`
//
// Activity enum: "working" | "online" | "offline".
// AttentionState enum: "idle" | "working" | "focus" | "preempting" | "stalled".
// ----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentActivityMsg {
    #[serde(rename = "agentId")]
    pub agent_id: String,
    /// "working" | "online" | "offline"
    pub activity: String,
    #[serde(
        default,
        rename = "attentionState",
        skip_serializing_if = "String::is_empty"
    )]
    pub attention_state: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub detail: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<std::collections::HashMap<String, serde_json::Value>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trajectory: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<TrajectoryEntry>,
    #[serde(default, rename = "launchId", skip_serializing_if = "String::is_empty")]
    pub launch_id: String,
    #[serde(
        default,
        rename = "launchGeneration",
        skip_serializing_if = "u64_is_zero"
    )]
    pub launch_generation: u64,
    #[serde(
        default,
        rename = "focusTaskId",
        skip_serializing_if = "String::is_empty"
    )]
    pub focus_task_id: String,
    #[serde(
        default,
        rename = "focusScope",
        skip_serializing_if = "String::is_empty"
    )]
    pub focus_scope: String,
    #[serde(default, rename = "focusSince", skip_serializing_if = "i64_is_zero")]
    pub focus_since: i64,
    #[serde(
        default,
        rename = "priorityClass",
        skip_serializing_if = "String::is_empty"
    )]
    pub priority_class: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub preempted: bool,

    // Token usage — populated on turn_end activity messages.
    #[serde(
        default,
        rename = "lastInputTokens",
        skip_serializing_if = "i32_is_zero"
    )]
    pub last_input_tokens: i32,
    #[serde(
        default,
        rename = "totalOutputTokens",
        skip_serializing_if = "i32_is_zero"
    )]
    pub total_output_tokens: i32,
    #[serde(default, rename = "contextWindow", skip_serializing_if = "i32_is_zero")]
    pub context_window: i32,
    #[serde(default, rename = "totalCostUSD", skip_serializing_if = "f64_is_zero")]
    pub total_cost_usd: f64,
    #[serde(default, rename = "turnCount", skip_serializing_if = "i32_is_zero")]
    pub turn_count: i32,
    #[serde(
        default,
        rename = "sessionType",
        skip_serializing_if = "String::is_empty"
    )]
    pub session_type: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub scope: String,
    #[serde(rename = "channelId")]
    pub channel_id: Uuid,
    #[serde(rename = "channelName")]
    pub channel_name: String,

    // Rate limit fields — Phase 0a sends as zero/empty unless runtime emits.
    #[serde(
        default,
        rename = "rateLimitType",
        skip_serializing_if = "String::is_empty"
    )]
    pub rate_limit_type: String,
    #[serde(
        default,
        rename = "rateLimitStatus",
        skip_serializing_if = "String::is_empty"
    )]
    pub rate_limit_status: String,
    #[serde(
        default,
        rename = "rateLimitResets",
        skip_serializing_if = "i64_is_zero"
    )]
    pub rate_limit_resets: i64,
    #[serde(
        default,
        rename = "overageStatus",
        skip_serializing_if = "String::is_empty"
    )]
    pub overage_status: String,
    #[serde(default, rename = "overageResets", skip_serializing_if = "i64_is_zero")]
    pub overage_resets: i64,
    #[serde(
        default,
        rename = "isUsingOverage",
        skip_serializing_if = "std::ops::Not::not"
    )]
    pub is_using_overage: bool,

    // Runtime recovery observability.
    #[serde(
        default,
        rename = "runtimeEvent",
        skip_serializing_if = "String::is_empty"
    )]
    pub runtime_event: String,
    #[serde(
        default,
        rename = "runtimeStream",
        skip_serializing_if = "String::is_empty"
    )]
    pub runtime_stream: String,
    #[serde(
        default,
        rename = "runtimeGapFrom",
        skip_serializing_if = "i64_is_zero"
    )]
    pub runtime_gap_from: i64,
    #[serde(
        default,
        rename = "runtimeGapOldest",
        skip_serializing_if = "i64_is_zero"
    )]
    pub runtime_gap_oldest: i64,
    #[serde(
        default,
        rename = "runtimeGapLast",
        skip_serializing_if = "i64_is_zero"
    )]
    pub runtime_gap_last: i64,
    #[serde(
        default,
        rename = "runtimeRecoverStrategy",
        skip_serializing_if = "String::is_empty"
    )]
    pub runtime_recover_strategy: String,

    // Scheduler observability.
    #[serde(
        default,
        rename = "schedulerEvent",
        skip_serializing_if = "String::is_empty"
    )]
    pub scheduler_event: String,
    #[serde(
        default,
        rename = "schedulerDecision",
        skip_serializing_if = "String::is_empty"
    )]
    pub scheduler_decision: String,
    #[serde(
        default,
        rename = "schedulerReason",
        skip_serializing_if = "String::is_empty"
    )]
    pub scheduler_reason: String,
    #[serde(
        default,
        rename = "schedulerQueueDepth",
        skip_serializing_if = "i32_is_zero"
    )]
    pub scheduler_queue_depth: i32,
    #[serde(
        default,
        rename = "schedulerViolation",
        skip_serializing_if = "std::ops::Not::not"
    )]
    pub scheduler_violation: bool,
}

// ----------------------------------------------------------------------------
// agent:turn — `daemon_msg.go:387`
//
// Turn-end summary (NOT a per-turn-boundary event). Sent once at end of turn
// with full TrajectoryEntry array + usage stats. See plan note about turn-end
// vs boundary distinction.
// ----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentTurnMsg {
    #[serde(rename = "agentId")]
    pub agent_id: String,
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(default, rename = "launchId", skip_serializing_if = "String::is_empty")]
    pub launch_id: String,
    #[serde(
        default,
        rename = "launchGeneration",
        skip_serializing_if = "u64_is_zero"
    )]
    pub launch_generation: u64,
    #[serde(rename = "turnNumber")]
    pub turn_number: i32,
    pub entries: Vec<TrajectoryEntry>,
    #[serde(default, rename = "inputTokens", skip_serializing_if = "i32_is_zero")]
    pub input_tokens: i32,
    #[serde(default, rename = "outputTokens", skip_serializing_if = "i32_is_zero")]
    pub output_tokens: i32,
    #[serde(default, rename = "costUsd", skip_serializing_if = "f64_is_zero")]
    pub cost_usd: f64,
    #[serde(default, rename = "contextWindow", skip_serializing_if = "i32_is_zero")]
    pub context_window: i32,
    #[serde(
        default,
        rename = "cacheCreationTokens",
        skip_serializing_if = "i32_is_zero"
    )]
    pub cache_creation_tokens: i32,
    #[serde(
        default,
        rename = "cacheReadTokens",
        skip_serializing_if = "i32_is_zero"
    )]
    pub cache_read_tokens: i32,
    #[serde(
        default,
        rename = "sessionType",
        skip_serializing_if = "String::is_empty"
    )]
    pub session_type: String,
    #[serde(rename = "channelId")]
    pub channel_id: Uuid,
    #[serde(rename = "channelName")]
    pub channel_name: String,
    #[serde(rename = "contextUsagePct")]
    pub context_usage_pct: f64,

    // Responsiveness metrics (N2 observability).
    #[serde(
        default,
        rename = "notifToCheckMs",
        skip_serializing_if = "i64_is_zero"
    )]
    pub notif_to_check_ms: i64,
    #[serde(default, rename = "checkCadenceS", skip_serializing_if = "f64_is_zero")]
    pub check_cadence_s: f64,
    #[serde(default, rename = "nudgesSent", skip_serializing_if = "i32_is_zero")]
    pub nudges_sent: i32,
    #[serde(
        default,
        rename = "nudgesEffective",
        skip_serializing_if = "i32_is_zero"
    )]
    pub nudges_effective: i32,
}

// ----------------------------------------------------------------------------
// agent:recovery:record — `daemon_msg.go:639`
// ----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentRecoveryRecordMsg {
    #[serde(rename = "agentId")]
    pub agent_id: String,
    #[serde(rename = "stoppedAtMs")]
    pub stopped_at_ms: i64,
    #[serde(rename = "stopReason")]
    pub stop_reason: String,
    #[serde(
        default,
        rename = "expectedRecoveryAtMs",
        skip_serializing_if = "i64_is_zero"
    )]
    pub expected_recovery_at_ms: i64,
    #[serde(
        default,
        rename = "lastInflightSummary",
        skip_serializing_if = "String::is_empty"
    )]
    pub last_inflight_summary: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub provider: String,
}

// ============================================================================
// Phase 0b — workspace ops responses (FPC #14)
// ============================================================================

/// Reply to `agent:workspace:list`. `files` lists immediate children of `dir_path`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentWorkspaceFileTreeMsg {
    #[serde(rename = "agentId")]
    pub agent_id: String,
    #[serde(rename = "requestId")]
    pub request_id: String,
    #[serde(rename = "dirPath")]
    pub dir_path: String,
    pub files: Vec<crate::types::FileTreeEntry>,
}

/// Reply to `agent:workspace:read`. `binary=true` indicates `content` is
/// base64-encoded; otherwise it's UTF-8 text.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentWorkspaceFileContentMsg {
    #[serde(rename = "agentId")]
    pub agent_id: String,
    #[serde(rename = "requestId")]
    pub request_id: String,
    pub content: String,
    pub binary: bool,
}

// ============================================================================
// Phase 0b — working-memory op response (FPC #16)
// ============================================================================

/// Reply to `agent:working:{set,get,clear}`.
/// `op` echoes the originating request kind ("set" | "get" | "clear").
/// `state` is `Some(_)` on set/get-with-anchor, `None` on clear or get-of-unset.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentWorkingResultMsg {
    #[serde(rename = "agentId")]
    pub agent_id: String,
    #[serde(rename = "requestId")]
    pub request_id: String,
    pub op: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<crate::types::WorkingStatePayload>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub error: String,
    #[serde(
        default,
        rename = "errorCode",
        skip_serializing_if = "String::is_empty"
    )]
    pub error_code: String,
}
