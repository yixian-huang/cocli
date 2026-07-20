//! Grok `--output-format streaming-json` line parser (NDJSON).
//!
//! One `parse_line` call per newline-delimited JSON line from stdout.
//! The parser is stateless. Non-JSON lines (e.g. stray stderr noise that
//! leaks) and unknown types are mapped to `Unknown` (tolerated per spike).
//!
//! Observed from spike (see spikes/grok-driver-spike.md for verbatim samples):
//!   - `{"type":"thought","data":"..."}` → ThinkingDelta (internal reasoning;
//!     often mentions search_tool/use_tool and `chat__send_message`)
//!   - `{"type":"text","data":"..."}` → TextDelta (visible chunks)
//!   - `{"type":"end","stopReason":"EndTurn","sessionId":"...","requestId":"..."}`
//!     → SessionStarted (for resume) + TurnEnd (stopReason for status)
//!   - `{"type":"error","message":"..."}` → Error
//!
//! No explicit ToolCall/ToolResult in the streaming-json (Grok handles MCP
//! tool use internally via its search_tool/use_tool flow; bridge side-effects
//! are the delivery mechanism). SessionId appears only on "end".
//!
//! Prefix for MCP tools (not in events): "chat__" (see driver mcp_tool_prefix later).

#[derive(Debug, Clone, PartialEq)]
pub enum GrokEvent {
    SessionStarted {
        session_id: String,
    },
    TextDelta {
        text: String,
    },
    ThinkingDelta {
        text: String,
    },
    TurnEnd {
        session_id: String,
        stop_reason: String,
    },
    Error {
        message: String,
        code: Option<String>,
    },
    /// Unrecognized type, non-JSON, or empty line (forward-compatible).
    Unknown,
}

/// Parse one streaming-json line. Returns 0+ events (normally 1; 2 for "end"
/// to surface both the sessionId (as SessionStarted) and TurnEnd).
///
/// Empty/invalid JSON/unrecognized → [Unknown] (consistent with gemini style).
pub fn parse_line(line: &str) -> Vec<GrokEvent> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return vec![GrokEvent::Unknown];
    }
    let raw: serde_json::Value = match serde_json::from_str(trimmed) {
        Ok(v) => v,
        Err(_) => return vec![GrokEvent::Unknown],
    };
    let ty = raw.get("type").and_then(|v| v.as_str()).unwrap_or("");
    match ty {
        "thought" => {
            // per-token reasoning chunks (internal; may mention chat__ tools)
            let text = raw
                .get("data")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            vec![GrokEvent::ThinkingDelta { text }]
        }
        "text" => {
            // final response chunks (assembled into visible message)
            let text = raw
                .get("data")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            vec![GrokEvent::TextDelta { text }]
        }
        "end" => {
            // turn completion; sessionId for future --resume; stopReason for TurnEnd status
            let session_id = raw
                .get("sessionId")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let stop_reason = raw
                .get("stopReason")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            // Emit both so SessionStarted is observable (for resume capture in actor)
            // and TurnEnd signals completion. Mirrors how gemini result can expand.
            vec![
                GrokEvent::SessionStarted {
                    session_id: session_id.clone(),
                },
                GrokEvent::TurnEnd {
                    session_id,
                    stop_reason,
                },
            ]
        }
        "error" => {
            let message = raw
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let code = raw.get("code").and_then(|v| v.as_str()).map(String::from);
            vec![GrokEvent::Error { message, code }]
        }
        _ => vec![GrokEvent::Unknown],
    }
}
