use chrono::{Duration, Utc};
use cocli_store::{AgentSessionFinish, AgentStatus, MessageRole, NewAgentTurn, Store};
use serde_json::json;

#[tokio::test]
async fn sessions_turns_and_activity_round_trip_with_message_links() {
    let store = Store::in_memory().await.expect("store should open");
    let channel = store
        .create_channel("runtime-history")
        .await
        .expect("channel should persist");
    let agent = store
        .create_agent(
            channel.id,
            "builder",
            "fake",
            Some("test-model"),
            AgentStatus::Running,
        )
        .await
        .expect("agent should persist");
    let source = store
        .append_message(channel.id, None, MessageRole::User, "build it")
        .await
        .expect("message should persist");

    let started_at = Utc::now();
    let session = store
        .create_agent_session(
            agent.id,
            Some(channel.id),
            "runtime-session-1",
            Some("launch-1"),
            None,
            "chat",
            started_at,
        )
        .await
        .expect("session should persist");

    let ended_at = started_at + Duration::seconds(2);
    let turn = store
        .upsert_agent_turn(&NewAgentTurn {
            agent_id: agent.id,
            session_id: session.session_id.clone(),
            launch_id: session.launch_id.clone(),
            turn_number: 1,
            started_at,
            ended_at: Some(ended_at),
            input_tokens: 120,
            output_tokens: 45,
            cost_usd: 0.0025,
            context_window: 200_000,
            entries: json!([
                {"kind": "input", "text": "build it"},
                {"kind": "tool_call", "input": {"name": "exec"}}
            ]),
            session_type: "chat".to_owned(),
            channel_id: Some(channel.id),
            source_message_id: Some(source.id),
        })
        .await
        .expect("turn should persist");
    assert_eq!(turn.duration_ms, Some(2_000));
    assert_eq!(
        turn.message_ref
            .as_ref()
            .map(|reference| reference.message_id),
        Some(source.id)
    );

    let updated = store
        .upsert_agent_turn(&NewAgentTurn {
            output_tokens: 50,
            entries: json!([{"kind": "text", "text": "done"}]),
            ..NewAgentTurn {
                agent_id: agent.id,
                session_id: session.session_id.clone(),
                launch_id: session.launch_id.clone(),
                turn_number: 1,
                started_at,
                ended_at: Some(ended_at),
                input_tokens: 120,
                output_tokens: 45,
                cost_usd: 0.0025,
                context_window: 200_000,
                entries: json!([]),
                session_type: "chat".to_owned(),
                channel_id: Some(channel.id),
                source_message_id: Some(source.id),
            }
        })
        .await
        .expect("turn upsert should be idempotent");
    assert_eq!(updated.output_tokens, 50);

    let active = store
        .current_agent_session(agent.id)
        .await
        .expect("active session query should work")
        .expect("session should remain active");
    assert_eq!(active.turn_count, 1);
    assert_eq!(active.input_tokens, 120);
    assert_eq!(active.output_tokens, 50);

    let activity = store
        .insert_agent_activity(
            agent.id,
            Some(session.id),
            Some(&session.session_id),
            "working",
            Some("exec"),
            &["exec".to_owned()],
            session.launch_id.as_deref(),
            started_at + Duration::seconds(1),
        )
        .await
        .expect("activity should persist");
    assert_eq!(activity.session_row_id, Some(session.id));

    let activities = store
        .list_agent_activity(agent.id, 50, 0)
        .await
        .expect("activity should list");
    assert_eq!(activities, vec![activity]);

    assert!(store
        .finish_agent_session(
            agent.id,
            "launch-1",
            &AgentSessionFinish {
                end_reason: "idle".to_owned(),
                turn_count: active.turn_count,
                input_tokens: active.input_tokens,
                output_tokens: active.output_tokens,
                cost_usd: active.cost_usd,
                context_window: active.context_window,
                task_summary: None,
                files_changed: Some(vec!["src/main.rs".to_owned()]),
                task_success: Some(true),
                ended_at,
            },
        )
        .await
        .expect("session should finish"));
    assert!(store
        .current_agent_session(agent.id)
        .await
        .expect("current session query should work")
        .is_none());

    let sessions = store
        .list_agent_sessions(agent.id, 20, Some("chat"))
        .await
        .expect("sessions should list");
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].end_reason.as_deref(), Some("idle"));
    assert_eq!(
        sessions[0].files_changed.as_deref(),
        Some(["src/main.rs".to_owned()].as_slice())
    );
    assert_eq!(sessions[0].task_success, Some(true));

    let turns = store
        .list_agent_turns(agent.id, Some("runtime-session-1"), 100, 0)
        .await
        .expect("turns should list");
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].id, updated.id);
    assert_eq!(turns[0].entries, json!([{"kind": "text", "text": "done"}]));
}
