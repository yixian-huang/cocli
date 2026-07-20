use chrono::{Duration, Utc};
use cocli_store::{AgentStatus, DeliveryState, MessageRole, Store, StoreError};

async fn fixture() -> (
    Store,
    cocli_store::Channel,
    cocli_store::Agent,
    cocli_store::Message,
) {
    let store = Store::in_memory().await.expect("store opens");
    let channel = store.create_channel("delivery").await.expect("channel");
    let agent = store
        .create_agent(
            channel.id,
            "worker",
            "fake",
            Some("test-model"),
            AgentStatus::Running,
        )
        .await
        .expect("agent");
    let message = store
        .append_message(channel.id, None, MessageRole::User, "do work")
        .await
        .expect("message");
    (store, channel, agent, message)
}

#[tokio::test]
async fn queue_reserves_defers_and_exhausts_without_losing_source_message() {
    let (store, _channel, agent, message) = fixture().await;
    let queued = store
        .enqueue_deliveries(&message, &[agent.id, agent.id])
        .await
        .expect("enqueue");
    assert_eq!(queued.len(), 1);
    assert_eq!(queued[0].state, DeliveryState::Pending);

    let now = Utc::now();
    let reserved = store
        .reserve_due_deliveries(10, 2, now)
        .await
        .expect("reserve");
    assert_eq!(reserved.len(), 1);
    assert_eq!(reserved[0].state, DeliveryState::InFlight);
    assert_eq!(reserved[0].attempts, 1);

    let deferred = store
        .defer_delivery(
            reserved[0].id,
            "temporary failure",
            now + Duration::seconds(2),
            2,
        )
        .await
        .expect("defer")
        .expect("delivery remains");
    assert_eq!(deferred.state, DeliveryState::Pending);
    assert_eq!(deferred.last_error.as_deref(), Some("temporary failure"));
    assert!(store
        .reserve_due_deliveries(10, 2, now + Duration::seconds(1))
        .await
        .expect("early reserve")
        .is_empty());

    let second = store
        .reserve_due_deliveries(10, 2, now + Duration::seconds(3))
        .await
        .expect("second reserve");
    assert_eq!(second[0].attempts, 2);
    let exhausted = store
        .defer_delivery(second[0].id, "still failing", now, 2)
        .await
        .expect("defer exhausted")
        .expect("delivery remains");
    assert_eq!(exhausted.state, DeliveryState::Exhausted);
    assert!(store
        .get_message(message.id)
        .await
        .expect("source lookup")
        .is_some());

    let stats = store.delivery_stats(Utc::now()).await.expect("stats");
    assert_eq!(stats.exhausted, 1);
    assert_eq!(stats.max_attempts, 2);
}

#[tokio::test]
async fn completion_appends_reply_and_removes_delivery_atomically() {
    let (store, channel, agent, message) = fixture().await;
    store
        .enqueue_deliveries(&message, &[agent.id])
        .await
        .expect("enqueue");
    let delivery = store
        .reserve_due_deliveries(1, 5, Utc::now())
        .await
        .expect("reserve")
        .pop()
        .expect("delivery");

    let reply = store
        .complete_delivery(&delivery, "finished")
        .await
        .expect("complete");

    assert_eq!(reply.agent_id, Some(agent.id));
    assert_eq!(reply.content, "finished");
    assert!(store
        .list_message_deliveries(message.id)
        .await
        .expect("queue lookup")
        .is_empty());
    let messages = store.list_messages(channel.id).await.expect("messages");
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[1], reply);

    let duplicate = store
        .complete_delivery(&delivery, "duplicate")
        .await
        .expect_err("completed delivery must be idempotently rejected");
    assert!(matches!(duplicate, StoreError::DeliveryNotInFlight(id) if id == delivery.id));
    assert_eq!(
        store
            .list_messages(channel.id)
            .await
            .expect("messages after duplicate")
            .len(),
        2
    );
}

#[tokio::test]
async fn user_message_and_running_agent_deliveries_are_created_together() {
    let store = Store::in_memory().await.expect("store opens");
    let channel = store.create_channel("atomic-post").await.expect("channel");
    let running = store
        .create_agent(channel.id, "running", "fake", None, AgentStatus::Running)
        .await
        .expect("running agent");
    store
        .create_agent(channel.id, "stopped", "fake", None, AgentStatus::Stopped)
        .await
        .expect("stopped agent");

    let message = store
        .append_user_message_with_deliveries(channel.id, "deliver atomically")
        .await
        .expect("atomic append");
    let deliveries = store
        .list_message_deliveries(message.id)
        .await
        .expect("deliveries");

    assert_eq!(deliveries.len(), 1);
    assert_eq!(deliveries[0].agent_id, running.id);
    assert_eq!(
        store.list_messages(channel.id).await.expect("messages"),
        vec![message]
    );
}

#[tokio::test]
async fn reopening_releases_in_flight_deliveries_for_retry() {
    let database_path =
        std::env::temp_dir().join(format!("cocli-delivery-{}.sqlite3", uuid::Uuid::new_v4()));
    let store = Store::open(&database_path).await.expect("store opens");
    let channel = store.create_channel("restart").await.expect("channel");
    let agent = store
        .create_agent(channel.id, "worker", "fake", None, AgentStatus::Running)
        .await
        .expect("agent");
    let message = store
        .append_message(channel.id, None, MessageRole::User, "persist")
        .await
        .expect("message");
    store
        .enqueue_deliveries(&message, &[agent.id])
        .await
        .expect("enqueue");
    store
        .reserve_due_deliveries(1, 5, Utc::now())
        .await
        .expect("reserve");
    drop(store);

    let reopened = Store::open(&database_path).await.expect("reopen");
    assert_eq!(
        reopened
            .release_in_flight_deliveries()
            .await
            .expect("release"),
        1
    );
    let reserved = reopened
        .reserve_due_deliveries(1, 5, Utc::now())
        .await
        .expect("reserve again");
    assert_eq!(reserved.len(), 1);
    assert_eq!(reserved[0].attempts, 2);

    drop(reopened);
    std::fs::remove_file(database_path).expect("remove temporary database");
}

#[tokio::test]
async fn stopped_agents_keep_pending_deliveries_without_spending_retry_budget() {
    let (store, _channel, agent, message) = fixture().await;
    store
        .enqueue_deliveries(&message, &[agent.id])
        .await
        .expect("enqueue");
    store
        .set_agent_status(agent.id, AgentStatus::Stopped)
        .await
        .expect("stop agent");

    assert!(store
        .reserve_due_deliveries(1, 3, Utc::now())
        .await
        .expect("reserve while stopped")
        .is_empty());
    let pending = store
        .list_message_deliveries(message.id)
        .await
        .expect("pending delivery");
    assert_eq!(pending[0].attempts, 0);
    assert_eq!(pending[0].state, DeliveryState::Pending);

    store
        .set_agent_status(agent.id, AgentStatus::Running)
        .await
        .expect("start agent");
    store.nudge_agent_deliveries(agent.id).await.expect("nudge");
    let reserved = store
        .reserve_due_deliveries(1, 3, Utc::now())
        .await
        .expect("reserve after start");
    assert_eq!(reserved.len(), 1);
    assert_eq!(reserved[0].attempts, 1);
}
