use std::path::PathBuf;

use chrono::Utc;
use cocli_store::{AgentStatus, MessageRole, Store};
use sqlx_core::query::query;
use sqlx_sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use uuid::Uuid;

fn temporary_database_path() -> PathBuf {
    std::env::temp_dir().join(format!("cocli-store-{}.sqlite3", Uuid::new_v4()))
}

#[tokio::test]
async fn store_restores_channels_agents_and_messages_after_reopen() {
    let database_path = temporary_database_path();
    let store = Store::open(&database_path)
        .await
        .expect("store should open");
    let channel = store
        .create_channel("local-loop")
        .await
        .expect("channel should be created");
    let agent = store
        .create_agent(
            channel.id,
            "echo",
            "fake",
            Some("test-model"),
            AgentStatus::Running,
        )
        .await
        .expect("agent should be created");
    store
        .append_message(
            channel.id,
            Some(agent.id),
            MessageRole::Assistant,
            "persisted reply",
        )
        .await
        .expect("message should be stored");
    drop(store);

    let reopened = Store::open(&database_path)
        .await
        .expect("store should reopen");
    let messages = reopened
        .list_messages(channel.id)
        .await
        .expect("messages should load");

    assert_eq!(messages[0].content, "persisted reply");

    std::fs::remove_file(database_path).expect("temporary database should be removable");
}

#[tokio::test]
async fn store_versions_and_upgrades_a_pre_versioned_database() {
    let database_path = temporary_database_path();
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(&database_path)
                .create_if_missing(true),
        )
        .await
        .expect("legacy database should open");
    query(
        "CREATE TABLE channels (\
            id TEXT PRIMARY KEY NOT NULL,\
            name TEXT NOT NULL,\
            created_at TEXT NOT NULL\
        )",
    )
    .execute(&pool)
    .await
    .expect("legacy schema");
    pool.close().await;

    let store = Store::open(&database_path)
        .await
        .expect("legacy database should migrate");
    store
        .create_channel("after-migration")
        .await
        .expect("legacy table remains usable");
    assert!(store
        .list_skill_library()
        .await
        .expect("latest schema should exist")
        .is_empty());
    drop(store);

    Store::open(&database_path)
        .await
        .expect("recorded migrations should not replay");
    std::fs::remove_file(database_path).expect("temporary database should be removable");
}

#[tokio::test]
async fn concurrent_channel_writes_allocate_unique_monotonic_sequences() {
    let database_path = temporary_database_path();
    let store = Store::open(&database_path)
        .await
        .expect("store should open");
    let channel = store
        .create_channel("concurrent-sequences")
        .await
        .expect("channel should be created");
    let mut writes = tokio::task::JoinSet::new();
    for index in 0..32 {
        let store = store.clone();
        writes.spawn(async move {
            store
                .append_message(
                    channel.id,
                    None,
                    MessageRole::User,
                    &format!("message-{index}"),
                )
                .await
        });
    }
    while let Some(result) = writes.join_next().await {
        result
            .expect("write task should not panic")
            .expect("concurrent write should succeed");
    }

    let messages = store
        .list_messages(channel.id)
        .await
        .expect("messages should list");
    assert_eq!(messages.len(), 32);
    assert_eq!(
        messages
            .iter()
            .map(|message| message.seq)
            .collect::<Vec<_>>(),
        (1..=32).collect::<Vec<_>>()
    );

    drop(store);
    std::fs::remove_file(database_path).expect("temporary database should be removable");
}

#[tokio::test]
async fn bridge_tokens_are_stable_and_stale_sessions_are_closed_on_reconcile() {
    let store = Store::in_memory().await.expect("store should open");
    let channel = store
        .create_channel("reconcile")
        .await
        .expect("channel should be created");
    let agent = store
        .create_agent(channel.id, "runtime", "fake", None, AgentStatus::Running)
        .await
        .expect("agent should be created");
    let first_token = store
        .ensure_agent_bridge_token(agent.id)
        .await
        .expect("token should be provisioned");
    let second_token = store
        .ensure_agent_bridge_token(agent.id)
        .await
        .expect("token should remain stable");
    assert_eq!(first_token, second_token);
    assert!(first_token.starts_with("cocli_bridge_"));

    store
        .create_agent_session(
            agent.id,
            Some(channel.id),
            "session-before-restart",
            Some("launch-before-restart"),
            None,
            "chat",
            Utc::now(),
        )
        .await
        .expect("open session should persist");
    assert!(store
        .current_agent_session(agent.id)
        .await
        .expect("current session query")
        .is_some());

    assert_eq!(
        store
            .close_stale_agent_sessions("process_restart", Utc::now())
            .await
            .expect("stale sessions should close"),
        1
    );
    assert!(store
        .current_agent_session(agent.id)
        .await
        .expect("current session query")
        .is_none());
    let sessions = store
        .list_agent_sessions(agent.id, 10, None)
        .await
        .expect("sessions should list");
    assert_eq!(sessions[0].end_reason.as_deref(), Some("process_restart"));
    assert!(sessions[0].ended_at.is_some());
}
