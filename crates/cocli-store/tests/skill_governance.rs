use std::path::{Path, PathBuf};

use chrono::{Duration, Utc};
use cocli_store::{
    NewSkillGovernanceApplyAction, NewSkillGovernanceApplyRun, NewSkillGovernanceGcReference,
    NewSkillGovernanceManagedArtifact, NewSkillGovernanceMaterialization, NewSkillLockSnapshot,
    SkillGovernanceApplyActionStatus, SkillGovernanceApplyRunStatus,
    SkillGovernanceInstallationMode, SkillGovernanceMaterializationOwnership,
    SkillGovernanceMaterializationRootKind, SkillGovernancePlanStatus,
    SkillGovernanceRecoveryStatus, SkillGovernanceScope, SkillGovernanceVerifyStatus, Store,
    StoreError,
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

#[tokio::test]
async fn managed_artifacts_materializations_and_adoption_audit_persist_after_reopen() {
    let database_path = temporary_database_path();
    let store = Store::open(&database_path)
        .await
        .expect("store should open");
    let artifact = store
        .create_skill_governance_managed_artifact(NewSkillGovernanceManagedArtifact {
            artifact_key: "skill:writer@sha256:aaa".to_owned(),
            artifact_kind: "skill".to_owned(),
            source_provenance: json!({"registry": "local", "package": "writer"}),
            content_digest: "sha256:aaa".to_owned(),
            manifest_digest: "sha256:manifest-aaa".to_owned(),
            schema_version: 1,
            revision: "rev-1".to_owned(),
            store_relative_path: "artifacts/skills/writer/sha256-aaa".to_owned(),
            artifact: json!({"name": "writer", "files": ["SKILL.md"]}),
            metadata: json!({"source": "registry"}),
        })
        .await
        .expect("artifact should create");
    assert_eq!(artifact.version, 1);
    assert_eq!(
        store
            .create_skill_governance_managed_artifact(NewSkillGovernanceManagedArtifact {
                artifact_key: "skill:writer@sha256:aaa".to_owned(),
                artifact_kind: "skill".to_owned(),
                source_provenance: json!({"registry": "local", "package": "writer"}),
                content_digest: "sha256:aaa".to_owned(),
                manifest_digest: "sha256:manifest-aaa".to_owned(),
                schema_version: 1,
                revision: "rev-1".to_owned(),
                store_relative_path: "artifacts/skills/writer/sha256-aaa".to_owned(),
                artifact: json!({"name": "writer", "files": ["SKILL.md"]}),
                metadata: json!({"source": "registry"}),
            })
            .await
            .expect("same immutable artifact returns existing")
            .id,
        artifact.id
    );
    assert!(matches!(
        store
            .create_skill_governance_managed_artifact(NewSkillGovernanceManagedArtifact {
                artifact_key: "skill:writer@sha256:aaa".to_owned(),
                artifact_kind: "skill".to_owned(),
                source_provenance: json!({"registry": "local"}),
                content_digest: "sha256:bbb".to_owned(),
                manifest_digest: "sha256:manifest-bbb".to_owned(),
                schema_version: 1,
                revision: "rev-2".to_owned(),
                store_relative_path: "artifacts/skills/writer/sha256-bbb".to_owned(),
                artifact: json!({"name": "writer", "files": ["OTHER.md"]}),
                metadata: json!({}),
            })
            .await,
        Err(StoreError::SkillGovernanceIdempotencyConflict { .. })
    ));

    let materialized = store
        .create_skill_governance_materialization(NewSkillGovernanceMaterialization {
            artifact_id: artifact.id,
            scope: SkillGovernanceScope::Workspace,
            scope_id: "workspace-managed".to_owned(),
            target_path: ".codex/skills/writer".to_owned(),
            target_runtime: "codex".to_owned(),
            root_kind: SkillGovernanceMaterializationRootKind::Workspace,
            installation_mode: SkillGovernanceInstallationMode::Copy,
            ownership: SkillGovernanceMaterializationOwnership::Unmanaged,
            content_digest: "sha256:aaa".to_owned(),
            expected_destination: ".codex/skills/writer/SKILL.md".to_owned(),
            expected_fingerprint: "fingerprint:writer-v1".to_owned(),
            verify_status: SkillGovernanceVerifyStatus::Unknown,
            receipt: json!({"detected": true}),
        })
        .await
        .expect("materialization should create");
    assert_eq!(
        materialized.ownership,
        SkillGovernanceMaterializationOwnership::Unmanaged
    );
    assert_eq!(materialized.target_runtime, "codex");
    assert_eq!(
        materialized.root_kind,
        SkillGovernanceMaterializationRootKind::Workspace
    );
    assert_eq!(
        materialized.installation_mode,
        SkillGovernanceInstallationMode::Copy
    );
    assert_eq!(
        materialized.expected_destination,
        ".codex/skills/writer/SKILL.md"
    );
    assert_eq!(materialized.expected_fingerprint, "fingerprint:writer-v1");
    assert_eq!(
        materialized.verify_status,
        SkillGovernanceVerifyStatus::Unknown
    );
    let foreign = store
        .create_skill_governance_materialization(NewSkillGovernanceMaterialization {
            artifact_id: artifact.id,
            scope: SkillGovernanceScope::Workspace,
            scope_id: "workspace-managed".to_owned(),
            target_path: ".codex/skills/writer-foreign".to_owned(),
            target_runtime: "codex".to_owned(),
            root_kind: SkillGovernanceMaterializationRootKind::Workspace,
            installation_mode: SkillGovernanceInstallationMode::InPlace,
            ownership: SkillGovernanceMaterializationOwnership::Foreign,
            content_digest: "sha256:foreign".to_owned(),
            expected_destination: ".codex/skills/writer-foreign/SKILL.md".to_owned(),
            expected_fingerprint: "fingerprint:foreign".to_owned(),
            verify_status: SkillGovernanceVerifyStatus::Verified,
            receipt: json!({"detected": "outside-management"}),
        })
        .await
        .expect("foreign materialization should create");
    assert_eq!(
        foreign.ownership,
        SkillGovernanceMaterializationOwnership::Foreign
    );
    let adopted = store
        .adopt_skill_governance_materialization(
            materialized.id,
            materialized.version,
            json!({"adoptedBy": "phase3c"}),
        )
        .await
        .expect("materialization should adopt");
    assert_eq!(
        adopted.ownership,
        SkillGovernanceMaterializationOwnership::Adopted
    );
    assert_eq!(adopted.version, 2);
    let audit = store
        .list_skill_governance_adoption_audit(materialized.id)
        .await
        .expect("audit should list");
    assert_eq!(audit.len(), 1);
    assert_eq!(
        audit[0].from_ownership,
        SkillGovernanceMaterializationOwnership::Unmanaged
    );
    assert_eq!(
        audit[0].to_ownership,
        SkillGovernanceMaterializationOwnership::Adopted
    );
    store.close().await;

    let reopened = Store::open(&database_path)
        .await
        .expect("store should reopen");
    let loaded_artifact = reopened
        .get_skill_governance_managed_artifact(artifact.id)
        .await
        .expect("artifact lookup should work")
        .expect("artifact should persist");
    assert_eq!(loaded_artifact.artifact["name"], "writer");
    assert_eq!(loaded_artifact.source_provenance["registry"], "local");
    assert_eq!(loaded_artifact.content_digest, "sha256:aaa");
    assert_eq!(loaded_artifact.manifest_digest, "sha256:manifest-aaa");
    assert_eq!(loaded_artifact.schema_version, 1);
    assert_eq!(loaded_artifact.revision, "rev-1");
    assert_eq!(
        loaded_artifact.store_relative_path,
        "artifacts/skills/writer/sha256-aaa"
    );
    let loaded_materialization = reopened
        .get_skill_governance_materialization(materialized.id)
        .await
        .expect("materialization lookup should work")
        .expect("materialization should persist");
    assert_eq!(
        loaded_materialization.ownership,
        SkillGovernanceMaterializationOwnership::Adopted
    );
    assert_eq!(
        loaded_materialization.expected_fingerprint,
        "fingerprint:writer-v1"
    );
    reopened.close().await;
    remove_database(&database_path).await;
}

#[tokio::test]
async fn materialization_upsert_requires_cas_for_replacement_and_preserves_ownership_rules() {
    let store = Store::in_memory().await.expect("store should open");
    let first_artifact = store
        .create_skill_governance_managed_artifact(NewSkillGovernanceManagedArtifact {
            artifact_key: "skill:upsert@sha256:first".to_owned(),
            artifact_kind: "skill".to_owned(),
            source_provenance: json!({"registry": "local"}),
            content_digest: "sha256:first".to_owned(),
            manifest_digest: "sha256:manifest-first".to_owned(),
            schema_version: 1,
            revision: "rev-first".to_owned(),
            store_relative_path: "artifacts/skills/upsert/first".to_owned(),
            artifact: json!({"name": "upsert", "rev": 1}),
            metadata: json!({}),
        })
        .await
        .expect("first artifact should create");
    let second_artifact = store
        .create_skill_governance_managed_artifact(NewSkillGovernanceManagedArtifact {
            artifact_key: "skill:upsert@sha256:second".to_owned(),
            artifact_kind: "skill".to_owned(),
            source_provenance: json!({"registry": "local"}),
            content_digest: "sha256:second".to_owned(),
            manifest_digest: "sha256:manifest-second".to_owned(),
            schema_version: 1,
            revision: "rev-second".to_owned(),
            store_relative_path: "artifacts/skills/upsert/second".to_owned(),
            artifact: json!({"name": "upsert", "rev": 2}),
            metadata: json!({}),
        })
        .await
        .expect("second artifact should create");
    let first_input = NewSkillGovernanceMaterialization {
        artifact_id: first_artifact.id,
        scope: SkillGovernanceScope::Workspace,
        scope_id: "workspace-upsert".to_owned(),
        target_path: ".codex/skills/upsert".to_owned(),
        target_runtime: "codex".to_owned(),
        root_kind: SkillGovernanceMaterializationRootKind::Workspace,
        installation_mode: SkillGovernanceInstallationMode::Copy,
        ownership: SkillGovernanceMaterializationOwnership::Managed,
        content_digest: "sha256:first".to_owned(),
        expected_destination: ".codex/skills/upsert/SKILL.md".to_owned(),
        expected_fingerprint: "fingerprint:first".to_owned(),
        verify_status: SkillGovernanceVerifyStatus::Verified,
        receipt: json!({"rev": 1}),
    };
    let created = store
        .upsert_skill_governance_materialization(first_input.clone(), None)
        .await
        .expect("missing target should create");
    assert_eq!(created.version, 1);
    assert_eq!(
        store
            .upsert_skill_governance_materialization(first_input.clone(), None)
            .await
            .expect("same target and values remain idempotent")
            .id,
        created.id
    );

    let replacement = NewSkillGovernanceMaterialization {
        artifact_id: second_artifact.id,
        content_digest: "sha256:second".to_owned(),
        expected_fingerprint: "fingerprint:second".to_owned(),
        verify_status: SkillGovernanceVerifyStatus::Unknown,
        receipt: json!({"rev": 2}),
        ..first_input.clone()
    };
    assert!(matches!(
        store
            .upsert_skill_governance_materialization(replacement.clone(), None)
            .await,
        Err(StoreError::SkillGovernanceVersionConflict { .. })
    ));
    assert!(matches!(
        store
            .upsert_skill_governance_materialization(
                replacement.clone(),
                Some(created.version + 1),
            )
            .await,
        Err(StoreError::SkillGovernanceVersionConflict { .. })
    ));
    let replaced = store
        .upsert_skill_governance_materialization(replacement.clone(), Some(created.version))
        .await
        .expect("current version should replace mutable materialization fields");
    assert_eq!(replaced.version, 2);
    assert_eq!(replaced.artifact_id, second_artifact.id);
    assert_eq!(replaced.content_digest, "sha256:second");
    assert_eq!(replaced.expected_fingerprint, "fingerprint:second");
    assert_eq!(replaced.verify_status, SkillGovernanceVerifyStatus::Unknown);
    assert_eq!(
        replaced.ownership,
        SkillGovernanceMaterializationOwnership::Managed
    );

    let adopted = store
        .upsert_skill_governance_materialization(
            NewSkillGovernanceMaterialization {
                ownership: SkillGovernanceMaterializationOwnership::Adopted,
                receipt: json!({"rev": 2, "adopted": true}),
                ..replacement.clone()
            },
            Some(replaced.version),
        )
        .await
        .expect("managed to adopted is valid with CAS");
    assert_eq!(
        adopted.ownership,
        SkillGovernanceMaterializationOwnership::Adopted
    );
    assert!(matches!(
        store
            .upsert_skill_governance_materialization(
                NewSkillGovernanceMaterialization {
                    ownership: SkillGovernanceMaterializationOwnership::Foreign,
                    ..replacement
                },
                Some(adopted.version),
            )
            .await,
        Err(StoreError::SkillGovernanceDeleteConflict { .. })
    ));
}

#[tokio::test]
async fn workspace_lockfile_records_are_versioned_and_restore_metadata_survives_reopen() {
    let database_path = temporary_database_path();
    let store = Store::open(&database_path)
        .await
        .expect("store should open");
    let created = store
        .upsert_skill_governance_workspace_lockfile(
            "workspace-lockfile",
            ".codex/skill-lock.json",
            "lock-hash-1",
            "disk-fingerprint-1",
            "disk-hash-1",
            json!({"skills": ["writer"]}),
            Some("/tmp/lock.bak"),
            Some("backup-hash-1"),
            json!({"operation": "initial-write", "previousHash": "old"}),
            json!({"backupPath": "/tmp/lock.bak", "previousHash": "old"}),
            None,
        )
        .await
        .expect("lockfile should create");
    assert_eq!(created.version, 1);
    assert!(matches!(
        store
            .upsert_skill_governance_workspace_lockfile(
                "workspace-lockfile",
                ".codex/skill-lock.json",
                "lock-hash-stale",
                "disk-fingerprint-stale",
                "disk-hash-stale",
                json!({}),
                None,
                None,
                json!({}),
                json!({}),
                Some(created.version + 1),
            )
            .await,
        Err(StoreError::SkillGovernanceVersionConflict { .. })
    ));
    let updated = store
        .upsert_skill_governance_workspace_lockfile(
            "workspace-lockfile",
            ".codex/skill-lock.json",
            "lock-hash-2",
            "disk-fingerprint-2",
            "disk-hash-2",
            json!({"skills": ["writer", "reviewer"]}),
            Some("/tmp/lock-2.bak"),
            Some("backup-hash-2"),
            json!({"operation": "restore-ready", "backupPath": "/tmp/lock-2.bak"}),
            json!({"backupPath": "/tmp/lock-2.bak", "previousHash": "lock-hash-1"}),
            Some(created.version),
        )
        .await
        .expect("lockfile should update with current version");
    assert_eq!(updated.version, 2);
    store.close().await;

    let reopened = Store::open(&database_path)
        .await
        .expect("store should reopen");
    let loaded = reopened
        .get_skill_governance_workspace_lockfile("workspace-lockfile", ".codex/skill-lock.json")
        .await
        .expect("lockfile lookup should work")
        .expect("lockfile should persist");
    assert_eq!(loaded.lock_hash, "lock-hash-2");
    assert_eq!(loaded.expected_disk_fingerprint, "disk-fingerprint-2");
    assert_eq!(loaded.expected_disk_hash, "disk-hash-2");
    assert_eq!(loaded.last_backup_path.as_deref(), Some("/tmp/lock-2.bak"));
    assert_eq!(loaded.last_backup_hash.as_deref(), Some("backup-hash-2"));
    assert_eq!(loaded.last_receipt["operation"], "restore-ready");
    assert_eq!(loaded.restore_metadata["previousHash"], "lock-hash-1");
    assert!(matches!(
        reopened
            .delete_skill_governance_workspace_lockfile(
                "workspace-lockfile",
                ".codex/skill-lock.json",
                loaded.version + 1,
            )
            .await,
        Err(StoreError::SkillGovernanceVersionConflict { .. })
    ));
    assert!(reopened
        .delete_skill_governance_workspace_lockfile(
            "workspace-lockfile",
            ".codex/skill-lock.json",
            loaded.version,
        )
        .await
        .expect("current version should delete lockfile"));
    assert!(reopened
        .get_skill_governance_workspace_lockfile("workspace-lockfile", ".codex/skill-lock.json")
        .await
        .expect("lockfile lookup should work")
        .is_none());
    reopened.close().await;
    remove_database(&database_path).await;
}

#[tokio::test]
async fn gc_preview_respects_recorded_references() {
    let store = Store::in_memory().await.expect("store should open");
    let artifact = store
        .create_skill_governance_managed_artifact(NewSkillGovernanceManagedArtifact {
            artifact_key: "skill:cleanup@sha256:ccc".to_owned(),
            artifact_kind: "skill".to_owned(),
            source_provenance: json!({"registry": "local"}),
            content_digest: "sha256:ccc".to_owned(),
            manifest_digest: "sha256:manifest-ccc".to_owned(),
            schema_version: 1,
            revision: "rev-cleanup".to_owned(),
            store_relative_path: "artifacts/skills/cleanup/sha256-ccc".to_owned(),
            artifact: json!({"name": "cleanup"}),
            metadata: json!({}),
        })
        .await
        .expect("artifact should create");
    let materialization = store
        .create_skill_governance_materialization(NewSkillGovernanceMaterialization {
            artifact_id: artifact.id,
            scope: SkillGovernanceScope::Agent,
            scope_id: "agent-gc".to_owned(),
            target_path: ".codex/skills/cleanup".to_owned(),
            target_runtime: "codex".to_owned(),
            root_kind: SkillGovernanceMaterializationRootKind::Agent,
            installation_mode: SkillGovernanceInstallationMode::Copy,
            ownership: SkillGovernanceMaterializationOwnership::Managed,
            content_digest: "sha256:ccc".to_owned(),
            expected_destination: ".codex/skills/cleanup/SKILL.md".to_owned(),
            expected_fingerprint: "fingerprint:cleanup".to_owned(),
            verify_status: SkillGovernanceVerifyStatus::Verified,
            receipt: json!({"installed": true}),
        })
        .await
        .expect("materialization should create");
    let before_refs = store
        .preview_skill_governance_gc()
        .await
        .expect("preview should work");
    assert!(!before_refs
        .iter()
        .any(|candidate| candidate.entity_type == "managed_artifact"
            && candidate.entity_id == artifact.id));
    assert!(before_refs
        .iter()
        .any(|candidate| candidate.entity_type == "materialization"
            && candidate.entity_id == materialization.id));
    let unmanaged = store
        .create_skill_governance_materialization(NewSkillGovernanceMaterialization {
            artifact_id: artifact.id,
            scope: SkillGovernanceScope::Agent,
            scope_id: "agent-gc".to_owned(),
            target_path: ".codex/skills/cleanup-unmanaged".to_owned(),
            target_runtime: "codex".to_owned(),
            root_kind: SkillGovernanceMaterializationRootKind::Agent,
            installation_mode: SkillGovernanceInstallationMode::InPlace,
            ownership: SkillGovernanceMaterializationOwnership::Unmanaged,
            content_digest: "sha256:ccc".to_owned(),
            expected_destination: ".codex/skills/cleanup-unmanaged/SKILL.md".to_owned(),
            expected_fingerprint: "fingerprint:cleanup-unmanaged".to_owned(),
            verify_status: SkillGovernanceVerifyStatus::Unknown,
            receipt: json!({"detected": true}),
        })
        .await
        .expect("unmanaged materialization should create");
    let with_unmanaged = store
        .preview_skill_governance_gc()
        .await
        .expect("preview should work with unmanaged");
    assert!(!with_unmanaged
        .iter()
        .any(|candidate| candidate.entity_type == "materialization"
            && candidate.entity_id == unmanaged.id));

    store
        .create_skill_governance_gc_reference(NewSkillGovernanceGcReference {
            source_type: "workspace_lockfile".to_owned(),
            source_id: Uuid::new_v4(),
            target_type: "materialization".to_owned(),
            target_id: materialization.id,
            reference_kind: "pins".to_owned(),
            metadata: json!({"reason": "lockfile"}),
        })
        .await
        .expect("materialization reference should persist");
    let after_refs = store
        .preview_skill_governance_gc()
        .await
        .expect("preview should work after refs");
    assert!(!after_refs
        .iter()
        .any(|candidate| candidate.entity_type == "managed_artifact"
            && candidate.entity_id == artifact.id));
    assert!(!after_refs
        .iter()
        .any(|candidate| candidate.entity_type == "materialization"
            && candidate.entity_id == materialization.id));
}

#[tokio::test]
async fn gc_delete_methods_are_cas_safe_and_respect_references_and_ownership() {
    let store = Store::in_memory().await.expect("store should open");
    let artifact = store
        .create_skill_governance_managed_artifact(NewSkillGovernanceManagedArtifact {
            artifact_key: "skill:delete@sha256:ddd".to_owned(),
            artifact_kind: "skill".to_owned(),
            source_provenance: json!({"registry": "local"}),
            content_digest: "sha256:ddd".to_owned(),
            manifest_digest: "sha256:manifest-ddd".to_owned(),
            schema_version: 1,
            revision: "rev-delete".to_owned(),
            store_relative_path: "artifacts/skills/delete/sha256-ddd".to_owned(),
            artifact: json!({"name": "delete"}),
            metadata: json!({}),
        })
        .await
        .expect("artifact should create");
    let managed = store
        .create_skill_governance_materialization(NewSkillGovernanceMaterialization {
            artifact_id: artifact.id,
            scope: SkillGovernanceScope::Workspace,
            scope_id: "workspace-delete".to_owned(),
            target_path: ".codex/skills/delete".to_owned(),
            target_runtime: "codex".to_owned(),
            root_kind: SkillGovernanceMaterializationRootKind::Workspace,
            installation_mode: SkillGovernanceInstallationMode::Copy,
            ownership: SkillGovernanceMaterializationOwnership::Managed,
            content_digest: "sha256:ddd".to_owned(),
            expected_destination: ".codex/skills/delete/SKILL.md".to_owned(),
            expected_fingerprint: "fingerprint:delete".to_owned(),
            verify_status: SkillGovernanceVerifyStatus::Verified,
            receipt: json!({"installed": true}),
        })
        .await
        .expect("managed materialization should create");
    assert!(matches!(
        store
            .delete_skill_governance_managed_artifact(artifact.id, artifact.version)
            .await,
        Err(StoreError::SkillGovernanceDeleteConflict { .. })
    ));
    assert!(matches!(
        store
            .delete_skill_governance_materialization_if_safe(
                managed.id,
                managed.version,
                Some("different-fingerprint"),
            )
            .await,
        Err(StoreError::SkillGovernanceDeleteConflict { .. })
    ));
    store
        .create_skill_governance_gc_reference(NewSkillGovernanceGcReference {
            source_type: "workspace_lockfile".to_owned(),
            source_id: Uuid::new_v4(),
            target_type: "materialization".to_owned(),
            target_id: managed.id,
            reference_kind: "pins".to_owned(),
            metadata: json!({}),
        })
        .await
        .expect("reference should create");
    assert!(matches!(
        store
            .delete_skill_governance_materialization_if_safe(
                managed.id,
                managed.version,
                Some("fingerprint:delete"),
            )
            .await,
        Err(StoreError::SkillGovernanceDeleteConflict { .. })
    ));

    let absent_ok = store
        .create_skill_governance_materialization(NewSkillGovernanceMaterialization {
            artifact_id: artifact.id,
            scope: SkillGovernanceScope::Workspace,
            scope_id: "workspace-delete".to_owned(),
            target_path: ".codex/skills/delete-absent".to_owned(),
            target_runtime: "codex".to_owned(),
            root_kind: SkillGovernanceMaterializationRootKind::Workspace,
            installation_mode: SkillGovernanceInstallationMode::Copy,
            ownership: SkillGovernanceMaterializationOwnership::Adopted,
            content_digest: "sha256:ddd".to_owned(),
            expected_destination: ".codex/skills/delete-absent/SKILL.md".to_owned(),
            expected_fingerprint: "fingerprint:delete-absent".to_owned(),
            verify_status: SkillGovernanceVerifyStatus::Missing,
            receipt: json!({"installed": true}),
        })
        .await
        .expect("adopted materialization should create");
    assert!(store
        .delete_skill_governance_materialization_if_safe(absent_ok.id, absent_ok.version, None,)
        .await
        .expect("absent adopted materialization should delete"));

    let unmanaged = store
        .create_skill_governance_materialization(NewSkillGovernanceMaterialization {
            artifact_id: artifact.id,
            scope: SkillGovernanceScope::Workspace,
            scope_id: "workspace-delete".to_owned(),
            target_path: ".codex/skills/delete-unmanaged".to_owned(),
            target_runtime: "codex".to_owned(),
            root_kind: SkillGovernanceMaterializationRootKind::Workspace,
            installation_mode: SkillGovernanceInstallationMode::InPlace,
            ownership: SkillGovernanceMaterializationOwnership::Unmanaged,
            content_digest: "sha256:ddd".to_owned(),
            expected_destination: ".codex/skills/delete-unmanaged/SKILL.md".to_owned(),
            expected_fingerprint: "fingerprint:delete-unmanaged".to_owned(),
            verify_status: SkillGovernanceVerifyStatus::Verified,
            receipt: json!({"detected": true}),
        })
        .await
        .expect("unmanaged materialization should create");
    assert!(matches!(
        store
            .delete_skill_governance_materialization_if_safe(
                unmanaged.id,
                unmanaged.version,
                Some("fingerprint:delete-unmanaged"),
            )
            .await,
        Err(StoreError::SkillGovernanceDeleteConflict { .. })
    ));
}
