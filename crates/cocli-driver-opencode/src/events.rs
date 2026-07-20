//! OpenCode `run --format json` line parser.
//!
//! OpenCode JSON output has changed shape across releases, so this parser
//! accepts both compact assistant content arrays and event-style records.

use cocli_driver_core::types::normalize_turn_status;
use cocli_driver_core::{DriverEvent, ErrorSeverity};

pub fn parse_line(line: &str) -> Vec<DriverEvent> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let raw: serde_json::Value = match serde_json::from_str(trimmed) {
        Ok(value) => value,
        Err(_) => return Vec::new(),
    };

    match raw.get("type").and_then(|v| v.as_str()).unwrap_or("") {
        "session" | "session_start" | "init" => vec![DriverEvent::SessionStarted {
            session_id: raw
                .get("id")
                .or_else(|| raw.get("session_id"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        }],
        "assistant" | "message" => parse_assistant(&raw),
        "tool" | "tool_call" => vec![tool_call(&raw)],
        "tool_result" | "tool_done" => vec![DriverEvent::ToolDone {
            id: raw
                .get("id")
                .or_else(|| raw.get("tool_id"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            result: raw
                .get("result")
                .or_else(|| raw.get("output"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            error: raw
                .get("error")
                .and_then(|v| v.as_str())
                .map(ToOwned::to_owned),
        }],
        "result" | "done" => parse_result(&raw),
        "error" => vec![DriverEvent::Error {
            message: error_message(&raw, "OpenCode emitted an error"),
            code: raw
                .get("code")
                .and_then(|v| v.as_str())
                .map(ToOwned::to_owned),
            severity: Some(ErrorSeverity::Error),
            http_status: None,
        }],
        _ => vec![DriverEvent::Unknown],
    }
}

fn parse_assistant(raw: &serde_json::Value) -> Vec<DriverEvent> {
    if let Some(content) = raw.get("content").and_then(|v| v.as_array()) {
        let mut out = Vec::new();
        for block in content {
            match block.get("type").and_then(|v| v.as_str()).unwrap_or("") {
                "text" => {
                    if let Some(text) = text_value(block) {
                        out.push(DriverEvent::TextDelta { text });
                    }
                }
                "thinking" => {
                    if let Some(text) = text_value(block) {
                        out.push(DriverEvent::ThinkingDelta { text });
                    }
                }
                "tool" | "tool_call" | "tool_use" => out.push(tool_call(block)),
                _ => {}
            }
        }
        return if out.is_empty() {
            vec![DriverEvent::Unknown]
        } else {
            out
        };
    }

    raw.get("text")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|text| {
            vec![DriverEvent::TextDelta {
                text: text.to_string(),
            }]
        })
        .unwrap_or_else(|| vec![DriverEvent::Unknown])
}

fn tool_call(raw: &serde_json::Value) -> DriverEvent {
    DriverEvent::ToolCall {
        id: raw
            .get("id")
            .or_else(|| raw.get("tool_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        name: raw
            .get("name")
            .or_else(|| raw.get("tool_name"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown_tool")
            .to_string(),
        input: raw
            .get("input")
            .or_else(|| raw.get("parameters"))
            .cloned()
            .unwrap_or(serde_json::Value::Null),
    }
}

fn parse_result(raw: &serde_json::Value) -> Vec<DriverEvent> {
    let status = raw
        .get("status")
        .or_else(|| raw.get("subtype"))
        .and_then(|v| v.as_str())
        .unwrap_or("success");
    let is_error = raw
        .get("is_error")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let normalized_status = if is_error && status == "success" {
        "failed"
    } else {
        status
    };

    let mut out = Vec::new();
    if is_error || status != "success" {
        out.push(DriverEvent::Error {
            message: error_message(raw, "OpenCode session ended with an error"),
            code: None,
            severity: Some(ErrorSeverity::Error),
            http_status: None,
        });
    }
    out.push(DriverEvent::TurnEnd {
        status: normalize_turn_status(normalized_status),
        input_tokens: usage(raw, "input_tokens"),
        output_tokens: usage(raw, "output_tokens"),
        cost_usd: raw
            .get("cost")
            .or_else(|| raw.get("total_cost_usd"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
        cache_creation_tokens: usage(raw, "cache_creation_tokens"),
        cache_read_tokens: usage(raw, "cache_read_tokens"),
        context_window_tokens: 0,
    });
    out
}

fn text_value(raw: &serde_json::Value) -> Option<String> {
    raw.get("text")
        .or_else(|| raw.get("content"))
        .or_else(|| raw.get("thinking"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
}

fn usage(raw: &serde_json::Value, key: &str) -> u64 {
    raw.get("usage")
        .and_then(|v| v.get(key))
        .or_else(|| raw.get(key))
        .and_then(|v| v.as_u64())
        .unwrap_or(0)
}

fn error_message(raw: &serde_json::Value, fallback: &str) -> String {
    raw.get("error")
        .and_then(|v| {
            v.as_str().map(ToOwned::to_owned).or_else(|| {
                v.get("message")
                    .and_then(|m| m.as_str())
                    .map(ToOwned::to_owned)
            })
        })
        .or_else(|| {
            raw.get("message")
                .and_then(|v| v.as_str())
                .map(ToOwned::to_owned)
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| fallback.to_string())
}
