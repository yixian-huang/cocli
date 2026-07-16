use std::path::PathBuf;

use cocli_store::{AgentStatus, MessageRole, Store};
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
