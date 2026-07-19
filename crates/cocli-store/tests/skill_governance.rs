use std::path::{Path, PathBuf};

use chrono::{Duration, Utc};
use cocli_store::{
    NewSkillGovernanceApplyAction, NewSkillGovernanceApplyRun, NewSkillLockSnapshot,
    SkillGovernanceApplyActionStatus, SkillGovernanceApplyRunStatus, SkillGovernancePlanStatus,
    SkillGovernanceRecoveryStatus, SkillGovernanceScope, Store, StoreError,
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

#[tokio::test]
async fn scoped_locks_are_unique_durable_and_support_stale_takeover() {
    let database_path = temporary_database_path();
    let store = Store::open(&database_path)
        .await
        .expect("store should open");
    let active = store
        .acquire_skill_governance_lock(
            SkillGovernanceScope::Workspace,
            "workspace-lock",
            "owner-a",
            Some(123),
            Some(Uuid::new_v4()),
            "nonce-a",
            Utc::now() + Duration::minutes(5),
        )
        .await
        .expect("active lock should acquire");
    assert!(!active.took_over_stale);
    assert_eq!(active.lock.version, 1);
    assert!(matches!(
        store
            .acquire_skill_governance_lock(
                SkillGovernanceScope::Workspace,
                "workspace-lock",
                "owner-b",
                Some(456),
                None,
                "nonce-b",
                Utc::now() + Duration::minutes(5),
            )
            .await,
        Err(StoreError::SkillGovernanceLockHeld { .. })
    ));
    let released = store
        .release_skill_governance_lock(active.lock.id, active.lock.version, "nonce-a")
        .await
        .expect("current nonce releases");
    assert!(released.released_at.is_some());

    let expired = store
        .acquire_skill_governance_lock(
            SkillGovernanceScope::Workspace,
            "workspace-lock",
            "owner-expired",
            None,
            None,
            "nonce-expired",
            Utc::now() - Duration::seconds(1),
        )
        .await
        .expect("expired lease can be created");
    let takeover = store
        .acquire_skill_governance_lock(
            SkillGovernanceScope::Workspace,
            "workspace-lock",
            "owner-c",
            Some(789),
            None,
            "nonce-c",
            Utc::now() + Duration::minutes(5),
        )
        .await
        .expect("expired lock should be taken over");
    assert!(takeover.took_over_stale);
    assert_eq!(takeover.lock.id, expired.lock.id);
    assert_eq!(
        takeover.lock.previous_owner.as_deref(),
        Some("owner-expired")
    );
    assert_eq!(takeover.lock.takeover_count, 1);
    store.close().await;

    let reopened = Store::open(&database_path)
        .await
        .expect("store should reopen");
    let loaded = reopened
        .get_skill_governance_lock(takeover.lock.id)
        .await
        .expect("lock lookup should work")
        .expect("lock should persist");
    assert_eq!(loaded.owner, "owner-c");
    assert_eq!(loaded.lease_nonce, "nonce-c");
    reopened.close().await;
    remove_database(&database_path).await;
}

#[tokio::test]
async fn apply_runs_and_actions_are_idempotent_audited_and_persist_after_reopen() {
    let database_path = temporary_database_path();
    let store = Store::open(&database_path)
        .await
        .expect("store should open");
    let plan = store
        .create_skill_governance_plan(
            SkillGovernanceScope::Machine,
            "machine-a",
            json!({"steps": ["apply"]}),
            "obs-run",
            "desired-run",
        )
        .await
        .expect("plan should persist");
    let lock = store
        .acquire_skill_governance_lock(
            SkillGovernanceScope::Machine,
            "machine-a",
            "applier",
            Some(42),
            None,
            "lock-nonce",
            Utc::now() + Duration::minutes(5),
        )
        .await
        .expect("lock should acquire")
        .lock;
    let run_input = NewSkillGovernanceApplyRun {
        scope: SkillGovernanceScope::Machine,
        scope_id: "machine-a".to_owned(),
        plan_id: Some(plan.id),
        lock_id: Some(lock.id),
        idempotency_key: "idem-1".to_owned(),
        nonce: "run-nonce".to_owned(),
        observation_hash: "obs-run".to_owned(),
        desired_hash: "desired-run".to_owned(),
        lock_hash: "lock-run".to_owned(),
        backup_path: None,
        quarantine_path: None,
        evidence: json!({"createdBy": "test"}),
    };
    let run = store
        .create_skill_governance_apply_run(run_input.clone())
        .await
        .expect("run should create");
    assert_eq!(run.status, SkillGovernanceApplyRunStatus::Pending);
    let attached_lock = store
        .attach_skill_governance_lock_run(
            lock.id,
            lock.version,
            "lock-nonce",
            run.id,
            Utc::now() + Duration::minutes(5),
        )
        .await
        .expect("lock should attach to run");
    assert_eq!(attached_lock.run_id, Some(run.id));
    assert_eq!(
        store
            .create_skill_governance_apply_run(run_input.clone())
            .await
            .expect("same idempotent input returns existing")
            .id,
        run.id
    );
    let mut conflicting_run = run_input;
    "different-nonce".clone_into(&mut conflicting_run.nonce);
    assert!(matches!(
        store
            .create_skill_governance_apply_run(conflicting_run)
            .await,
        Err(StoreError::SkillGovernanceIdempotencyConflict { .. })
    ));
    let running = store
        .transition_skill_governance_apply_run(
            run.id,
            run.version,
            SkillGovernanceApplyRunStatus::Running,
            SkillGovernanceRecoveryStatus::NotRequired,
            Some("/tmp/run-backup"),
            None,
            json!({"phase": "running"}),
            None,
        )
        .await
        .expect("run should transition");
    assert_eq!(running.attempts, 1);
    assert_eq!(running.backup_path.as_deref(), Some("/tmp/run-backup"));

    let action_input = NewSkillGovernanceApplyAction {
        run_id: run.id,
        sequence: 0,
        action_key: "write-skill-lock".to_owned(),
        request_hash: "request-hash".to_owned(),
        backup_path: None,
        quarantine_path: None,
        evidence: json!({"queued": true}),
    };
    let action = store
        .create_skill_governance_apply_action(action_input.clone())
        .await
        .expect("action should create");
    assert_eq!(
        store
            .create_skill_governance_apply_action(action_input.clone())
            .await
            .expect("same action key returns existing")
            .id,
        action.id
    );
    let mut conflicting_action = action_input;
    "different-request".clone_into(&mut conflicting_action.request_hash);
    assert!(matches!(
        store
            .create_skill_governance_apply_action(conflicting_action)
            .await,
        Err(StoreError::SkillGovernanceIdempotencyConflict { .. })
    ));
    let preflight = store
        .transition_skill_governance_apply_action(
            action.id,
            action.version,
            SkillGovernanceApplyActionStatus::Preflight,
            None,
            Some("/tmp/action-backup"),
            None,
            json!({"phase": "preflight"}),
            None,
        )
        .await
        .expect("action should preflight");
    assert_eq!(preflight.attempts, 1);
    assert_eq!(preflight.backup_path.as_deref(), Some("/tmp/action-backup"));
    let verified = store
        .transition_skill_governance_apply_action(
            action.id,
            preflight.version,
            SkillGovernanceApplyActionStatus::Verified,
            Some("result-hash"),
            None,
            Some("/tmp/action-quarantine"),
            json!({"phase": "verified"}),
            None,
        )
        .await
        .expect("action should verify");
    assert_eq!(verified.result_hash.as_deref(), Some("result-hash"));
    assert_eq!(
        verified.quarantine_path.as_deref(),
        Some("/tmp/action-quarantine")
    );
    let recovery_required = store
        .transition_skill_governance_apply_run(
            run.id,
            running.version,
            SkillGovernanceApplyRunStatus::RecoveryRequired,
            SkillGovernanceRecoveryStatus::Pending,
            None,
            Some("/tmp/run-quarantine"),
            json!({"phase": "recovery_required"}),
            Some("verification failed"),
        )
        .await
        .expect("run should record recovery requirement");
    assert_eq!(
        recovery_required.quarantine_path.as_deref(),
        Some("/tmp/run-quarantine")
    );
    assert_eq!(
        recovery_required.recovery_status,
        SkillGovernanceRecoveryStatus::Pending
    );
    assert_eq!(
        store
            .list_skill_governance_apply_audit("run", run.id)
            .await
            .expect("run audit should list")
            .len(),
        3
    );
    assert_eq!(
        store
            .list_skill_governance_apply_actions(run.id)
            .await
            .expect("actions should list")
            .len(),
        1
    );
    store.close().await;

    let reopened = Store::open(&database_path)
        .await
        .expect("store should reopen");
    let loaded_run = reopened
        .get_skill_governance_apply_run(run.id)
        .await
        .expect("run lookup should work")
        .expect("run should survive reopen");
    assert_eq!(
        loaded_run.status,
        SkillGovernanceApplyRunStatus::RecoveryRequired
    );
    assert_eq!(
        loaded_run.last_error.as_deref(),
        Some("verification failed")
    );
    let loaded_action = reopened
        .get_skill_governance_apply_action(action.id)
        .await
        .expect("action lookup should work")
        .expect("action should survive reopen");
    assert_eq!(
        loaded_action.status,
        SkillGovernanceApplyActionStatus::Verified
    );
    assert_eq!(loaded_action.result_hash.as_deref(), Some("result-hash"));
    reopened.close().await;
    remove_database(&database_path).await;
}

#[tokio::test]
async fn apply_state_transitions_require_current_versions() {
    let store = Store::in_memory().await.expect("store should open");
    let lock = store
        .acquire_skill_governance_lock(
            SkillGovernanceScope::Agent,
            "agent-version",
            "owner",
            None,
            None,
            "nonce",
            Utc::now() + Duration::minutes(5),
        )
        .await
        .expect("lock should acquire")
        .lock;
    assert!(matches!(
        store
            .renew_skill_governance_lock(
                lock.id,
                lock.version + 1,
                "nonce",
                Utc::now() + Duration::minutes(10),
            )
            .await,
        Err(StoreError::SkillGovernanceVersionConflict { .. })
    ));
    let run = store
        .create_skill_governance_apply_run(NewSkillGovernanceApplyRun {
            scope: SkillGovernanceScope::Agent,
            scope_id: "agent-version".to_owned(),
            plan_id: None,
            lock_id: Some(lock.id),
            idempotency_key: "idem-version".to_owned(),
            nonce: "run-version".to_owned(),
            observation_hash: "obs".to_owned(),
            desired_hash: "desired".to_owned(),
            lock_hash: "lock".to_owned(),
            backup_path: None,
            quarantine_path: None,
            evidence: json!({}),
        })
        .await
        .expect("run should create");
    assert!(matches!(
        store
            .transition_skill_governance_apply_run(
                run.id,
                run.version + 1,
                SkillGovernanceApplyRunStatus::Running,
                SkillGovernanceRecoveryStatus::NotRequired,
                None,
                None,
                json!({}),
                None,
            )
            .await,
        Err(StoreError::SkillGovernanceVersionConflict { .. })
    ));
    let action = store
        .create_skill_governance_apply_action(NewSkillGovernanceApplyAction {
            run_id: run.id,
            sequence: 1,
            action_key: "versioned-action".to_owned(),
            request_hash: "request".to_owned(),
            backup_path: None,
            quarantine_path: None,
            evidence: json!({}),
        })
        .await
        .expect("action should create");
    assert!(matches!(
        store
            .transition_skill_governance_apply_action(
                action.id,
                action.version + 1,
                SkillGovernanceApplyActionStatus::Preflight,
                None,
                None,
                None,
                json!({}),
                None,
            )
            .await,
        Err(StoreError::SkillGovernanceVersionConflict { .. })
    ));
}
