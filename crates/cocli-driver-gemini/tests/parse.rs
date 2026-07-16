//! Wire-shape fixture tests for `parse_line`. Fixtures come from
//! `daemon/drivers/gemini_test.go` so the Go and Rust parsers stay
//! observable-equivalent on shared lines.

use cocli_driver_gemini::*;

#[test]
fn parses_session_started() {
    let line = r#"{"type":"init","session_id":"sess-123","model":"gemini-2.5-pro"}"#;
    let evs = parse_line(line);
    assert_eq!(evs.len(), 1);
    match &evs[0] {
        GeminiEvent::SessionStarted { session_id } => assert_eq!(session_id, "sess-123"),
        other => panic!("expected SessionStarted, got {other:?}"),
    }
}

#[test]
fn parses_message_assistant_text() {
    let line = r#"{"type":"message","role":"assistant","content":"hello","delta":true}"#;
    let evs = parse_line(line);
    assert_eq!(evs.len(), 1);
    match &evs[0] {
        GeminiEvent::TextDelta { text } => assert_eq!(text, "hello"),
        other => panic!("expected TextDelta, got {other:?}"),
    }
}

#[test]
fn parses_message_non_assistant_role_is_unknown() {
    let line = r#"{"type":"message","role":"user","content":"x"}"#;
    let evs = parse_line(line);
    assert_eq!(evs.len(), 1);
    assert!(matches!(evs[0], GeminiEvent::Unknown));
}

#[test]
fn parses_tool_use() {
    let line = r#"{"type":"tool_use","tool_name":"mcp_chat_send_message","tool_id":"call_xyz_001","parameters":{"text":"hi"}}"#;
    let evs = parse_line(line);
    assert_eq!(evs.len(), 1);
    match &evs[0] {
        GeminiEvent::ToolCall { id, name, input } => {
            assert_eq!(id, "call_xyz_001");
            assert_eq!(name, "mcp_chat_send_message");
            assert_eq!(input["text"], "hi");
        }
        other => panic!("expected ToolCall, got {other:?}"),
    }
}

#[test]
fn parses_tool_result_success() {
    let line = r#"{"type":"tool_result","tool_id":"call_4","status":"success","output":"done"}"#;
    let evs = parse_line(line);
    assert_eq!(evs.len(), 1);
    match &evs[0] {
        GeminiEvent::ToolResult { id, output, error } => {
            assert_eq!(id, "call_4");
            assert_eq!(output, "done");
            assert!(error.is_none());
        }
        other => panic!("expected ToolResult, got {other:?}"),
    }
}

#[test]
fn parses_tool_result_error_with_type_and_message() {
    let line = r#"{"type":"tool_result","tool_id":"call_1","status":"error","error":{"type":"TOOL_EXECUTION_ERROR","message":"command not found: xyz"}}"#;
    let evs = parse_line(line);
    assert_eq!(evs.len(), 1);
    match &evs[0] {
        GeminiEvent::ToolResult { id, output, error } => {
            assert_eq!(id, "call_1");
            assert_eq!(output, "");
            assert_eq!(
                error.as_deref(),
                Some("TOOL_EXECUTION_ERROR: command not found: xyz")
            );
        }
        other => panic!("expected ToolResult, got {other:?}"),
    }
}

#[test]
fn parses_tool_result_error_message_only() {
    let line = r#"{"type":"tool_result","tool_id":"call_2","status":"error","error":{"message":"timeout"}}"#;
    let evs = parse_line(line);
    match &evs[0] {
        GeminiEvent::ToolResult { error, .. } => {
            assert_eq!(error.as_deref(), Some("timeout"));
        }
        other => panic!("expected ToolResult, got {other:?}"),
    }
}

#[test]
fn parses_tool_result_error_no_detail_fallback() {
    let line = r#"{"type":"tool_result","tool_id":"call_3","status":"error"}"#;
    let evs = parse_line(line);
    match &evs[0] {
        GeminiEvent::ToolResult { error, .. } => {
            assert_eq!(error.as_deref(), Some("tool reported error (no detail)"));
        }
        other => panic!("expected ToolResult, got {other:?}"),
    }
}

#[test]
fn parses_result_success_with_stats() {
    let line = r#"{"type":"result","status":"success","stats":{"total_tokens":15000,"input_tokens":12000,"output_tokens":3000,"cached":8000,"input":4000,"duration_ms":18532,"tool_calls":4}}"#;
    let evs = parse_line(line);
    assert_eq!(evs.len(), 1);
    match &evs[0] {
        GeminiEvent::TurnEnd {
            input_tokens,
            output_tokens,
            cache_read_tokens,
            error_before_turn_end,
        } => {
            assert_eq!(*input_tokens, 12000);
            assert_eq!(*output_tokens, 3000);
            assert_eq!(*cache_read_tokens, 8000);
            assert!(error_before_turn_end.is_none());
        }
        other => panic!("expected TurnEnd, got {other:?}"),
    }
}

#[test]
fn parses_result_with_nested_error_object() {
    // {error: {type, message}} shape (e.g. INVALID_STREAM)
    let line = r#"{"type":"result","status":"error","error":{"type":"INVALID_STREAM","message":"Invalid stream: malformed tool call"},"stats":{"input_tokens":100,"output_tokens":20}}"#;
    let evs = parse_line(line);
    assert_eq!(evs.len(), 1);
    match &evs[0] {
        GeminiEvent::TurnEnd {
            input_tokens,
            output_tokens,
            error_before_turn_end,
            ..
        } => {
            assert_eq!(*input_tokens, 100);
            assert_eq!(*output_tokens, 20);
            assert_eq!(
                error_before_turn_end.as_deref(),
                Some("Invalid stream: malformed tool call")
            );
        }
        other => panic!("expected TurnEnd, got {other:?}"),
    }
}

#[test]
fn parses_error_top_level_message() {
    let line = r#"{"type":"error","severity":"error","message":"something unexpected happened"}"#;
    let evs = parse_line(line);
    assert_eq!(evs.len(), 1);
    match &evs[0] {
        GeminiEvent::Error {
            message,
            severity,
            code,
        } => {
            assert_eq!(message, "something unexpected happened");
            assert_eq!(severity.as_deref(), Some("error"));
            assert!(code.is_none());
        }
        other => panic!("expected Error, got {other:?}"),
    }
}

#[test]
fn parses_error_warning_severity() {
    let line =
        r#"{"type":"error","severity":"warning","message":"Loop detected, stopping execution"}"#;
    let evs = parse_line(line);
    match &evs[0] {
        GeminiEvent::Error { severity, .. } => {
            assert_eq!(severity.as_deref(), Some("warning"));
        }
        other => panic!("expected Error, got {other:?}"),
    }
}

#[test]
fn parses_unknown_type_safely() {
    let line = r#"{"type":"unknown_future_type","data":{}}"#;
    let evs = parse_line(line);
    assert_eq!(evs.len(), 1);
    assert!(matches!(evs[0], GeminiEvent::Unknown));
}

#[test]
fn parses_invalid_json_as_unknown() {
    let line = "not even json";
    let evs = parse_line(line);
    assert_eq!(evs.len(), 1);
    assert!(matches!(evs[0], GeminiEvent::Unknown));
}

#[test]
fn parses_empty_line_as_unknown() {
    let evs = parse_line("");
    assert_eq!(evs.len(), 1);
    assert!(matches!(evs[0], GeminiEvent::Unknown));
}
