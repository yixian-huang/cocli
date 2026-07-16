//! Stream-json parse tests for kimi-code.

use cocli_driver_kimi::{parse_line, KimiEvent};

#[test]
fn parses_assistant_text() {
    let line = r#"{"role":"assistant","content":"hello world"}"#;
    let evs = parse_line(line);
    assert_eq!(evs.len(), 1);
    match &evs[0] {
        KimiEvent::TextDelta { text } => assert_eq!(text, "hello world"),
        other => panic!("expected TextDelta, got {other:?}"),
    }
}

#[test]
fn parses_assistant_empty_content_drops() {
    let line = r#"{"role":"assistant","content":""}"#;
    let evs = parse_line(line);
    assert!(evs.is_empty());
}

#[test]
fn parses_session_resume_hint() {
    let line = r#"{"role":"meta","type":"session.resume_hint","session_id":"sess_abc","command":"kimi -r sess_abc","content":"To resume this session: kimi -r sess_abc"}"#;
    let evs = parse_line(line);
    assert_eq!(evs.len(), 1);
    match &evs[0] {
        KimiEvent::SessionStarted { session_id } => assert_eq!(session_id, "sess_abc"),
        other => panic!("expected SessionStarted, got {other:?}"),
    }
}

#[test]
fn parses_meta_unknown_type_drops() {
    let line = r#"{"role":"meta","type":"telemetry","data":{}}"#;
    let evs = parse_line(line);
    assert!(evs.is_empty());
}

#[test]
fn parses_user_role_drops() {
    let line = r#"{"role":"user","content":"hi"}"#;
    assert!(parse_line(line).is_empty());
}

#[test]
fn parses_blank_line_safely() {
    assert!(parse_line("").is_empty());
    assert!(parse_line("   \n").is_empty());
}

#[test]
fn parses_non_json_drops_silently() {
    assert!(parse_line("kimi-code v0.6.0 starting").is_empty());
}

#[test]
fn parses_missing_role_drops() {
    let line = r#"{"content":"hello"}"#;
    assert!(parse_line(line).is_empty());
}

#[test]
fn parses_wire_content_part_text() {
    let line = r#"{"jsonrpc":"2.0","method":"event","params":{"type":"ContentPart","payload":{"type":"text","text":"hello wire"}}}"#;
    let evs = parse_line(line);
    assert_eq!(evs.len(), 1);
    match &evs[0] {
        KimiEvent::TextDelta { text } => assert_eq!(text, "hello wire"),
        other => panic!("expected TextDelta, got {other:?}"),
    }
}

#[test]
fn parses_wire_tool_call_arguments_json() {
    let line = r##"{"jsonrpc":"2.0","method":"event","params":{"type":"ToolCall","payload":{"function":{"name":"mcp__chat__send_message","arguments":"{\"target\":\"#general\"}"}}}}"##;
    let evs = parse_line(line);
    assert_eq!(evs.len(), 1);
    match &evs[0] {
        KimiEvent::ToolCall { name, input } => {
            assert_eq!(name, "mcp__chat__send_message");
            assert_eq!(input["target"], "#general");
        }
        other => panic!("expected ToolCall, got {other:?}"),
    }
}

#[test]
fn parses_wire_turn_end() {
    let line = r#"{"jsonrpc":"2.0","method":"event","params":{"type":"TurnEnd","payload":{}}}"#;
    assert!(matches!(parse_line(line).as_slice(), [KimiEvent::TurnEnd]));
}
