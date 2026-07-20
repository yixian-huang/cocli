use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use cocli_store::{
    AgentStatus, Store, StoreError, SubjectType, WorkspaceBindingState, WorkspaceProviderKey,
};
use sqlx_core::query::query;
use sqlx_core::query_scalar::query_scalar;
use sqlx_sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use uuid::Uuid;

fn temporary_database_path() -> PathBuf {
    std::env::temp_dir().join(format!("cocli-workspace-{}.sqlite3", Uuid::new_v4()))
}

fn temporary_directory_path() -> PathBuf {
    std::env::temp_dir().join(format!("cocli-workspace-dir-{}", Uuid::new_v4()))
}

async fn remove_database(path: &Path) {
    let _ = tokio::fs::remove_file(path).await;
}

#[tokio::test]
async fn inline_owner_rows_migrate_without_losing_workspace_data() {
    let database_path = temporary_database_path();
    let store = Store::open(&database_path)
        .await
        .expect("latest store should open");
    let channel = store
        .create_channel("migration-owner")
        .await
        .expect("channel should be created");
    let installation_id = store.current_installation_id().to_owned();
    store.close().await;

    let workspace_id = Uuid::new_v4();
    let unbound_workspace_id = Uuid::new_v4();
    let created_at = DateTime::parse_from_rfc3339("2025-02-03T04:05:06Z")
        .expect("timestamp")
        .with_timezone(&Utc);
    let updated_at = DateTime::parse_from_rfc3339("2025-03-04T05:06:07Z")
        .expect("timestamp")
        .with_timezone(&Utc);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::new().filename(&database_path))
        .await
        .expect("database should reopen");
    query("PRAGMA foreign_keys = OFF")
        .execute(&pool)
        .await
        .expect("foreign keys should disable for fixture setup");
    query("DELETE FROM cocli_schema_migrations WHERE version = 12")
        .execute(&pool)
        .await
        .expect("workspace migration marker should reset");
    query("DROP TABLE workspace_bindings")
        .execute(&pool)
        .await
        .expect("bindings table should drop");
    query("DROP TABLE subject_workspaces")
        .execute(&pool)
        .await
        .expect("attachments table should drop");
    query("DROP TABLE workspaces")
        .execute(&pool)
        .await
        .expect("portable workspace table should drop");
    query(
        "CREATE TABLE workspaces (\
            id TEXT PRIMARY KEY NOT NULL,\
            owner_type TEXT NOT NULL CHECK (owner_type IN ('agent', 'channel')),\
            owner_id TEXT NOT NULL,\
            kind TEXT NOT NULL CHECK (kind IN ('managed', 'directory', 'git', 'external')),\
            locator TEXT,\
            metadata_json TEXT NOT NULL DEFAULT '{}',\
            created_at TEXT NOT NULL,\
            updated_at TEXT NOT NULL\
        )",
    )
    .execute(&pool)
    .await
    .expect("legacy workspace table should exist");
    query(
        "INSERT INTO workspaces \
         (id, owner_type, owner_id, kind, locator, metadata_json, created_at, updated_at) \
         VALUES (?, 'channel', ?, 'directory', ?, ?, ?, ?)",
    )
    .bind(workspace_id)
    .bind(channel.id)
    .bind("/legacy/research")
    .bind(r#"{"label":"Research corpus","opaque":{"keep":true}}"#)
    .bind(created_at)
    .bind(updated_at)
    .execute(&pool)
    .await
    .expect("legacy workspace should seed");
    query(
        "INSERT INTO workspaces \
         (id, owner_type, owner_id, kind, locator, metadata_json, created_at, updated_at) \
         VALUES (?, 'channel', ?, 'managed', NULL, '{}', ?, ?)",
    )
    .bind(unbound_workspace_id)
    .bind(channel.id)
    .bind(created_at)
    .bind(updated_at)
    .execute(&pool)
    .await
    .expect("unbound legacy workspace should seed");
    pool.close().await;

    let migrated = Store::open(&database_path)
        .await
        .expect("workspace migration should run");
    let workspace = migrated
        .get_workspace(workspace_id)
        .await
        .expect("workspace query should work")
        .expect("workspace should survive");
    assert_eq!(workspace.provider_key.as_str(), "directory");
    assert_eq!(workspace.metadata["opaque"]["keep"], true);
    assert_eq!(workspace.created_at, created_at);
    assert_eq!(workspace.updated_at, updated_at);

    let attachments = migrated
        .list_workspace_attachments(workspace_id)
        .await
        .expect("attachments should list");
    assert_eq!(attachments.len(), 1);
    assert_eq!(attachments[0].subject_type, SubjectType::Channel);
    assert_eq!(attachments[0].subject_id, channel.id);
    assert_eq!(attachments[0].attached_at, created_at);

    let binding = migrated
        .get_workspace_binding(workspace_id, &installation_id)
        .await
        .expect("binding query should work")
        .expect("legacy locator should become a local binding");
    assert_eq!(binding.local_locator.as_deref(), Some("/legacy/research"));
    let unbound = migrated
        .current_workspace_binding(unbound_workspace_id)
        .await
        .expect("unbound binding query should work")
        .expect("unbound state should be structured");
    assert_eq!(unbound.state, WorkspaceBindingState::Unbound);
    assert_eq!(unbound.error_code.as_deref(), Some("binding_missing"));
    migrated.close().await;
    remove_database(&database_path).await;
}

#[tokio::test]
async fn one_workspace_is_shared_and_detach_does_not_delete_it() {
    let store = Store::in_memory().await.expect("store should open");
    let channel = store
        .create_channel("shared")
        .await
        .expect("channel should be created");
    let agent = store
        .create_agent(channel.id, "researcher", "fake", None, AgentStatus::Running)
        .await
        .expect("agent should be created");
    let workspace = store
        .create_workspace(
            WorkspaceProviderKey::new("directory").expect("provider key"),
            "Shared corpus",
            None,
            serde_json::json!({"purpose": "research"}),
        )
        .await
        .expect("workspace should be created");

    let first_attachment = store
        .attach_existing_workspace(
            workspace.id,
            SubjectType::Channel,
            channel.id,
            Some("primary"),
        )
        .await
        .expect("channel attachment should succeed");
    let updated_attachment = store
        .attach_existing_workspace(
            workspace.id,
            SubjectType::Channel,
            channel.id,
            Some("secondary"),
        )
        .await
        .expect("reattaching should update role without replacing identity");
    assert_eq!(updated_attachment.attached_at, first_attachment.attached_at);
    assert_eq!(updated_attachment.role.as_deref(), Some("secondary"));
    store
        .attach_existing_workspace(workspace.id, SubjectType::Agent, agent.id, None)
        .await
        .expect("agent attachment should succeed");
    assert_eq!(
        store
            .list_workspace_attachments(workspace.id)
            .await
            .expect("attachments should list")
            .len(),
        2
    );

    assert!(store
        .detach_workspace(workspace.id, SubjectType::Channel, channel.id)
        .await
        .expect("detach should succeed"));
    assert!(store
        .get_workspace(workspace.id)
        .await
        .expect("workspace query should work")
        .is_some());
    assert_eq!(
        store
            .list_workspaces("agent", agent.id)
            .await
            .expect("legacy owner-scoped list should still work")
            .len(),
        1
    );
}

#[tokio::test]
async fn subject_attachments_require_existing_agents_or_channels_and_are_cleaned_on_delete() {
    let store = Store::in_memory().await.expect("store should open");
    let channel = store
        .create_channel("subject-cleanup")
        .await
        .expect("channel should be created");
    let agent = store
        .create_agent(
            channel.id,
            "workspace-owner",
            "fake",
            None,
            AgentStatus::Running,
        )
        .await
        .expect("agent should be created");
    let workspace = store
        .create_workspace(
            WorkspaceProviderKey::new("directory").expect("provider key"),
            "Subject scoped",
            None,
            serde_json::json!({}),
        )
        .await
        .expect("workspace should be created");
    let missing_agent_id = Uuid::new_v4();
    let missing_channel_id = Uuid::new_v4();

    assert!(matches!(
        store
            .attach_existing_workspace(workspace.id, SubjectType::Agent, missing_agent_id, None)
            .await,
        Err(StoreError::SubjectNotFound {
            subject_type: "agent",
            subject_id
        }) if subject_id == missing_agent_id
    ));
    assert!(matches!(
        store
            .attach_existing_workspace(workspace.id, SubjectType::Channel, missing_channel_id, None)
            .await,
        Err(StoreError::SubjectNotFound {
            subject_type: "channel",
            subject_id
        }) if subject_id == missing_channel_id
    ));

    store
        .attach_existing_workspace(workspace.id, SubjectType::Agent, agent.id, None)
        .await
        .expect("agent attachment should succeed");
    store
        .attach_existing_workspace(workspace.id, SubjectType::Channel, channel.id, None)
        .await
        .expect("channel attachment should succeed");
    assert_eq!(
        store
            .list_workspace_attachments(workspace.id)
            .await
            .expect("attachments should list")
            .len(),
        2
    );

    store
        .delete_agent(agent.id)
        .await
        .expect("agent should delete");
    let after_agent_delete = store
        .list_workspace_attachments(workspace.id)
        .await
        .expect("attachments should list after agent delete");
    assert_eq!(after_agent_delete.len(), 1);
    assert_eq!(after_agent_delete[0].subject_type, SubjectType::Channel);
    assert_eq!(after_agent_delete[0].subject_id, channel.id);

    store
        .delete_channel(channel.id)
        .await
        .expect("channel should delete");
    assert!(store
        .list_workspace_attachments(workspace.id)
        .await
        .expect("attachments should list after channel delete")
        .is_empty());
}

#[tokio::test]
async fn file_backed_subject_attachment_insert_rejects_missing_subject_atomically() {
    let database_path = temporary_database_path();
    let store = Store::open(&database_path)
        .await
        .expect("store should open");
    let workspace = store
        .create_workspace(
            WorkspaceProviderKey::new("directory").expect("provider key"),
            "Atomic attachment",
            None,
            serde_json::json!({}),
        )
        .await
        .expect("workspace should be created");
    let missing_channel_id = Uuid::new_v4();
    assert!(matches!(
        store
            .attach_existing_workspace(
                workspace.id,
                SubjectType::Channel,
                missing_channel_id,
                None
            )
            .await,
        Err(StoreError::SubjectNotFound {
            subject_type: "channel",
            subject_id
        }) if subject_id == missing_channel_id
    ));
    store.close().await;

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(&database_path)
                .foreign_keys(true),
        )
        .await
        .expect("database should reopen");
    let orphan_count: i64 = query_scalar(
        "SELECT COUNT(*) FROM subject_workspaces \
         WHERE subject_type = 'channel' AND subject_id = ?",
    )
    .bind(missing_channel_id)
    .fetch_one(&pool)
    .await
    .expect("subject attachment count should query");
    assert_eq!(orphan_count, 0);
    pool.close().await;
    remove_database(&database_path).await;
}

#[tokio::test]
async fn current_installation_never_selects_an_imported_binding() {
    let source_path = temporary_database_path();
    let snapshot_path = temporary_database_path();
    let source = Store::open(&source_path).await.expect("source should open");
    let source_installation_id = source.current_installation_id().to_owned();
    let workspace = source
        .create_workspace(
            WorkspaceProviderKey::new("directory").expect("provider key"),
            "Portable",
            None,
            serde_json::json!({}),
        )
        .await
        .expect("workspace should be created");
    source
        .bind_workspace(workspace.id, "/source/path", None)
        .await
        .expect("source binding should persist");
    source
        .bind_workspace_for_installation(workspace.id, "other-installation", "/other/path", None)
        .await
        .expect("a second installation binding should persist independently");
    source
        .export_snapshot(&snapshot_path)
        .await
        .expect("snapshot should export");
    source.close().await;

    let imported = Store::open(&snapshot_path)
        .await
        .expect("snapshot should open as another installation");
    assert_ne!(imported.current_installation_id(), source_installation_id);
    let current_binding = imported
        .current_workspace_binding(workspace.id)
        .await
        .expect("current binding query should work")
        .expect("an unresolved current binding should be synthesized");
    assert_eq!(current_binding.state, WorkspaceBindingState::Unbound);
    assert_eq!(
        current_binding.installation_id,
        imported.current_installation_id()
    );
    assert_eq!(
        current_binding.error_code.as_deref(),
        Some("binding_missing")
    );
    assert_ne!(current_binding.installation_id, source_installation_id);
    let bindings = imported
        .list_workspace_bindings(workspace.id)
        .await
        .expect("bindings should list");
    assert_eq!(bindings.len(), 2);
    assert!(bindings
        .iter()
        .any(|binding| binding.installation_id == source_installation_id));
    assert!(bindings.iter().any(|binding| {
        binding.installation_id == "other-installation"
            && binding.local_locator.as_deref() == Some("/other/path")
    }));
    imported.close().await;
    remove_database(&source_path).await;
    remove_database(&snapshot_path).await;
}

#[tokio::test]
async fn missing_resources_degrade_without_blocking_subject_reads() {
    let store = Store::in_memory().await.expect("store should open");
    let channel = store
        .create_channel("recoverable")
        .await
        .expect("channel should be created");
    let workspace = store
        .attach_workspace(
            "channel",
            channel.id,
            "directory",
            Some("/definitely/missing/cocli-workspace"),
            serde_json::json!({"label": "Moved directory"}),
        )
        .await
        .expect("legacy attach should persist even when the path is missing");
    let binding = store
        .verify_workspace_binding(workspace.id)
        .await
        .expect("verification should return recoverable state");
    assert_eq!(binding.state, WorkspaceBindingState::NeedsAttention);
    assert_eq!(binding.error_code.as_deref(), Some("path_not_found"));
    assert_eq!(
        store
            .list_channels()
            .await
            .expect("subject reads must remain available")[0]
            .id,
        channel.id
    );
}

#[tokio::test]
async fn directory_and_git_providers_validate_existing_local_resources() {
    let directory_path = temporary_directory_path();
    let git_path = temporary_directory_path();
    tokio::fs::create_dir_all(&directory_path)
        .await
        .expect("directory fixture should exist");
    tokio::fs::create_dir_all(&git_path)
        .await
        .expect("git fixture should exist");
    tokio::fs::write(
        git_path.join(".git"),
        "gitdir: /tmp/cocli-worktree-metadata\n",
    )
    .await
    .expect("worktree git metadata should exist");

    let store = Store::in_memory().await.expect("store should open");
    let directory = store
        .create_workspace(
            WorkspaceProviderKey::new("directory").expect("provider key"),
            "Directory",
            None,
            serde_json::json!({}),
        )
        .await
        .expect("directory workspace should be created");
    let directory_binding = store
        .bind_workspace(
            directory.id,
            directory_path.to_str().expect("utf-8 directory path"),
            None,
        )
        .await
        .expect("directory should validate");
    assert_eq!(directory_binding.state, WorkspaceBindingState::Ready);
    assert_eq!(
        store
            .resolve_workspace(directory.id)
            .await
            .expect("directory should resolve")
            .as_deref(),
        directory_path.to_str()
    );
    let channel = store
        .create_channel("directory-owner")
        .await
        .expect("channel should be created");
    store
        .attach_existing_workspace(directory.id, SubjectType::Channel, channel.id, None)
        .await
        .expect("directory should attach");
    store
        .detach_workspace(directory.id, SubjectType::Channel, channel.id)
        .await
        .expect("directory should detach");
    assert!(
        directory_path.is_dir(),
        "detach must not delete external data"
    );

    let git = store
        .create_workspace(
            WorkspaceProviderKey::new("git").expect("provider key"),
            "Git worktree",
            Some("https://example.invalid/cocli.git"),
            serde_json::json!({}),
        )
        .await
        .expect("git workspace should be created");
    let git_binding = store
        .bind_workspace(git.id, git_path.to_str().expect("utf-8 git path"), None)
        .await
        .expect("git worktree should validate");
    assert_eq!(git_binding.state, WorkspaceBindingState::Ready);
    assert_eq!(git_binding.capabilities["git"], true);

    tokio::fs::remove_dir_all(directory_path)
        .await
        .expect("directory fixture should clean up");
    tokio::fs::remove_dir_all(git_path)
        .await
        .expect("git fixture should clean up");
}

#[tokio::test]
async fn directory_and_git_bindings_require_absolute_paths() {
    let store = Store::in_memory().await.expect("store should open");
    let directory = store
        .create_workspace(
            WorkspaceProviderKey::new("directory").expect("provider key"),
            "Relative directory",
            None,
            serde_json::json!({}),
        )
        .await
        .expect("directory workspace should be created");
    let directory_binding = store
        .bind_workspace(directory.id, "relative/path", None)
        .await
        .expect("relative directory binding should persist as recoverable state");
    assert_eq!(
        directory_binding.state,
        WorkspaceBindingState::NeedsAttention
    );
    assert_eq!(
        directory_binding.error_code.as_deref(),
        Some("path_must_be_absolute")
    );

    let git = store
        .create_workspace(
            WorkspaceProviderKey::new("git").expect("provider key"),
            "Relative git",
            Some("https://example.invalid/cocli.git"),
            serde_json::json!({}),
        )
        .await
        .expect("git workspace should be created");
    let git_binding = store
        .bind_workspace(git.id, "relative/git", None)
        .await
        .expect("relative git binding should persist as recoverable state");
    assert_eq!(git_binding.state, WorkspaceBindingState::NeedsAttention);
    assert_eq!(
        git_binding.error_code.as_deref(),
        Some("path_must_be_absolute")
    );
}

#[tokio::test]
async fn canonical_workspace_writes_reject_installation_local_portable_locators() {
    let store = Store::in_memory().await.expect("store should open");
    assert!(matches!(
        store
            .create_workspace(
                WorkspaceProviderKey::new("git").expect("provider key"),
                "Local Git path",
                Some("/Users/example/repository"),
                serde_json::json!({}),
            )
            .await,
        Err(StoreError::InvalidWorkspacePortableLocator { .. })
    ));
    assert!(matches!(
        store
            .create_workspace(
                WorkspaceProviderKey::new("directory").expect("provider key"),
                "Local directory path",
                Some("/Users/example/documents"),
                serde_json::json!({}),
            )
            .await,
        Err(StoreError::InvalidWorkspacePortableLocator { .. })
    ));

    let workspace = store
        .create_workspace(
            WorkspaceProviderKey::new("git").expect("provider key"),
            "Remote Git",
            Some("https://example.invalid/repository.git"),
            serde_json::json!({}),
        )
        .await
        .expect("canonical remote should persist");
    assert!(matches!(
        store
            .update_workspace(
                workspace.id,
                "Moved incorrectly",
                Some("/Users/example/moved-repository"),
                serde_json::json!({}),
            )
            .await,
        Err(StoreError::InvalidWorkspacePortableLocator { .. })
    ));
}

#[tokio::test]
async fn managed_provider_does_not_treat_external_directories_as_ready() {
    let managed_path = temporary_directory_path();
    tokio::fs::create_dir_all(&managed_path)
        .await
        .expect("managed fixture should exist");
    let store = Store::in_memory().await.expect("store should open");
    let workspace = store
        .create_workspace(
            WorkspaceProviderKey::new("managed").expect("provider key"),
            "Managed",
            None,
            serde_json::json!({}),
        )
        .await
        .expect("managed workspace should be created");
    let binding = store
        .bind_workspace(
            workspace.id,
            managed_path.to_str().expect("utf-8 managed path"),
            None,
        )
        .await
        .expect("managed binding should persist");

    assert_eq!(binding.state, WorkspaceBindingState::Unavailable);
    assert_eq!(
        binding.error_code.as_deref(),
        Some("managed_materialization_unavailable")
    );
    assert!(store
        .resolve_workspace(workspace.id)
        .await
        .expect("managed resolve should not fail")
        .is_none());

    tokio::fs::remove_dir_all(managed_path)
        .await
        .expect("managed fixture should clean up");
}

#[tokio::test]
async fn unknown_provider_descriptors_round_trip_without_data_loss() {
    let store = Store::in_memory().await.expect("store should open");
    let metadata = serde_json::json!({
        "externalObjectId": "opaque-42",
        "nested": {"keep": [1, 2, 3]}
    });
    let workspace = store
        .create_workspace(
            WorkspaceProviderKey::new("vendor.future-provider").expect("provider key"),
            "Future provider",
            Some("vendor://opaque-42"),
            metadata.clone(),
        )
        .await
        .expect("unknown provider should persist");
    store
        .bind_workspace(workspace.id, "vendor-local://opaque-42", None)
        .await
        .expect("unknown binding should persist");
    let verified = store
        .verify_workspace_binding(workspace.id)
        .await
        .expect("unknown provider should degrade instead of failing");
    assert_eq!(verified.state, WorkspaceBindingState::Unavailable);
    assert_eq!(verified.error_code.as_deref(), Some("provider_unavailable"));

    let loaded = store
        .get_workspace(workspace.id)
        .await
        .expect("workspace query should work")
        .expect("workspace should exist");
    assert_eq!(loaded.provider_key.as_str(), "vendor.future-provider");
    assert_eq!(
        loaded.portable_locator.as_deref(),
        Some("vendor://opaque-42")
    );
    assert_eq!(loaded.metadata, metadata);
}
