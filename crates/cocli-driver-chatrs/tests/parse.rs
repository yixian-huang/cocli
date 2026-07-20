//! Parser regression tests — fixtures lifted from
//! `daemon/drivers/chatrs_test.go`. Each test confirms `parse_line` returns
//! the expected `ChatrsEvent` variant for the canonical wire shape.

use cocli_driver_chatrs::*;

#[test]
fn parses_init() {
    let line = r#"{"kind":"init","session_id":"abc-123"}"#;
    match parse_line(line) {
        ChatrsEvent::Init { session_id } => assert_eq!(session_id, "abc-123"),
        other => panic!("expected Init, got {:?}", other),
    }
}

#[test]
fn parses_text_delta() {
    let line = r#"{"kind":"text_delta","content":"Hello, world!"}"#;
    match parse_line(line) {
        ChatrsEvent::TextDelta { content } => assert_eq!(content, "Hello, world!"),
        other => panic!("expected TextDelta, got {:?}", other),
    }
}

#[test]
fn parses_thinking_delta() {
    let line = r#"{"kind":"thinking_delta","content":"Let me think..."}"#;
    match parse_line(line) {
        ChatrsEvent::ThinkingDelta { content } => assert_eq!(content, "Let me think..."),
        other => panic!("expected ThinkingDelta, got {:?}", other),
    }
}

#[test]
fn parses_tool_use() {
    let line = r#"{"kind":"tool_use","id":"call-42","name":"mcp__chat__send_message","args":{"channel_id":"chan-1","content":"hi"}}"#;
    match parse_line(line) {
        ChatrsEvent::ToolUse { id, name, args } => {
            assert_eq!(id, "call-42");
            assert_eq!(name, "mcp__chat__send_message");
            assert_eq!(args["channel_id"], "chan-1");
            assert_eq!(args["content"], "hi");
        }
        other => panic!("expected ToolUse, got {:?}", other),
    }
}

#[test]
fn parses_tool_done_success() {
    let line =
        r#"{"kind":"tool_done","id":"call-42","ok":true,"output":"Message sent successfully"}"#;
    match parse_line(line) {
        ChatrsEvent::ToolDone { id, ok, output } => {
            assert_eq!(id, "call-42");
            assert!(ok);
            assert_eq!(output, "Message sent successfully");
        }
        other => panic!("expected ToolDone, got {:?}", other),
    }
}

#[test]
fn parses_tool_done_failure() {
    let line = r#"{"kind":"tool_done","id":"call-99","ok":false,"output":"permission denied"}"#;
    match parse_line(line) {
        ChatrsEvent::ToolDone { id, ok, output } => {
            assert_eq!(id, "call-99");
            assert!(!ok);
            assert_eq!(output, "permission denied");
        }
        other => panic!("expected ToolDone, got {:?}", other),
    }
}

#[test]
fn parses_turn_end() {
    let line = r#"{"kind":"turn_end","status":"completed","input_tokens":1500,"output_tokens":250,"cost_usd":0.00042,"context_window":200000,"cache_creation":100,"cache_read":1400}"#;
    match parse_line(line) {
        ChatrsEvent::TurnEnd {
            status,
            input_tokens,
            output_tokens,
            cost_usd,
            context_window,
            cache_creation_tokens,
            cache_read_tokens,
        } => {
            assert_eq!(status, "completed");
            assert_eq!(input_tokens, 1500);
            assert_eq!(output_tokens, 250);
            assert!((cost_usd - 0.00042).abs() < 1e-9);
            assert_eq!(context_window, 200_000);
            assert_eq!(cache_creation_tokens, 100);
            assert_eq!(cache_read_tokens, 1400);
        }
        other => panic!("expected TurnEnd, got {:?}", other),
    }
}

#[test]
fn parses_rate_limit() {
    let line = r#"{"kind":"rate_limit","type":"five_hour","status":"limited","resets":1746700000}"#;
    match parse_line(line) {
        ChatrsEvent::RateLimit {
            limit_type,
            status,
            resets_at,
        } => {
            assert_eq!(limit_type, "five_hour");
            assert_eq!(status, "limited");
            assert_eq!(resets_at, 1_746_700_000);
        }
        other => panic!("expected RateLimit, got {:?}", other),
    }
}

#[test]
fn parses_error_retryable() {
    let line = r#"{"kind":"error","message":"upstream overloaded","retryable":true,"terminal":false,"overflow":false}"#;
    match parse_line(line) {
        ChatrsEvent::Error {
            message,
            retryable,
            terminal,
            overflow,
            code,
        } => {
            assert_eq!(message, "upstream overloaded");
            assert!(retryable);
            assert!(!terminal);
            assert!(!overflow);
            assert!(code.is_none());
        }
        other => panic!("expected Error, got {:?}", other),
    }
}

#[test]
fn parses_error_terminal_with_code() {
    let line = r#"{"kind":"error","message":"invalid API key","retryable":false,"terminal":true,"overflow":false,"code":"auth_failed"}"#;
    match parse_line(line) {
        ChatrsEvent::Error { terminal, code, .. } => {
            assert!(terminal);
            assert_eq!(code.as_deref(), Some("auth_failed"));
        }
        other => panic!("expected Error, got {:?}", other),
    }
}

#[test]
fn parses_blank_and_malformed_safely() {
    // Blank line — Unknown.
    assert!(matches!(parse_line(""), ChatrsEvent::Unknown));
    // Malformed JSON — Unknown.
    assert!(matches!(
        parse_line("{not valid json"),
        ChatrsEvent::Unknown
    ));
    // Unknown kind — Unknown.
    assert!(matches!(
        parse_line(r#"{"kind":"future_unknown_event","payload":{}}"#),
        ChatrsEvent::Unknown
    ));
}

#[test]
fn encode_user_message_user_mode() {
    use cocli_driver_core::types::MessageMode;
    let s = encode_stdin_message("hi there", None, MessageMode::User);
    let v: serde_json::Value = serde_json::from_str(&s).unwrap();
    assert_eq!(v["kind"], "user");
    assert_eq!(v["text"], "hi there");
    // Phase 2a-prime contract: actor adds the newline; encoder MUST NOT.
    assert!(!s.ends_with('\n'));
}

#[test]
fn encode_user_message_notification_mode() {
    use cocli_driver_core::types::MessageMode;
    let s = encode_stdin_message("system event", None, MessageMode::Notification);
    let v: serde_json::Value = serde_json::from_str(&s).unwrap();
    assert_eq!(v["kind"], "system");
    assert_eq!(v["text"], "system event");
}
