use cocli_driver_claude::*;

#[test]
fn parses_session_started() {
    let line = r#"{"type":"system","session_id":"sess-123","subtype":"init"}"#;
    match parse_line(line) {
        ClaudeEvent::SessionStarted { session_id } => assert_eq!(session_id, "sess-123"),
        other => panic!("expected SessionStarted, got {:?}", other),
    }
}

#[test]
fn parses_text_delta() {
    let line = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"hello"}]}}"#;
    match parse_line(line) {
        ClaudeEvent::TextDelta { text } => assert_eq!(text, "hello"),
        other => panic!("expected TextDelta, got {:?}", other),
    }
}

#[test]
fn parses_tool_call() {
    let line = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"tu_1","name":"send_message","input":{"text":"hi"}}]}}"#;
    match parse_line(line) {
        ClaudeEvent::ToolCall { id, name, .. } => {
            assert_eq!(id, "tu_1");
            assert_eq!(name, "send_message");
        }
        other => panic!("expected ToolCall, got {:?}", other),
    }
}

#[test]
fn parses_turn_end() {
    let line = r#"{"type":"result","usage":{"input_tokens":100,"output_tokens":50,"cache_creation_input_tokens":0,"cache_read_input_tokens":80},"total_cost_usd":0.012}"#;
    match parse_line(line) {
        ClaudeEvent::TurnEnd {
            input_tokens,
            output_tokens,
            cost_usd,
            cache_read_tokens,
            ..
        } => {
            assert_eq!(input_tokens, 100);
            assert_eq!(output_tokens, 50);
            assert_eq!(cache_read_tokens, 80);
            assert!((cost_usd - 0.012).abs() < 1e-6);
        }
        other => panic!("expected TurnEnd, got {:?}", other),
    }
}

#[test]
fn parses_rate_limit() {
    let line = r#"{"type":"rate_limit_event","limit_type":"five_hour","status":"limited","resets_at":1700000000}"#;
    match parse_line(line) {
        ClaudeEvent::RateLimit {
            limit_type,
            status,
            resets_at,
        } => {
            assert_eq!(limit_type, "five_hour");
            assert_eq!(status, "limited");
            assert_eq!(resets_at, 1700000000);
        }
        other => panic!("expected RateLimit, got {:?}", other),
    }
}

#[test]
fn parses_unknown_safely() {
    let line = r#"{"type":"unknown_future_type","data":{}}"#;
    assert!(matches!(parse_line(line), ClaudeEvent::Unknown));
}

#[test]
fn encode_user_message_includes_session() {
    let s = encode_user_message("hello", Some("sid-1"));
    let v: serde_json::Value = serde_json::from_str(&s).unwrap();
    assert_eq!(v["type"], "user");
    assert_eq!(v["session_id"], "sid-1");
    assert_eq!(v["message"]["role"], "user");
    assert_eq!(v["message"]["content"][0]["type"], "text");
    assert_eq!(v["message"]["content"][0]["text"], "hello");
}

#[test]
fn encode_user_message_omits_session_when_none() {
    let s = encode_user_message("hello", None);
    let v: serde_json::Value = serde_json::from_str(&s).unwrap();
    assert!(v.get("session_id").is_none());
    assert_eq!(v["type"], "user");
    assert_eq!(v["message"]["content"][0]["text"], "hello");
}
