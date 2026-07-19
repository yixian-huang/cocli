use std::path::{Path, PathBuf};

use chrono::{Duration, Utc};
use cocli_driver_core::{
    McpApplyActionResult, McpApplyActionStatus, McpApplyExecutionResult, McpApplyJournalEntry,
    McpApplyJournalPhase, McpBackupDescriptor, McpDesiredTarget, McpEffectiveDesiredState, McpPlan,
    McpReloadResult, McpReloadStatus, McpRollbackExecutionResult, McpVerificationResult,
    McpVerificationStatus,
};
use cocli_store::{
    McpApplyRunStatus, McpPlanDecisionStatus, NewMcpApplyRun, NewMcpPlanDecision, Store, StoreError,
};
use uuid::Uuid;

fn temporary_database_path() -> PathBuf {
    std::env::temp_dir().join(format!("cocli-mcp-apply-{}.sqlite3", Uuid::new_v4()))
}

async fn remove_database(path: &Path) {
    let _ = tokio::fs::remove_file(path).await;
}

fn plan(id: &str) -> McpPlan {
    let target = McpDesiredTarget {
        machine_id: "machine-test".to_owned(),
        workspace_id: None,
        agent_id: None,
    };
    McpPlan {
        id: id.to_owned(),
        target: target.clone(),
        effective_desired_state: McpEffectiveDesiredState {
            target,
            servers: Vec::new(),
            conflicts: Vec::new(),
            resolution: Vec::new(),
        },
        actions: Vec::new(),
        observation_hash: "observation-hash".to_owned(),
        config_hash: "config-hash".to_owned(),
        capability_hash: "capability-hash".to_owned(),
        plan_hash: "plan-hash".to_owned(),
        generated_at: Utc::now().to_rfc3339(),
        dry_run: true,
        applied: false,
    }
}

async fn approved_plan(store: &Store) -> (McpPlan, uuid::Uuid) {
    let plan = plan("plan-apply");
    store.save_mcp_plan(&plan).await.expect("save plan");
    let decision = store
        .record_mcp_plan_decision(NewMcpPlanDecision {
            plan_id: plan.id.clone(),
            decision: McpPlanDecisionStatus::Approved,
            plan_hash: plan.plan_hash.clone(),
            observation_hash: plan.observation_hash.clone(),
            config_hash: plan.config_hash.clone(),
            actor: "test-actor".to_owned(),
            reason: None,
            expires_at: Some(Utc::now() + Duration::minutes(5)),
        })
        .await
        .expect("approve plan");
    (plan, decision.id)
}

fn execution(reason: &str) -> McpApplyExecutionResult {
    McpApplyExecutionResult {
        actions: vec![McpApplyActionResult {
            action_index: 0,
            runtime: "cursor".to_owned(),
            server_id: "docs".to_owned(),
            status: McpApplyActionStatus::Verified,
            reason: reason.to_owned(),
            backup: Some(McpBackupDescriptor {
                id: "backup-1".to_owned(),
                runtime: "cursor".to_owned(),
                source_path: "/tmp/config.json".to_owned(),
                backup_path: "/tmp/backup.json".to_owned(),
                source_hash: "before".to_owned(),
                backup_hash: "before".to_owned(),
                applied_hash: "after".to_owned(),
                source_existed: true,
            }),
            before_source_hash: Some("before".to_owned()),
            after_source_hash: Some("after".to_owned()),
        }],
        reloads: vec![McpReloadResult {
            runtime: "cursor".to_owned(),
            status: McpReloadStatus::Deferred,
            reason: "active sessions were not restarted".to_owned(),
        }],
        verification: McpVerificationResult {
            status: McpVerificationStatus::Matched,
            observation_hash: "after-observation".to_owned(),
            mismatches: Vec::new(),
            written_config_hashes: Default::default(),
            session_effective: Default::default(),
        },
        journal: vec![journal(1, McpApplyJournalPhase::Written)],
    }
}

fn journal(sequence: u64, phase: McpApplyJournalPhase) -> McpApplyJournalEntry {
    McpApplyJournalEntry {
        sequence,
        action_index: 0,
        runtime: "cursor".to_owned(),
        server_id: "docs".to_owned(),
        idempotency_key: "idem-cursor-docs".to_owned(),
        phase,
        attempt: 1,
        expected_source_hash: Some("before".to_owned()),
        expected_schema_hash: Some("schema".to_owned()),
        backup: None,
        reason: "redacted journal checkpoint".to_owned(),
        evidence: Vec::new(),
    }
}

#[tokio::test]
async fn apply_run_is_idempotent_redacted_and_persists_across_reopen() {
    let database_path = temporary_database_path();
    let store = Store::open(&database_path).await.expect("open store");
    let (plan, approval_id) = approved_plan(&store).await;
    let input = NewMcpApplyRun {
        plan_id: plan.id.clone(),
        approval_id,
        plan_hash: plan.plan_hash.clone(),
        observation_hash: plan.observation_hash.clone(),
        config_hash: plan.config_hash.clone(),
        capability_hash: plan.capability_hash.clone(),
        actor: "desktop-user".to_owned(),
        confirm_high_risk: true,
    };
    let created = store
        .create_mcp_apply_run(input.clone())
        .await
        .expect("create run");
    let repeated = store
        .create_mcp_apply_run(input)
        .await
        .expect("idempotent create");
    assert_eq!(created.id, repeated.id);
    store.close().await;

    let store = Store::open(&database_path)
        .await
        .expect("reopen interrupted run");
    let interrupted = store
        .get_mcp_apply_run(created.id)
        .await
        .expect("read interrupted run")
        .expect("interrupted run exists");
    assert_eq!(interrupted.status, McpApplyRunStatus::Running);
    assert_eq!(interrupted.capability_hash, "capability-hash");

    let secret = store
        .complete_mcp_apply_run(created.id, &execution("token=must-not-persist"))
        .await
        .expect_err("secret-like result must be rejected");
    assert!(matches!(secret, StoreError::InvalidMcpApplyRun(_)));
    let completed = store
        .complete_mcp_apply_run(created.id, &execution("verified safely"))
        .await
        .expect("complete run");
    assert_eq!(completed.status, McpApplyRunStatus::Verified);
    assert_eq!(completed.journal.len(), 1);
    assert!(completed.can_rollback);
    let rollback = McpRollbackExecutionResult {
        actions: vec![
            McpApplyActionResult {
                action_index: 0,
                runtime: "cursor".to_owned(),
                server_id: "docs".to_owned(),
                status: McpApplyActionStatus::RolledBack,
                reason: "backup restored atomically".to_owned(),
                backup: completed.actions[0].backup.clone(),
                before_source_hash: None,
                after_source_hash: None,
            },
            McpApplyActionResult {
                action_index: 1,
                runtime: "claude".to_owned(),
                server_id: "ops".to_owned(),
                status: McpApplyActionStatus::Blocked,
                reason: "backup checksum mismatch".to_owned(),
                backup: None,
                before_source_hash: None,
                after_source_hash: None,
            },
        ],
        verification: McpVerificationResult {
            status: McpVerificationStatus::Matched,
            observation_hash: "rollback-observation".to_owned(),
            mismatches: Vec::new(),
            written_config_hashes: Default::default(),
            session_effective: Default::default(),
        },
    };
    let rolled_back = store
        .complete_mcp_rollback(completed.id, "rollback-actor", &rollback)
        .await
        .expect("complete rollback");
    assert_eq!(rolled_back.rollback_status, Some(McpApplyRunStatus::Failed));
    assert!(rolled_back.can_rollback);
    assert_eq!(
        rolled_back.rollback_actions[0].reason,
        "backup restored atomically"
    );
    store.close().await;

    let reopened = Store::open(&database_path).await.expect("reopen store");
    let persisted = reopened
        .get_mcp_apply_run(created.id)
        .await
        .expect("read persisted run")
        .expect("run exists");
    assert_eq!(persisted.status, McpApplyRunStatus::Verified);
    assert_eq!(persisted.actions[0].reason, "verified safely");
    assert_eq!(persisted.journal[0].phase, McpApplyJournalPhase::Written);
    assert_eq!(persisted.rollback_actor.as_deref(), Some("rollback-actor"));
    assert_eq!(persisted.rollback_actions.len(), 2);
    assert_eq!(
        persisted.rollback_actions[0].status,
        McpApplyActionStatus::RolledBack
    );
    assert_eq!(
        persisted.rollback_actions[1].reason,
        "backup checksum mismatch"
    );
    reopened.close().await;
    remove_database(&database_path).await;
}

#[tokio::test]
async fn apply_run_preserves_partial_runtime_failure_details() {
    let database_path = temporary_database_path();
    let store = Store::open(&database_path).await.expect("open store");
    let (plan, approval_id) = approved_plan(&store).await;
    let run = store
        .create_mcp_apply_run(NewMcpApplyRun {
            plan_id: plan.id,
            approval_id,
            plan_hash: plan.plan_hash,
            observation_hash: plan.observation_hash,
            config_hash: plan.config_hash,
            capability_hash: plan.capability_hash,
            actor: "api-test".to_owned(),
            confirm_high_risk: true,
        })
        .await
        .expect("create run");
    let mut result = execution("cursor verified");
    result.actions.push(McpApplyActionResult {
        action_index: 1,
        runtime: "claude".to_owned(),
        server_id: "ops".to_owned(),
        status: McpApplyActionStatus::Failed,
        reason: "atomic replace failed".to_owned(),
        backup: None,
        before_source_hash: None,
        after_source_hash: None,
    });
    result.verification.status = McpVerificationStatus::Mismatched;
    result
        .verification
        .mismatches
        .push("claude/ops was not applied".to_owned());

    let completed = store
        .complete_mcp_apply_run(run.id, &result)
        .await
        .expect("complete partial run");
    assert_eq!(completed.status, McpApplyRunStatus::Partial);
    assert_eq!(completed.actions[0].status, McpApplyActionStatus::Verified);
    assert_eq!(completed.actions[1].status, McpApplyActionStatus::Failed);
    assert_eq!(completed.actions[1].reason, "atomic replace failed");
    store.close().await;
    remove_database(&database_path).await;
}

#[tokio::test]
async fn apply_run_journal_checkpoints_are_idempotent_and_recoverable() {
    let database_path = temporary_database_path();
    let store = Store::open(&database_path).await.expect("open store");
    let (plan, approval_id) = approved_plan(&store).await;
    let run = store
        .create_mcp_apply_run(NewMcpApplyRun {
            plan_id: plan.id,
            approval_id,
            plan_hash: plan.plan_hash,
            observation_hash: plan.observation_hash,
            config_hash: plan.config_hash,
            capability_hash: plan.capability_hash,
            actor: "journal-test".to_owned(),
            confirm_high_risk: true,
        })
        .await
        .expect("create run");

    let boundaries = [
        (
            McpApplyJournalPhase::Preflight,
            McpApplyRunStatus::Preflight,
        ),
        (McpApplyJournalPhase::Locked, McpApplyRunStatus::Locked),
        (McpApplyJournalPhase::BackedUp, McpApplyRunStatus::BackedUp),
        (McpApplyJournalPhase::Written, McpApplyRunStatus::Written),
        (
            McpApplyJournalPhase::ReloadPending,
            McpApplyRunStatus::ReloadPending,
        ),
        (McpApplyJournalPhase::Reloaded, McpApplyRunStatus::Reloaded),
        (
            McpApplyJournalPhase::RecoveryRequired,
            McpApplyRunStatus::RecoveryRequired,
        ),
    ];
    let mut latest = run;
    for (index, (phase, status)) in boundaries.into_iter().enumerate() {
        let entry = journal(index as u64 + 1, phase);
        latest = store
            .checkpoint_mcp_apply_run(
                latest.id,
                phase,
                &entry,
                None,
                (phase == McpApplyJournalPhase::RecoveryRequired)
                    .then_some("restart found reload pending"),
            )
            .await
            .expect("checkpoint");
        assert_eq!(latest.status, status);
        let repeated = store
            .checkpoint_mcp_apply_run(latest.id, phase, &entry, None, None)
            .await
            .expect("repeat checkpoint");
        assert_eq!(repeated.journal.len(), latest.journal.len());
        latest = repeated;
    }
    assert_eq!(
        latest.recovery_reason.as_deref(),
        Some("restart found reload pending")
    );
    store.close().await;

    let reopened = Store::open(&database_path).await.expect("reopen store");
    let recovered = reopened
        .get_mcp_apply_run(latest.id)
        .await
        .expect("read run")
        .expect("run exists");
    assert_eq!(recovered.status, McpApplyRunStatus::RecoveryRequired);
    assert_eq!(recovered.journal.len(), 7);
    assert!(recovered.attempt >= 8);

    let secret = reopened
        .checkpoint_mcp_apply_run(
            recovered.id,
            McpApplyJournalPhase::Failed,
            &McpApplyJournalEntry {
                reason: "api_key=must-not-persist".to_owned(),
                ..journal(99, McpApplyJournalPhase::Failed)
            },
            None,
            None,
        )
        .await
        .expect_err("secret-like journal must be rejected");
    assert!(matches!(secret, StoreError::InvalidMcpApplyRun(_)));

    let manual = reopened
        .record_mcp_manual_recovery(
            recovered.id,
            "ops-user",
            "operator verified rollback outside active session",
        )
        .await
        .expect("record manual recovery");
    assert_eq!(manual.status, McpApplyRunStatus::RecoveryRequired);
    assert_eq!(
        manual.recovery_reason.as_deref(),
        Some("operator verified rollback outside active session")
    );
    assert!(manual
        .stale_reasons
        .iter()
        .any(|reason| reason == "manual_recovery_recorded_by:ops-user"));

    reopened.close().await;
    remove_database(&database_path).await;
}
