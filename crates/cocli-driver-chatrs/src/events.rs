//! Chatrs stdout JSONL parser.
//!
//! Mirrors Go `daemon/drivers/chatrs.go::ParseLine` (lines 257-377).
//! chatrs emits one JSON object per stdout line; the `"kind"` field is the
//! discriminator. Unknown kinds and parse errors return `Unknown`.
//!
//! Field names mirror chatrs's `Event` enum in `miniagent-core/src/event.rs`
//! (`serde(tag = "kind", rename_all = "snake_case")`). Renamed JSON keys
//! (`type` -> `LimitType`, `resets` -> `ResetsAt`) match the Go struct tags.

#[derive(Debug, Clone)]
pub enum ChatrsEvent {
    Init {
        session_id: String,
    },
    TextDelta {
        content: String,
    },
    ThinkingDelta {
        content: String,
    },
    ToolUse {
        id: String,
        name: String,
        args: serde_json::Value,
    },
    ToolDone {
        id: String,
        ok: bool,
        output: String,
    },
    TurnEnd {
        status: String,
        input_tokens: u64,
        output_tokens: u64,
        cost_usd: f64,
        context_window: u64,
        cache_creation_tokens: u64,
        cache_read_tokens: u64,
    },
    RateLimit {
        limit_type: String,
        status: String,
        resets_at: i64,
    },
    Error {
        message: String,
        retryable: bool,
        terminal: bool,
        overflow: bool,
        code: Option<String>,
    },
    /// Empty line, malformed JSON, or unknown discriminator (forward-compat).
    Unknown,
}

/// Parse one JSONL line from chatrs stdout. Empty / malformed / unknown
/// lines return `ChatrsEvent::Unknown` (the Go variant returns an empty
/// `[]Event` slice; we return one Unknown so callers can keep a 1:1
/// line/event count).
pub fn parse_line(line: &str) -> ChatrsEvent {
    if line.is_empty() {
        return ChatrsEvent::Unknown;
    }

    let raw: serde_json::Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => return ChatrsEvent::Unknown,
    };

    let kind = raw.get("kind").and_then(|v| v.as_str()).unwrap_or("");

    match kind {
        "init" => {
            let session_id = raw
                .get("session_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            ChatrsEvent::Init { session_id }
        }
        "text_delta" => {
            let content = raw
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            ChatrsEvent::TextDelta { content }
        }
        "thinking_delta" => {
            let content = raw
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            ChatrsEvent::ThinkingDelta { content }
        }
        "tool_use" => {
            let id = raw
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let name = raw
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let args = raw.get("args").cloned().unwrap_or(serde_json::Value::Null);
            ChatrsEvent::ToolUse { id, name, args }
        }
        "tool_done" => {
            let id = raw
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let ok = raw.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
            let output = raw
                .get("output")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            ChatrsEvent::ToolDone { id, ok, output }
        }
        "turn_end" => {
            let status = raw
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let input_tokens = raw
                .get("input_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let output_tokens = raw
                .get("output_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let cost_usd = raw.get("cost_usd").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let context_window = raw
                .get("context_window")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let cache_creation_tokens = raw
                .get("cache_creation")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let cache_read_tokens = raw.get("cache_read").and_then(|v| v.as_u64()).unwrap_or(0);
            ChatrsEvent::TurnEnd {
                status,
                input_tokens,
                output_tokens,
                cost_usd,
                context_window,
                cache_creation_tokens,
                cache_read_tokens,
            }
        }
        "rate_limit" => {
            let limit_type = raw
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let status = raw
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let resets_at = raw.get("resets").and_then(|v| v.as_i64()).unwrap_or(0);
            ChatrsEvent::RateLimit {
                limit_type,
                status,
                resets_at,
            }
        }
        "error" => {
            let message = raw
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let retryable = raw
                .get("retryable")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let terminal = raw
                .get("terminal")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let overflow = raw
                .get("overflow")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let code = raw
                .get("code")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(String::from);
            ChatrsEvent::Error {
                message,
                retryable,
                terminal,
                overflow,
                code,
            }
        }
        _ => ChatrsEvent::Unknown,
    }
}
