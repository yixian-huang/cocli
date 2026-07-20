//! Stable, side-effect-bounded MCP Runtime adapter SDK contract.
//!
//! The SDK is intentionally library-only. It defines host-injected ports and a
//! reusable conformance harness, but it does not load third-party code, execute
//! scripts, or grant adapters direct access to Store/UI internals.

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{
    McpApplyActionResult, McpApplyActionStatus, McpApplyJournalEntry, McpBackupDescriptor,
    McpCapabilityDetail, McpCapabilityOperation, McpCapabilitySnapshot, McpCapabilitySupport,
    McpDesiredServer, McpEvidence, McpInventory, McpPlanAction, McpPreflightAction,
    McpReloadResult, McpReloadStrategy, McpRuntimeCapability, McpSecretRef,
    McpSessionEffectiveStatus, McpVerificationResult, McpVerificationStatus,
};

pub const MCP_ADAPTER_SDK_CONTRACT_VERSION: &str = "mcp-adapter-sdk.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpAdapterIdentity {
    pub runtime: String,
    pub adapter: String,
    pub adapter_version: String,
    pub contract_version: String,
    #[serde(default)]
    pub evidence: Vec<McpEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpAdapterSdkContext {
    pub home_dir: PathBuf,
    pub workspace_dir: PathBuf,
    pub config_root: PathBuf,
    pub allowed_write_roots: Vec<PathBuf>,
    pub now: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpResolvedSecret {
    pub reference: McpSecretRef,
    pub value_sha256: String,
}

#[async_trait]
pub trait McpSecretResolver: Send + Sync {
    async fn resolve(&self, reference: &McpSecretRef)
        -> Result<McpResolvedSecret, McpAdapterError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpAdapterActionRequest {
    pub run_id: String,
    pub action_index: usize,
    pub action: McpPlanAction,
    pub idempotency_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desired: Option<McpDesiredServer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_source_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_schema_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpAdapterPreflightDecision {
    pub action: McpPreflightAction,
    pub reason: String,
    #[serde(default)]
    pub evidence: Vec<McpEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpAdapterWriteOperation {
    Create,
    Update,
    Remove,
    Restore,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpAdapterWriteEffect {
    pub path: PathBuf,
    pub operation: McpAdapterWriteOperation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_hash: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpAdapterApplyOutcome {
    pub result: Option<McpApplyActionResult>,
    #[serde(default)]
    pub writes: Vec<McpAdapterWriteEffect>,
    #[serde(default)]
    pub journal: Vec<McpApplyJournalEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpAdapterRecoveryDecision {
    Resume,
    Rollback,
    ManualRecoveryRequired,
    AlreadyCompleted,
}

#[derive(Debug, Error)]
pub enum McpAdapterError {
    #[error("adapter operation is unsupported")]
    Unsupported,
    #[error("adapter operation is blocked: {0}")]
    Blocked(String),
    #[error("adapter precondition failed: {0}")]
    PreconditionFailed(String),
    #[error("adapter execution failed: {0}")]
    ExecutionFailed(String),
}

#[async_trait]
pub trait McpRuntimeAdapter: Send + Sync {
    fn identity(&self) -> McpAdapterIdentity;

    async fn probe_capabilities(
        &self,
        context: &McpAdapterSdkContext,
    ) -> Result<McpCapabilitySnapshot, McpAdapterError>;

    async fn readback(
        &self,
        context: &McpAdapterSdkContext,
    ) -> Result<McpInventory, McpAdapterError>;

    async fn preflight_action(
        &self,
        context: &McpAdapterSdkContext,
        request: &McpAdapterActionRequest,
    ) -> Result<McpAdapterPreflightDecision, McpAdapterError>;

    async fn apply_action(
        &self,
        context: &McpAdapterSdkContext,
        request: &McpAdapterActionRequest,
    ) -> Result<McpAdapterApplyOutcome, McpAdapterError>;

    async fn reload(
        &self,
        context: &McpAdapterSdkContext,
    ) -> Result<McpReloadResult, McpAdapterError>;

    async fn verify(
        &self,
        context: &McpAdapterSdkContext,
    ) -> Result<McpVerificationResult, McpAdapterError>;

    async fn rollback(
        &self,
        context: &McpAdapterSdkContext,
        backup: &McpBackupDescriptor,
    ) -> Result<McpAdapterApplyOutcome, McpAdapterError>;

    async fn recover(
        &self,
        context: &McpAdapterSdkContext,
        journal: &[McpApplyJournalEntry],
    ) -> Result<McpAdapterRecoveryDecision, McpAdapterError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpAdapterConformanceScenario {
    pub action_request: McpAdapterActionRequest,
    pub canary_secret: String,
    #[serde(default)]
    pub allow_side_effects: bool,
    #[serde(default)]
    pub require_write_evidence_for_supported_writes: bool,
    #[serde(default)]
    pub recovery_journal: Vec<McpApplyJournalEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rollback_backup: Option<McpBackupDescriptor>,
    #[serde(default)]
    pub require_idempotent_apply: bool,
    #[serde(default)]
    pub require_cas_rejection: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lock_path: Option<PathBuf>,
    #[serde(default)]
    pub preservation_checks: Vec<McpAdapterPreservationCheck>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_reload_strategy: Option<McpReloadStrategy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_verification_status: Option<McpVerificationStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_session_effective: Option<McpSessionEffectiveStatus>,
    #[serde(default)]
    pub recovery_expectations: Vec<McpAdapterRecoveryExpectation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpAdapterPreservationCheck {
    pub path: PathBuf,
    pub json_pointer: String,
    pub expected: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpAdapterRecoveryExpectation {
    pub journal: Vec<McpApplyJournalEntry>,
    pub expected: McpAdapterRecoveryDecision,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpAdapterConformanceStatus {
    Passed,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpAdapterConformanceCaseResult {
    pub name: String,
    pub status: McpAdapterConformanceStatus,
    pub reason: String,
    #[serde(default)]
    pub evidence: Vec<McpEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpAdapterConformanceReport {
    pub schema_version: String,
    pub adapter: McpAdapterIdentity,
    pub passed: bool,
    pub cases: Vec<McpAdapterConformanceCaseResult>,
    pub report_hash: String,
}

#[must_use]
pub fn hash_mcp_adapter_conformance_reports(reports: &[McpAdapterConformanceReport]) -> String {
    let mut stable = reports.to_vec();
    stable.sort_by(|left, right| {
        (&left.adapter.runtime, &left.adapter.adapter)
            .cmp(&(&right.adapter.runtime, &right.adapter.adapter))
    });
    digest_json(&stable)
}

pub async fn run_mcp_adapter_conformance(
    adapter: &dyn McpRuntimeAdapter,
    context: &McpAdapterSdkContext,
    scenario: &McpAdapterConformanceScenario,
) -> McpAdapterConformanceReport {
    let identity = adapter.identity();
    let mut cases = Vec::new();
    let monitored_before = snapshot_monitored_roots(context);

    cases.push(case_bool(
        "identity_contract",
        !identity.runtime.trim().is_empty()
            && !identity.adapter.trim().is_empty()
            && !identity.adapter_version.trim().is_empty()
            && identity.contract_version == MCP_ADAPTER_SDK_CONTRACT_VERSION,
        "adapter identity must include runtime, adapter, version, and current SDK contract",
        &scenario.canary_secret,
    ));

    let first_capabilities = adapter.probe_capabilities(context).await;
    let second_capabilities = adapter.probe_capabilities(context).await;
    match (&first_capabilities, &second_capabilities) {
        (Ok(first), Ok(second)) => {
            cases.push(case_bool(
                "capability_probe_deterministic",
                stable_capability_digest(first) == stable_capability_digest(second),
                "capability probe must be deterministic for the same adapter context",
                &scenario.canary_secret,
            ));
            cases.push(validate_capability_evidence(
                first,
                &identity.runtime,
                &scenario.canary_secret,
            ));
        }
        (Err(error), _) | (_, Err(error)) => cases.push(case_result(
            "capability_probe_deterministic",
            McpAdapterConformanceStatus::Failed,
            format!("capability probe failed: {error}"),
            Vec::new(),
            &scenario.canary_secret,
        )),
    }

    match adapter.readback(context).await {
        Ok(inventory) => cases.push(case_bool(
            "readback_redacted",
            !serialized_contains(&inventory, &scenario.canary_secret),
            "readback output must not contain secret canaries",
            &scenario.canary_secret,
        )),
        Err(McpAdapterError::Unsupported) => cases.push(case_result(
            "readback_redacted",
            McpAdapterConformanceStatus::Skipped,
            "readback is unsupported by this adapter".to_owned(),
            Vec::new(),
            &scenario.canary_secret,
        )),
        Err(error) => cases.push(case_result(
            "readback_redacted",
            McpAdapterConformanceStatus::Failed,
            format!("readback failed: {error}"),
            Vec::new(),
            &scenario.canary_secret,
        )),
    }

    let preflight = adapter
        .preflight_action(context, &scenario.action_request)
        .await;
    match &preflight {
        Ok(decision) => cases.push(case_bool(
            "preflight_redacted",
            !serialized_contains(decision, &scenario.canary_secret),
            "preflight output must not contain secret canaries",
            &scenario.canary_secret,
        )),
        Err(McpAdapterError::Unsupported) => cases.push(case_result(
            "preflight_redacted",
            McpAdapterConformanceStatus::Skipped,
            "preflight is unsupported by this adapter".to_owned(),
            Vec::new(),
            &scenario.canary_secret,
        )),
        Err(error) => cases.push(case_result(
            "preflight_redacted",
            McpAdapterConformanceStatus::Failed,
            format!("preflight failed: {error}"),
            Vec::new(),
            &scenario.canary_secret,
        )),
    }

    let write_capability = first_capabilities
        .as_ref()
        .ok()
        .and_then(|snapshot| find_operation(snapshot, McpCapabilityOperation::AddConfigure));
    if write_capability.is_some_and(|detail| detail.support == McpCapabilitySupport::Supported)
        && !scenario.allow_side_effects
    {
        cases.push(case_result(
            "apply_side_effect_gate",
            McpAdapterConformanceStatus::Skipped,
            "write-capable adapter was not executed because the scenario disallows side effects"
                .to_owned(),
            Vec::new(),
            &scenario.canary_secret,
        ));
    } else {
        match adapter
            .apply_action(context, &scenario.action_request)
            .await
        {
            Ok(outcome) => {
                cases.push(case_bool(
                    "apply_redacted",
                    !serialized_contains(&outcome, &scenario.canary_secret),
                    "apply output must not contain secret canaries",
                    &scenario.canary_secret,
                ));
                cases.push(validate_write_confinement(
                    &outcome,
                    &context.allowed_write_roots,
                    &scenario.canary_secret,
                ));
                if scenario.require_write_evidence_for_supported_writes
                    && write_capability
                        .is_some_and(|detail| detail.support == McpCapabilitySupport::Supported)
                {
                    cases.push(case_bool(
                        "supported_write_has_evidence",
                        outcome.result.as_ref().is_some_and(|result| {
                            result.status != McpApplyActionStatus::Applied
                                || !outcome.writes.is_empty()
                                || result.backup.is_some()
                        }),
                        "supported write actions must report write or backup evidence",
                        &scenario.canary_secret,
                    ));
                }
                if scenario.require_idempotent_apply {
                    let repeated = adapter
                        .apply_action(context, &scenario.action_request)
                        .await;
                    cases.push(case_bool(
                        "idempotent_apply",
                        repeated
                            .as_ref()
                            .is_ok_and(|second| digest_json(second) == digest_json(&outcome)),
                        "repeating an idempotency key must return the same structured outcome",
                        &scenario.canary_secret,
                    ));
                }
                for check in &scenario.preservation_checks {
                    cases.push(validate_preservation(check, &scenario.canary_secret));
                }
                if scenario.require_cas_rejection {
                    let mut drifted = scenario.action_request.clone();
                    drifted.idempotency_key.push_str(":cas-drift");
                    drifted.expected_source_hash = Some("0".repeat(64));
                    let rejected = matches!(
                        adapter.apply_action(context, &drifted).await,
                        Err(McpAdapterError::PreconditionFailed(_))
                            | Err(McpAdapterError::Blocked(_))
                    );
                    cases.push(case_bool(
                        "source_hash_cas",
                        rejected,
                        "a supported writer must reject source-hash drift before mutation",
                        &scenario.canary_secret,
                    ));
                }
                if let Some(lock_path) = &scenario.lock_path {
                    let lock_created = create_conformance_lock(lock_path);
                    let mut locked = scenario.action_request.clone();
                    locked.idempotency_key.push_str(":lock-contention");
                    locked.expected_source_hash = outcome
                        .result
                        .as_ref()
                        .and_then(|result| result.after_source_hash.clone());
                    let rejected = lock_created
                        && matches!(
                            adapter.apply_action(context, &locked).await,
                            Err(McpAdapterError::Blocked(_))
                                | Err(McpAdapterError::PreconditionFailed(_))
                        );
                    let _ = fs::remove_file(lock_path);
                    cases.push(case_bool(
                        "lock_contention",
                        rejected,
                        "a held adapter lock must prevent a concurrent mutation",
                        &scenario.canary_secret,
                    ));
                }
            }
            Err(McpAdapterError::Unsupported) => cases.push(case_result(
                "unsupported_safe_degrade",
                McpAdapterConformanceStatus::Passed,
                "unsupported apply degraded without side effects".to_owned(),
                Vec::new(),
                &scenario.canary_secret,
            )),
            Err(error) => cases.push(case_result(
                "apply_contract",
                McpAdapterConformanceStatus::Failed,
                format!("apply failed: {error}"),
                Vec::new(),
                &scenario.canary_secret,
            )),
        }
    }

    match adapter.reload(context).await {
        Ok(result) => {
            cases.push(case_bool(
                "reload_redacted",
                !serialized_contains(&result, &scenario.canary_secret),
                "reload output must not contain secret canaries",
                &scenario.canary_secret,
            ));
            if let Some(expected) = scenario.expected_reload_strategy {
                let actual = reload_strategy_from_result(&result);
                cases.push(case_bool(
                    "reload_strategy_boundary",
                    actual == expected,
                    "reload result must match the declared active-session boundary",
                    &scenario.canary_secret,
                ));
            }
        }
        Err(McpAdapterError::Unsupported) => cases.push(case_result(
            "reload_redacted",
            McpAdapterConformanceStatus::Skipped,
            "reload is unsupported by this adapter".to_owned(),
            Vec::new(),
            &scenario.canary_secret,
        )),
        Err(error) => cases.push(case_result(
            "reload_redacted",
            McpAdapterConformanceStatus::Failed,
            format!("reload failed: {error}"),
            Vec::new(),
            &scenario.canary_secret,
        )),
    }

    match adapter.verify(context).await {
        Ok(result) => {
            cases.push(case_bool(
                "verify_redacted",
                !serialized_contains(&result, &scenario.canary_secret),
                "verify output must not contain secret canaries",
                &scenario.canary_secret,
            ));
            if let Some(expected) = scenario.expected_verification_status {
                cases.push(case_bool(
                    "verify_status",
                    result.status == expected,
                    "verification must surface match or mismatch without claiming false success",
                    &scenario.canary_secret,
                ));
            }
            if let Some(expected) = scenario.expected_session_effective {
                cases.push(case_bool(
                    "session_effective_boundary",
                    result.session_effective == expected,
                    "configuration discovery must not be reported as active-session effectiveness",
                    &scenario.canary_secret,
                ));
            }
        }
        Err(McpAdapterError::Unsupported) => cases.push(case_result(
            "verify_redacted",
            McpAdapterConformanceStatus::Skipped,
            "verify is unsupported by this adapter".to_owned(),
            Vec::new(),
            &scenario.canary_secret,
        )),
        Err(error) => cases.push(case_result(
            "verify_redacted",
            McpAdapterConformanceStatus::Failed,
            format!("verify failed: {error}"),
            Vec::new(),
            &scenario.canary_secret,
        )),
    }

    match adapter.recover(context, &scenario.recovery_journal).await {
        Ok(decision) => cases.push(case_bool(
            "recovery_redacted",
            !serialized_contains(&decision, &scenario.canary_secret),
            "recovery decisions must be structured and redacted",
            &scenario.canary_secret,
        )),
        Err(McpAdapterError::Unsupported) => cases.push(case_result(
            "recovery_redacted",
            McpAdapterConformanceStatus::Skipped,
            "durable recovery is unsupported by this adapter".to_owned(),
            Vec::new(),
            &scenario.canary_secret,
        )),
        Err(error) => cases.push(case_result(
            "recovery_redacted",
            McpAdapterConformanceStatus::Failed,
            format!("recovery failed: {error}"),
            Vec::new(),
            &scenario.canary_secret,
        )),
    }

    for expectation in &scenario.recovery_expectations {
        let decision = adapter.recover(context, &expectation.journal).await;
        cases.push(case_bool(
            "crash_recovery_decision",
            decision.as_ref().is_ok_and(|actual| actual == &expectation.expected),
            "journal boundaries must deterministically choose resume, rollback, completion, or manual recovery",
            &scenario.canary_secret,
        ));
    }

    if let Some(backup) = &scenario.rollback_backup {
        match adapter.rollback(context, backup).await {
            Ok(outcome) => {
                cases.push(case_bool(
                    "rollback_redacted",
                    !serialized_contains(&outcome, &scenario.canary_secret),
                    "rollback output must not contain secret canaries",
                    &scenario.canary_secret,
                ));
                cases.push(validate_write_confinement(
                    &outcome,
                    &context.allowed_write_roots,
                    &scenario.canary_secret,
                ));
            }
            Err(McpAdapterError::Unsupported) => cases.push(case_result(
                "rollback_redacted",
                McpAdapterConformanceStatus::Skipped,
                "rollback is unsupported by this adapter".to_owned(),
                Vec::new(),
                &scenario.canary_secret,
            )),
            Err(error) => cases.push(case_result(
                "rollback_redacted",
                McpAdapterConformanceStatus::Failed,
                format!("rollback failed: {error}"),
                Vec::new(),
                &scenario.canary_secret,
            )),
        }
    }

    let monitored_after = snapshot_monitored_roots(context);
    cases.push(case_bool(
        "filesystem_escape_detection",
        filesystem_changes_are_confined(
            &monitored_before,
            &monitored_after,
            &context.allowed_write_roots,
        ),
        "actual filesystem changes under host-monitored roots must stay inside allowed write roots",
        &scenario.canary_secret,
    ));

    let passed = cases
        .iter()
        .all(|case| case.status != McpAdapterConformanceStatus::Failed);
    let mut report = McpAdapterConformanceReport {
        schema_version: "mcp-adapter-conformance.v1".to_owned(),
        adapter: redact_identity(identity, &scenario.canary_secret),
        passed,
        cases,
        report_hash: String::new(),
    };
    report.report_hash = stable_report_digest(&report);
    report
}

fn validate_capability_evidence(
    snapshot: &McpCapabilitySnapshot,
    expected_runtime: &str,
    canary: &str,
) -> McpAdapterConformanceCaseResult {
    let mut ok = !snapshot.runtimes.is_empty();
    for runtime in &snapshot.runtimes {
        ok &= runtime.runtime == expected_runtime;
        ok &= !serialized_contains(runtime, canary);
        for detail in runtime.operations.values() {
            if detail.support != McpCapabilitySupport::Unknown {
                ok &= !detail.reason.trim().is_empty() && !detail.evidence.is_empty();
            }
        }
    }
    case_bool(
        "capability_evidence_redacted",
        ok,
        "capabilities must match adapter runtime and include redacted evidence for known support states",
        canary,
    )
}

fn validate_write_confinement(
    outcome: &McpAdapterApplyOutcome,
    allowed_roots: &[PathBuf],
    canary: &str,
) -> McpAdapterConformanceCaseResult {
    let confined = outcome.writes.iter().all(|write| {
        allowed_roots
            .iter()
            .any(|root| is_within(root, &write.path))
    });
    case_bool(
        "write_root_confinement",
        confined,
        "adapter write effects must stay within explicit host-provided roots",
        canary,
    )
}

fn validate_preservation(
    check: &McpAdapterPreservationCheck,
    canary: &str,
) -> McpAdapterConformanceCaseResult {
    let preserved = fs::read(&check.path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|value| value.pointer(&check.json_pointer).cloned())
        .is_some_and(|value| value == check.expected);
    case_bool(
        "unknown_field_preservation",
        preserved,
        "structured writers must preserve fields outside their owned MCP subtree",
        canary,
    )
}

fn create_conformance_lock(path: &Path) -> bool {
    if let Some(parent) = path.parent() {
        if fs::create_dir_all(parent).is_err() {
            return false;
        }
    }
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .is_ok()
}

fn reload_strategy_from_result(result: &McpReloadResult) -> McpReloadStrategy {
    match result.status {
        crate::McpReloadStatus::Reloaded => McpReloadStrategy::NativeReload,
        crate::McpReloadStatus::Deferred => McpReloadStrategy::NewSessionOnly,
        crate::McpReloadStatus::NotRequired => McpReloadStrategy::Deferred,
        crate::McpReloadStatus::Blocked | crate::McpReloadStatus::Failed => {
            McpReloadStrategy::Unsupported
        }
    }
}

fn snapshot_monitored_roots(context: &McpAdapterSdkContext) -> BTreeMap<PathBuf, String> {
    let mut snapshot = BTreeMap::new();
    for root in [&context.home_dir, &context.workspace_dir] {
        snapshot_tree(root, &mut snapshot);
    }
    snapshot
}

fn snapshot_tree(path: &Path, snapshot: &mut BTreeMap<PathBuf, String>) {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return;
    };
    if metadata.file_type().is_symlink() {
        let target = fs::read_link(path)
            .map(|target| target.display().to_string())
            .unwrap_or_else(|_| "unreadable".to_owned());
        snapshot.insert(path.to_path_buf(), format!("symlink:{target}"));
        return;
    }
    if metadata.is_file() {
        let digest = fs::read(path)
            .map(|bytes| format!("{:x}", Sha256::digest(bytes)))
            .unwrap_or_else(|_| "unreadable".to_owned());
        snapshot.insert(path.to_path_buf(), digest);
        return;
    }
    if metadata.is_dir() {
        let Ok(entries) = fs::read_dir(path) else {
            return;
        };
        let mut entries = entries
            .flatten()
            .map(|entry| entry.path())
            .collect::<Vec<_>>();
        entries.sort();
        for entry in entries {
            snapshot_tree(&entry, snapshot);
        }
    }
}

fn filesystem_changes_are_confined(
    before: &BTreeMap<PathBuf, String>,
    after: &BTreeMap<PathBuf, String>,
    allowed_roots: &[PathBuf],
) -> bool {
    let mut paths = before.keys().chain(after.keys()).collect::<Vec<_>>();
    paths.sort();
    paths.dedup();
    paths.into_iter().all(|path| {
        before.get(path) == after.get(path)
            || allowed_roots.iter().any(|root| is_within(root, path))
    })
}

fn find_operation(
    snapshot: &McpCapabilitySnapshot,
    operation: McpCapabilityOperation,
) -> Option<&McpCapabilityDetail> {
    snapshot
        .runtimes
        .iter()
        .find_map(|runtime| runtime.operations.get(&operation))
}

fn is_within(root: &Path, path: &Path) -> bool {
    let Some(root) = normalized_path(root) else {
        return false;
    };
    let Some(path) = normalized_path(path) else {
        return false;
    };
    let root_components = root.components().collect::<Vec<_>>();
    let path_components = path.components().collect::<Vec<_>>();
    path_components.starts_with(&root_components)
}

fn normalized_path(path: &Path) -> Option<PathBuf> {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            std::path::Component::RootDir => normalized.push(Path::new("/")),
            std::path::Component::CurDir => {}
            std::path::Component::Normal(value) => normalized.push(value),
            std::path::Component::ParentDir => {
                if !normalized.pop() {
                    return None;
                }
            }
        }
    }
    Some(normalized)
}

fn case_bool(
    name: &str,
    passed: bool,
    reason: &str,
    canary: &str,
) -> McpAdapterConformanceCaseResult {
    case_result(
        name,
        if passed {
            McpAdapterConformanceStatus::Passed
        } else {
            McpAdapterConformanceStatus::Failed
        },
        reason.to_owned(),
        Vec::new(),
        canary,
    )
}

fn case_result(
    name: &str,
    status: McpAdapterConformanceStatus,
    reason: String,
    evidence: Vec<McpEvidence>,
    canary: &str,
) -> McpAdapterConformanceCaseResult {
    McpAdapterConformanceCaseResult {
        name: name.to_owned(),
        status,
        reason: redact_text(&reason, canary),
        evidence: evidence
            .into_iter()
            .map(|item| redact_evidence(item, canary))
            .collect(),
    }
}

fn redact_identity(mut identity: McpAdapterIdentity, canary: &str) -> McpAdapterIdentity {
    identity.runtime = redact_text(&identity.runtime, canary);
    identity.adapter = redact_text(&identity.adapter, canary);
    identity.adapter_version = redact_text(&identity.adapter_version, canary);
    identity.contract_version = redact_text(&identity.contract_version, canary);
    identity.evidence = identity
        .evidence
        .into_iter()
        .map(|item| redact_evidence(item, canary))
        .collect();
    identity
}

fn redact_evidence(mut evidence: McpEvidence, canary: &str) -> McpEvidence {
    evidence.source = redact_text(&evidence.source, canary);
    evidence.detail = redact_text(&evidence.detail, canary);
    evidence.source_path = evidence.source_path.map(|path| redact_text(&path, canary));
    evidence
}

fn redact_text(value: &str, canary: &str) -> String {
    if canary.is_empty() {
        value.to_owned()
    } else {
        value.replace(canary, "[REDACTED]")
    }
}

fn serialized_contains<T: Serialize>(value: &T, needle: &str) -> bool {
    !needle.is_empty()
        && serde_json::to_string(value)
            .map(|serialized| serialized.contains(needle))
            .unwrap_or(false)
}

fn stable_capability_digest(snapshot: &McpCapabilitySnapshot) -> String {
    let mut runtimes = snapshot.runtimes.clone();
    runtimes.sort_by(|left, right| left.runtime.cmp(&right.runtime));
    digest_json(&runtimes)
}

fn stable_report_digest(report: &McpAdapterConformanceReport) -> String {
    let mut clone = report.clone();
    clone.report_hash.clear();
    digest_json(&clone)
}

fn digest_json<T: Serialize>(value: &T) -> String {
    let json = serde_json::to_vec(value).expect("adapter SDK value is serializable");
    let digest = Sha256::digest(json);
    format!("{digest:x}")
}

pub fn mcp_conformance_evidence(source: &str, detail: &str) -> McpEvidence {
    McpEvidence {
        source: source.to_owned(),
        detail: detail.to_owned(),
        source_path: None,
        proves_runtime_loaded: false,
        proves_current_session_visibility: false,
    }
}

#[derive(Debug)]
pub struct FakeMcpAdapter {
    pub identity: McpAdapterIdentity,
    pub add_configure_support: McpCapabilitySupport,
    pub leak_secret: Option<String>,
    pub write_outside_root: bool,
    pub false_supported_write: bool,
    pub perform_filesystem_write: bool,
    pub enforce_source_cas: bool,
    pub verify_mismatch: bool,
    outcomes: Mutex<BTreeMap<String, McpAdapterApplyOutcome>>,
}

impl FakeMcpAdapter {
    #[must_use]
    pub fn new(runtime: &str) -> Self {
        Self {
            identity: McpAdapterIdentity {
                runtime: runtime.to_owned(),
                adapter: format!("{runtime}-fake-adapter"),
                adapter_version: "1.0.0".to_owned(),
                contract_version: MCP_ADAPTER_SDK_CONTRACT_VERSION.to_owned(),
                evidence: vec![mcp_conformance_evidence("fake", "identity fixture")],
            },
            add_configure_support: McpCapabilitySupport::Supported,
            leak_secret: None,
            write_outside_root: false,
            false_supported_write: false,
            perform_filesystem_write: false,
            enforce_source_cas: false,
            verify_mismatch: false,
            outcomes: Mutex::new(BTreeMap::new()),
        }
    }

    fn capability(&self, context: &McpAdapterSdkContext) -> McpCapabilitySnapshot {
        let mut operations = BTreeMap::new();
        operations.insert(
            McpCapabilityOperation::AddConfigure,
            McpCapabilityDetail {
                support: self.add_configure_support,
                reason: self
                    .leak_secret
                    .clone()
                    .unwrap_or_else(|| "fake add/configure capability".to_owned()),
                evidence: vec![mcp_conformance_evidence("fake", "capability fixture")],
            },
        );
        operations.insert(
            McpCapabilityOperation::Verify,
            McpCapabilityDetail {
                support: McpCapabilitySupport::Supported,
                reason: "fake verify capability".to_owned(),
                evidence: vec![mcp_conformance_evidence("fake", "verify fixture")],
            },
        );
        McpCapabilitySnapshot {
            hash: String::new(),
            observed_at: context.now.clone(),
            runtimes: vec![McpRuntimeCapability {
                runtime: self.identity.runtime.clone(),
                adapter: self.identity.adapter.clone(),
                binary_path: None,
                binary_version: Some(self.identity.adapter_version.clone()),
                config_schema_version: "fake.schema.v1".to_owned(),
                destination: context.config_root.display().to_string(),
                allowed_subtree: "mcp.servers".to_owned(),
                reload_strategy: McpReloadStrategy::NewSessionOnly,
                operations,
            }],
        }
    }
}

#[async_trait]
impl McpRuntimeAdapter for FakeMcpAdapter {
    fn identity(&self) -> McpAdapterIdentity {
        self.identity.clone()
    }

    async fn probe_capabilities(
        &self,
        context: &McpAdapterSdkContext,
    ) -> Result<McpCapabilitySnapshot, McpAdapterError> {
        Ok(self.capability(context))
    }

    async fn readback(
        &self,
        context: &McpAdapterSdkContext,
    ) -> Result<McpInventory, McpAdapterError> {
        Ok(McpInventory {
            observed_at: context.now.clone(),
            ..McpInventory::default()
        })
    }

    async fn preflight_action(
        &self,
        context: &McpAdapterSdkContext,
        request: &McpAdapterActionRequest,
    ) -> Result<McpAdapterPreflightDecision, McpAdapterError> {
        let support = self.add_configure_support;
        let executable = support == McpCapabilitySupport::Supported;
        Ok(McpAdapterPreflightDecision {
            action: McpPreflightAction {
                action_index: request.action_index,
                runtime: self.identity.runtime.clone(),
                server_id: request.action.server_id.clone(),
                operation: McpCapabilityOperation::AddConfigure,
                support,
                executable,
                reason: self
                    .leak_secret
                    .clone()
                    .unwrap_or_else(|| "fake preflight".to_owned()),
                adapter: self.identity.adapter.clone(),
                destination: context.config_root.display().to_string(),
                allowed_subtree: "mcp.servers".to_owned(),
                reload_strategy: McpReloadStrategy::NewSessionOnly,
                idempotency_key: request.idempotency_key.clone(),
                expected_source_hash: request.expected_source_hash.clone(),
                expected_schema_hash: request.expected_schema_hash.clone(),
            },
            reason: "fake preflight decision".to_owned(),
            evidence: vec![mcp_conformance_evidence("fake", "preflight fixture")],
        })
    }

    async fn apply_action(
        &self,
        context: &McpAdapterSdkContext,
        request: &McpAdapterActionRequest,
    ) -> Result<McpAdapterApplyOutcome, McpAdapterError> {
        if self.add_configure_support != McpCapabilitySupport::Supported {
            return Err(McpAdapterError::Unsupported);
        }
        if let Some(outcome) = self
            .outcomes
            .lock()
            .expect("fake adapter outcomes lock")
            .get(&request.idempotency_key)
            .cloned()
        {
            return Ok(outcome);
        }
        let path = if self.write_outside_root {
            context.home_dir.join("outside-mcp.json")
        } else {
            context.config_root.join("mcp.json")
        };
        let before_bytes = fs::read(&path).unwrap_or_default();
        let before_hash = format!("{:x}", Sha256::digest(&before_bytes));
        if self.enforce_source_cas
            && request.expected_source_hash.as_deref() != Some(before_hash.as_str())
        {
            return Err(McpAdapterError::PreconditionFailed(
                "source hash changed".to_owned(),
            ));
        }
        if context.config_root.join(".mcp-conformance.lock").exists() {
            return Err(McpAdapterError::Blocked(
                "adapter lock is already held".to_owned(),
            ));
        }
        let mut result = McpApplyActionResult {
            action_index: request.action_index,
            runtime: self.identity.runtime.clone(),
            server_id: request.action.server_id.clone(),
            status: McpApplyActionStatus::Applied,
            reason: self
                .leak_secret
                .clone()
                .unwrap_or_else(|| "fake apply".to_owned()),
            backup: None,
            before_source_hash: Some("before".to_owned()),
            after_source_hash: Some("after".to_owned()),
        };
        if self.false_supported_write {
            return Ok(McpAdapterApplyOutcome {
                result: Some(result),
                ..McpAdapterApplyOutcome::default()
            });
        }
        let after_hash = if self.perform_filesystem_write {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| McpAdapterError::ExecutionFailed(error.to_string()))?;
            }
            let mut document = serde_json::from_slice::<serde_json::Value>(&before_bytes)
                .unwrap_or_else(|_| serde_json::json!({}));
            let root = document.as_object_mut().ok_or_else(|| {
                McpAdapterError::Blocked("configuration root is not an object".to_owned())
            })?;
            let servers = root
                .entry("mcpServers")
                .or_insert_with(|| serde_json::json!({}))
                .as_object_mut()
                .ok_or_else(|| {
                    McpAdapterError::Blocked("MCP subtree is not an object".to_owned())
                })?;
            servers.insert(
                request.action.server_id.clone(),
                serde_json::json!({"command": "contract-test", "args": []}),
            );
            let bytes = serde_json::to_vec_pretty(&document)
                .map_err(|error| McpAdapterError::ExecutionFailed(error.to_string()))?;
            let temporary = path.with_extension("cocli.tmp");
            fs::write(&temporary, &bytes)
                .and_then(|()| fs::rename(&temporary, &path))
                .map_err(|error| McpAdapterError::ExecutionFailed(error.to_string()))?;
            format!("{:x}", Sha256::digest(bytes))
        } else {
            "after".to_owned()
        };
        result.before_source_hash = Some(before_hash.clone());
        result.after_source_hash = Some(after_hash.clone());
        let outcome = McpAdapterApplyOutcome {
            result: Some(result),
            writes: vec![McpAdapterWriteEffect {
                path,
                operation: McpAdapterWriteOperation::Update,
                before_hash: Some(before_hash),
                after_hash: Some(after_hash),
            }],
            journal: Vec::new(),
        };
        self.outcomes
            .lock()
            .expect("fake adapter outcomes lock")
            .insert(request.idempotency_key.clone(), outcome.clone());
        Ok(outcome)
    }

    async fn reload(
        &self,
        _context: &McpAdapterSdkContext,
    ) -> Result<McpReloadResult, McpAdapterError> {
        Ok(McpReloadResult {
            runtime: self.identity.runtime.clone(),
            status: crate::McpReloadStatus::Deferred,
            reason: "fake adapter uses new-session-only reload".to_owned(),
        })
    }

    async fn verify(
        &self,
        _context: &McpAdapterSdkContext,
    ) -> Result<McpVerificationResult, McpAdapterError> {
        Ok(McpVerificationResult {
            status: if self.verify_mismatch {
                McpVerificationStatus::Mismatched
            } else {
                McpVerificationStatus::Matched
            },
            observation_hash: "fake-observation".to_owned(),
            mismatches: Vec::new(),
            written_config_hashes: BTreeMap::new(),
            session_effective: McpSessionEffectiveStatus::NewSessionRequired,
        })
    }

    async fn rollback(
        &self,
        _context: &McpAdapterSdkContext,
        backup: &McpBackupDescriptor,
    ) -> Result<McpAdapterApplyOutcome, McpAdapterError> {
        if !self.perform_filesystem_write {
            return Ok(McpAdapterApplyOutcome::default());
        }
        let source = PathBuf::from(&backup.source_path);
        let backup_path = PathBuf::from(&backup.backup_path);
        let bytes = fs::read(backup_path)
            .map_err(|error| McpAdapterError::ExecutionFailed(error.to_string()))?;
        let temporary = source.with_extension("rollback.tmp");
        fs::write(&temporary, bytes)
            .and_then(|()| fs::rename(&temporary, &source))
            .map_err(|error| McpAdapterError::ExecutionFailed(error.to_string()))?;
        Ok(McpAdapterApplyOutcome {
            result: None,
            writes: vec![McpAdapterWriteEffect {
                path: source,
                operation: McpAdapterWriteOperation::Restore,
                before_hash: Some(backup.applied_hash.clone()),
                after_hash: Some(backup.source_hash.clone()),
            }],
            journal: Vec::new(),
        })
    }

    async fn recover(
        &self,
        _context: &McpAdapterSdkContext,
        journal: &[McpApplyJournalEntry],
    ) -> Result<McpAdapterRecoveryDecision, McpAdapterError> {
        Ok(match journal.last().map(|entry| entry.phase) {
            Some(
                crate::McpApplyJournalPhase::Verified | crate::McpApplyJournalPhase::RolledBack,
            ) => McpAdapterRecoveryDecision::AlreadyCompleted,
            Some(crate::McpApplyJournalPhase::BackedUp) => McpAdapterRecoveryDecision::Resume,
            Some(
                crate::McpApplyJournalPhase::Written
                | crate::McpApplyJournalPhase::ReloadPending
                | crate::McpApplyJournalPhase::Reloaded,
            ) => McpAdapterRecoveryDecision::Rollback,
            _ => McpAdapterRecoveryDecision::ManualRecoveryRequired,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{McpApprovalMode, McpPlanActionKind, McpRiskLevel, McpStateSummary};

    fn context() -> McpAdapterSdkContext {
        let root = std::env::temp_dir().join("cocli-mcp-adapter-sdk-test");
        McpAdapterSdkContext {
            home_dir: root.join("home"),
            workspace_dir: root.join("workspace"),
            config_root: root.join("config"),
            allowed_write_roots: vec![root.join("config")],
            now: "2026-07-19T00:00:00Z".to_owned(),
        }
    }

    fn scenario() -> McpAdapterConformanceScenario {
        McpAdapterConformanceScenario {
            action_request: McpAdapterActionRequest {
                run_id: "run-1".to_owned(),
                action_index: 0,
                action: McpPlanAction {
                    kind: McpPlanActionKind::AddConfigure,
                    runtime: "fake".to_owned(),
                    scope: "machine".to_owned(),
                    target: "machine:test".to_owned(),
                    server_id: "docs".to_owned(),
                    server_fingerprint: "docs".to_owned(),
                    before: McpStateSummary {
                        configured: Some(false),
                        enabled: Some(false),
                        endpoint_fingerprint: None,
                        allow_tools: Vec::new(),
                        deny_tools: Vec::new(),
                        approval_mode: Some(McpApprovalMode::Manual),
                        secret_ref_count: 0,
                    },
                    after: McpStateSummary {
                        configured: Some(true),
                        enabled: Some(true),
                        endpoint_fingerprint: Some("fingerprint".to_owned()),
                        allow_tools: Vec::new(),
                        deny_tools: Vec::new(),
                        approval_mode: Some(McpApprovalMode::Manual),
                        secret_ref_count: 0,
                    },
                    risk: McpRiskLevel::Medium,
                    reason: "test action".to_owned(),
                    evidence: vec![mcp_conformance_evidence("test", "action")],
                    expected_source_hash: Some("before".to_owned()),
                    expected_schema_hash: Some("schema".to_owned()),
                    blocked: false,
                },
                idempotency_key: "idem".to_owned(),
                desired: None,
                expected_source_hash: Some("before".to_owned()),
                expected_schema_hash: Some("schema".to_owned()),
            },
            canary_secret: "sk-phase3a-canary".to_owned(),
            allow_side_effects: true,
            require_write_evidence_for_supported_writes: true,
            recovery_journal: Vec::new(),
            rollback_backup: Some(McpBackupDescriptor {
                id: "fixture-backup".to_owned(),
                runtime: "fake".to_owned(),
                source_path: context().config_root.join("mcp.json").display().to_string(),
                backup_path: context()
                    .config_root
                    .join("mcp.backup")
                    .display()
                    .to_string(),
                source_hash: "before".to_owned(),
                backup_hash: "before".to_owned(),
                applied_hash: "after".to_owned(),
                source_existed: true,
            }),
            require_idempotent_apply: false,
            require_cas_rejection: false,
            lock_path: None,
            preservation_checks: Vec::new(),
            expected_reload_strategy: Some(McpReloadStrategy::NewSessionOnly),
            expected_verification_status: Some(McpVerificationStatus::Matched),
            expected_session_effective: Some(McpSessionEffectiveStatus::NewSessionRequired),
            recovery_expectations: Vec::new(),
        }
    }

    #[tokio::test]
    async fn conformance_passes_supported_fake_adapter_and_hashes_stably() {
        let adapter = FakeMcpAdapter::new("fake");
        let first = run_mcp_adapter_conformance(&adapter, &context(), &scenario()).await;
        let second = run_mcp_adapter_conformance(&adapter, &context(), &scenario()).await;
        assert!(first.passed);
        assert_eq!(first.report_hash, second.report_hash);
        assert!(!serde_json::to_string(&first)
            .expect("report json")
            .contains("sk-phase3a-canary"));
    }

    #[tokio::test]
    async fn conformance_fails_false_support_and_out_of_root_writes() {
        let mut false_support = FakeMcpAdapter::new("fake");
        false_support.false_supported_write = true;
        let report = run_mcp_adapter_conformance(&false_support, &context(), &scenario()).await;
        assert!(!report.passed);
        assert!(report
            .cases
            .iter()
            .any(|case| case.name == "supported_write_has_evidence"
                && case.status == McpAdapterConformanceStatus::Failed));

        let mut outside = FakeMcpAdapter::new("fake");
        outside.write_outside_root = true;
        let report = run_mcp_adapter_conformance(&outside, &context(), &scenario()).await;
        assert!(!report.passed);
        assert!(report
            .cases
            .iter()
            .any(|case| case.name == "write_root_confinement"
                && case.status == McpAdapterConformanceStatus::Failed));
    }

    #[tokio::test]
    async fn conformance_allows_unsupported_safe_degrade() {
        let mut adapter = FakeMcpAdapter::new("fake");
        adapter.add_configure_support = McpCapabilitySupport::Unsupported;
        let report = run_mcp_adapter_conformance(&adapter, &context(), &scenario()).await;
        assert!(report.passed);
        assert!(report
            .cases
            .iter()
            .any(|case| case.name == "unsupported_safe_degrade"));
    }
}
