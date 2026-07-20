//! `GeminiEvent` → `DriverEvent` mapping helpers.
//!
//! Gemini is one of two runtimes (with codex) that emit multiple semantic
//! events per stdout line (`result` with `status != "success"` yields both
//! an Error and a TurnEnd). We model this by having `parse_line` return
//! `Vec<GeminiEvent>` and the per-event conversion done one-at-a-time here.
//!
//! Severity: gemini's wire carries `"warning"` | `"error"` on `error`
//! events (loop-detected / max-turns are warnings; the actor uses the tag
//! to skip the quota classifier). Map verbatim to `ErrorSeverity`.

use cocli_driver_core::types::TurnStatus;
use cocli_driver_core::{DriverEvent, ErrorSeverity};

use crate::events::GeminiEvent;

impl From<GeminiEvent> for DriverEvent {
    fn from(e: GeminiEvent) -> Self {
        match e {
            GeminiEvent::SessionStarted { session_id } => {
                DriverEvent::SessionStarted { session_id }
            }
            GeminiEvent::TextDelta { text } => DriverEvent::TextDelta { text },
            GeminiEvent::ToolCall { id, name, input } => DriverEvent::ToolCall { id, name, input },
            GeminiEvent::ToolResult { id, output, error } => {
                // Gemini distinguishes tool_use (call) from tool_result (done)
                // on the wire. We route both `success` and `error` paths to
                // DriverEvent::ToolDone so the actor can present them as
                // turn-trace tool completions.
                DriverEvent::ToolDone {
                    id,
                    result: output,
                    error,
                }
            }
            GeminiEvent::TurnEnd {
                input_tokens,
                output_tokens,
                cache_read_tokens,
                error_before_turn_end: _,
            } => DriverEvent::TurnEnd {
                status: TurnStatus::Completed,
                input_tokens,
                output_tokens,
                cost_usd: 0.0, // gemini-cli does not emit total_cost_usd
                cache_creation_tokens: 0,
                cache_read_tokens,
                context_window_tokens: 0,
            },
            GeminiEvent::Error {
                message,
                code,
                severity,
            } => DriverEvent::Error {
                message,
                code,
                severity: severity.and_then(|s| match s.as_str() {
                    "warning" => Some(ErrorSeverity::Warning),
                    "error" => Some(ErrorSeverity::Error),
                    _ => None,
                }),
                http_status: None,
            },
            GeminiEvent::Unknown => DriverEvent::Unknown,
        }
    }
}

/// Convert a `Vec<GeminiEvent>` into `Vec<DriverEvent>`, expanding the
/// `TurnEnd { error_before_turn_end }` case into a preceding `Error` event.
/// Mirrors Go `gemini.go::ParseLine`'s `case "result"` branch, which appends
/// an EventError before the EventTurnEnd when the result carried a non-success
/// status.
pub fn to_driver_events(events: Vec<GeminiEvent>) -> Vec<DriverEvent> {
    let mut out = Vec::with_capacity(events.len());
    for ev in events {
        match ev {
            GeminiEvent::TurnEnd {
                error_before_turn_end: Some(msg),
                input_tokens,
                output_tokens,
                cache_read_tokens,
            } => {
                out.push(DriverEvent::Error {
                    message: msg,
                    code: None,
                    severity: Some(ErrorSeverity::Error),
                    http_status: None,
                });
                out.push(DriverEvent::TurnEnd {
                    status: TurnStatus::Failed,
                    input_tokens,
                    output_tokens,
                    cost_usd: 0.0,
                    cache_creation_tokens: 0,
                    cache_read_tokens,
                    context_window_tokens: 0,
                });
            }
            other => out.push(other.into()),
        }
    }
    out
}
