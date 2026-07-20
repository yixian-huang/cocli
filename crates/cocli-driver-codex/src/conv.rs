//! Map `CodexEvent` → `DriverEvent`.
//!
//! Codex JSON-RPC is richer than claude's stream-json: a single line may
//! produce multiple driver events (e.g. an `error` line that classifies as
//! rate-limit emits both `Error` and a synthetic `RateLimit`). We return
//! `Vec<DriverEvent>` to model that fan-out cleanly.
//!
//! Helpers in this module are pure (no state — driver.rs handles the
//! stateful pieces like pending-tokens consumption + snapshot caching).

use cocli_driver_core::event::{DriverEvent, ErrorSeverity, SignalType};
use cocli_driver_core::types::{normalize_turn_status, TurnStatus};

use crate::events::CodexEvent;
use crate::types::{is_rate_limit_message, RateLimitSnapshot};

/// Flatten a single `CodexEvent` into zero-or-more `DriverEvent`s.
///
/// NOTE: state-dependent fan-outs (TurnEnd merging with pending-tokens,
/// Error classifying as rate-limit, post-error RateLimit enrichment from
/// snapshot) happen in `driver.rs`; this is the pure mapping.
impl From<CodexEvent> for Vec<DriverEvent> {
    fn from(ev: CodexEvent) -> Self {
        match ev {
            CodexEvent::SessionStarted { session_id } => {
                vec![DriverEvent::SessionStarted { session_id }]
            }
            CodexEvent::Thinking => vec![DriverEvent::ThinkingDelta {
                text: String::new(),
            }],
            CodexEvent::TurnStarted { turn_id: _ } => vec![DriverEvent::ThinkingDelta {
                text: String::new(),
            }],
            CodexEvent::ToolCall { id, name, input } => {
                vec![DriverEvent::ToolCall { id, name, input }]
            }
            CodexEvent::ToolDone { id } => vec![DriverEvent::ToolDone {
                id,
                result: String::new(),
                error: None,
            }],
            CodexEvent::Text { text } => vec![DriverEvent::TextDelta { text }],
            CodexEvent::TurnEnd {
                status,
                input_tokens,
                output_tokens,
                context_window,
                cache_read_tokens,
            } => vec![DriverEvent::TurnEnd {
                status: normalize_turn_status(&status),
                input_tokens,
                output_tokens,
                cost_usd: 0.0,
                cache_creation_tokens: 0,
                cache_read_tokens,
                context_window_tokens: context_window,
            }],
            CodexEvent::TokenUsage { .. } => {
                // Absorbed by driver.rs (stash on pendingTokens). Pure
                // mapping yields nothing — preserves Phase 2a's invariant
                // that TokenUsage is silent until next TurnEnd.
                Vec::new()
            }
            CodexEvent::RateLimits { snapshot } => {
                // Pure mapping: surface as RateLimit only when bucket
                // reached. Driver.rs additionally stashes the snapshot
                // for error enrichment.
                if snapshot.bucket_reached() {
                    vec![event_rate_limit_from_snapshot(&snapshot)]
                } else {
                    Vec::new()
                }
            }
            CodexEvent::ThreadCompacted => vec![DriverEvent::CompactFinished],
            CodexEvent::Error {
                message,
                info,
                http_status,
                will_retry,
            } => {
                if will_retry {
                    // H1 (codex.go:772): codex `willRetry=true` means the
                    // app-server is retrying internally; surfacing as
                    // EventError causes a false-positive attention state.
                    return Vec::new();
                }
                // Pure mapping: surface text + classification severity.
                // Driver.rs additionally fans out RateLimit when
                // `is_rate_limit_message(message)` OR info is
                // `usageLimitExceeded`.
                let class = info.classify();
                // Overflow is non-terminal but must not be downgraded to
                // Warning — the actor skips overflow/fork paths for warnings
                // (gemini recoverable signals). Mirrors Go `parseCodexDriverError`
                // attaching `ErrOverflow()` without `ErrTerminal()`.
                let severity = if class.terminal || class.overflow {
                    Some(ErrorSeverity::Error)
                } else {
                    Some(ErrorSeverity::Warning)
                };
                let http = if http_status > 0 {
                    Some(http_status)
                } else {
                    None
                };
                let mut out = vec![DriverEvent::Error {
                    message: message.clone(),
                    code: Some(info.as_str().to_string()).filter(|s| !s.is_empty()),
                    severity,
                    http_status: http,
                }];
                let info_canon = info.canon();
                if is_rate_limit_message(&message) || info_canon == "usagelimitexceeded" {
                    // Empty snapshot — driver.rs replaces with real one when
                    // available.
                    out.push(DriverEvent::RateLimit {
                        limit_type: "unknown".to_string(),
                        status: "limited".to_string(),
                        resets_at: 0,
                        overage_status: None,
                        overage_resets: None,
                        is_using_overage: false,
                    });
                }
                out
            }
            CodexEvent::ThreadClosed { thread_id } => {
                let msg = if thread_id.is_empty() {
                    "codex thread closed by server".to_string()
                } else {
                    format!("codex thread closed by server (threadId={thread_id})")
                };
                vec![DriverEvent::Error {
                    message: msg,
                    code: Some("threadClosed".to_string()),
                    severity: Some(ErrorSeverity::Error),
                    http_status: None,
                }]
            }
            CodexEvent::ModelRerouted {
                from_model,
                to_model,
                reason,
            } => {
                let text = if reason.is_empty() {
                    format!("[model_rerouted] {from_model} → {to_model}")
                } else {
                    format!("[model_rerouted] {from_model} → {to_model} (reason={reason})")
                };
                vec![DriverEvent::Signal {
                    signal_type: SignalType::Other("model_rerouted".to_string()),
                    data: serde_json::json!({
                        "text": text,
                        "from": from_model,
                        "to": to_model,
                        "reason": reason,
                    }),
                }]
            }
            CodexEvent::ProcessExited {
                handle,
                exit_code,
                stderr_excerpt,
            } => {
                let mut text = format!("[process_exited code={exit_code} handle={handle}]");
                if !stderr_excerpt.is_empty() {
                    text.push('\n');
                    text.push_str(&stderr_excerpt);
                }
                vec![DriverEvent::Signal {
                    signal_type: SignalType::Other("process_exited".to_string()),
                    data: serde_json::json!({
                        "text": text,
                        "handle": handle,
                        "exit_code": exit_code,
                    }),
                }]
            }
            CodexEvent::Write { data } => vec![DriverEvent::Write { data }],
            CodexEvent::Unknown { .. } => Vec::new(),
        }
    }
}

/// Public helper so driver.rs can re-enrich a RateLimit event with a real
/// snapshot. Mirrors Go `eventRateLimitFromSnapshot` (codex.go:1726).
pub(crate) fn event_rate_limit_from_snapshot(snap: &RateLimitSnapshot) -> DriverEvent {
    let limit_type = if !snap.limit_id.is_empty() {
        snap.limit_id.clone()
    } else if let Some(p) = &snap.primary {
        if p.window_duration_min > 0 {
            format!("{}min", p.window_duration_min)
        } else {
            "unknown".to_string()
        }
    } else {
        "unknown".to_string()
    };
    let resets_at = snap.primary.as_ref().map(|p| p.resets_at).unwrap_or(0);
    DriverEvent::RateLimit {
        limit_type,
        status: "limited".to_string(),
        resets_at,
        overage_status: None,
        overage_resets: None,
        is_using_overage: false,
    }
}

/// Helper used by integration tests that need the canonical `TurnStatus`
/// for a raw status string.
#[allow(dead_code)]
pub(crate) fn map_turn_status(raw: &str) -> TurnStatus {
    normalize_turn_status(raw)
}
