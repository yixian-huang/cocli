//! Runtime-neutral event shape emitted by [`crate::Driver::parse_event`].

use crate::types::TurnStatus;

#[derive(Debug, Clone)]
pub enum DriverEvent {
    SessionStarted {
        session_id: String,
    },
    MessageStart,
    ThinkingDelta {
        text: String,
    },
    TextDelta {
        text: String,
    },
    ToolCall {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        id: String,
        output: String,
        error: Option<String>,
    },
    MessageStop {
        stop_reason: String,
    },
    Unknown,
    TurnEnd {
        status: TurnStatus,
        input_tokens: u64,
        output_tokens: u64,
        cost_usd: f64,
        cache_creation_tokens: u64,
        cache_read_tokens: u64,
        /// Per-turn context window reported by the runtime. Zero means use the
        /// driver's static fallback.
        context_window_tokens: u64,
    },
    RateLimit {
        limit_type: String,
        status: String,
        resets_at: i64,
        overage_status: Option<String>,
        overage_resets: Option<i64>,
        is_using_overage: bool,
    },
    Error {
        message: String,
        code: Option<String>,
        severity: Option<ErrorSeverity>,
        http_status: Option<u16>,
    },
    ToolDone {
        id: String,
        result: String,
        error: Option<String>,
    },
    Signal {
        signal_type: SignalType,
        data: serde_json::Value,
    },
    Write {
        data: String,
    },
    CompactStarted,
    CompactFinished,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorSeverity {
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignalType {
    Progress,
    Question,
    Complete,
    Error,
    Other(String),
}
