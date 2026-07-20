use cocli_driver_core::{
    run_mcp_adapter_conformance, FakeMcpAdapter, McpAdapterActionRequest,
    McpAdapterConformanceScenario, McpAdapterConformanceStatus, McpAdapterPreservationCheck,
    McpAdapterRecoveryDecision, McpAdapterRecoveryExpectation, McpAdapterSdkContext,
    McpApplyActionStatus, McpApplyJournalEntry, McpApplyJournalPhase, McpCapabilitySupport,
    McpPlanAction, McpPlanActionKind, McpRiskLevel, McpRuntimeAdapter, McpStateSummary,
};
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_CONTEXT: AtomicUsize = AtomicUsize::new(1);

fn context() -> McpAdapterSdkContext {
    let root = std::env::temp_dir().join(format!(
        "cocli-mcp-adapter-sdk-test-{}-{}",
        std::process::id(),
        NEXT_CONTEXT.fetch_add(1, Ordering::Relaxed)
    ));
    McpAdapterSdkContext {
        home_dir: root.join("home"),
        workspace_dir: root.join("workspace"),
        config_root: root.join("home/.config/runtime"),
        allowed_write_roots: vec![root.join("home/.config/runtime")],
        now: "2026-07-19T00:00:00Z".to_owned(),
    }
}

fn scenario(ctx: &McpAdapterSdkContext, canary: &str) -> McpAdapterConformanceScenario {
    McpAdapterConformanceScenario {
        action_request: McpAdapterActionRequest {
            run_id: "run-sdk".to_owned(),
            action_index: 0,
            action: McpPlanAction {
                kind: McpPlanActionKind::AddConfigure,
                runtime: "fake".to_owned(),
                scope: "workspace".to_owned(),
                target: "workspace:example".to_owned(),
                server_id: "docs".to_owned(),
                server_fingerprint: "fingerprint".to_owned(),
                before: empty_state(),
                after: McpStateSummary {
                    configured: Some(true),
                    enabled: Some(true),
                    endpoint_fingerprint: Some("fingerprint".to_owned()),
                    allow_tools: Vec::new(),
                    deny_tools: Vec::new(),
                    approval_mode: None,
                    secret_ref_count: 0,
                },
                risk: McpRiskLevel::Medium,
                reason: "configure fake server".to_owned(),
                evidence: Vec::new(),
                expected_source_hash: Some("before".to_owned()),
                expected_schema_hash: Some("schema".to_owned()),
                blocked: false,
            },
            idempotency_key: "run-sdk:0".to_owned(),
            desired: None,
            expected_source_hash: Some("before".to_owned()),
            expected_schema_hash: Some("schema".to_owned()),
        },
        canary_secret: canary.to_owned(),
        allow_side_effects: true,
        require_write_evidence_for_supported_writes: true,
        recovery_journal: Vec::new(),
        rollback_backup: Some(cocli_driver_core::McpBackupDescriptor {
            id: "fixture-backup".to_owned(),
            runtime: "fake".to_owned(),
            source_path: ctx.config_root.join("mcp.json").display().to_string(),
            backup_path: ctx.config_root.join("mcp.backup").display().to_string(),
            source_hash: "before".to_owned(),
            backup_hash: "before".to_owned(),
            applied_hash: "after".to_owned(),
            source_existed: true,
        }),
        require_idempotent_apply: false,
        require_cas_rejection: false,
        lock_path: None,
        preservation_checks: Vec::new(),
        expected_reload_strategy: Some(cocli_driver_core::McpReloadStrategy::NewSessionOnly),
        expected_verification_status: Some(cocli_driver_core::McpVerificationStatus::Matched),
        expected_session_effective: Some(
            cocli_driver_core::McpSessionEffectiveStatus::NewSessionRequired,
        ),
        recovery_expectations: Vec::new(),
    }
}

fn empty_state() -> McpStateSummary {
    McpStateSummary {
        configured: None,
        enabled: None,
        endpoint_fingerprint: None,
        allow_tools: Vec::new(),
        deny_tools: Vec::new(),
        approval_mode: None,
        secret_ref_count: 0,
    }
}

fn case_status(
    report: &cocli_driver_core::McpAdapterConformanceReport,
    name: &str,
) -> Option<McpAdapterConformanceStatus> {
    report
        .cases
        .iter()
        .find(|case| case.name == name)
        .map(|case| case.status)
}

#[tokio::test]
async fn fake_supported_adapter_passes_conformance() {
    let adapter = FakeMcpAdapter::new("fake");
    let ctx = context();
    let report =
        run_mcp_adapter_conformance(&adapter, &ctx, &scenario(&ctx, "SECRET_CANARY")).await;

    assert!(report.passed, "{report:#?}");
    assert_eq!(
        case_status(&report, "supported_write_has_evidence"),
        Some(McpAdapterConformanceStatus::Passed)
    );
    assert!(!report.report_hash.is_empty());
}

#[tokio::test]
async fn unsupported_adapter_degrades_without_writes() {
    let mut adapter = FakeMcpAdapter::new("fake");
    adapter.add_configure_support = McpCapabilitySupport::Unsupported;
    let ctx = context();
    let report =
        run_mcp_adapter_conformance(&adapter, &ctx, &scenario(&ctx, "SECRET_CANARY")).await;

    assert!(report.passed, "{report:#?}");
    assert_eq!(
        case_status(&report, "unsupported_safe_degrade"),
        Some(McpAdapterConformanceStatus::Passed)
    );
}

#[tokio::test]
async fn false_supported_write_claim_fails_conformance() {
    let mut adapter = FakeMcpAdapter::new("fake");
    adapter.false_supported_write = true;
    let ctx = context();
    let report =
        run_mcp_adapter_conformance(&adapter, &ctx, &scenario(&ctx, "SECRET_CANARY")).await;

    assert!(!report.passed);
    assert_eq!(
        case_status(&report, "supported_write_has_evidence"),
        Some(McpAdapterConformanceStatus::Failed)
    );
}

#[tokio::test]
async fn secret_canary_is_redacted_from_report_even_when_adapter_leaks() {
    let mut adapter = FakeMcpAdapter::new("fake");
    adapter.leak_secret = Some("SECRET_CANARY".to_owned());
    let ctx = context();
    let report =
        run_mcp_adapter_conformance(&adapter, &ctx, &scenario(&ctx, "SECRET_CANARY")).await;
    let serialized = serde_json::to_string(&report).expect("serialize report");

    assert!(!report.passed);
    assert!(!serialized.contains("SECRET_CANARY"));
}

#[tokio::test]
async fn writes_outside_host_roots_fail_conformance() {
    let mut adapter = FakeMcpAdapter::new("fake");
    adapter.write_outside_root = true;
    adapter.perform_filesystem_write = true;
    let ctx = context();
    let report =
        run_mcp_adapter_conformance(&adapter, &ctx, &scenario(&ctx, "SECRET_CANARY")).await;

    assert!(!report.passed);
    assert_eq!(
        case_status(&report, "write_root_confinement"),
        Some(McpAdapterConformanceStatus::Failed)
    );
    assert_eq!(
        case_status(&report, "filesystem_escape_detection"),
        Some(McpAdapterConformanceStatus::Failed)
    );
    let _ = std::fs::remove_dir_all(ctx.home_dir);
}

#[tokio::test]
async fn temp_root_contract_covers_preservation_cas_lock_idempotency_recovery_and_rollback() {
    let ctx = context();
    let _ = std::fs::remove_dir_all(&ctx.home_dir);
    std::fs::create_dir_all(&ctx.config_root).expect("create isolated config root");
    let config_path = ctx.config_root.join("mcp.json");
    let backup_path = ctx.config_root.join("mcp.backup.json");
    let original = br#"{"theme":"dark","mcpServers":{"existing":{"command":"keep"}}}"#;
    std::fs::write(&config_path, original).expect("seed config");
    std::fs::write(&backup_path, original).expect("seed backup");
    let source_hash = format!("{:x}", Sha256::digest(original));

    let mut scenario = scenario(&ctx, "SECRET_CANARY");
    scenario.action_request.expected_source_hash = Some(source_hash.clone());
    scenario.require_idempotent_apply = true;
    scenario.require_cas_rejection = true;
    scenario.lock_path = Some(ctx.config_root.join(".mcp-conformance.lock"));
    scenario.preservation_checks = vec![McpAdapterPreservationCheck {
        path: config_path.clone(),
        json_pointer: "/theme".to_owned(),
        expected: serde_json::json!("dark"),
    }];
    scenario.rollback_backup = Some(cocli_driver_core::McpBackupDescriptor {
        id: "isolated-backup".to_owned(),
        runtime: "fake".to_owned(),
        source_path: config_path.display().to_string(),
        backup_path: backup_path.display().to_string(),
        source_hash: source_hash.clone(),
        backup_hash: source_hash,
        applied_hash: "applied".to_owned(),
        source_existed: true,
    });
    scenario.recovery_expectations = vec![
        recovery_expectation(
            McpApplyJournalPhase::BackedUp,
            McpAdapterRecoveryDecision::Resume,
        ),
        recovery_expectation(
            McpApplyJournalPhase::Written,
            McpAdapterRecoveryDecision::Rollback,
        ),
        recovery_expectation(
            McpApplyJournalPhase::ReloadPending,
            McpAdapterRecoveryDecision::Rollback,
        ),
        recovery_expectation(
            McpApplyJournalPhase::Verified,
            McpAdapterRecoveryDecision::AlreadyCompleted,
        ),
    ];

    let mut adapter = FakeMcpAdapter::new("fake");
    adapter.perform_filesystem_write = true;
    adapter.enforce_source_cas = true;
    let report = run_mcp_adapter_conformance(&adapter, &ctx, &scenario).await;
    assert!(report.passed, "{report:#?}");
    for name in [
        "unknown_field_preservation",
        "source_hash_cas",
        "lock_contention",
        "idempotent_apply",
        "crash_recovery_decision",
        "filesystem_escape_detection",
    ] {
        assert_eq!(
            case_status(&report, name),
            Some(McpAdapterConformanceStatus::Passed),
            "missing passing case {name}: {report:#?}"
        );
    }
    assert_eq!(
        std::fs::read(&config_path).expect("read restored config"),
        original
    );
    assert!(!ctx.config_root.join("mcp.cocli.tmp").exists());
    assert!(!ctx.config_root.join("mcp.rollback.tmp").exists());
    let _ = std::fs::remove_dir_all(&ctx.home_dir);
}

fn recovery_expectation(
    phase: McpApplyJournalPhase,
    expected: McpAdapterRecoveryDecision,
) -> McpAdapterRecoveryExpectation {
    McpAdapterRecoveryExpectation {
        journal: vec![McpApplyJournalEntry {
            sequence: 1,
            action_index: 0,
            runtime: "fake".to_owned(),
            server_id: "docs".to_owned(),
            idempotency_key: "run-sdk:0".to_owned(),
            phase,
            attempt: 1,
            expected_source_hash: None,
            expected_schema_hash: None,
            backup: None,
            reason: "crash boundary".to_owned(),
            evidence: Vec::new(),
        }],
        expected,
    }
}

#[tokio::test]
async fn verify_mismatch_is_reported_without_claiming_match() {
    let mut adapter = FakeMcpAdapter::new("fake");
    adapter.verify_mismatch = true;
    let ctx = context();
    let mut scenario = scenario(&ctx, "SECRET_CANARY");
    scenario.expected_verification_status =
        Some(cocli_driver_core::McpVerificationStatus::Mismatched);
    let report = run_mcp_adapter_conformance(&adapter, &ctx, &scenario).await;
    assert!(report.passed, "{report:#?}");
    assert_eq!(
        case_status(&report, "verify_status"),
        Some(McpAdapterConformanceStatus::Passed)
    );
}

#[tokio::test]
async fn fake_apply_result_stays_redacted_and_structured() {
    let adapter = FakeMcpAdapter::new("fake");
    let ctx = context();
    let scenario = scenario(&ctx, "SECRET_CANARY");
    let outcome = adapter
        .apply_action(&ctx, &scenario.action_request)
        .await
        .expect("fake apply");

    let result = outcome.result.expect("action result");
    assert_eq!(result.status, McpApplyActionStatus::Applied);
    assert_eq!(outcome.writes.len(), 1);
    assert!(outcome.writes[0].path.starts_with(ctx.config_root));
}
