//! Kimi-code `--output-format stream-json` line parser.
//!
//! One `parse_line` call per newline-delimited JSON line. The parser is
//! stateless: every line is decoded independently and returns zero-or-more
//! `KimiEvent`s.
//!
//! Observed wire shapes (kimi-code v0.6.0):
//!   - `{"role":"assistant","content":"..."}` → TextDelta
//!   - `{"role":"meta","type":"session.resume_hint","session_id":"..."}` → SessionStarted
//!
//! The stream-json format is intentionally high-level: tool calls are executed
//! internally by kimi-code and do not appear as separate stream events.
//! Process exit marks turn end (handled by the actor's turn-exit lifecycle).

#[derive(Debug, Clone)]
pub enum KimiEvent {
    SessionStarted {
        session_id: String,
    },
    ThinkingDelta {
        text: String,
    },
    TextDelta {
        text: String,
    },
    ToolCall {
        name: String,
        input: serde_json::Value,
    },
    TurnEnd,
    CompactStarted,
    CompactFinished,
    Error {
        message: String,
    },
    /// Forward-compatible bucket for stream-json types we don't yet model.
    Unknown,
}

/// Parse one stream-json line from kimi-code stdout.
pub fn parse_line(line: &str) -> Vec<KimiEvent> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let raw: serde_json::Value = match serde_json::from_str(trimmed) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    if raw.get("method").and_then(|v| v.as_str()) == Some("event") {
        return parse_wire_event(&raw);
    }
    if raw.get("error").is_some() {
        return vec![
            KimiEvent::Error {
                message: raw
                    .get("error")
                    .and_then(|v| v.get("message"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown Kimi error")
                    .to_string(),
            },
            KimiEvent::TurnEnd,
        ];
    }

    let role = raw.get("role").and_then(|v| v.as_str()).unwrap_or("");
    match role {
        "assistant" => {
            let content = raw.get("content").and_then(|v| v.as_str()).unwrap_or("");
            if content.is_empty() {
                Vec::new()
            } else {
                vec![KimiEvent::TextDelta {
                    text: content.to_string(),
                }]
            }
        }
        "meta" => {
            let ty = raw.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if ty == "session.resume_hint" {
                let sid = raw
                    .get("session_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                vec![KimiEvent::SessionStarted { session_id: sid }]
            } else {
                Vec::new()
            }
        }
        _ => Vec::new(),
    }
}

fn parse_wire_event(raw: &serde_json::Value) -> Vec<KimiEvent> {
    let params = raw.get("params").unwrap_or(&serde_json::Value::Null);
    let event_type = params.get("type").and_then(|v| v.as_str()).unwrap_or("");
    let payload = params.get("payload").unwrap_or(&serde_json::Value::Null);
    match event_type {
        "StepBegin" => vec![KimiEvent::ThinkingDelta {
            text: String::new(),
        }],
        "CompactionBegin" => vec![KimiEvent::CompactStarted],
        "CompactionEnd" => vec![KimiEvent::CompactFinished],
        "ContentPart" => match payload.get("type").and_then(|v| v.as_str()).unwrap_or("") {
            "think" => payload
                .get("think")
                .and_then(|v| v.as_str())
                .map(|text| KimiEvent::ThinkingDelta {
                    text: text.to_string(),
                })
                .into_iter()
                .collect(),
            "text" => payload
                .get("text")
                .and_then(|v| v.as_str())
                .map(|text| KimiEvent::TextDelta {
                    text: text.to_string(),
                })
                .into_iter()
                .collect(),
            _ => Vec::new(),
        },
        "ToolCall" => {
            let function = payload.get("function").unwrap_or(&serde_json::Value::Null);
            vec![KimiEvent::ToolCall {
                name: function
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown_tool")
                    .to_string(),
                input: parse_tool_arguments(function.get("arguments")),
            }]
        }
        "TurnEnd" => vec![KimiEvent::TurnEnd],
        "StepInterrupted" => vec![
            KimiEvent::Error {
                message: "Turn interrupted".to_string(),
            },
            KimiEvent::TurnEnd,
        ],
        _ => Vec::new(),
    }
}

fn parse_tool_arguments(raw: Option<&serde_json::Value>) -> serde_json::Value {
    match raw {
        Some(serde_json::Value::String(s)) => {
            serde_json::from_str(s).unwrap_or_else(|_| serde_json::Value::String(s.clone()))
        }
        Some(value) => value.clone(),
        None => serde_json::Value::Null,
    }
}
