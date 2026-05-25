use cocli_driver::Event;

#[test]
fn session_started_event() {
    let e = Event::SessionStarted {
        session_id: "s1".into(),
    };
    if let Event::SessionStarted { session_id } = e {
        assert_eq!(session_id, "s1");
    } else {
        panic!()
    }
}

#[test]
fn tool_use_and_done_pair() {
    let u = Event::ToolUse {
        id: "t1".into(),
        name: "bash".into(),
        input: serde_json::json!({"cmd": "ls"}),
    };
    let d_ok = Event::ToolDone {
        id: "t1".into(),
        output: Some("ok".into()),
        error: None,
    };
    let d_err = Event::ToolDone {
        id: "t2".into(),
        output: None,
        error: Some("boom".into()),
    };
    match u {
        Event::ToolUse { id, .. } => assert_eq!(id, "t1"),
        _ => panic!(),
    }
    match d_ok {
        Event::ToolDone { output, error, .. } => {
            assert_eq!(output.unwrap(), "ok");
            assert!(error.is_none());
        }
        _ => panic!(),
    }
    match d_err {
        Event::ToolDone { output, error, .. } => {
            assert!(output.is_none());
            assert_eq!(error.unwrap(), "boom");
        }
        _ => panic!(),
    }
}

#[test]
fn turn_end_carries_token_counts() {
    let e = Event::TurnEnd {
        status: Some("completed".into()),
        input_tokens: 100,
        output_tokens: 200,
        cached_input_tokens: 50,
        cost_usd: 0.0,
        context_window: Some(200000),
        cache_creation_tokens: 0,
        cache_read_tokens: 50,
    };
    if let Event::TurnEnd {
        input_tokens,
        output_tokens,
        ..
    } = e
    {
        assert_eq!(input_tokens, 100);
        assert_eq!(output_tokens, 200);
    } else {
        panic!()
    }
}

#[test]
fn unknown_passthrough() {
    let e = Event::Unknown;
    assert!(matches!(e, Event::Unknown));
}
