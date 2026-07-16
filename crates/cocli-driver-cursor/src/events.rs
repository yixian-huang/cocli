//! Cursor Agent `--output-format stream-json` line parser.
//!
//! Cursor's headless stream shape is close to Claude's JSON stream for
//! assistant content blocks, while lifecycle/status lines are runtime-specific.
//! The parser is intentionally permissive so future Cursor fields do not break
//! daemon routing.

use cocli_driver_core::types::normalize_turn_status;
use cocli_driver_core::{DriverEvent, ErrorSeverity};

/// Parse one Cursor stream-json stdout line into zero-or-more driver events.
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
        "system" => parse_system(&raw),
        "assistant" => parse_assistant(&raw),
        "result" => parse_result(&raw),
        "error" => vec![DriverEvent::Error {
            message: error_message(&raw, "Cursor emitted an error"),
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

fn parse_system(raw: &serde_json::Value) -> Vec<DriverEvent> {
    match raw.get("subtype").and_then(|v| v.as_str()).unwrap_or("") {
        "init" => vec![DriverEvent::SessionStarted {
            session_id: raw
                .get("session_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        }],
        "status" => match raw.get("status").and_then(|v| v.as_str()).unwrap_or("") {
            "compacting" | "compact_started" => vec![DriverEvent::CompactStarted],
            "compact_done" | "compacted" => vec![DriverEvent::CompactFinished],
            _ => vec![DriverEvent::Unknown],
        },
        "compact_boundary" | "compact_done" => vec![DriverEvent::CompactFinished],
        _ => vec![DriverEvent::Unknown],
    }
}

fn parse_assistant(raw: &serde_json::Value) -> Vec<DriverEvent> {
    let Some(content) = raw
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_array())
    else {
        return vec![DriverEvent::Unknown];
    };

    let mut out = Vec::new();
    for block in content {
        match block.get("type").and_then(|v| v.as_str()).unwrap_or("") {
            "thinking" => {
                if let Some(text) = block_text(block, &["thinking", "text"]) {
                    out.push(DriverEvent::ThinkingDelta { text });
                }
            }
            "text" => {
                if let Some(text) = block_text(block, &["text"]) {
                    out.push(DriverEvent::TextDelta { text });
                }
            }
            "tool_use" => out.push(DriverEvent::ToolCall {
                id: block
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                name: block
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown_tool")
                    .to_string(),
                input: block
                    .get("input")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null),
            }),
            _ => {}
        }
    }

    if out.is_empty() {
        vec![DriverEvent::Unknown]
    } else {
        out
    }
}

fn parse_result(raw: &serde_json::Value) -> Vec<DriverEvent> {
    let subtype = raw
        .get("subtype")
        .and_then(|v| v.as_str())
        .unwrap_or("success");
    let is_error = raw
        .get("is_error")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let status = if is_error && subtype == "success" {
        "failed"
    } else {
        subtype
    };

    let mut out = Vec::new();
    if is_error || subtype != "success" {
        out.push(DriverEvent::Error {
            message: error_message(raw, "Cursor session ended with an error"),
            code: None,
            severity: Some(ErrorSeverity::Error),
            http_status: None,
        });
    }
    out.push(DriverEvent::TurnEnd {
        status: normalize_turn_status(status),
        input_tokens: number_field(raw, &["usage", "input_tokens"]),
        output_tokens: number_field(raw, &["usage", "output_tokens"]),
        cost_usd: raw
            .get("total_cost_usd")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
        cache_creation_tokens: number_field(raw, &["usage", "cache_creation_tokens"]),
        cache_read_tokens: number_field(raw, &["usage", "cache_read_tokens"]),
        context_window_tokens: 0,
    });
    out
}

fn block_text(block: &serde_json::Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| block.get(*key).and_then(|v| v.as_str()))
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
}

fn number_field(raw: &serde_json::Value, path: &[&str]) -> u64 {
    let mut cursor = raw;
    for key in path {
        cursor = match cursor.get(*key) {
            Some(value) => value,
            None => return 0,
        };
    }
    cursor.as_u64().unwrap_or(0)
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
