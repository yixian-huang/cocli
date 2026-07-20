//! `From<ChatrsEvent> for DriverEvent` — chatrs JSONL maps cleanly to the
//! unified event enum. chatrs emits a distinct `tool_done` (success/error
//! demuxed by the `ok` boolean) so we map to `DriverEvent::ToolDone`, not
//! `ToolResult` (which claude uses for combined response+result blocks).
//!
//! Phase 2a-prime new fields:
//! - `TurnEnd.status` is normalised via `normalize_turn_status` (chatrs
//!   advertises `"completed" | "cancelled" | "failed" | "max_steps"`).
//! - `RateLimit.overage_*` default to None/None/false (chatrs V1 has no
//!   overage signalling — codex driver will populate these in Task 1d).
//! - `Error.severity` defaults to None (chatrs has retryable/terminal/
//!   overflow booleans but no severity classification).

use cocli_driver_core::types::normalize_turn_status;
use cocli_driver_core::DriverEvent;

use crate::events::ChatrsEvent;

impl From<ChatrsEvent> for DriverEvent {
    fn from(e: ChatrsEvent) -> Self {
        match e {
            ChatrsEvent::Init { session_id } => DriverEvent::SessionStarted { session_id },
            ChatrsEvent::TextDelta { content } => DriverEvent::TextDelta { text: content },
            ChatrsEvent::ThinkingDelta { content } => DriverEvent::ThinkingDelta { text: content },
            ChatrsEvent::ToolUse { id, name, args } => DriverEvent::ToolCall {
                id,
                name,
                input: args,
            },
            ChatrsEvent::ToolDone { id, ok, output } => {
                if ok {
                    DriverEvent::ToolDone {
                        id,
                        result: output,
                        error: None,
                    }
                } else {
                    DriverEvent::ToolDone {
                        id,
                        result: String::new(),
                        error: Some(output),
                    }
                }
            }
            ChatrsEvent::TurnEnd {
                status,
                input_tokens,
                output_tokens,
                cost_usd,
                context_window,
                cache_creation_tokens,
                cache_read_tokens,
            } => DriverEvent::TurnEnd {
                status: normalize_turn_status(&status),
                input_tokens,
                output_tokens,
                cost_usd,
                cache_creation_tokens,
                cache_read_tokens,
                context_window_tokens: context_window,
            },
            ChatrsEvent::RateLimit {
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
            ChatrsEvent::Error {
                message,
                retryable: _,
                terminal: _,
                overflow: _,
                code,
            } => DriverEvent::Error {
                message,
                code,
                severity: None,
                http_status: None,
            },
            ChatrsEvent::Unknown => DriverEvent::Unknown,
        }
    }
}
