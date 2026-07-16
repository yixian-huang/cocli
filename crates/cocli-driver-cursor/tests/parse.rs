use cocli_driver_core::{types::TurnStatus, DriverEvent};
use cocli_driver_cursor::events::parse_line;

#[test]
fn cursor_stream_json_maps_session_text_tools_and_turn_end() {
    let session = parse_line(r#"{"type":"system","subtype":"init","session_id":"sid-1"}"#);
    assert!(matches!(
        session.as_slice(),
        [DriverEvent::SessionStarted { session_id }] if session_id == "sid-1"
    ));

    let assistant = parse_line(
        r##"{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"hmm"},{"type":"text","text":"hello"},{"type":"tool_use","name":"mcp__chat__send_message","input":{"target":"#general"}}]}}"##,
    );
    assert!(matches!(assistant[0], DriverEvent::ThinkingDelta { .. }));
    assert!(matches!(assistant[1], DriverEvent::TextDelta { .. }));
    assert!(matches!(assistant[2], DriverEvent::ToolCall { .. }));

    let result = parse_line(r#"{"type":"result","subtype":"success","session_id":"sid-1"}"#);
    assert!(matches!(
        result[0],
        DriverEvent::TurnEnd {
            status: TurnStatus::Completed,
            ..
        }
    ));
}
