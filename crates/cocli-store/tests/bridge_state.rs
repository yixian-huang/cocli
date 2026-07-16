use chrono::Utc;
use cocli_store::{AgentStatus, MessageRole, Store};

#[tokio::test]
async fn inbox_cursor_skips_completed_source_and_own_replies() {
    let store = Store::in_memory().await.expect("store opens");
    let channel = store.create_channel("agents").await.expect("channel");
    let first = store
        .create_agent(channel.id, "first", "fake", None, AgentStatus::Running)
        .await
        .expect("first agent");
    let second = store
        .create_agent(channel.id, "second", "fake", None, AgentStatus::Running)
        .await
        .expect("second agent");
    let user = store
        .append_message(channel.id, None, MessageRole::User, "hello agents")
        .await
        .expect("user message");
    store
        .enqueue_deliveries(&user, &[first.id])
        .await
        .expect("enqueue");
    let delivery = store
        .reserve_due_deliveries(1, 3, Utc::now())
        .await
        .expect("reserve")
        .pop()
        .expect("delivery");
    store
        .complete_delivery(&delivery, "first reply")
        .await
        .expect("complete");
    store
        .append_message(
            channel.id,
            Some(second.id),
            MessageRole::Assistant,
            "second update",
        )
        .await
        .expect("peer update");

    let first_inbox = store
        .consume_agent_inbox(first.id, 50)
        .await
        .expect("first inbox");
    assert_eq!(first_inbox.len(), 1);
    assert_eq!(first_inbox[0].content, "second update");
    assert!(store
        .consume_agent_inbox(first.id, 50)
        .await
        .expect("first inbox again")
        .is_empty());

    let second_inbox = store
        .consume_agent_inbox(second.id, 50)
        .await
        .expect("second inbox");
    assert_eq!(
        second_inbox
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>(),
        vec!["hello agents", "first reply"]
    );
}

#[tokio::test]
async fn history_page_and_working_state_are_persistent_and_bounded() {
    let store = Store::in_memory().await.expect("store opens");
    let channel = store.create_channel("history").await.expect("channel");
    let agent = store
        .create_agent(channel.id, "worker", "fake", None, AgentStatus::Running)
        .await
        .expect("agent");
    for content in ["one", "two", "three", "four"] {
        store
            .append_message(channel.id, None, MessageRole::User, content)
            .await
            .expect("message");
    }

    let latest = store
        .list_message_page(channel.id, 2, None, None)
        .await
        .expect("latest page");
    assert_eq!(
        latest
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>(),
        vec!["three", "four"]
    );
    let older = store
        .list_message_page(channel.id, 2, Some(latest[0].seq), None)
        .await
        .expect("older page");
    assert_eq!(
        older
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>(),
        vec!["one", "two"]
    );

    let first = store
        .set_working_state(
            agent.id,
            "implement bridge",
            Some("history"),
            Some(7),
            Some("add MCP server"),
        )
        .await
        .expect("set work");
    let updated = store
        .set_working_state(agent.id, "verify bridge", None, None, Some("run tests"))
        .await
        .expect("update work");
    assert_eq!(updated.started_at, first.started_at);
    assert_eq!(updated.summary, "verify bridge");
    assert_eq!(
        store.get_working_state(agent.id).await.expect("get work"),
        Some(updated)
    );
    assert!(store
        .clear_working_state(agent.id)
        .await
        .expect("clear work"));
    assert!(store
        .get_working_state(agent.id)
        .await
        .expect("get cleared work")
        .is_none());
}
