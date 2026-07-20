//! `From<ClaudeEvent> for DriverEvent` — one-to-one mapping (claude is the
//! validator runtime, so the unified enum mirrors claude's shape).
//!
//! Phase 2a-prime updates: TurnEnd arm defaults `status: TurnStatus::Completed`
//! (claude stream-json has no turn-status field; cancellation classification
//! happens at the actor level). RateLimit defaults overage_* to None/false.
//! Error defaults `severity` to None. ToolResult mapping preserved (NOT mapped
//! to ToolDone — claude's stream-json uses ToolResult; Phase 2b drivers that
//! emit distinct done-events will populate ToolDone where appropriate).

use cocli_driver_core::DriverEvent;

use crate::events::ClaudeEvent;

impl From<ClaudeEvent> for DriverEvent {
    fn from(e: ClaudeEvent) -> Self {
        match e {
            ClaudeEvent::SessionStarted { session_id } => {
                DriverEvent::SessionStarted { session_id }
            }
            ClaudeEvent::MessageStart => DriverEvent::MessageStart,
            ClaudeEvent::ThinkingDelta { text } => DriverEvent::ThinkingDelta { text },
            ClaudeEvent::TextDelta { text } => DriverEvent::TextDelta { text },
            ClaudeEvent::ToolCall { id, name, input } => DriverEvent::ToolCall { id, name, input },
            ClaudeEvent::ToolResult { id, output, error } => {
                DriverEvent::ToolResult { id, output, error }
            }
            ClaudeEvent::MessageStop { stop_reason } => DriverEvent::MessageStop { stop_reason },
            ClaudeEvent::TurnEnd {
                input_tokens,
                output_tokens,
                cost_usd,
                cache_creation_tokens,
                cache_read_tokens,
            } => DriverEvent::TurnEnd {
                status: cocli_driver_core::types::TurnStatus::Completed, // Task 4 will refine
                input_tokens,
                output_tokens,
                cost_usd,
                cache_creation_tokens,
                cache_read_tokens,
                context_window_tokens: 0,
            },
            ClaudeEvent::RateLimit {
                limit_type,
                status,
                resets_at,
            } => DriverEvent::RateLimit {
                limit_type,
                status,
                resets_at,
                overage_status: None,
                overage_resets: None,
                is_using_overage: false,
            },
            ClaudeEvent::Error { message, code } => DriverEvent::Error {
                message,
                code,
                severity: None,
                http_status: None,
            },
            ClaudeEvent::Unknown => DriverEvent::Unknown,
        }
    }
}
