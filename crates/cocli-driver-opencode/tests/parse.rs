use cocli_driver_core::{types::TurnStatus, DriverEvent};
use cocli_driver_opencode::events::parse_line;

#[test]
fn opencode_json_maps_session_text_tools_and_turn_end() {
    let session = parse_line(r#"{"type":"session","id":"sid-1"}"#);
    assert!(matches!(
        session.as_slice(),
        [DriverEvent::SessionStarted { session_id }] if session_id == "sid-1"
    ));

    let assistant = parse_line(
        r##"{"type":"assistant","content":[{"type":"text","text":"hello"},{"type":"tool","id":"tool-1","name":"mcp__chat__send_message","input":{"target":"#general"}}]}"##,
    );
    assert!(matches!(assistant[0], DriverEvent::TextDelta { .. }));
    assert!(matches!(assistant[1], DriverEvent::ToolCall { .. }));

    let result = parse_line(
        r#"{"type":"result","status":"success","usage":{"input_tokens":3,"output_tokens":5}}"#,
    );
    assert!(matches!(
        result[0],
        DriverEvent::TurnEnd {
            status: TurnStatus::Completed,
            input_tokens: 3,
            output_tokens: 5,
            ..
        }
    ));
}
