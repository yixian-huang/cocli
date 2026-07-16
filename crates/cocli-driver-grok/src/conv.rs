//! `GrokEvent` → `DriverEvent` mapping helpers.
//!
//! Grok's streaming-json is high-level (like kimi): only thought/text/end/error.
//! Tool invocations for the bridge (e.g. chat__send_message) are internal to
//! Grok (via search_tool/use_tool per its skills); not surfaced as ToolCall
//! events here. See spikes/grok-driver-spike.md and design spec.
//!
//! Token usage is resolved from Grok on-disk telemetry (`signals.json` +
//! `unified.jsonl`) at turn end — the public `end` line carries no usage.

use cocli_driver_core::types::{normalize_turn_status, TurnStatus};
use cocli_driver_core::DriverEvent;

use crate::errors::{classify_grok_error_message, grok_error_class_code};
use crate::events::GrokEvent;
use crate::usage::GrokTurnUsage;

fn grok_stop_reason_to_turn_status(stop: &str) -> TurnStatus {
    match stop.trim().to_lowercase().as_str() {
        "endturn" | "end_turn" | "end turn" => TurnStatus::Completed,
        other => normalize_turn_status(other),
    }
}

pub fn grok_event_to_driver_event(e: GrokEvent, usage: Option<GrokTurnUsage>) -> DriverEvent {
    match e {
        GrokEvent::SessionStarted { session_id } => DriverEvent::SessionStarted { session_id },
        GrokEvent::TextDelta { text } => DriverEvent::TextDelta { text },
        GrokEvent::ThinkingDelta { text } => DriverEvent::ThinkingDelta { text },
        GrokEvent::TurnEnd { stop_reason, .. } => {
            let usage = usage.unwrap_or_default();
            DriverEvent::TurnEnd {
                status: grok_stop_reason_to_turn_status(&stop_reason),
                input_tokens: usage.input_tokens,
                output_tokens: usage.output_tokens,
                cost_usd: 0.0,
                cache_creation_tokens: 0,
                cache_read_tokens: usage.cache_read_tokens,
                context_window_tokens: 0,
            }
        }
        GrokEvent::Error { message, code } => {
            let class = classify_grok_error_message(&message);
            DriverEvent::Error {
                message,
                code: code.or_else(|| grok_error_class_code(class).map(str::to_string)),
                severity: None,
                http_status: None,
            }
        }
        GrokEvent::Unknown => DriverEvent::Unknown,
    }
}

/// Convert a `Vec<GrokEvent>` (from parse_line) into `Vec<DriverEvent>`.
///
/// The "end" line produces two GrokEvents (SessionStarted from its sessionId
/// + TurnEnd). Pass `usage` when mapping the terminal TurnEnd.
pub fn to_driver_events(events: Vec<GrokEvent>, usage: Option<GrokTurnUsage>) -> Vec<DriverEvent> {
    let mut out = Vec::with_capacity(events.len());
    for event in events {
        let event_usage = match &event {
            GrokEvent::TurnEnd { .. } => usage,
            _ => None,
        };
        out.push(grok_event_to_driver_event(event, event_usage));
    }
    out
}

impl From<GrokEvent> for DriverEvent {
    fn from(e: GrokEvent) -> Self {
        grok_event_to_driver_event(e, None)
    }
}
