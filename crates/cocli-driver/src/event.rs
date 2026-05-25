//! Generic Event enum emitted by all driver implementations.
//!
//! Driver impls translate runtime-specific protocols (claude stream-json,
//! codex JSON-RPC notifications, kimi RPC events, chatrs JSONL) into this
//! common enum. cocli-agent consumes only this type.

#[derive(Debug, Clone)]
pub enum Event {
    /// Driver observed a session start (with id).
    SessionStarted {
        session_id: String,
    },

    /// Initial init event (provider + model, possibly without session id).
    Init {
        session_id: Option<String>,
        provider: Option<String>,
        model: Option<String>,
    },

    /// Beginning of an assistant message (claude semantic).
    MessageStart,

    /// Extended thinking content delta.
    Thinking {
        text: String,
    },

    /// Visible assistant text delta or complete text.
    Text {
        text: String,
        role: Option<String>,
    },

    /// Tool call initiation by the agent.
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },

    /// Tool call completion (success → output; failure → error).
    ToolDone {
        id: String,
        output: Option<String>,
        error: Option<String>,
    },

    /// Assistant message stop (claude).
    MessageStop {
        stop_reason: String,
    },

    /// Turn boundary with token/cost accounting.
    TurnEnd {
        status: Option<String>,
        input_tokens: u64,
        output_tokens: u64,
        cached_input_tokens: u64,
        cost_usd: f64,
        context_window: Option<u64>,
        cache_creation_tokens: u64,
        cache_read_tokens: u64,
    },

    /// Provider rate-limit event.
    RateLimit {
        limit_type: String,
        status: String,
        resets_at: i64,
    },

    /// Runtime error (driver-side classified retryability flags).
    Error {
        message: String,
        code: Option<String>,
        retryable: bool,
        terminal: bool,
        overflow: bool,
    },

    /// Compaction lifecycle (kimi).
    CompactStarted,
    CompactFinished,

    /// Visible plan display ("[plan] ..." prefix in trajectory).
    PlanDisplay {
        text: String,
    },

    /// Unknown / unmodeled line (forward-compatible default).
    Unknown,
}
