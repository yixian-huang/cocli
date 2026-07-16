//! Gemini CLI `--output-format stream-json` line parser.
//!
//! One `parse_line` call per newline-delimited JSON line. The parser is
//! stateless: every line is decoded independently and returns a single
//! `GeminiEvent`. Unknown / future fields are tolerated.
//!
//! Wire-shape references (port of `daemon/drivers/gemini.go::ParseLine`):
//!   - `init`        → SessionStarted (raw["session_id"])
//!   - `message`     → TextDelta when role=assistant + non-empty content
//!   - `tool_use`    → ToolCall {tool_id, tool_name, parameters}
//!   - `tool_result` → ToolResult {tool_id, output} / status="error" → error captured
//!   - `result`      → TurnEnd (tokens from stats.*); preceded by Error if status != "success"
//!   - `error`       → Error {message, severity, code?}
//!
//! Severity is gemini-specific; see `packages/core/src/output/types.ts:75-79`.

#[derive(Debug, Clone)]
pub enum GeminiEvent {
    SessionStarted {
        session_id: String,
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
    TurnEnd {
        input_tokens: u64,
        output_tokens: u64,
        cache_read_tokens: u64,
        /// Set when the `result` event carried `status="error"` —
        /// surfaced as a separate `Error` event preceding the `TurnEnd`.
        error_before_turn_end: Option<String>,
    },
    Error {
        message: String,
        code: Option<String>,
        /// "warning" | "error" verbatim from the wire (gemini-cli output
        /// types). Mapped to `ErrorSeverity` in `conv.rs`.
        severity: Option<String>,
    },
    /// Forward-compatible bucket for stream-json types we don't yet model.
    Unknown,
}

/// Parse one stream-json line from gemini stdout.
///
/// Returns a `Vec` because gemini's `result` event with `status != "success"`
/// produces both an `Error` and a `TurnEnd` in sequence (matches Go's
/// `case "result"` branch in `gemini.go::ParseLine`).
pub fn parse_line(line: &str) -> Vec<GeminiEvent> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return vec![GeminiEvent::Unknown];
    }
    let raw: serde_json::Value = match serde_json::from_str(trimmed) {
        Ok(v) => v,
        Err(_) => return vec![GeminiEvent::Unknown],
    };
    let ty = raw.get("type").and_then(|v| v.as_str()).unwrap_or("");
    match ty {
        "init" => {
            // {"type":"init","session_id":"...","model":"..."}
            let sid = raw
                .get("session_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            vec![GeminiEvent::SessionStarted { session_id: sid }]
        }
        "message" => {
            // {"type":"message","role":"assistant","content":"...","delta":true}
            let role = raw.get("role").and_then(|v| v.as_str()).unwrap_or("");
            if role != "assistant" {
                return vec![GeminiEvent::Unknown];
            }
            let content = raw.get("content").and_then(|v| v.as_str()).unwrap_or("");
            if content.is_empty() {
                return vec![GeminiEvent::Unknown];
            }
            vec![GeminiEvent::TextDelta {
                text: content.to_string(),
            }]
        }
        "tool_use" => {
            // {"type":"tool_use","tool_name":"mcp_chat_check_messages","tool_id":"...","parameters":{}}
            let name = raw
                .get("tool_name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let id = raw
                .get("tool_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let input = raw
                .get("parameters")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            vec![GeminiEvent::ToolCall { id, name, input }]
        }
        "tool_result" => {
            // types.ts:64-73 — { type, timestamp, tool_id, status, output?, error?: {type, message} }
            let id = raw
                .get("tool_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let status = raw.get("status").and_then(|v| v.as_str()).unwrap_or("");
            if status == "error" {
                let err_text = if let Some(err_obj) = raw.get("error").and_then(|v| v.as_object()) {
                    let err_type = err_obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    let err_msg = err_obj
                        .get("message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    match (err_type.is_empty(), err_msg.is_empty()) {
                        (false, false) => format!("{err_type}: {err_msg}"),
                        (true, false) => err_msg.to_string(),
                        (false, true) => err_type.to_string(),
                        (true, true) => "tool reported error (no detail)".to_string(),
                    }
                } else {
                    "tool reported error (no detail)".to_string()
                };
                vec![GeminiEvent::ToolResult {
                    id,
                    output: String::new(),
                    error: Some(err_text),
                }]
            } else {
                let output = raw
                    .get("output")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                vec![GeminiEvent::ToolResult {
                    id,
                    output,
                    error: None,
                }]
            }
        }
        "result" => {
            // Stats schema (packages/core/src/output/types.ts:89-99): total_tokens,
            // input_tokens, output_tokens, cached, input, duration_ms, tool_calls.
            // gemini-cli does NOT emit total_cost_usd; cost stays 0.
            let mut input_tokens = 0u64;
            let mut output_tokens = 0u64;
            let mut cache_read_tokens = 0u64;
            if let Some(stats) = raw.get("stats").and_then(|v| v.as_object()) {
                input_tokens = stats
                    .get("input_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                output_tokens = stats
                    .get("output_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                cache_read_tokens = stats.get("cached").and_then(|v| v.as_u64()).unwrap_or(0);
            }

            // Wire shape: result.error is an object {type, message}, NOT a
            // top-level string. See packages/core/src/output/types.ts:101-109.
            let status = raw.get("status").and_then(|v| v.as_str()).unwrap_or("");
            let err_text = if !status.is_empty() && status != "success" {
                let msg = if let Some(err_obj) = raw.get("error").and_then(|v| v.as_object()) {
                    err_obj
                        .get("message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string()
                } else if let Some(s) = raw.get("error").and_then(|v| v.as_str()) {
                    s.to_string()
                } else if let Some(s) = raw.get("error_message").and_then(|v| v.as_str()) {
                    s.to_string()
                } else if let Some(s) = raw.get("message").and_then(|v| v.as_str()) {
                    s.to_string()
                } else {
                    String::new()
                };
                let msg = if msg.is_empty() {
                    format!("Gemini session ended with status: {status}")
                } else {
                    msg
                };
                Some(msg)
            } else {
                None
            };

            vec![GeminiEvent::TurnEnd {
                input_tokens,
                output_tokens,
                cache_read_tokens,
                error_before_turn_end: err_text,
            }]
        }
        "error" => {
            // Real wire: {type, timestamp, severity, message} — message at top level.
            // packages/core/src/output/types.ts:75-79.
            let message = raw
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let severity = raw
                .get("severity")
                .and_then(|v| v.as_str())
                .map(String::from);
            let code = raw.get("code").and_then(|v| v.as_str()).map(String::from);
            vec![GeminiEvent::Error {
                message,
                code,
                severity,
            }]
        }
        _ => vec![GeminiEvent::Unknown],
    }
}
