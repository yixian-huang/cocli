use std::path::PathBuf;

use chrono::Utc;
use cocli_store::{AgentLifecycleStatus, AgentStatus, MemoryNamespace, MessageRole, Store};
use sqlx_core::query::query;
use sqlx_core::query_scalar::query_scalar;
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
async fn store_exports_a_consistent_reopenable_snapshot() {
    let database_path = temporary_database_path();
    let snapshot_path = temporary_database_path();
    let store = Store::open(&database_path)
        .await
        .expect("store should open");
    let channel = store
        .create_channel("portable-state")
        .await
        .expect("channel should be created");
    let agent = store
        .create_agent(
            channel.id,
            "portable-agent",
            "fake",
            None,
            AgentStatus::Running,
        )
        .await
        .expect("agent should be created");
    let source_token = store
        .ensure_agent_bridge_token(agent.id)
        .await
        .expect("source token should be provisioned");

    store
        .export_snapshot(&snapshot_path)
        .await
        .expect("snapshot should export");
    let snapshot = Store::open(&snapshot_path)
        .await
        .expect("snapshot should reopen");

    assert_eq!(
        snapshot
            .list_channels()
            .await
            .expect("snapshot channels should load")[0]
            .name,
        "portable-state"
    );
    assert!(snapshot
        .agent_bridge_token(agent.id)
        .await
        .expect("snapshot token query should work")
        .is_none());
    let snapshot_token = snapshot
        .ensure_agent_bridge_token(agent.id)
        .await
        .expect("snapshot should provision a fresh token");
    assert_ne!(snapshot_token, source_token);
    assert_eq!(
        store
            .agent_bridge_token(agent.id)
            .await
            .expect("live token query should work")
            .as_deref(),
        Some(source_token.as_str())
    );
    drop(snapshot);
    drop(store);
    std::fs::remove_file(database_path).expect("database should be removable");
    std::fs::remove_file(snapshot_path).expect("snapshot should be removable");
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
async fn memory_storage_migration_preserves_legacy_documents() {
    let database_path = temporary_database_path();
    let store = Store::open(&database_path)
        .await
        .expect("store should open");
    drop(store);

    let agent_id = Uuid::new_v4();
    let path = format!("agents/{agent_id}/memory/project_migration.md");
    let ordinary_page_id = Uuid::new_v4();
    let ordinary_revision_id = Uuid::new_v4();
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::new().filename(&database_path))
        .await
        .expect("legacy database should reopen");
    query("DELETE FROM cocli_schema_migrations WHERE version = 11")
        .execute(&pool)
        .await
        .expect("memory migration marker should reset");
    query("DELETE FROM memory_documents")
        .execute(&pool)
        .await
        .expect("memory destination should reset");
    query("DELETE FROM wiki_links")
        .execute(&pool)
        .await
        .expect("legacy wiki links should reset");
    query("DELETE FROM wiki_revisions")
        .execute(&pool)
        .await
        .expect("legacy wiki revisions should reset");
    query("DELETE FROM wiki_pages")
        .execute(&pool)
        .await
        .expect("legacy wiki pages should reset");
    query(
        "INSERT INTO wiki_pages \
         (id, path, title, content_md, tags, version, created_at, updated_at, updated_by) \
         VALUES (?, ?, ?, ?, '[]', 3, ?, ?, ?)",
    )
    .bind(Uuid::new_v4())
    .bind(&path)
    .bind("migration")
    .bind("---\ndescription: Migrated memory\ntype: project\nupdated: 2026-07-18\n---\n\nPreserve me.")
    .bind(Utc::now())
    .bind(Utc::now())
    .bind("legacy-agent")
    .execute(&pool)
    .await
    .expect("legacy memory should seed");
    query(
        "INSERT INTO wiki_pages \
         (id, path, title, content_md, tags, version, created_at, updated_at, updated_by) \
         VALUES (?, 'docs/ordinary.md', 'Ordinary', 'Keep this wiki page.', '[\"wiki\"]', 7, ?, ?, 'wiki-agent')",
    )
    .bind(ordinary_page_id)
    .bind(Utc::now())
    .bind(Utc::now())
    .execute(&pool)
    .await
    .expect("ordinary wiki page should seed");
    query(
        "INSERT INTO wiki_revisions \
         (id, page_id, version, title, content_md, tags, created_at, created_by, reason) \
         VALUES (?, ?, 7, 'Ordinary', 'Keep this revision.', '[\"wiki\"]', ?, 'wiki-agent', 'migration fixture')",
    )
    .bind(ordinary_revision_id)
    .bind(ordinary_page_id)
    .bind(Utc::now())
    .execute(&pool)
    .await
    .expect("ordinary wiki revision should seed");
    query("INSERT INTO wiki_links (source_page_id, target_path) VALUES (?, 'docs/target.md')")
        .bind(ordinary_page_id)
        .execute(&pool)
        .await
        .expect("ordinary wiki link should seed");
    pool.close().await;

    let migrated = Store::open(&database_path)
        .await
        .expect("memory migration should run")
        .get_memory_topic(MemoryNamespace::Agent(agent_id), "project", "migration")
        .await
        .expect("migrated memory should load")
        .expect("migrated memory should exist");
    assert_eq!(migrated.version, 3);
    assert_eq!(migrated.description, "Migrated memory");
    assert!(migrated.body.ends_with("Preserve me."));

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::new().filename(&database_path))
        .await
        .expect("migrated database should reopen");
    let legacy_table_count: i64 = query_scalar(
        "SELECT COUNT(*) FROM sqlite_master \
         WHERE type = 'table' AND name IN ('wiki_pages', 'wiki_revisions', 'wiki_links')",
    )
    .fetch_one(&pool)
    .await
    .expect("legacy tables should be inspectable");
    assert_eq!(legacy_table_count, 3);
    let ordinary_page_count: i64 =
        query_scalar("SELECT COUNT(*) FROM wiki_pages WHERE id = ? AND content_md = ?")
            .bind(ordinary_page_id)
            .bind("Keep this wiki page.")
            .fetch_one(&pool)
            .await
            .expect("ordinary wiki page should survive");
    assert_eq!(ordinary_page_count, 1);
    let ordinary_revision_count: i64 =
        query_scalar("SELECT COUNT(*) FROM wiki_revisions WHERE id = ? AND content_md = ?")
            .bind(ordinary_revision_id)
            .bind("Keep this revision.")
            .fetch_one(&pool)
            .await
            .expect("ordinary wiki revision should survive");
    assert_eq!(ordinary_revision_count, 1);
    let ordinary_link_count: i64 = query_scalar(
        "SELECT COUNT(*) FROM wiki_links WHERE source_page_id = ? AND target_path = ?",
    )
    .bind(ordinary_page_id)
    .bind("docs/target.md")
    .fetch_one(&pool)
    .await
    .expect("ordinary wiki link should survive");
    assert_eq!(ordinary_link_count, 1);
    pool.close().await;

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

#[tokio::test]
async fn agents_are_independent_and_membership_survives_channel_deletion() {
    let store = Store::in_memory().await.expect("store should open");
    let first = store
        .create_channel("first")
        .await
        .expect("first channel should be created");
    let second = store
        .create_channel("second")
        .await
        .expect("second channel should be created");
    let agent = store
        .create_agent(first.id, "durable", "fake", None, AgentStatus::Running)
        .await
        .expect("agent should be created");

    store
        .add_agent_to_channel(
            second.id,
            agent.id,
            Some("reviewer"),
            Some("subscribed"),
            None,
            Some(first.id),
        )
        .await
        .expect("membership should be created");

    assert_eq!(
        store
            .list_channel_agents(first.id)
            .await
            .expect("first members should list")
            .len(),
        1
    );
    assert_eq!(
        store
            .list_agent_channels(agent.id)
            .await
            .expect("agent channels should list")
            .iter()
            .map(|channel| channel.id)
            .collect::<Vec<_>>(),
        vec![first.id, second.id]
    );

    store
        .delete_channel(first.id)
        .await
        .expect("channel deletion should not delete agent");
    let surviving = store
        .get_agent(agent.id)
        .await
        .expect("agent query should work")
        .expect("agent should survive");
    assert_eq!(surviving.lifecycle_status, AgentLifecycleStatus::Active);
    assert_eq!(surviving.channel_id, second.id);
    assert_eq!(
        store
            .set_agent_lifecycle_status(agent.id, AgentLifecycleStatus::Paused)
            .await
            .expect("lifecycle should update")
            .expect("agent should exist")
            .lifecycle_status,
        AgentLifecycleStatus::Paused
    );
    assert_eq!(
        store
            .list_agent_channels(agent.id)
            .await
            .expect("remaining channels should list")
            .iter()
            .map(|channel| channel.id)
            .collect::<Vec<_>>(),
        vec![second.id]
    );
}

#[tokio::test]
async fn standalone_agents_get_idempotent_direct_channels() {
    let store = Store::in_memory().await.expect("store should open");
    let agent = store
        .create_standalone_agent("solo", "fake", None, AgentStatus::Running, None, None)
        .await
        .expect("standalone agent should be created");

    let first = store
        .ensure_direct_channel_for_agent(agent.id)
        .await
        .expect("direct channel should be created");
    let second = store
        .ensure_direct_channel_for_agent(agent.id)
        .await
        .expect("direct channel should be reused");

    assert_eq!(first.id, second.id);
    assert_eq!(agent.channel_id, first.id);
    assert_eq!(first.kind, "direct");
    assert!(first.is_system);
    assert_eq!(first.direct_agent_id, Some(agent.id));
    assert_eq!(
        store
            .list_channel_agents(first.id)
            .await
            .expect("direct channel members should list")[0]
            .id,
        agent.id
    );
}

#[tokio::test]
async fn workspaces_attach_to_agent_or_channel_owners() {
    let store = Store::in_memory().await.expect("store should open");
    let channel = store
        .create_channel("research")
        .await
        .expect("channel should be created");
    let workspace = store
        .attach_workspace(
            "channel",
            channel.id,
            "directory",
            Some("/tmp/research"),
            serde_json::json!({"label": "Research corpus"}),
        )
        .await
        .expect("workspace should attach");

    assert_eq!(
        store
            .get_workspace(workspace.id)
            .await
            .expect("workspace query should work")
            .expect("workspace should exist")
            .metadata["label"],
        "Research corpus"
    );
    assert_eq!(
        store
            .list_workspaces("channel", channel.id)
            .await
            .expect("workspace list should work")
            .len(),
        1
    );
}
