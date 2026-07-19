use std::path::{Path, PathBuf};

use cocli_store::Store;
use sqlx_core::query::query;
use sqlx_core::query_scalar::query_scalar;
use sqlx_sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use uuid::Uuid;

struct TempDatabase {
    path: PathBuf,
}

impl TempDatabase {
    fn new(label: &str) -> Self {
        Self {
            path: std::env::temp_dir().join(format!(
                "cocli-governance-migration-{label}-{}.sqlite3",
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

const MCP_MIGRATIONS: [(i64, &str, &str); 4] = [
    (
        13,
        "mcp_governance_phase_2a",
        include_str!("../migrations/0013_mcp_governance_phase_2a.sql"),
    ),
    (
        14,
        "mcp_governance_phase_2b",
        include_str!("../migrations/0014_mcp_governance_phase_2b.sql"),
    ),
    (
        15,
        "mcp_governance_phase_2c",
        include_str!("../migrations/0015_mcp_governance_phase_2c.sql"),
    ),
    (
        16,
        "mcp_governance_phase_3a",
        include_str!("../migrations/0016_mcp_governance_phase_3a.sql"),
    ),
];

const LEGACY_SKILL_MIGRATIONS: [(i64, &str, &str); 3] = [
    (
        13,
        "skill_governance",
        include_str!("../migrations/0017_skill_governance.sql"),
    ),
    (
        14,
        "skill_governance_apply_state",
        include_str!("../migrations/0018_skill_governance_apply_state.sql"),
    ),
    (
        15,
        "skill_governance_managed_scopes",
        include_str!("../migrations/0019_skill_governance_managed_scopes.sql"),
    ),
];

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

async fn connect(path: &Path) -> SqlitePool {
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::new().filename(path))
        .await
        .expect("migration fixture database should open")
}

async fn apply_sql(pool: &SqlitePool, migration: &str) {
    let mut transaction = pool.begin().await.expect("migration transaction");
    for statement in migration.split(';') {
        let statement = statement.trim();
        if !statement.is_empty() {
            query(statement)
                .execute(&mut *transaction)
                .await
                .expect("fixture migration statement should apply");
        }
    }
    transaction
        .commit()
        .await
        .expect("fixture migration should commit");
}

async fn seed_lineage(pool: &SqlitePool, migrations: &[(i64, &str, &str)]) {
    for (version, name, migration) in migrations {
        apply_sql(pool, migration).await;
        query(
            "INSERT INTO cocli_schema_migrations (version, name, applied_at) \
             VALUES (?, ?, '2026-07-19T00:00:00Z')",
        )
        .bind(version)
        .bind(name)
        .execute(pool)
        .await
        .expect("fixture migration marker should insert");
    }
}

async fn downgrade_to_0012(path: &Path) {
    let store = Store::open(path).await.expect("fresh store should open");
    store.close().await;

    let pool = connect(path).await;
    query("PRAGMA foreign_keys = OFF")
        .execute(&pool)
        .await
        .expect("fixture foreign keys should disable");
    query("DELETE FROM cocli_schema_migrations WHERE version >= 13")
        .execute(&pool)
        .await
        .expect("governance migration markers should clear");
    for table in GOVERNANCE_TABLES {
        query(&format!("DROP TABLE IF EXISTS {table}"))
            .execute(&pool)
            .await
            .expect("governance table should drop for lineage fixture");
    }
    pool.close().await;
}

async fn migration_versions(path: &Path) -> Vec<i64> {
    let pool = connect(path).await;
    let versions = query_scalar("SELECT version FROM cocli_schema_migrations ORDER BY version")
        .fetch_all(&pool)
        .await
        .expect("migration versions should query");
    pool.close().await;
    versions
}

async fn table_exists(pool: &SqlitePool, name: &str) -> bool {
    query_scalar("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?)")
        .bind(name)
        .fetch_one(pool)
        .await
        .expect("table existence should query")
}

#[tokio::test]
async fn fresh_and_0012_lineages_apply_all_governance_migrations_idempotently() {
    let database = TempDatabase::new("fresh");
    let path = database.path();

    let store = Store::open(&path)
        .await
        .expect("fresh store should migrate");
    store.close().await;
    assert_eq!(
        migration_versions(path).await,
        (1_i64..=19).collect::<Vec<_>>()
    );

    let pool = connect(path).await;
    query("UPDATE cocli_schema_migrations SET name = 'legacy_workspace_name' WHERE version = 12")
        .execute(&pool)
        .await
        .expect("legacy pre-governance migration name should seed");
    pool.close().await;
    let reopened = Store::open(path).await.expect("fresh store should reopen");
    reopened.close().await;
    assert_eq!(
        migration_versions(path).await,
        (1_i64..=19).collect::<Vec<_>>()
    );

    downgrade_to_0012(path).await;
    assert_eq!(
        migration_versions(path).await,
        (1_i64..=12).collect::<Vec<_>>()
    );
    let upgraded = Store::open(&path)
        .await
        .expect("0012 lineage should upgrade");
    upgraded.close().await;
    assert_eq!(
        migration_versions(path).await,
        (1_i64..=19).collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn mcp_only_lineage_adds_skill_governance_without_losing_mcp_audit_data() {
    let database = TempDatabase::new("mcp-only");
    let path = database.path();
    downgrade_to_0012(path).await;

    let pool = connect(path).await;
    seed_lineage(&pool, &MCP_MIGRATIONS).await;
    query(
        "INSERT INTO mcp_bundle_import_audits \
         (id, bundle_hash, schema_version, actor, status, bundle_json, rebindings_json, \
          preview_json, created_at, updated_at) \
         VALUES ('bundle-audit', 'bundle-hash', 2, 'fixture', 'previewed', '{}', '{}', '{}', \
                 '2026-07-19T00:00:00Z', '2026-07-19T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .expect("MCP bundle audit should seed");
    pool.close().await;

    let store = Store::open(&path)
        .await
        .expect("MCP-only lineage should add Skill migrations");
    store.close().await;
    assert_eq!(
        migration_versions(path).await,
        (1_i64..=19).collect::<Vec<_>>()
    );

    let pool = connect(path).await;
    assert!(table_exists(&pool, "skill_governance_workspace_lockfiles").await);
    let bundle_hash: String =
        query_scalar("SELECT bundle_hash FROM mcp_bundle_import_audits WHERE id = 'bundle-audit'")
            .fetch_one(&pool)
            .await
            .expect("MCP bundle audit should survive");
    assert_eq!(bundle_hash, "bundle-hash");
    pool.close().await;
}

#[tokio::test]
async fn skill_only_development_lineage_is_reconciled_without_losing_artifacts() {
    let database = TempDatabase::new("skill-only");
    let path = database.path();
    downgrade_to_0012(path).await;

    let pool = connect(path).await;
    seed_lineage(&pool, &LEGACY_SKILL_MIGRATIONS).await;
    query(
        "INSERT INTO skill_governance_managed_artifacts \
         (id, artifact_key, artifact_kind, content_digest, manifest_digest, schema_version, \
          revision, store_relative_path, artifact_json, created_at) \
         VALUES ('artifact-1', 'skill:reviewer', 'skill', 'content-hash', 'manifest-hash', 1, \
                 'rev-1', 'artifacts/reviewer', '{}', '2026-07-19T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .expect("Skill artifact should seed");
    pool.close().await;

    let store = Store::open(&path)
        .await
        .expect("Skill-only lineage should reconcile and add MCP migrations");
    store.close().await;
    assert_eq!(
        migration_versions(path).await,
        (1_i64..=19).collect::<Vec<_>>()
    );

    let pool = connect(path).await;
    assert!(table_exists(&pool, "mcp_bundle_import_audits").await);
    let artifact_key: String = query_scalar(
        "SELECT artifact_key FROM skill_governance_managed_artifacts WHERE id = 'artifact-1'",
    )
    .fetch_one(&pool)
    .await
    .expect("Skill artifact should survive reconciliation");
    assert_eq!(artifact_key, "skill:reviewer");
    for (version, name) in [
        (17_i64, "skill_governance"),
        (18, "skill_governance_apply_state"),
        (19, "skill_governance_managed_scopes"),
    ] {
        let recorded: String =
            query_scalar("SELECT name FROM cocli_schema_migrations WHERE version = ?")
                .bind(version)
                .fetch_one(&pool)
                .await
                .expect("reconciled Skill marker should exist");
        assert_eq!(recorded, name);
    }
    pool.close().await;
}

#[tokio::test]
async fn failed_governance_migration_rolls_back_and_recovers_on_restart() {
    let database = TempDatabase::new("failure-recovery");
    let path = database.path();
    downgrade_to_0012(path).await;

    let pool = connect(path).await;
    query("CREATE TABLE mcp_profile_bindings_target_idx (id INTEGER PRIMARY KEY)")
        .execute(&pool)
        .await
        .expect("blocking fixture object should create");
    pool.close().await;

    let error = Store::open(&path)
        .await
        .expect_err("migration should fail on the conflicting index name");
    assert!(error
        .to_string()
        .contains("mcp_profile_bindings_target_idx"));

    let pool = connect(path).await;
    assert!(!table_exists(&pool, "mcp_profiles").await);
    let version_13: bool =
        query_scalar("SELECT EXISTS(SELECT 1 FROM cocli_schema_migrations WHERE version = 13)")
            .fetch_one(&pool)
            .await
            .expect("failed migration marker should query");
    assert!(!version_13);
    query("DROP TABLE mcp_profile_bindings_target_idx")
        .execute(&pool)
        .await
        .expect("blocking fixture object should remove");
    pool.close().await;

    let recovered = Store::open(&path)
        .await
        .expect("migration should recover after the conflict is removed");
    recovered.close().await;
    assert_eq!(
        migration_versions(path).await,
        (1_i64..=19).collect::<Vec<_>>()
    );
}
