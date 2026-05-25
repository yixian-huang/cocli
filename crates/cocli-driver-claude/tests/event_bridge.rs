use cocli_driver::Event;
use cocli_driver_claude::parse_line_to_events;

#[test]
fn session_started_maps_to_generic() {
    let line = r#"{"type":"system","subtype":"init","session_id":"abc-123"}"#;
    let evs = parse_line_to_events(line);
    assert!(evs
        .iter()
        .any(|e| matches!(e, Event::SessionStarted { session_id } if session_id == "abc-123")));
}

#[test]
fn text_delta_maps_to_generic() {
    let line = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"hello"}]}}"#;
    let evs = parse_line_to_events(line);
    assert!(evs
        .iter()
        .any(|e| matches!(e, Event::Text { text, .. } if text == "hello")));
}

#[test]
fn malformed_line_returns_unknown() {
    let evs = parse_line_to_events("not json at all");
    assert_eq!(evs.len(), 1);
    assert!(matches!(&evs[0], Event::Unknown));
}
