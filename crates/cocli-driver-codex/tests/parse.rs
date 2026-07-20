//! Stateless parser tests for codex JSON-RPC fixture lines.
//!
//! These exercise `parse_line` directly (stateless); behaviours that
//! depend on driver state (token usage merge into TurnEnd, error
//! enrichment with cached rate-limit snapshot) live in
//! `tests/driver_impl.rs`.

use cocli_driver_codex::types::{is_rate_limit_message, CodexErrorInfo};
use cocli_driver_codex::{parse_line, CodexEvent};

#[test]
fn parses_thread_started() {
    let line = r#"{"jsonrpc":"2.0","method":"thread/started","params":{"threadId":"th-abc"}}"#;
    let evs = parse_line(line);
    assert_eq!(evs.len(), 1);
    match &evs[0] {
        CodexEvent::SessionStarted { session_id } => assert_eq!(session_id, "th-abc"),
        other => panic!("expected SessionStarted, got {other:?}"),
    }
}

#[test]
fn parses_turn_started_with_turn_id() {
    let line = r#"{"jsonrpc":"2.0","method":"turn/started","params":{"turnId":"turn-1"}}"#;
    let evs = parse_line(line);
    assert_eq!(evs.len(), 1);
    match &evs[0] {
        CodexEvent::TurnStarted { turn_id } => assert_eq!(turn_id.as_deref(), Some("turn-1")),
        other => panic!("expected TurnStarted, got {other:?}"),
    }
}

#[test]
fn parses_turn_started_with_nested_turn_id() {
    let line = r#"{"jsonrpc":"2.0","method":"turn/started","params":{"turn":{"id":"turn-9"}}}"#;
    let evs = parse_line(line);
    assert_eq!(evs.len(), 1);
    match &evs[0] {
        CodexEvent::TurnStarted { turn_id } => assert_eq!(turn_id.as_deref(), Some("turn-9")),
        other => panic!("expected TurnStarted, got {other:?}"),
    }
}

#[test]
fn parses_turn_started_without_turn_id() {
    let line = r#"{"jsonrpc":"2.0","method":"turn/started","params":{}}"#;
    let evs = parse_line(line);
    assert_eq!(evs.len(), 1);
    match &evs[0] {
        CodexEvent::TurnStarted { turn_id } => assert!(turn_id.is_none()),
        other => panic!("expected TurnStarted, got {other:?}"),
    }
}

#[test]
fn parses_turn_completed_status_top_level() {
    let line = r#"{"jsonrpc":"2.0","method":"turn/completed","params":{"turnId":"t1","status":"cancelled"}}"#;
    let evs = parse_line(line);
    match &evs[0] {
        CodexEvent::TurnEnd { status, .. } => assert_eq!(status, "cancelled"),
        other => panic!("expected TurnEnd, got {other:?}"),
    }
}

#[test]
fn parses_turn_completed_nested_status() {
    let line = r#"{"jsonrpc":"2.0","method":"turn/completed","params":{"turn":{"id":"t1","status":"failed"}}}"#;
    let evs = parse_line(line);
    match &evs[0] {
        CodexEvent::TurnEnd { status, .. } => assert_eq!(status, "failed"),
        other => panic!("expected TurnEnd, got {other:?}"),
    }
}

#[test]
fn parses_token_usage_silently() {
    let line = r#"{"jsonrpc":"2.0","method":"thread/tokenUsage/updated","params":{"tokenUsage":{"last":{"inputTokens":300,"outputTokens":75,"cachedInputTokens":50},"modelContextWindow":128000}}}"#;
    let evs = parse_line(line);
    assert_eq!(evs.len(), 1);
    match &evs[0] {
        CodexEvent::TokenUsage {
            input_tokens,
            output_tokens,
            cached_input_tokens,
            model_context_window,
        } => {
            assert_eq!(*input_tokens, 300);
            assert_eq!(*output_tokens, 75);
            assert_eq!(*cached_input_tokens, 50);
            assert_eq!(*model_context_window, 128_000);
        }
        other => panic!("expected TokenUsage, got {other:?}"),
    }
}

#[test]
fn parses_item_started_mcp_tool_call() {
    let line = r#"{"jsonrpc":"2.0","method":"item/started","params":{"item":{"type":"mcpToolCall","tool":"send_message","id":"i1"}}}"#;
    let evs = parse_line(line);
    match &evs[0] {
        CodexEvent::ToolCall { id, name, input } => {
            assert_eq!(id, "i1");
            assert_eq!(name, "send_message");
            assert!(input.is_null());
        }
        other => panic!("expected ToolCall, got {other:?}"),
    }
}

#[test]
fn parses_item_started_command_execution() {
    let line = r#"{"jsonrpc":"2.0","method":"item/started","params":{"item":{"type":"commandExecution","id":"i1","command":"ls"}}}"#;
    let evs = parse_line(line);
    match &evs[0] {
        CodexEvent::ToolCall { name, input, .. } => {
            assert_eq!(name, "command_execution");
            assert_eq!(input["command"], "ls");
        }
        other => panic!("expected ToolCall, got {other:?}"),
    }
}

#[test]
fn parses_item_started_dynamic_tool_call_with_namespace() {
    let line = r#"{"jsonrpc":"2.0","method":"item/started","params":{"item":{"type":"dynamicToolCall","id":"i1","tool":"search","namespace":"web"}}}"#;
    let evs = parse_line(line);
    match &evs[0] {
        CodexEvent::ToolCall { name, .. } => assert_eq!(name, "web__search"),
        other => panic!("expected ToolCall, got {other:?}"),
    }
}

#[test]
fn parses_item_started_dynamic_tool_call_no_namespace() {
    let line = r#"{"jsonrpc":"2.0","method":"item/started","params":{"item":{"type":"dynamicToolCall","id":"i1","tool":"plain_tool"}}}"#;
    let evs = parse_line(line);
    match &evs[0] {
        CodexEvent::ToolCall { name, .. } => assert_eq!(name, "plain_tool"),
        other => panic!("expected ToolCall, got {other:?}"),
    }
}

#[test]
fn parses_item_started_reasoning_as_thinking() {
    let line = r#"{"jsonrpc":"2.0","method":"item/started","params":{"item":{"type":"reasoning","id":"i1"}}}"#;
    let evs = parse_line(line);
    assert!(matches!(&evs[0], CodexEvent::Thinking));
}

#[test]
fn parses_item_started_agent_message_silently() {
    let line = r#"{"jsonrpc":"2.0","method":"item/started","params":{"item":{"type":"agentMessage","id":"i1","text":""}}}"#;
    let evs = parse_line(line);
    assert!(evs.is_empty());
}

#[test]
fn parses_item_completed_agent_message_as_text() {
    let line = r#"{"jsonrpc":"2.0","method":"item/completed","params":{"item":{"type":"agentMessage","id":"i1","text":"final answer"}}}"#;
    let evs = parse_line(line);
    match &evs[0] {
        CodexEvent::Text { text } => assert_eq!(text, "final answer"),
        other => panic!("expected Text, got {other:?}"),
    }
}

#[test]
fn parses_item_completed_plan_with_tag() {
    let line = r#"{"jsonrpc":"2.0","method":"item/completed","params":{"item":{"type":"plan","id":"i1","text":"step 1\nstep 2"}}}"#;
    let evs = parse_line(line);
    match &evs[0] {
        CodexEvent::Text { text } => assert!(text.starts_with("[plan] ")),
        other => panic!("expected Text, got {other:?}"),
    }
}

#[test]
fn parses_item_completed_command_execution_as_tool_done() {
    let line = r#"{"jsonrpc":"2.0","method":"item/completed","params":{"item":{"type":"commandExecution","id":"i1"}}}"#;
    let evs = parse_line(line);
    match &evs[0] {
        CodexEvent::ToolDone { id } => assert_eq!(id, "i1"),
        other => panic!("expected ToolDone, got {other:?}"),
    }
}

#[test]
fn parses_item_completed_unknown_type_is_silent() {
    let line = r#"{"jsonrpc":"2.0","method":"item/completed","params":{"item":{"type":"futureWidget","id":"i9"}}}"#;
    assert!(parse_line(line).is_empty());
}

#[test]
fn parses_item_completed_error() {
    let line = r#"{"jsonrpc":"2.0","method":"item/completed","params":{"item":{"type":"error","id":"i1","text":"something went wrong"}}}"#;
    let evs = parse_line(line);
    match &evs[0] {
        CodexEvent::Error { message, .. } => assert_eq!(message, "something went wrong"),
        other => panic!("expected Error, got {other:?}"),
    }
}

#[test]
fn parses_thread_compacted() {
    let line = r#"{"jsonrpc":"2.0","method":"thread/compacted","params":{}}"#;
    let evs = parse_line(line);
    assert!(matches!(&evs[0], CodexEvent::ThreadCompacted));
}

#[test]
fn parses_thread_closed() {
    let line = r#"{"jsonrpc":"2.0","method":"thread/closed","params":{"threadId":"th-1"}}"#;
    let evs = parse_line(line);
    match &evs[0] {
        CodexEvent::ThreadClosed { thread_id } => assert_eq!(thread_id, "th-1"),
        other => panic!("expected ThreadClosed, got {other:?}"),
    }
}

#[test]
fn parses_model_rerouted() {
    let line = r#"{"jsonrpc":"2.0","method":"model/rerouted","params":{"fromModel":"gpt-5","toModel":"gpt-5-safe","reason":"highRiskCyberActivity"}}"#;
    let evs = parse_line(line);
    match &evs[0] {
        CodexEvent::ModelRerouted {
            from_model,
            to_model,
            reason,
        } => {
            assert_eq!(from_model, "gpt-5");
            assert_eq!(to_model, "gpt-5-safe");
            assert_eq!(reason, "highRiskCyberActivity");
        }
        other => panic!("expected ModelRerouted, got {other:?}"),
    }
}

#[test]
fn parses_process_exited_zero_is_silent() {
    let line = r#"{"jsonrpc":"2.0","method":"process/exited","params":{"exitCode":0}}"#;
    let evs = parse_line(line);
    assert!(evs.is_empty());
}

#[test]
fn parses_process_exited_nonzero() {
    let line = r#"{"jsonrpc":"2.0","method":"process/exited","params":{"exitCode":1,"processHandle":"sh-1","stderr":"oops"}}"#;
    let evs = parse_line(line);
    match &evs[0] {
        CodexEvent::ProcessExited {
            exit_code,
            handle,
            stderr_excerpt,
        } => {
            assert_eq!(*exit_code, 1);
            assert_eq!(handle, "sh-1");
            assert_eq!(stderr_excerpt, "oops");
        }
        other => panic!("expected ProcessExited, got {other:?}"),
    }
}

#[test]
fn parses_error_notification_string_info() {
    let line = r#"{"jsonrpc":"2.0","method":"error","params":{"error":{"message":"context full","codexErrorInfo":"contextWindowExceeded"}}}"#;
    let evs = parse_line(line);
    match &evs[0] {
        CodexEvent::Error {
            message,
            info,
            http_status,
            will_retry,
        } => {
            assert_eq!(message, "context full");
            assert_eq!(info.as_str(), "contextWindowExceeded");
            assert_eq!(*http_status, 0);
            assert!(!*will_retry);
            // classification
            let c = info.classify();
            assert!(c.overflow);
            assert!(c.recognised);
            let events: Vec<cocli_driver_core::DriverEvent> = evs[0].clone().into();
            match &events[0] {
                cocli_driver_core::DriverEvent::Error { severity, .. } => {
                    assert_eq!(*severity, Some(cocli_driver_core::ErrorSeverity::Error));
                }
                other => panic!("expected DriverEvent::Error, got {other:?}"),
            }
        }
        other => panic!("expected Error, got {other:?}"),
    }
}

#[test]
fn parses_error_notification_object_info_with_http_status() {
    let line = r#"{"jsonrpc":"2.0","method":"error","params":{"error":{"message":"transport","codexErrorInfo":{"responseStreamDisconnected":{"httpStatusCode":503}}}}}"#;
    let evs = parse_line(line);
    match &evs[0] {
        CodexEvent::Error {
            info, http_status, ..
        } => {
            assert_eq!(info.as_str(), "responseStreamDisconnected");
            assert_eq!(*http_status, 503);
            let c = info.classify();
            assert!(c.retryable);
        }
        other => panic!("expected Error, got {other:?}"),
    }
}

#[test]
fn parses_error_notification_will_retry_flag() {
    let line = r#"{"jsonrpc":"2.0","method":"error","params":{"willRetry":true,"error":{"message":"hiccup","codexErrorInfo":"serverOverloaded"}}}"#;
    let evs = parse_line(line);
    match &evs[0] {
        CodexEvent::Error { will_retry, .. } => assert!(*will_retry),
        other => panic!("expected Error, got {other:?}"),
    }
}

#[test]
fn parses_account_rate_limits_updated_bucket_reached() {
    let line = r#"{"jsonrpc":"2.0","method":"account/rateLimits/updated","params":{"rateLimits":{"limitId":"codex","primary":{"usedPercent":100,"windowDurationMins":300,"resetsAt":1700000000}}}}"#;
    let evs = parse_line(line);
    match &evs[0] {
        CodexEvent::RateLimits { snapshot } => {
            assert!(snapshot.bucket_reached());
            assert_eq!(snapshot.limit_id, "codex");
            let primary = snapshot.primary.clone().unwrap();
            assert_eq!(primary.used_percent, 100);
            assert_eq!(primary.window_duration_min, 300);
            assert_eq!(primary.resets_at, 1_700_000_000);
        }
        other => panic!("expected RateLimits, got {other:?}"),
    }
}

#[test]
fn approval_server_request_emits_auto_approve_write() {
    let line = r#"{"jsonrpc":"2.0","id":42,"method":"item/approvalRequest","params":{}}"#;
    let evs = parse_line(line);
    match &evs[0] {
        CodexEvent::Write { data } => {
            let v: serde_json::Value = serde_json::from_str(data).unwrap();
            assert_eq!(v["jsonrpc"], "2.0");
            assert_eq!(v["id"], 42);
            assert_eq!(v["result"]["approved"], true);
        }
        other => panic!("expected Write, got {other:?}"),
    }
}

#[test]
fn approval_notification_without_id_is_silent() {
    let line = r#"{"jsonrpc":"2.0","method":"item/approvalRequest","params":{}}"#;
    assert!(parse_line(line).is_empty());
}

#[test]
fn unknown_server_request_surfaces_unknown_event() {
    let line = r#"{"jsonrpc":"2.0","id":77,"method":"server/futureRequest","params":{}}"#;
    let evs = parse_line(line);
    assert_eq!(evs.len(), 2);
    match &evs[0] {
        CodexEvent::Unknown { reason } => {
            assert!(reason.contains("unknown server request server/futureRequest"));
        }
        other => panic!("expected Unknown, got {other:?}"),
    }
    match &evs[1] {
        CodexEvent::Write { data } => {
            let v: serde_json::Value = serde_json::from_str(data).unwrap();
            assert_eq!(v["id"], 77);
            assert_eq!(v["error"]["code"], -32601);
            assert_eq!(v["error"]["message"], "Method not found");
        }
        other => panic!("expected Write, got {other:?}"),
    }
}

#[test]
fn unknown_method_surfaces_unknown_event() {
    let line = r#"{"jsonrpc":"2.0","method":"future/unknown","params":{}}"#;
    let evs = parse_line(line);
    match &evs[0] {
        CodexEvent::Unknown { reason } => {
            assert!(reason.contains("unknown method future/unknown"));
        }
        other => panic!("expected Unknown, got {other:?}"),
    }
}

#[test]
fn known_silent_notification_returns_empty() {
    let line = r#"{"jsonrpc":"2.0","method":"thread/status/changed","params":{}}"#;
    assert!(parse_line(line).is_empty());
}

#[test]
fn known_silent_prefix_notification_returns_empty() {
    let line = r#"{"jsonrpc":"2.0","method":"mcpServer/chat/startupStatus/updated","params":{}}"#;
    assert!(parse_line(line).is_empty());
}

#[test]
fn unknown_item_type_surfaces_unknown_event() {
    let line = r#"{"jsonrpc":"2.0","method":"item/started","params":{"item":{"type":"brandNewTool","id":"x1"}}}"#;
    let evs = parse_line(line);
    match &evs[0] {
        CodexEvent::Unknown { reason } => {
            assert!(reason.contains("unknown item type brandNewTool"));
        }
        other => panic!("expected Unknown, got {other:?}"),
    }
}

#[test]
fn process_exited_stderr_truncation_is_utf8_safe() {
    let stderr = "错".repeat(100);
    let line = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "process/exited",
        "params": {
            "exitCode": 1,
            "processHandle": "proc-utf8",
            "stderr": stderr,
        },
    })
    .to_string();
    let evs = parse_line(&line);
    match &evs[0] {
        CodexEvent::ProcessExited { stderr_excerpt, .. } => {
            assert!(stderr_excerpt.ends_with("..."));
            assert!(stderr_excerpt.is_char_boundary(stderr_excerpt.len()));
        }
        other => panic!("expected ProcessExited, got {other:?}"),
    }
}

#[test]
fn unparseable_json_returns_empty() {
    assert!(parse_line("not even json").is_empty());
}

#[test]
fn blank_line_returns_empty() {
    assert!(parse_line("   ").is_empty());
}

#[test]
fn rate_limit_message_text_classifier() {
    assert!(is_rate_limit_message("Rate limit exceeded"));
    assert!(is_rate_limit_message("HTTP 429"));
    assert!(is_rate_limit_message("quota_exhausted"));
    assert!(!is_rate_limit_message("network unreachable"));
}

#[test]
fn classify_codex_error_info_known_variants() {
    use cocli_driver_codex::types::ErrorClassification;
    let overflow = CodexErrorInfo("contextWindowExceeded".to_string()).classify();
    assert!(matches!(
        overflow,
        ErrorClassification {
            overflow: true,
            recognised: true,
            ..
        }
    ));
    let terminal = CodexErrorInfo("Unauthorized".to_string()).classify();
    assert!(terminal.terminal);
    let not_steerable = CodexErrorInfo("activeTurnNotSteerable".to_string()).classify();
    assert!(not_steerable.retryable);
    assert!(not_steerable.not_steerable);
}
