use std::path::{Path, PathBuf};

use cocli_store::{MemoryNamespace, MessageRole, Store, CURRENT_SCHEMA_VERSION};
use sqlx_core::query::query;
use sqlx_core::query_scalar::query_scalar;
use sqlx_sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use uuid::Uuid;

const SCHEMA12_FIXTURE: &str = include_str!("fixtures/schema12_pre_governance.sql");

const CHANNEL_ID: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
const AGENT_ID: &str = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
const MESSAGE_ID: &str = "cccccccc-cccc-4ccc-8ccc-cccccccccccc";
const MEMORY_ID: &str = "dddddddd-dddd-4ddd-8ddd-dddddddddddd";
const TASK_ID: &str = "eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee";
const WORKSPACE_ID: &str = "ffffffff-ffff-4fff-8fff-ffffffffffff";
const INSTALLATION_ID: &str = "12121212-1212-4121-8121-121212121212";

const GOVERNANCE_TABLES: [&str; 20] = [
    "mcp_bundle_import_audits",
    "mcp_apply_runs",
    "mcp_plan_decisions",
    "mcp_plans",
    "mcp_profile_bindings",
    "mcp_profiles",
    "skill_governance_gc_references",
    "skill_governance_workspace_lockfiles",
    "skill_governance_adoption_audit",
    "skill_governance_materializations",
    "skill_governance_managed_artifacts",
    "skill_governance_apply_audit",
    "skill_governance_apply_actions",
    "skill_governance_apply_runs",
    "skill_governance_scoped_locks",
    "skill_governance_plan_audit",
    "skill_governance_plans",
    "skill_lock_snapshots",
    "skill_profile_bindings",
    "skill_profiles",
];

struct TempDatabase {
    path: PathBuf,
}

impl TempDatabase {
    fn new(label: &str) -> Self {
        Self {
            path: std::env::temp_dir().join(format!(
                "cocli-schema12-upgrade-{label}-{}.sqlite3",
                Uuid::new_v4()
            )),
        }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDatabase {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        let _ = std::fs::remove_file(self.path.with_extension("sqlite3-shm"));
        let _ = std::fs::remove_file(self.path.with_extension("sqlite3-wal"));
    }
}

fn fixture_uuid(value: &str) -> Uuid {
    Uuid::parse_str(value).expect("fixture id should be a uuid")
}

async fn connect(path: &Path) -> SqlitePool {
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(path)
                .create_if_missing(true)
                .foreign_keys(true),
        )
        .await
        .expect("schema-12 fixture database should open")
}

async fn apply_sql(pool: &SqlitePool, migration: &str) {
    let mut transaction = pool.begin().await.expect("fixture transaction");
    for statement in migration.split(';') {
        let statement = statement.trim();
        if !statement.is_empty() {
            query(statement)
                .execute(&mut *transaction)
                .await
                .expect("fixture statement should apply");
        }
    }
    transaction.commit().await.expect("fixture should commit");
}

async fn table_exists(pool: &SqlitePool, name: &str) -> bool {
    query_scalar("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?)")
        .bind(name)
        .fetch_one(pool)
        .await
        .expect("table existence should query")
}

async fn max_schema_version(pool: &SqlitePool) -> i64 {
    query_scalar("SELECT COALESCE(MAX(version), 0) FROM cocli_schema_migrations")
        .fetch_one(pool)
        .await
        .expect("schema version should query")
}

#[tokio::test]
async fn portable_schema_upgrade_restores_schema12_subjects_through_governance_migrations() {
    let database = TempDatabase::new("pre-governance");
    let path = database.path();

    let pool = connect(path).await;
    apply_sql(&pool, SCHEMA12_FIXTURE).await;

    let pre_version = max_schema_version(&pool).await;
    eprintln!("schema12 fixture MAX(version) before Store::open = {pre_version}");
    assert_eq!(pre_version, 12);
    for table in GOVERNANCE_TABLES {
        assert!(
            !table_exists(&pool, table).await,
            "{table} must not exist on a schema-12 fixture"
        );
    }

    let channel_id: Uuid = query_scalar("SELECT id FROM channels")
        .fetch_one(&pool)
        .await
        .expect("schema-12 fixture should contain one channel");
    let agent_id: Uuid = query_scalar("SELECT id FROM agents")
        .fetch_one(&pool)
        .await
        .expect("schema-12 fixture should contain one agent");
    let membership_count: i64 = query_scalar("SELECT COUNT(*) FROM channel_agents")
        .fetch_one(&pool)
        .await
        .expect("schema-12 membership should query");
    let message_id: Uuid = query_scalar("SELECT id FROM messages")
        .fetch_one(&pool)
        .await
        .expect("schema-12 fixture should contain one message");
    let memory_id: Uuid = query_scalar("SELECT id FROM memory_documents")
        .fetch_one(&pool)
        .await
        .expect("schema-12 fixture should contain one memory document");
    let task_id: Uuid = query_scalar("SELECT id FROM tasks")
        .fetch_one(&pool)
        .await
        .expect("schema-12 fixture should contain one task");
    let workspace_id: Uuid = query_scalar("SELECT id FROM workspaces")
        .fetch_one(&pool)
        .await
        .expect("schema-12 fixture should contain one workspace");
    let binding_installation: String =
        query_scalar("SELECT installation_id FROM workspace_bindings")
            .fetch_one(&pool)
            .await
            .expect("schema-12 fixture should contain one source binding");

    assert_eq!(channel_id, fixture_uuid(CHANNEL_ID));
    assert_eq!(agent_id, fixture_uuid(AGENT_ID));
    assert_eq!(membership_count, 1);
    assert_eq!(message_id, fixture_uuid(MESSAGE_ID));
    assert_eq!(memory_id, fixture_uuid(MEMORY_ID));
    assert_eq!(task_id, fixture_uuid(TASK_ID));
    assert_eq!(workspace_id, fixture_uuid(WORKSPACE_ID));
    assert_eq!(binding_installation, INSTALLATION_ID);
    pool.close().await;

    let store = Store::open(path)
        .await
        .expect("schema-12 fixture should migrate through governance");
    assert_eq!(store.current_installation_id(), INSTALLATION_ID);

    let channel = store
        .get_channel(fixture_uuid(CHANNEL_ID))
        .await
        .expect("channel query should work")
        .expect("channel id should survive");
    assert_eq!(channel.name, "schema12-recovery");

    let agent = store
        .get_agent(fixture_uuid(AGENT_ID))
        .await
        .expect("agent query should work")
        .expect("agent id should survive");
    assert_eq!(agent.name, "restorer");
    assert_eq!(agent.channel_id, channel.id);

    let members = store
        .list_channel_agents(channel.id)
        .await
        .expect("membership should load");
    assert_eq!(members.len(), 1);
    assert_eq!(members[0].id, agent.id);

    let message = store
        .get_message(fixture_uuid(MESSAGE_ID))
        .await
        .expect("message query should work")
        .expect("message id should survive");
    assert_eq!(message.role, MessageRole::User);
    assert_eq!(message.channel_id, channel.id);

    let task = store
        .get_task(channel.id, 1)
        .await
        .expect("task query should work")
        .expect("task should survive");
    assert_eq!(task.id, fixture_uuid(TASK_ID));
    assert_eq!(task.message_id, Some(message.id));

    let memory = store
        .list_memory_namespace(MemoryNamespace::Channel(channel.id))
        .await
        .expect("memory namespace should load");
    assert_eq!(memory.len(), 1);
    assert!(memory[0]
        .path
        .starts_with(&format!("channels/{CHANNEL_ID}/notes/")));

    let workspace = store
        .get_workspace(fixture_uuid(WORKSPACE_ID))
        .await
        .expect("workspace query should work")
        .expect("workspace id should survive");
    assert_eq!(workspace.display_name, "Recovery workspace");
    let binding = store
        .get_workspace_binding(workspace.id, INSTALLATION_ID)
        .await
        .expect("binding query should work")
        .expect("source binding should survive");
    assert_eq!(binding.local_locator.as_deref(), Some("/source/recovery"));
    store.close().await;

    let pool = connect(path).await;
    let post_version = max_schema_version(&pool).await;
    eprintln!("schema12 fixture MAX(version) after Store::open = {post_version}");
    assert_eq!(post_version, 19);
    assert_eq!(post_version, CURRENT_SCHEMA_VERSION);

    let survived_memory_id: Uuid = query_scalar("SELECT id FROM memory_documents WHERE id = ?")
        .bind(fixture_uuid(MEMORY_ID))
        .fetch_one(&pool)
        .await
        .expect("memory id should survive");
    assert_eq!(survived_memory_id, fixture_uuid(MEMORY_ID));
    pool.close().await;
}
