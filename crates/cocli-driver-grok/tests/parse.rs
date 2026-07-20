//! Stream-json parse tests for grok (using `--output-format streaming-json`).
//!
//! Samples derived from spike artifacts (see spikes/grok-driver-spike.md):
//!   /tmp/grok-spike-output.jsonl , /tmp/grok-spike-resume.jsonl , /tmp/err1.jsonl
//! (anonymized verbatim NDJSON; "thought"/"text" are per-chunk; "end" carries sid).

use cocli_driver_grok::*;

#[test]
fn parses_text_delta_from_spike_sample() {
    // real sample excerpt from spike main output:
    let line = r#"{"type":"text","data":" was"}"#;
    let evs = parse_line(line);
    assert_eq!(evs.len(), 1);
    assert!(matches!(&evs[0], GrokEvent::TextDelta { .. }));
    match &evs[0] {
        GrokEvent::TextDelta { text } => assert_eq!(text, " was"),
        other => panic!("expected TextDelta, got {other:?}"),
    }
}

#[test]
fn parses_thought_delta_from_spike_sample() {
    // real per-token thought from spike (internal reasoning with tool mention)
    let line = r#"{"type":"thought","data":" tool"}"#;
    let evs = parse_line(line);
    assert_eq!(evs.len(), 1);
    match &evs[0] {
        GrokEvent::ThinkingDelta { text } => assert_eq!(text, " tool"),
        other => panic!("expected ThinkingDelta, got {other:?}"),
    }
}

#[test]
fn parses_end_yields_session_started_and_turn_end() {
    // exact end line from spike (first run)
    let line = r#"{"type":"end","stopReason":"EndTurn","sessionId":"019e89ac-d61e-70f2-be42-30dba3d2ff43","requestId":"d946aa12-e43e-4347-86ef-5210faddde27"}"#;
    let evs = parse_line(line);
    assert_eq!(evs.len(), 2, "end must expand to SessionStarted + TurnEnd");
    match &evs[0] {
        GrokEvent::SessionStarted { session_id } => {
            assert_eq!(session_id, "019e89ac-d61e-70f2-be42-30dba3d2ff43");
        }
        other => panic!("expected first SessionStarted, got {other:?}"),
    }
    match &evs[1] {
        GrokEvent::TurnEnd {
            session_id,
            stop_reason,
        } => {
            assert_eq!(session_id, "019e89ac-d61e-70f2-be42-30dba3d2ff43");
            assert_eq!(stop_reason, "EndTurn");
        }
        other => panic!("expected second TurnEnd, got {other:?}"),
    }
}

#[test]
fn parses_end_from_resume_sample() {
    // end from resume run (same sid, different request)
    let line = r#"{"type":"end","stopReason":"EndTurn","sessionId":"019e89ac-d61e-70f2-be42-30dba3d2ff43","requestId":"85b6baaa-b4e1-4369-98bb-4dadc6fe5b2b"}"#;
    let evs = parse_line(line);
    assert_eq!(evs.len(), 2);
    if let GrokEvent::SessionStarted { session_id } = &evs[0] {
        assert_eq!(session_id, "019e89ac-d61e-70f2-be42-30dba3d2ff43");
    } else {
        panic!("bad");
    }
}

#[test]
fn parses_error_from_spike_sample() {
    // exact from /tmp/err1.jsonl bad-resume case
    let line = r#"{"type":"error","message":"Couldn't create session: Session does not exist"}"#;
    let evs = parse_line(line);
    assert_eq!(evs.len(), 1);
    match &evs[0] {
        GrokEvent::Error { message, code: _ } => {
            assert_eq!(message, "Couldn't create session: Session does not exist");
        }
        other => panic!("expected Error, got {other:?}"),
    }
}

#[test]
fn parses_unknown_type_safely() {
    let line = r#"{"type":"future_thing","foo":"bar"}"#;
    let evs = parse_line(line);
    assert_eq!(evs.len(), 1);
    assert!(matches!(evs[0], GrokEvent::Unknown));
}

#[test]
fn parses_invalid_json_as_unknown() {
    let line = "not json at all, or partial";
    let evs = parse_line(line);
    assert_eq!(evs.len(), 1);
    assert!(matches!(evs[0], GrokEvent::Unknown));
}

#[test]
fn parses_empty_line_as_unknown() {
    let evs = parse_line("");
    assert_eq!(evs.len(), 1);
    assert!(matches!(evs[0], GrokEvent::Unknown));
}

#[test]
fn parses_whitespace_only_as_unknown() {
    let evs = parse_line("   \n\t  ");
    assert_eq!(evs.len(), 1);
    assert!(matches!(evs[0], GrokEvent::Unknown));
}
