use std::path::{Path, PathBuf};

use cocli_store::{
    NewSkillLockSnapshot, SkillGovernancePlanStatus, SkillGovernanceScope, Store, StoreError,
};
use serde_json::json;
use sqlx_core::query::query;
use sqlx_sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use uuid::Uuid;

fn temporary_database_path() -> PathBuf {
    std::env::temp_dir().join(format!("cocli-skill-governance-{}.sqlite3", Uuid::new_v4()))
}

async fn remove_database(path: &Path) {
    let _ = tokio::fs::remove_file(path).await;
}

#[tokio::test]
async fn profiles_bindings_snapshots_and_plans_persist_after_reopen() {
    let database_path = temporary_database_path();
    let store = Store::open(&database_path)
        .await
        .expect("store should open");
    let profile = store
        .create_skill_profile(json!({
            "skills": [{"name": "analysis", "policy": {"allow": true}}],
            "opaque": {"nested": [1, 2, 3]}
        }))
        .await
        .expect("profile should persist");
    let binding = store
        .bind_skill_profile(
            SkillGovernanceScope::Machine,
            "local-installation",
            profile.id,
        )
        .await
        .expect("binding should persist");
    let snapshot = store
        .create_skill_lock_snapshot(NewSkillLockSnapshot {
            scope: SkillGovernanceScope::Machine,
            scope_id: "local-installation".to_owned(),
            profile_id: Some(profile.id),
            snapshot: json!({"locks": [{"name": "analysis", "source": "profile"}]}),
            observation_hash: "obs-1".to_owned(),
            desired_hash: "desired-1".to_owned(),
            lock_hash: "lock-1".to_owned(),
        })
        .await
        .expect("snapshot should persist");
    let plan = store
        .create_skill_governance_plan(
            SkillGovernanceScope::Machine,
            "local-installation",
            json!({"steps": [{"install": "analysis"}]}),
            "obs-1",
            "desired-1",
        )
        .await
        .expect("plan should persist");
    store.close().await;

    let reopened = Store::open(&database_path)
        .await
        .expect("store should reopen");
    let loaded_profile = reopened
        .get_skill_profile(profile.id)
        .await
        .expect("profile lookup should work")
        .expect("profile should survive reopen");
    assert_eq!(
        loaded_profile.document["opaque"]["nested"],
        json!([1, 2, 3])
    );
    assert_eq!(loaded_profile.version, 1);
    assert_eq!(
        reopened
            .get_skill_profile_binding(binding.id)
            .await
            .expect("binding lookup should work")
            .expect("binding should survive reopen"),
        binding
    );
    assert_eq!(
        reopened
            .list_skill_lock_snapshots(SkillGovernanceScope::Machine, "local-installation")
            .await
            .expect("snapshots should list")[0]
            .id,
        snapshot.id
    );
    assert_eq!(
        reopened
            .get_skill_governance_plan(plan.id)
            .await
            .expect("plan lookup should work")
            .expect("plan should survive reopen")
            .status,
        SkillGovernancePlanStatus::Draft
    );
    reopened.close().await;
    remove_database(&database_path).await;
}

#[tokio::test]
async fn same_layer_bindings_coexist_and_unbind_requires_expected_version() {
    let store = Store::in_memory().await.expect("store should open");
    let first_profile = store
        .create_skill_profile(json!({"profile": "first"}))
        .await
        .expect("first profile should persist");
    let second_profile = store
        .create_skill_profile(json!({"profile": "second"}))
        .await
        .expect("second profile should persist");

    let created = store
        .bind_skill_profile(
            SkillGovernanceScope::Workspace,
            "workspace-a",
            first_profile.id,
        )
        .await
        .expect("first binding should create");
    assert_eq!(created.version, 1);
    let second = store
        .bind_skill_profile(
            SkillGovernanceScope::Workspace,
            "workspace-a",
            second_profile.id,
        )
        .await
        .expect("same-layer binding should remain visible for conflict resolution");
    assert_eq!(second.version, 1);
    assert_eq!(
        store
            .list_skill_profile_bindings_for_scope(SkillGovernanceScope::Workspace, "workspace-a")
            .await
            .expect("bindings should list")
            .len(),
        2
    );
    assert!(matches!(
        store.unbind_skill_profile(created.id, 2).await,
        Err(StoreError::SkillGovernanceVersionConflict { .. })
    ));
    assert!(store
        .unbind_skill_profile(created.id, created.version)
        .await
        .expect("binding should delete with current version"));
    assert!(store
        .get_skill_profile_binding(created.id)
        .await
        .expect("binding lookup should work")
        .is_none());
    assert!(store
        .get_skill_profile_binding(second.id)
        .await
        .expect("second lookup")
        .is_some());
}

#[tokio::test]
async fn plan_transitions_are_audited_and_versioned() {
    let store = Store::in_memory().await.expect("store should open");
    let plan = store
        .create_skill_governance_plan(
            SkillGovernanceScope::Agent,
            "agent-a",
            json!({"diff": [{"enable": "writer"}]}),
            "obs-a",
            "desired-a",
        )
        .await
        .expect("plan should persist");
    let approved = store
        .approve_skill_governance_plan(plan.id, plan.version)
        .await
        .expect("plan should approve");
    assert_eq!(approved.status, SkillGovernancePlanStatus::Approved);
    assert_eq!(approved.version, 2);
    assert!(matches!(
        store
            .reject_skill_governance_plan(plan.id, plan.version)
            .await,
        Err(StoreError::SkillGovernanceVersionConflict { .. })
    ));
    assert!(matches!(
        store
            .reject_skill_governance_plan(plan.id, approved.version)
            .await,
        Err(StoreError::SkillGovernanceTransitionConflict { .. })
    ));
    let stale = store
        .mark_skill_governance_plan_stale(plan.id, approved.version)
        .await
        .expect("approved plan can be marked stale");
    assert_eq!(stale.status, SkillGovernancePlanStatus::Stale);
    assert_eq!(stale.version, 3);

    let audit = store
        .list_skill_governance_plan_audit(plan.id)
        .await
        .expect("audit should list");
    assert_eq!(audit.len(), 2);
    assert_eq!(audit[0].action, "approve");
    assert_eq!(audit[0].from_status, SkillGovernancePlanStatus::Draft);
    assert_eq!(audit[0].to_status, SkillGovernancePlanStatus::Approved);
    assert_eq!(audit[0].from_version, 1);
    assert_eq!(audit[0].to_version, 2);
    assert_eq!(audit[1].action, "stale");
    assert_eq!(audit[1].from_status, SkillGovernancePlanStatus::Approved);
    assert_eq!(audit[1].to_status, SkillGovernancePlanStatus::Stale);
}

#[tokio::test]
async fn json_documents_roundtrip_opaquely_and_conflict_errors_do_not_include_json() {
    let store = Store::in_memory().await.expect("store should open");
    let profile = store
        .create_skill_profile(json!({
            "secret": "super-sensitive-token",
            "typedElsewhere": {"scope": "not validated here"}
        }))
        .await
        .expect("profile should persist");
    let updated = store
        .update_skill_profile(
            profile.id,
            json!({"secret": "new-sensitive-token"}),
            profile.version,
        )
        .await
        .expect("profile should update");
    assert_eq!(updated.version, 2);
    assert_eq!(updated.document["secret"], "new-sensitive-token");

    let error = store
        .update_skill_profile(
            profile.id,
            json!({"secret": "should-not-appear-in-error"}),
            profile.version,
        )
        .await
        .expect_err("stale update should fail");
    let message = error.to_string();
    assert!(!message.contains("super-sensitive-token"));
    assert!(!message.contains("new-sensitive-token"));
    assert!(!message.contains("should-not-appear-in-error"));

    assert!(matches!(
        store.delete_skill_profile(profile.id, 1).await,
        Err(StoreError::SkillGovernanceVersionConflict { .. })
    ));
    assert!(store
        .delete_skill_profile(profile.id, updated.version)
        .await
        .expect("current version deletes"));
}

#[tokio::test]
async fn database_constraints_reject_unknown_scope_and_status_values() {
    let database_path = temporary_database_path();
    let store = Store::open(&database_path)
        .await
        .expect("store should open");
    store.close().await;
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::new().filename(&database_path))
        .await
        .expect("raw database should open");
    let id = Uuid::new_v4();
    assert!(query(
        "INSERT INTO skill_governance_plans \
         (id, scope, scope_id, plan_json, observation_hash, desired_hash, status, version, \
          created_at, updated_at) \
         VALUES (?, 'global', 'scope', '{}', 'obs', 'desired', 'draft', 1, \
                 '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    )
    .bind(id)
    .execute(&pool)
    .await
    .is_err());
    assert!(query(
        "INSERT INTO skill_governance_plans \
         (id, scope, scope_id, plan_json, observation_hash, desired_hash, status, version, \
          created_at, updated_at) \
         VALUES (?, 'machine', 'scope', '{}', 'obs', 'desired', 'pending', 1, \
                 '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    )
    .bind(Uuid::new_v4())
    .execute(&pool)
    .await
    .is_err());
    pool.close().await;
    remove_database(&database_path).await;
}
