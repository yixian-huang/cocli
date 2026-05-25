//! Claude `--output-format stream-json` line parser.
//!
//! One `parse_line` call per newline-delimited JSON line. The parser is
//! stateless: every line is decoded independently and returns a single
//! `ClaudeEvent`. Lines we don't yet model fall through to `Unknown`.
//!
//! We deliberately use raw `serde_json::Value` lookups instead of typed
//! `Deserialize` structs so unknown / future claude CLI fields are tolerated
//! without parse errors.

#[derive(Debug, Clone)]
pub enum ClaudeEvent {
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
    TurnEnd {
        input_tokens: u64,
        output_tokens: u64,
        cost_usd: f64,
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
        code: Option<String>,
    },
    /// Raw event we don't yet care about (forward-compatible).
    Unknown,
}

/// Parse one stream-json line from claude stdout.
///
/// claude stream-json shape: `{"type":"...","..."}` per line.
pub fn parse_line(line: &str) -> ClaudeEvent {
    let raw: serde_json::Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => return ClaudeEvent::Unknown,
    };
    let ty = raw.get("type").and_then(|v| v.as_str()).unwrap_or("");
    match ty {
        "system" => {
            // claude system-init line; carries session_id
            if let Some(sid) = raw.get("session_id").and_then(|v| v.as_str()) {
                return ClaudeEvent::SessionStarted {
                    session_id: sid.to_string(),
                };
            }
            ClaudeEvent::Unknown
        }
        "assistant" => {
            // Stream-json assistant turn segment. The message block is in raw["message"].
            // For Phase 0a we don't disambiguate streaming deltas vs full message.
            // Each `assistant` line ~ a content block, parse content[].type.
            let msg = match raw.get("message") {
                Some(m) => m,
                None => return ClaudeEvent::Unknown,
            };
            // first content block's type signals the variant
            let content = msg
                .get("content")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            for block in content {
                let bt = block.get("type").and_then(|v| v.as_str()).unwrap_or("");
                match bt {
                    "thinking" => {
                        let text = block
                            .get("thinking")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        return ClaudeEvent::ThinkingDelta { text };
                    }
                    "text" => {
                        let text = block
                            .get("text")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        return ClaudeEvent::TextDelta { text };
                    }
                    "tool_use" => {
                        let id = block
                            .get("id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let name = block
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let input = block
                            .get("input")
                            .cloned()
                            .unwrap_or(serde_json::Value::Null);
                        return ClaudeEvent::ToolCall { id, name, input };
                    }
                    _ => continue,
                }
            }
            ClaudeEvent::Unknown
        }
        "user" => {
            // tool result back to claude (a "user" message containing tool_result content)
            let content = raw
                .get("message")
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_array())
                .cloned()
                .unwrap_or_default();
            for block in content {
                if block.get("type").and_then(|v| v.as_str()) == Some("tool_result") {
                    let id = block
                        .get("tool_use_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let is_error = block
                        .get("is_error")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let content_val = block.get("content");
                    let (output, error) = if is_error {
                        (String::new(), Some(value_to_string(content_val)))
                    } else {
                        (value_to_string(content_val), None)
                    };
                    return ClaudeEvent::ToolResult { id, output, error };
                }
            }
            ClaudeEvent::Unknown
        }
        "result" => {
            // turn-end summary line
            let input = raw
                .get("usage")
                .and_then(|u| u.get("input_tokens"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let output = raw
                .get("usage")
                .and_then(|u| u.get("output_tokens"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let cost = raw
                .get("total_cost_usd")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let cc = raw
                .get("usage")
                .and_then(|u| u.get("cache_creation_input_tokens"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let cr = raw
                .get("usage")
                .and_then(|u| u.get("cache_read_input_tokens"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            ClaudeEvent::TurnEnd {
                input_tokens: input,
                output_tokens: output,
                cost_usd: cost,
                cache_creation_tokens: cc,
                cache_read_tokens: cr,
            }
        }
        "rate_limit_event" => {
            let limit_type = raw
                .get("limit_type")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let status = raw
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let resets_at = raw.get("resets_at").and_then(|v| v.as_i64()).unwrap_or(0);
            ClaudeEvent::RateLimit {
                limit_type,
                status,
                resets_at,
            }
        }
        "error" => {
            let message = raw
                .get("error")
                .and_then(|e| e.as_object())
                .and_then(|o| o.get("message"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let code = raw
                .get("error")
                .and_then(|e| e.as_object())
                .and_then(|o| o.get("code"))
                .and_then(|v| v.as_str())
                .map(String::from);
            ClaudeEvent::Error { message, code }
        }
        _ => ClaudeEvent::Unknown,
    }
}

fn value_to_string(v: Option<&serde_json::Value>) -> String {
    match v {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Array(arr)) => arr
            .iter()
            .filter_map(|item| item.get("text").and_then(|t| t.as_str()).map(String::from))
            .collect::<Vec<_>>()
            .join(""),
        Some(other) => other.to_string(),
        None => String::new(),
    }
}

/// Generic Event bridge for trait conformance.
///
/// Most claude lines emit exactly one generic Event. A few lines emit multiple
/// (e.g., assistant message with text + tool_use blocks).
pub fn parse_line_to_events(line: &str) -> Vec<cocli_driver::Event> {
    use cocli_driver::Event as E;
    let claude_ev = parse_line(line);
    match claude_ev {
        ClaudeEvent::SessionStarted { session_id } => vec![E::SessionStarted { session_id }],
        ClaudeEvent::MessageStart => vec![E::MessageStart],
        ClaudeEvent::ThinkingDelta { text } => vec![E::Thinking { text }],
        ClaudeEvent::TextDelta { text } => vec![E::Text {
            text,
            role: Some("assistant".into()),
        }],
        ClaudeEvent::ToolCall { id, name, input } => vec![E::ToolUse { id, name, input }],
        ClaudeEvent::ToolResult { id, output, error } => {
            vec![E::ToolDone {
                id,
                output: Some(output),
                error,
            }]
        }
        ClaudeEvent::MessageStop { stop_reason } => vec![E::MessageStop { stop_reason }],
        ClaudeEvent::TurnEnd {
            input_tokens,
            output_tokens,
            cost_usd,
            cache_creation_tokens,
            cache_read_tokens,
        } => {
            vec![E::TurnEnd {
                status: Some("completed".into()),
                input_tokens,
                output_tokens,
                cached_input_tokens: cache_read_tokens,
                cost_usd,
                context_window: None,
                cache_creation_tokens,
                cache_read_tokens,
            }]
        }
        ClaudeEvent::RateLimit {
            limit_type,
            status,
            resets_at,
        } => {
            vec![E::RateLimit {
                limit_type,
                status,
                resets_at,
            }]
        }
        ClaudeEvent::Error { message, code } => vec![E::Error {
            message,
            code,
            retryable: false,
            terminal: false,
            overflow: false,
        }],
        ClaudeEvent::Unknown => vec![E::Unknown],
    }
}
