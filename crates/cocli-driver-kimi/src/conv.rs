//! `From<KimiEvent> for Vec<DriverEvent>` — stream-json shapes → runtime-neutral
//! events.

use cocli_driver_core::DriverEvent;

use crate::events::KimiEvent;

impl From<KimiEvent> for Vec<DriverEvent> {
    fn from(e: KimiEvent) -> Self {
        match e {
            KimiEvent::SessionStarted { session_id } => {
                vec![DriverEvent::SessionStarted { session_id }]
            }
            KimiEvent::ThinkingDelta { text } => vec![DriverEvent::ThinkingDelta { text }],
            KimiEvent::TextDelta { text } => vec![DriverEvent::TextDelta { text }],
            KimiEvent::ToolCall { name, input } => vec![DriverEvent::ToolCall {
                id: String::new(),
                name,
                input,
            }],
            KimiEvent::TurnEnd => vec![DriverEvent::TurnEnd {
                status: cocli_driver_core::types::TurnStatus::Completed,
                input_tokens: 0,
                output_tokens: 0,
                cost_usd: 0.0,
                cache_creation_tokens: 0,
                cache_read_tokens: 0,
                context_window_tokens: 0,
            }],
            KimiEvent::CompactStarted => vec![DriverEvent::CompactStarted],
            KimiEvent::CompactFinished => vec![DriverEvent::CompactFinished],
            KimiEvent::Error { message } => vec![DriverEvent::Error {
                message,
                code: None,
                severity: None,
                http_status: None,
            }],
            KimiEvent::Unknown => Vec::new(),
        }
    }
}
