use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use async_trait::async_trait;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use cocli_driver_core::{
    bind_mcp_plan_capabilities, export_mcp_governance_bundle, generate_mcp_plan,
    hash_mcp_adapter_conformance_reports, hash_mcp_config, hash_mcp_observation,
    parse_mcp_governance_bundle, resolve_mcp_desired_state, run_mcp_adapter_conformance,
    validate_mcp_governance_bundle, McpAdapterActionRequest, McpAdapterApplyOutcome,
    McpAdapterConformanceReport, McpAdapterConformanceScenario, McpAdapterError,
    McpAdapterIdentity, McpAdapterPreflightDecision, McpAdapterRecoveryDecision,
    McpAdapterSdkContext, McpApplyExecutionRequest, McpApplyExecutionResult, McpApplyJournalEntry,
    McpApplyJournalPhase, McpBindingTargetType, McpBundleDiagnostic, McpBundleError,
    McpBundleRebindings, McpCapabilitySnapshot, McpDesiredServer, McpDesiredTarget,
    McpDiagnosticSeverity, McpDoctorReport, McpDoctorSummary, McpEffectiveDesiredState,
    McpGovernanceBundle, McpInventory, McpPlan, McpPlanAction, McpPlanActionKind,
    McpPortabilityClass, McpPreflightReport, McpProfile, McpProfileBinding, McpRiskLevel,
    McpRollbackExecutionRequest, McpRuntimeAdapter, McpSessionEffectiveStatus, McpStateSummary,
    McpVerificationResult, McpVerificationStatus, MCP_ADAPTER_SDK_CONTRACT_VERSION,
};
use cocli_store::{
    McpApplyRun, McpApplyRunStatus, McpBundleImportAudit, McpBundleImportBindingMutation,
    McpBundleImportCommit, McpBundleImportProfileMutation, McpBundleImportStatus, McpPlanDecision,
    McpPlanDecisionStatus, NewMcpApplyRun, NewMcpBundleImportAudit, NewMcpPlanDecision,
    NewMcpProfile, NewMcpProfileBinding, UpdateMcpProfile,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use super::{ApiError, AppState};
use crate::{McpApplyJournalSink, RuntimeError};

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/runtimes/mcp/inventory", get(machine_mcp_inventory))
        .route("/api/runtimes/mcp/doctor", get(machine_mcp_doctor))
        .route(
            "/api/runtimes/mcp/capabilities",
            get(machine_mcp_capabilities),
        )
        .route(
            "/api/runtimes/mcp/conformance",
            get(machine_mcp_conformance),
        )
        .route(
            "/api/runtimes/mcp/bundles/export-preview",
            post(export_bundle_preview),
        )
        .route("/api/runtimes/mcp/bundles/export", post(export_bundle))
        .route(
            "/api/runtimes/mcp/bundles/import-preview",
            post(import_bundle_preview),
        )
        .route(
            "/api/runtimes/mcp/bundles/imports",
            get(list_bundle_imports),
        )
        .route(
            "/api/runtimes/mcp/bundles/imports/:audit_id",
            get(get_bundle_import),
        )
        .route(
            "/api/runtimes/mcp/bundles/imports/:audit_id/rebind",
            post(rebind_bundle_import),
        )
        .route(
            "/api/runtimes/mcp/bundles/imports/:audit_id/commit",
            post(commit_bundle_import),
        )
        .route(
            "/api/runtimes/mcp/bundles/imports/:audit_id/cancel",
            post(cancel_bundle_import),
        )
        .route(
            "/api/runtimes/mcp/profiles",
            get(list_profiles).post(create_profile),
        )
        .route(
            "/api/runtimes/mcp/profiles/:profile_id",
            get(get_profile).put(update_profile).delete(delete_profile),
        )
        .route(
            "/api/runtimes/mcp/bindings",
            get(list_bindings).post(create_binding),
        )
        .route(
            "/api/runtimes/mcp/bindings/:binding_id",
            axum::routing::delete(delete_binding),
        )
        .route("/api/runtimes/mcp/effective", get(effective_state))
        .route("/api/runtimes/mcp/plans", post(create_plan))
        .route("/api/runtimes/mcp/plans/:plan_id", get(get_plan))
        .route(
            "/api/runtimes/mcp/plans/:plan_id/preflight",
            get(preflight_plan),
        )
        .route(
            "/api/runtimes/mcp/plans/:plan_id/approve",
            post(approve_plan),
        )
        .route("/api/runtimes/mcp/plans/:plan_id/reject", post(reject_plan))
        .route("/api/runtimes/mcp/plans/:plan_id/apply", post(apply_plan))
        .route("/api/runtimes/mcp/apply-runs/:run_id", get(get_apply_run))
        .route(
            "/api/runtimes/mcp/apply-runs/:run_id/rollback",
            post(rollback_apply_run),
        )
        .route(
            "/api/runtimes/mcp/apply-runs/:run_id/manual-recovery",
            post(record_manual_recovery),
        )
}

async fn machine_mcp_inventory(
    State(state): State<AppState>,
) -> Result<Json<McpInventory>, ApiError> {
    Ok(Json(state.runtime.inspect_mcp().await?))
}

async fn machine_mcp_doctor(
    State(state): State<AppState>,
) -> Result<Json<McpDoctorReport>, ApiError> {
    let inventory = state.runtime.inspect_mcp().await?;
    Ok(Json(doctor_report(inventory)))
}

async fn machine_mcp_capabilities(
    State(state): State<AppState>,
) -> Result<Json<McpCapabilitySnapshot>, ApiError> {
    Ok(Json(state.runtime.inspect_mcp_capabilities().await?))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct McpConformanceSummary {
    schema_version: &'static str,
    generated_at: String,
    reports: Vec<McpAdapterConformanceReport>,
    report_hash: String,
    note: &'static str,
}

async fn machine_mcp_conformance(
    State(state): State<AppState>,
) -> Result<Json<McpConformanceSummary>, ApiError> {
    let capabilities = state.runtime.inspect_mcp_capabilities().await?;
    let inventory = state.runtime.inspect_mcp().await?;
    let root = std::env::temp_dir().join("cocli-mcp-adapter-conformance-no-write");
    let context = McpAdapterSdkContext {
        home_dir: root.join("home"),
        workspace_dir: root.join("workspace"),
        config_root: root.join("config"),
        allowed_write_roots: vec![root.join("config")],
        now: capabilities.observed_at.clone(),
    };
    let mut reports = Vec::new();
    for runtime in capabilities.runtimes.iter().filter(|runtime| {
        matches!(
            runtime.runtime.as_str(),
            "codex" | "cursor" | "claude" | "grok"
        )
    }) {
        let adapter = ObservedFirstPartyAdapter {
            capability: runtime.clone(),
            observed_at: capabilities.observed_at.clone(),
            inventory: inventory.clone(),
        };
        reports.push(
            run_mcp_adapter_conformance(
                &adapter,
                &context,
                &observed_conformance_scenario(&runtime.runtime),
            )
            .await,
        );
    }
    reports.sort_by(|left, right| left.adapter.runtime.cmp(&right.adapter.runtime));
    let report_hash = hash_mcp_adapter_conformance_reports(&reports);
    Ok(Json(McpConformanceSummary {
        schema_version: "mcp-adapter-conformance-summary.v1",
        generated_at: Utc::now().to_rfc3339(),
        reports,
        report_hash,
        note: "Observed reports wrap production RuntimeService capability and inventory paths only. Skipped write/reload/recovery cases are not certified; offline temp-root conformance tests cover side effects. Live preflight remains authoritative.",
    }))
}

#[derive(Debug, Clone)]
struct ObservedFirstPartyAdapter {
    capability: cocli_driver_core::McpRuntimeCapability,
    observed_at: String,
    inventory: McpInventory,
}

#[async_trait]
impl McpRuntimeAdapter for ObservedFirstPartyAdapter {
    fn identity(&self) -> McpAdapterIdentity {
        McpAdapterIdentity {
            runtime: self.capability.runtime.clone(),
            adapter: format!("{}:observed-runtime-service", self.capability.adapter),
            adapter_version: self
                .capability
                .binary_version
                .clone()
                .unwrap_or_else(|| self.capability.config_schema_version.clone()),
            contract_version: MCP_ADAPTER_SDK_CONTRACT_VERSION.to_owned(),
            evidence: vec![cocli_driver_core::mcp_conformance_evidence(
                "runtime_service",
                "identity and capability are observed through the production RuntimeService path",
            )],
        }
    }

    async fn probe_capabilities(
        &self,
        _context: &McpAdapterSdkContext,
    ) -> Result<McpCapabilitySnapshot, McpAdapterError> {
        let mut snapshot = McpCapabilitySnapshot {
            hash: String::new(),
            observed_at: self.observed_at.clone(),
            runtimes: vec![self.capability.clone()],
        };
        snapshot.hash = cocli_driver_core::hash_mcp_capabilities(&snapshot);
        Ok(snapshot)
    }

    async fn readback(
        &self,
        _context: &McpAdapterSdkContext,
    ) -> Result<McpInventory, McpAdapterError> {
        Ok(self.inventory.clone())
    }

    async fn preflight_action(
        &self,
        _context: &McpAdapterSdkContext,
        _request: &McpAdapterActionRequest,
    ) -> Result<McpAdapterPreflightDecision, McpAdapterError> {
        Err(McpAdapterError::Unsupported)
    }

    async fn apply_action(
        &self,
        _context: &McpAdapterSdkContext,
        _request: &McpAdapterActionRequest,
    ) -> Result<McpAdapterApplyOutcome, McpAdapterError> {
        Err(McpAdapterError::Unsupported)
    }

    async fn reload(
        &self,
        _context: &McpAdapterSdkContext,
    ) -> Result<cocli_driver_core::McpReloadResult, McpAdapterError> {
        Err(McpAdapterError::Unsupported)
    }

    async fn verify(
        &self,
        _context: &McpAdapterSdkContext,
    ) -> Result<McpVerificationResult, McpAdapterError> {
        Err(McpAdapterError::Unsupported)
    }

    async fn rollback(
        &self,
        _context: &McpAdapterSdkContext,
        _backup: &cocli_driver_core::McpBackupDescriptor,
    ) -> Result<McpAdapterApplyOutcome, McpAdapterError> {
        Err(McpAdapterError::Unsupported)
    }

    async fn recover(
        &self,
        _context: &McpAdapterSdkContext,
        _journal: &[McpApplyJournalEntry],
    ) -> Result<McpAdapterRecoveryDecision, McpAdapterError> {
        Err(McpAdapterError::Unsupported)
    }
}

fn observed_conformance_scenario(runtime: &str) -> McpAdapterConformanceScenario {
    McpAdapterConformanceScenario {
        action_request: McpAdapterActionRequest {
            run_id: "phase3a-conformance".to_owned(),
            action_index: 0,
            action: McpPlanAction {
                kind: McpPlanActionKind::AddConfigure,
                runtime: runtime.to_owned(),
                scope: "observed_no_write".to_owned(),
                target: "observed_no_write".to_owned(),
                server_id: "not_executed".to_owned(),
                server_fingerprint: "not_executed".to_owned(),
                before: empty_conformance_state(),
                after: McpStateSummary {
                    configured: Some(true),
                    enabled: Some(true),
                    endpoint_fingerprint: Some("fixture".to_owned()),
                    ..empty_conformance_state()
                },
                risk: McpRiskLevel::Medium,
                reason: "online conformance is observation-only".to_owned(),
                evidence: Vec::new(),
                expected_source_hash: Some("fixture-before".to_owned()),
                expected_schema_hash: Some("fixture-schema".to_owned()),
                blocked: false,
            },
            idempotency_key: format!("phase3a:{runtime}"),
            desired: None,
            expected_source_hash: Some("fixture-before".to_owned()),
            expected_schema_hash: Some("fixture-schema".to_owned()),
        },
        canary_secret: "PHASE3A_CONFORMANCE_SECRET_CANARY".to_owned(),
        allow_side_effects: false,
        require_write_evidence_for_supported_writes: false,
        recovery_journal: Vec::new(),
        rollback_backup: None,
        require_idempotent_apply: false,
        require_cas_rejection: false,
        lock_path: None,
        preservation_checks: Vec::new(),
        expected_reload_strategy: None,
        expected_verification_status: None,
        expected_session_effective: None,
        recovery_expectations: Vec::new(),
    }
}

fn empty_conformance_state() -> McpStateSummary {
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportBundleRequest {
    actor: String,
    #[serde(default)]
    include_capability_expectations: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BundleExportView {
    bundle: McpGovernanceBundle,
    diagnostics: Vec<McpBundleDiagnostic>,
    dry_run: bool,
    filename: String,
    file_content: String,
}

async fn export_bundle_preview(
    State(state): State<AppState>,
    Json(request): Json<ExportBundleRequest>,
) -> Result<Json<BundleExportView>, ApiError> {
    Ok(Json(export_bundle_view(&state, request, true).await?))
}

async fn export_bundle(
    State(state): State<AppState>,
    Json(request): Json<ExportBundleRequest>,
) -> Result<Json<BundleExportView>, ApiError> {
    Ok(Json(export_bundle_view(&state, request, false).await?))
}

async fn export_bundle_view(
    state: &AppState,
    request: ExportBundleRequest,
    dry_run: bool,
) -> Result<BundleExportView, ApiError> {
    let profiles = state.store.list_mcp_profiles().await?;
    let bindings = state.store.list_mcp_profile_bindings(None).await?;
    let capabilities = if request.include_capability_expectations {
        Some(state.runtime.inspect_mcp_capabilities().await?)
    } else {
        None
    };
    let bundle =
        export_mcp_governance_bundle(&profiles, &bindings, capabilities.as_ref(), &request.actor)
            .map_err(bundle_error)?;
    Ok(BundleExportView {
        diagnostics: bundle.portability.clone(),
        filename: format!("cocli-mcp-governance-{}.json", &bundle.content_hash[..12]),
        file_content: serde_json::to_string_pretty(&bundle)
            .map_err(|_| ApiError::bad_request("bundle cannot be serialized"))?,
        bundle,
        dry_run,
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportBundlePreviewRequest {
    bundle: Value,
    actor: String,
    #[serde(default)]
    rebindings: McpBundleRebindings,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RebindBundleImportRequest {
    expected_version: i64,
    rebindings: McpBundleRebindings,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommitBundleImportRequest {
    expected_version: i64,
    actor: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BundleImportView {
    audit: McpBundleImportAudit,
    preview: Value,
    can_commit: bool,
}

async fn import_bundle_preview(
    State(state): State<AppState>,
    Json(request): Json<ImportBundlePreviewRequest>,
) -> Result<(StatusCode, Json<BundleImportView>), ApiError> {
    let bytes = serde_json::to_vec(&request.bundle)
        .map_err(|_| ApiError::bad_request("bundle cannot be serialized"))?;
    let bundle = parse_mcp_governance_bundle(&bytes).map_err(bundle_error)?;
    let preview = build_bundle_import_preview(&state, &bundle, &request.rebindings).await?;
    let audit = state
        .store
        .create_mcp_bundle_import_audit(NewMcpBundleImportAudit {
            bundle,
            actor: request.actor,
            rebindings: request.rebindings,
            preview: preview.clone(),
        })
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(BundleImportView {
            can_commit: import_preview_can_commit(&preview),
            audit,
            preview,
        }),
    ))
}

async fn list_bundle_imports(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({
        "imports": state.store.list_mcp_bundle_import_audits().await?,
    })))
}

async fn get_bundle_import(
    State(state): State<AppState>,
    Path(audit_id): Path<Uuid>,
) -> Result<Json<BundleImportView>, ApiError> {
    let audit = state
        .store
        .get_mcp_bundle_import_audit(audit_id)
        .await?
        .ok_or_else(|| ApiError::not_found("MCP bundle import audit not found"))?;
    if audit.status == McpBundleImportStatus::Committed {
        return Ok(Json(BundleImportView {
            can_commit: false,
            preview: audit.preview.clone(),
            audit,
        }));
    }
    Ok(Json(BundleImportView {
        can_commit: import_preview_can_commit(&audit.preview),
        preview: audit.preview.clone(),
        audit,
    }))
}

async fn rebind_bundle_import(
    State(state): State<AppState>,
    Path(audit_id): Path<Uuid>,
    Json(request): Json<RebindBundleImportRequest>,
) -> Result<Json<BundleImportView>, ApiError> {
    let audit = state
        .store
        .get_mcp_bundle_import_audit(audit_id)
        .await?
        .ok_or_else(|| ApiError::not_found("MCP bundle import audit not found"))?;
    let preview = build_bundle_import_preview(&state, &audit.bundle, &request.rebindings).await?;
    let audit = state
        .store
        .update_mcp_bundle_import_preview(
            audit_id,
            request.expected_version,
            &request.rebindings,
            &preview,
        )
        .await?;
    Ok(Json(BundleImportView {
        can_commit: import_preview_can_commit(&preview),
        audit,
        preview,
    }))
}

async fn cancel_bundle_import(
    State(state): State<AppState>,
    Path(audit_id): Path<Uuid>,
    Json(request): Json<CommitBundleImportRequest>,
) -> Result<Json<BundleImportView>, ApiError> {
    if request.actor.trim().is_empty() {
        return Err(ApiError::bad_request("import actor is required"));
    }
    let audit = state
        .store
        .cancel_mcp_bundle_import_audit(audit_id, request.expected_version)
        .await?;
    Ok(Json(BundleImportView {
        can_commit: false,
        preview: audit.preview.clone(),
        audit,
    }))
}

async fn commit_bundle_import(
    State(state): State<AppState>,
    Path(audit_id): Path<Uuid>,
    Json(request): Json<CommitBundleImportRequest>,
) -> Result<Json<BundleImportView>, ApiError> {
    let audit = state
        .store
        .get_mcp_bundle_import_audit(audit_id)
        .await?
        .ok_or_else(|| ApiError::not_found("MCP bundle import audit not found"))?;
    if audit.status == McpBundleImportStatus::Committed {
        return Ok(Json(BundleImportView {
            can_commit: false,
            preview: audit.preview.clone(),
            audit,
        }));
    }
    if audit.actor != request.actor {
        return Err(ApiError::conflict(
            "MCP bundle import must be committed by the preview actor",
        ));
    }
    if audit.status == cocli_store::McpBundleImportStatus::Committed {
        return Ok(Json(BundleImportView {
            can_commit: false,
            preview: audit.preview.clone(),
            audit,
        }));
    }
    if audit.status != cocli_store::McpBundleImportStatus::Previewed
        || audit.version != request.expected_version
    {
        return Err(ApiError::conflict(
            "MCP bundle import audit version changed before commit",
        ));
    }
    validate_mcp_governance_bundle(&audit.bundle).map_err(bundle_error)?;
    let preview = build_bundle_import_preview(&state, &audit.bundle, &audit.rebindings).await?;
    if !import_preview_can_commit(&preview) {
        return Err(ApiError::conflict(
            "MCP bundle import has unresolved rebinding or compatibility diagnostics",
        ));
    }
    let commit = build_bundle_import_commit(&audit.bundle, &audit.rebindings)?;
    let audit = state
        .store
        .commit_mcp_bundle_import(audit_id, request.expected_version, commit)
        .await?;
    Ok(Json(BundleImportView {
        can_commit: false,
        preview: audit.preview.clone(),
        audit,
    }))
}

#[derive(Debug, Serialize)]
struct ProfileListResponse {
    profiles: Vec<McpProfile>,
}

async fn list_profiles(
    State(state): State<AppState>,
) -> Result<Json<ProfileListResponse>, ApiError> {
    Ok(Json(ProfileListResponse {
        profiles: state.store.list_mcp_profiles().await?,
    }))
}

async fn create_profile(
    State(state): State<AppState>,
    Json(input): Json<NewMcpProfile>,
) -> Result<(StatusCode, Json<McpProfile>), ApiError> {
    let profile = state.store.create_mcp_profile(input).await?;
    Ok((StatusCode::CREATED, Json(profile)))
}

async fn get_profile(
    State(state): State<AppState>,
    Path(profile_id): Path<Uuid>,
) -> Result<Json<McpProfile>, ApiError> {
    state
        .store
        .get_mcp_profile(profile_id)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::not_found("MCP profile not found"))
}

async fn update_profile(
    State(state): State<AppState>,
    Path(profile_id): Path<Uuid>,
    Json(input): Json<UpdateMcpProfile>,
) -> Result<Json<McpProfile>, ApiError> {
    Ok(Json(
        state.store.update_mcp_profile(profile_id, input).await?,
    ))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExpectedVersionQuery {
    expected_version: i64,
}

async fn delete_profile(
    State(state): State<AppState>,
    Path(profile_id): Path<Uuid>,
    Query(query): Query<ExpectedVersionQuery>,
) -> Result<Json<Value>, ApiError> {
    if state
        .store
        .delete_mcp_profile(profile_id, query.expected_version)
        .await?
    {
        Ok(Json(json!({ "deleted": profile_id })))
    } else {
        Err(ApiError::not_found("MCP profile not found"))
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateBindingRequest {
    profile_id: Uuid,
    target_type: McpBindingTargetType,
    #[serde(default)]
    target_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct BindingListResponse {
    bindings: Vec<McpProfileBinding>,
}

async fn list_bindings(
    State(state): State<AppState>,
) -> Result<Json<BindingListResponse>, ApiError> {
    Ok(Json(BindingListResponse {
        bindings: state.store.list_mcp_profile_bindings(None).await?,
    }))
}

async fn create_binding(
    State(state): State<AppState>,
    Json(input): Json<CreateBindingRequest>,
) -> Result<(StatusCode, Json<McpProfileBinding>), ApiError> {
    let target_id = match input.target_type {
        McpBindingTargetType::Machine => input
            .target_id
            .unwrap_or_else(|| state.store.current_installation_id().to_owned()),
        McpBindingTargetType::Workspace | McpBindingTargetType::Agent => input
            .target_id
            .ok_or_else(|| ApiError::bad_request("targetId is required for this target type"))?,
    };
    let binding = state
        .store
        .create_mcp_profile_binding(NewMcpProfileBinding {
            profile_id: input.profile_id,
            target_type: input.target_type,
            target_id,
        })
        .await?;
    Ok((StatusCode::CREATED, Json(binding)))
}

async fn delete_binding(
    State(state): State<AppState>,
    Path(binding_id): Path<Uuid>,
    Query(query): Query<ExpectedVersionQuery>,
) -> Result<Json<Value>, ApiError> {
    if state
        .store
        .delete_mcp_profile_binding(binding_id, query.expected_version)
        .await?
    {
        Ok(Json(json!({ "deleted": binding_id })))
    } else {
        Err(ApiError::not_found("MCP profile binding not found"))
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TargetInput {
    #[serde(default)]
    workspace_id: Option<Uuid>,
    #[serde(default)]
    agent_id: Option<Uuid>,
}

async fn effective_state(
    State(state): State<AppState>,
    Query(target): Query<TargetInput>,
) -> Result<Json<McpEffectiveDesiredState>, ApiError> {
    Ok(Json(resolve_state(&state, target).await?))
}

async fn create_plan(
    State(state): State<AppState>,
    Json(target): Json<TargetInput>,
) -> Result<(StatusCode, Json<McpPlanView>), ApiError> {
    let effective = resolve_state(&state, target).await?;
    let inventory = state.runtime.inspect_mcp().await?;
    let mut plan = generate_mcp_plan(
        Uuid::new_v4().to_string(),
        Utc::now().to_rfc3339(),
        effective,
        &inventory,
    );
    let capabilities = state.runtime.inspect_mcp_capabilities().await?;
    bind_mcp_plan_capabilities(&mut plan, &capabilities);
    state.store.save_mcp_plan(&plan).await?;
    Ok((StatusCode::CREATED, Json(McpPlanView::pending(plan))))
}

async fn preflight_plan(
    State(state): State<AppState>,
    Path(plan_id): Path<String>,
) -> Result<Json<McpPreflightReport>, ApiError> {
    let plan = require_plan(&state, &plan_id).await?;
    Ok(Json(state.runtime.preflight_mcp(&plan).await?))
}

async fn get_plan(
    State(state): State<AppState>,
    Path(plan_id): Path<String>,
) -> Result<Json<McpPlanView>, ApiError> {
    let plan = require_plan(&state, &plan_id).await?;
    Ok(Json(plan_view(&state, plan).await?))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApprovePlanRequest {
    plan_hash: String,
    actor: String,
    expires_at: DateTime<Utc>,
}

async fn approve_plan(
    State(state): State<AppState>,
    Path(plan_id): Path<String>,
    Json(request): Json<ApprovePlanRequest>,
) -> Result<Json<McpPlanView>, ApiError> {
    let plan = require_plan(&state, &plan_id).await?;
    if request.plan_hash != plan.plan_hash {
        return Err(ApiError::conflict("MCP plan hash is stale"));
    }
    let (observation_hash, config_hash, capability_hash) = current_hashes(&state, &plan).await?;
    if observation_hash != plan.observation_hash
        || config_hash != plan.config_hash
        || capability_hash != plan.capability_hash
    {
        return Err(ApiError::conflict(
            "MCP plan observation or desired configuration is stale",
        ));
    }
    state
        .store
        .record_mcp_plan_decision(NewMcpPlanDecision {
            plan_id: plan.id.clone(),
            decision: McpPlanDecisionStatus::Approved,
            plan_hash: plan.plan_hash.clone(),
            observation_hash: plan.observation_hash.clone(),
            config_hash: plan.config_hash.clone(),
            actor: request.actor,
            reason: None,
            expires_at: Some(request.expires_at),
        })
        .await?;
    Ok(Json(plan_view(&state, plan).await?))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RejectPlanRequest {
    plan_hash: String,
    actor: String,
    reason: String,
}

async fn reject_plan(
    State(state): State<AppState>,
    Path(plan_id): Path<String>,
    Json(request): Json<RejectPlanRequest>,
) -> Result<Json<McpPlanView>, ApiError> {
    let plan = require_plan(&state, &plan_id).await?;
    if request.plan_hash != plan.plan_hash {
        return Err(ApiError::conflict("MCP plan hash is stale"));
    }
    state
        .store
        .record_mcp_plan_decision(NewMcpPlanDecision {
            plan_id: plan.id.clone(),
            decision: McpPlanDecisionStatus::Rejected,
            plan_hash: plan.plan_hash.clone(),
            observation_hash: plan.observation_hash.clone(),
            config_hash: plan.config_hash.clone(),
            actor: request.actor,
            reason: Some(request.reason),
            expires_at: None,
        })
        .await?;
    Ok(Json(plan_view(&state, plan).await?))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApplyPlanRequest {
    plan_hash: String,
    observation_hash: String,
    config_hash: String,
    actor: String,
    confirm_high_risk: bool,
}

#[derive(Debug, Serialize)]
struct ApplyRunView {
    run: McpApplyRun,
}

#[derive(Clone)]
struct StoreMcpApplyJournalSink {
    store: cocli_store::Store,
}

#[async_trait::async_trait]
impl McpApplyJournalSink for StoreMcpApplyJournalSink {
    async fn checkpoint(
        &self,
        run_id: &str,
        entry: &McpApplyJournalEntry,
    ) -> Result<(), RuntimeError> {
        let run_id = Uuid::parse_str(run_id).map_err(|_| {
            RuntimeError::Delivery("MCP journal run identifier is invalid".to_owned())
        })?;
        self.store
            .checkpoint_mcp_apply_run(run_id, entry.phase, entry, None, None)
            .await
            .map(|_| ())
            .map_err(|_| RuntimeError::Delivery("MCP journal checkpoint failed".to_owned()))
    }
}

async fn apply_plan(
    State(state): State<AppState>,
    Path(plan_id): Path<String>,
    Json(request): Json<ApplyPlanRequest>,
) -> Result<Json<ApplyRunView>, ApiError> {
    let lock = {
        let mut locks = state.mcp_apply_locks.lock().await;
        locks
            .entry(plan_id.clone())
            .or_insert_with(|| std::sync::Arc::new(tokio::sync::Mutex::new(())))
            .clone()
    };
    let _guard = lock.lock().await;
    let plan = require_plan(&state, &plan_id).await?;
    let decision = valid_apply_approval(&state, &plan, &request).await?;
    let existing = state
        .store
        .get_mcp_apply_run_for_approval(&plan.id, decision.id)
        .await?;
    if let Some(existing) = existing
        .as_ref()
        .filter(|run| !is_active_apply_status(run.status))
    {
        return Ok(Json(ApplyRunView {
            run: existing.clone(),
        }));
    }
    let (observation_hash, config_hash, capability_hash) = current_hashes(&state, &plan).await?;
    let recovering = existing.as_ref().is_some_and(|run| {
        is_active_apply_status(run.status)
            && run.journal.iter().any(|entry| {
                matches!(
                    entry.phase,
                    McpApplyJournalPhase::BackedUp
                        | McpApplyJournalPhase::Written
                        | McpApplyJournalPhase::ReloadPending
                        | McpApplyJournalPhase::Reloaded
                        | McpApplyJournalPhase::Verified
                )
            })
    });
    let observation_drift = observation_hash != plan.observation_hash;
    let unrecoverable_drift = config_hash != plan.config_hash
        || capability_hash != plan.capability_hash
        || (observation_drift && !recovering);
    if unrecoverable_drift {
        if let Some(existing) = existing
            .as_ref()
            .filter(|run| is_active_apply_status(run.status))
        {
            let run = state
                .store
                .mark_mcp_apply_recovery_required(
                    existing.id,
                    "observation or desired configuration drifted during an interrupted apply",
                )
                .await?;
            return Ok(Json(ApplyRunView { run }));
        }
        return Err(ApiError::conflict(
            "MCP observation or desired configuration drifted after approval",
        ));
    }
    let high_risk = plan
        .actions
        .iter()
        .any(|action| action.risk >= McpRiskLevel::High);
    if high_risk && !request.confirm_high_risk {
        return Err(ApiError::bad_request(
            "high-risk MCP plan requires explicit second confirmation",
        ));
    }
    let preflight = state.runtime.preflight_mcp(&plan).await?;
    if preflight
        .stale_reasons
        .iter()
        .any(|reason| reason != "observation_drift" || !recovering)
    {
        return Err(ApiError::conflict(
            "MCP adapter preflight is stale; regenerate and approve the plan",
        ));
    }
    let run = match existing {
        Some(existing) => {
            if existing.actor != request.actor {
                return Err(ApiError::conflict(
                    "interrupted MCP apply must be resumed by the original actor",
                ));
            }
            existing
        }
        None => {
            state
                .store
                .create_mcp_apply_run(NewMcpApplyRun {
                    plan_id: plan.id.clone(),
                    approval_id: decision.id,
                    plan_hash: plan.plan_hash.clone(),
                    observation_hash: plan.observation_hash.clone(),
                    config_hash: plan.config_hash.clone(),
                    capability_hash: plan.capability_hash.clone(),
                    actor: request.actor.clone(),
                    confirm_high_risk: request.confirm_high_risk,
                })
                .await?
        }
    };
    let run = checkpoint_preflight(&state, run, &preflight).await?;
    let capability_hash = plan.capability_hash.clone();
    let journal_sink: Arc<dyn McpApplyJournalSink> = Arc::new(StoreMcpApplyJournalSink {
        store: state.store.clone(),
    });
    let execution = match state
        .runtime
        .apply_mcp(
            McpApplyExecutionRequest {
                run_id: run.id.to_string(),
                plan,
                actor: request.actor,
                confirm_high_risk: request.confirm_high_risk,
                capability_hash,
                resume_journal: run.journal.clone(),
            },
            journal_sink,
        )
        .await
    {
        Ok(result) => result,
        Err(error) => failed_execution(&run, error),
    };
    let run = state
        .store
        .complete_mcp_apply_run(run.id, &execution)
        .await?;
    Ok(Json(ApplyRunView { run }))
}

async fn checkpoint_preflight(
    state: &AppState,
    run: McpApplyRun,
    preflight: &McpPreflightReport,
) -> Result<McpApplyRun, ApiError> {
    let mut next = run;
    for action in &preflight.actions {
        let entry = McpApplyJournalEntry {
            sequence: next
                .journal
                .iter()
                .map(|entry| entry.sequence)
                .max()
                .unwrap_or(0)
                + 1,
            action_index: action.action_index,
            runtime: action.runtime.clone(),
            server_id: action.server_id.clone(),
            idempotency_key: action.idempotency_key.clone(),
            phase: McpApplyJournalPhase::Preflight,
            attempt: (next.attempt + 1).max(1) as u32,
            expected_source_hash: action.expected_source_hash.clone(),
            expected_schema_hash: action.expected_schema_hash.clone(),
            backup: None,
            reason: action.reason.clone(),
            evidence: Vec::new(),
        };
        next = state
            .store
            .checkpoint_mcp_apply_run(
                next.id,
                McpApplyJournalPhase::Preflight,
                &entry,
                Some(preflight),
                None,
            )
            .await?;
    }
    Ok(next)
}

fn is_active_apply_status(status: McpApplyRunStatus) -> bool {
    matches!(
        status,
        McpApplyRunStatus::Pending
            | McpApplyRunStatus::Running
            | McpApplyRunStatus::Preflight
            | McpApplyRunStatus::Locked
            | McpApplyRunStatus::BackedUp
            | McpApplyRunStatus::Written
            | McpApplyRunStatus::ReloadPending
            | McpApplyRunStatus::Reloaded
            | McpApplyRunStatus::RecoveryRequired
    )
}

async fn valid_apply_approval(
    state: &AppState,
    plan: &McpPlan,
    request: &ApplyPlanRequest,
) -> Result<McpPlanDecision, ApiError> {
    if request.actor.trim().is_empty() {
        return Err(ApiError::bad_request("apply actor is required"));
    }
    if request.plan_hash != plan.plan_hash
        || request.observation_hash != plan.observation_hash
        || request.config_hash != plan.config_hash
    {
        return Err(ApiError::conflict(
            "MCP apply request does not match the persisted plan hashes",
        ));
    }
    let decision = state
        .store
        .get_mcp_plan_decision(&plan.id)
        .await?
        .ok_or_else(|| ApiError::conflict("MCP plan has no approval"))?;
    if decision.decision != McpPlanDecisionStatus::Approved {
        return Err(ApiError::conflict("MCP plan was not approved"));
    }
    if decision.plan_hash != plan.plan_hash
        || decision.observation_hash != plan.observation_hash
        || decision.config_hash != plan.config_hash
    {
        return Err(ApiError::conflict("MCP approval hashes are stale"));
    }
    if decision
        .expires_at
        .map_or(true, |expiry| expiry <= Utc::now())
    {
        return Err(ApiError::conflict("MCP approval is expired"));
    }
    Ok(decision)
}

fn failed_execution(run: &McpApplyRun, error: super::RuntimeError) -> McpApplyExecutionResult {
    let (verification_status, reason) = match error {
        super::RuntimeError::Unsupported(_) => (
            McpVerificationStatus::Blocked,
            "Runtime apply adapter is unsupported",
        ),
        _ => (
            McpVerificationStatus::Failed,
            "Runtime apply adapter failed",
        ),
    };
    McpApplyExecutionResult {
        actions: Vec::new(),
        reloads: Vec::new(),
        verification: McpVerificationResult {
            status: verification_status,
            observation_hash: run.observation_hash.clone(),
            mismatches: vec![format!("{reason}; no configuration success was recorded")],
            written_config_hashes: Default::default(),
            session_effective: McpSessionEffectiveStatus::Unknown,
        },
        journal: run.journal.clone(),
    }
}

async fn get_apply_run(
    State(state): State<AppState>,
    Path(run_id): Path<Uuid>,
) -> Result<Json<ApplyRunView>, ApiError> {
    let run = state
        .store
        .get_mcp_apply_run(run_id)
        .await?
        .ok_or_else(|| ApiError::not_found("MCP apply run not found"))?;
    Ok(Json(ApplyRunView { run }))
}

#[derive(Debug, Deserialize)]
struct RollbackRunRequest {
    actor: Option<String>,
}

async fn rollback_apply_run(
    State(state): State<AppState>,
    Path(run_id): Path<Uuid>,
    Json(request): Json<RollbackRunRequest>,
) -> Result<Json<ApplyRunView>, ApiError> {
    let initial = state
        .store
        .get_mcp_apply_run(run_id)
        .await?
        .ok_or_else(|| ApiError::not_found("MCP apply run not found"))?;
    let lock = {
        let mut locks = state.mcp_apply_locks.lock().await;
        locks
            .entry(initial.plan_id.clone())
            .or_insert_with(|| std::sync::Arc::new(tokio::sync::Mutex::new(())))
            .clone()
    };
    let _guard = lock.lock().await;
    let run = state
        .store
        .get_mcp_apply_run(run_id)
        .await?
        .ok_or_else(|| ApiError::not_found("MCP apply run not found"))?;
    if matches!(
        run.status,
        McpApplyRunStatus::Pending
            | McpApplyRunStatus::Running
            | McpApplyRunStatus::Preflight
            | McpApplyRunStatus::Locked
    ) {
        return Err(ApiError::conflict(
            "MCP apply run must finish before rollback",
        ));
    }
    if run.rollback_status == Some(McpApplyRunStatus::RolledBack) {
        return Ok(Json(ApplyRunView { run }));
    }
    let mut backups = run
        .actions
        .iter()
        .filter_map(|action| action.backup.clone())
        .collect::<Vec<_>>();
    for backup in run.journal.iter().filter_map(|entry| entry.backup.clone()) {
        if !backups.iter().any(|existing| existing.id == backup.id) {
            backups.push(backup);
        }
    }
    backups.reverse();
    if backups.is_empty() {
        return Err(ApiError::conflict("MCP apply run has no restorable backup"));
    }
    let actor = request.actor.unwrap_or_else(|| run.actor.clone());
    if actor.trim().is_empty() {
        return Err(ApiError::bad_request("rollback actor is required"));
    }
    let first_action = run.actions.iter().find(|action| action.backup.is_some());
    let first_journal = run.journal.iter().find(|entry| entry.backup.is_some());
    let action_index = first_action
        .map(|action| action.action_index)
        .or_else(|| first_journal.map(|entry| entry.action_index))
        .ok_or_else(|| ApiError::conflict("MCP apply run has no restorable action"))?;
    let runtime = first_action
        .map(|action| action.runtime.clone())
        .or_else(|| first_journal.map(|entry| entry.runtime.clone()))
        .unwrap_or_default();
    let server_id = first_action
        .map(|action| action.server_id.clone())
        .or_else(|| first_journal.map(|entry| entry.server_id.clone()))
        .unwrap_or_default();
    let expected_source_hash = first_action
        .and_then(|action| action.before_source_hash.clone())
        .or_else(|| first_journal.and_then(|entry| entry.expected_source_hash.clone()));
    let first_backup = first_action
        .and_then(|action| action.backup.clone())
        .or_else(|| first_journal.and_then(|entry| entry.backup.clone()));
    let rolling_back = McpApplyJournalEntry {
        sequence: run
            .journal
            .iter()
            .map(|entry| entry.sequence)
            .max()
            .unwrap_or(0)
            + 1,
        action_index,
        runtime,
        server_id,
        idempotency_key: format!("rollback:{}:{action_index}", run.id),
        phase: McpApplyJournalPhase::RollingBack,
        attempt: run.attempt.max(1) as u32,
        expected_source_hash,
        expected_schema_hash: None,
        backup: first_backup,
        reason: "compensating rollback started under the apply plan lock".to_owned(),
        evidence: Vec::new(),
    };
    state
        .store
        .checkpoint_mcp_apply_run(
            run.id,
            McpApplyJournalPhase::RollingBack,
            &rolling_back,
            None,
            None,
        )
        .await?;
    let result = state
        .runtime
        .rollback_mcp(McpRollbackExecutionRequest {
            run_id: run.id.to_string(),
            actor: actor.clone(),
            backups,
        })
        .await?;
    let run = state
        .store
        .complete_mcp_rollback(run.id, &actor, &result)
        .await?;
    Ok(Json(ApplyRunView { run }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManualRecoveryRequest {
    actor: Option<String>,
    reason: String,
}

async fn record_manual_recovery(
    State(state): State<AppState>,
    Path(run_id): Path<Uuid>,
    Json(request): Json<ManualRecoveryRequest>,
) -> Result<Json<ApplyRunView>, ApiError> {
    let actor = request.actor.unwrap_or_else(|| "desktop-user".to_owned());
    if actor.trim().is_empty() || request.reason.trim().is_empty() {
        return Err(ApiError::bad_request(
            "manual recovery actor and reason are required",
        ));
    }
    let run = state
        .store
        .record_mcp_manual_recovery(run_id, &actor, &request.reason)
        .await?;
    Ok(Json(ApplyRunView { run }))
}

async fn build_bundle_import_preview(
    state: &AppState,
    bundle: &McpGovernanceBundle,
    rebindings: &McpBundleRebindings,
) -> Result<Value, ApiError> {
    let profiles = state.store.list_mcp_profiles().await?;
    let profile_by_id = profiles
        .iter()
        .map(|profile| (profile.id.clone(), profile))
        .collect::<BTreeMap<_, _>>();
    let capabilities = state.runtime.inspect_mcp_capabilities().await?;
    let runtime_caps = capabilities
        .runtimes
        .iter()
        .map(|runtime| (runtime.runtime.as_str(), runtime))
        .collect::<BTreeMap<_, _>>();
    let mut diagnostics = bundle
        .portability
        .iter()
        .filter(|diagnostic| {
            matches!(
                diagnostic.classification,
                McpPortabilityClass::Blocked | McpPortabilityClass::MachineLocal
            )
        })
        .cloned()
        .collect::<Vec<_>>();
    let mut profile_changes = Vec::new();
    let mut binding_changes = Vec::new();

    for expectation in &bundle.capability_expectations {
        let runtime = rebindings
            .runtimes
            .get(&format!("runtime:{}", expectation.runtime))
            .map_or(expectation.runtime.as_str(), String::as_str);
        match runtime_caps.get(runtime) {
            Some(actual) => {
                for (operation, expected) in &expectation.operations {
                    let actual_support = actual
                        .operations
                        .get(operation)
                        .map_or(cocli_driver_core::McpCapabilitySupport::Unknown, |detail| {
                            detail.support
                        });
                    if &actual_support < expected {
                        diagnostics.push(bundle_diagnostic(
                            "capability_mismatch",
                            McpPortabilityClass::RequiresRebind,
                            None,
                            None,
                            Some(format!("{runtime}.{operation:?}")),
                            Some(format!("runtime:{}", expectation.runtime)),
                            "destination runtime capability is weaker than the bundle expectation",
                        ));
                    }
                }
            }
            None => diagnostics.push(bundle_diagnostic(
                "runtime_missing",
                McpPortabilityClass::RequiresRebind,
                None,
                None,
                Some("runtime".to_owned()),
                Some(format!("runtime:{}", expectation.runtime)),
                "destination runtime installation is missing or unbound",
            )),
        }
    }

    for profile in &bundle.profiles {
        match rebindings.profiles.get(&profile.profile_ref) {
            Some(target) => match profile_by_id.get(&target.profile_id) {
                Some(existing) if existing.version == target.expected_version => {
                    profile_changes.push(json!({
                        "profileRef": profile.profile_ref,
                        "operation": "update",
                        "profileId": target.profile_id,
                        "expectedVersion": target.expected_version,
                    }));
                }
                Some(existing) => {
                    diagnostics.push(bundle_diagnostic(
                        "profile_version_conflict",
                        McpPortabilityClass::Blocked,
                        Some(profile.profile_ref.clone()),
                        None,
                        Some("profiles".to_owned()),
                        Some(profile.profile_ref.clone()),
                        &format!(
                            "destination profile version {} does not match expected version {}",
                            existing.version, target.expected_version
                        ),
                    ));
                }
                None => diagnostics.push(bundle_diagnostic(
                    "profile_rebind_missing_target",
                    McpPortabilityClass::Blocked,
                    Some(profile.profile_ref.clone()),
                    None,
                    Some("profiles".to_owned()),
                    Some(profile.profile_ref.clone()),
                    "profile rebinding references a missing destination profile",
                )),
            },
            None => profile_changes.push(json!({
                "profileRef": profile.profile_ref,
                "operation": "create",
            })),
        }
        for server in &profile.servers {
            let runtime_key = format!("runtime:{}", server.runtime);
            let destination_runtime = rebindings
                .runtimes
                .get(&runtime_key)
                .or_else(|| rebindings.runtimes.get(&server.runtime));
            match destination_runtime {
                None => diagnostics.push(bundle_diagnostic(
                    "runtime_rebind_missing",
                    McpPortabilityClass::RequiresRebind,
                    Some(profile.profile_ref.clone()),
                    Some(server.server_id.clone()),
                    Some("runtime".to_owned()),
                    Some(runtime_key),
                    "runtime must be explicitly rebound; names are not guessed",
                )),
                Some(runtime) if !runtime_caps.contains_key(runtime.as_str()) => {
                    diagnostics.push(bundle_diagnostic(
                        "runtime_installation_missing",
                        McpPortabilityClass::Blocked,
                        Some(profile.profile_ref.clone()),
                        Some(server.server_id.clone()),
                        Some("runtime".to_owned()),
                        Some(runtime_key),
                        "rebound runtime is not installed or discoverable",
                    ));
                }
                Some(_) => {}
            }
            for secret_ref in &server.secret_refs {
                if !rebindings.secret_refs.contains_key(&secret_ref.reference) {
                    diagnostics.push(bundle_diagnostic(
                        "secret_ref_rebind_missing",
                        McpPortabilityClass::RequiresRebind,
                        Some(profile.profile_ref.clone()),
                        Some(server.server_id.clone()),
                        Some(format!("secretRefs.{}", secret_ref.location)),
                        Some(secret_ref.reference.clone()),
                        "secret reference must be explicitly rebound on import",
                    ));
                }
            }
            if let Some(definition) = &server.definition {
                for value in definition
                    .command
                    .iter()
                    .chain(definition.args.iter())
                    .chain(definition.endpoint.iter())
                {
                    for key in rebind_placeholders(value) {
                        if !rebindings.machine_local_values.contains_key(&key) {
                            diagnostics.push(bundle_diagnostic(
                                "machine_local_rebind_missing",
                                McpPortabilityClass::RequiresRebind,
                                Some(profile.profile_ref.clone()),
                                Some(server.server_id.clone()),
                                Some("definition".to_owned()),
                                Some(key),
                                "machine-local command, argument, or endpoint requires explicit rebinding",
                            ));
                        }
                    }
                }
            }
        }
    }

    for binding in &bundle.relative_bindings {
        let target_id = rebindings.targets.get(&binding.target_ref);
        match target_id {
            None => diagnostics.push(bundle_diagnostic(
                "target_rebind_missing",
                McpPortabilityClass::RequiresRebind,
                Some(binding.profile_ref.clone()),
                None,
                Some("binding.target".to_owned()),
                Some(binding.target_ref.clone()),
                "binding target must be explicitly rebound",
            )),
            Some(target_id)
                if !bundle_target_exists(state, binding.target_type, target_id).await? =>
            {
                diagnostics.push(bundle_diagnostic(
                    "target_missing",
                    McpPortabilityClass::Blocked,
                    Some(binding.profile_ref.clone()),
                    None,
                    Some("binding.target".to_owned()),
                    Some(binding.target_ref.clone()),
                    "rebound target is not part of the destination ownership model",
                ));
            }
            Some(_) => {}
        }
        binding_changes.push(json!({
            "profileRef": binding.profile_ref,
            "operation": if target_id.is_some() { "bind" } else { "missing_target_rebind" },
            "targetType": binding.target_type,
            "targetRef": binding.target_ref,
            "targetId": target_id,
        }));
    }

    diagnostics.sort_by(|left, right| {
        (
            left.classification,
            &left.profile_ref,
            &left.server_id,
            &left.field,
            &left.code,
        )
            .cmp(&(
                right.classification,
                &right.profile_ref,
                &right.server_id,
                &right.field,
                &right.code,
            ))
    });
    diagnostics.dedup();
    let blocking_count = diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic.classification == McpPortabilityClass::Blocked
                || diagnostic.code.contains("_missing")
                || diagnostic.code.contains("_mismatch")
                || diagnostic.code.contains("_conflict")
        })
        .count();
    Ok(json!({
        "schemaVersion": bundle.schema_version,
        "bundleHash": bundle.content_hash,
        "diagnostics": diagnostics,
        "profileChanges": profile_changes,
        "bindingChanges": binding_changes,
        "approvalImported": false,
        "applyImported": false,
        "blockingCount": blocking_count,
        "capabilityExpectationOnly": true,
    }))
}

async fn bundle_target_exists(
    state: &AppState,
    target_type: McpBindingTargetType,
    target_id: &str,
) -> Result<bool, ApiError> {
    match target_type {
        McpBindingTargetType::Machine => Ok(target_id == state.store.current_installation_id()),
        McpBindingTargetType::Workspace => {
            let Ok(id) = Uuid::parse_str(target_id) else {
                return Ok(false);
            };
            Ok(state.store.get_workspace(id).await?.is_some())
        }
        McpBindingTargetType::Agent => {
            let Ok(id) = Uuid::parse_str(target_id) else {
                return Ok(false);
            };
            Ok(state.store.get_agent(id).await?.is_some())
        }
    }
}

fn build_bundle_import_commit(
    bundle: &McpGovernanceBundle,
    rebindings: &McpBundleRebindings,
) -> Result<McpBundleImportCommit, ApiError> {
    let mut profiles = Vec::with_capacity(bundle.profiles.len());
    for profile in &bundle.profiles {
        let servers = profile
            .servers
            .iter()
            .map(|server| rebind_desired_server(server, rebindings))
            .collect::<Result<Vec<_>, _>>()?;
        let (profile_id, expected_version) =
            if let Some(target) = rebindings.profiles.get(&profile.profile_ref) {
                (
                    Some(Uuid::parse_str(&target.profile_id).map_err(|_| {
                        ApiError::bad_request("profile rebinding id must be a UUID")
                    })?),
                    Some(target.expected_version),
                )
            } else {
                (None, None)
            };
        profiles.push(McpBundleImportProfileMutation {
            profile_ref: profile.profile_ref.clone(),
            profile_id,
            expected_version,
            name: profile.name.clone(),
            description: profile.description.clone(),
            servers,
        });
    }
    let bindings = bundle
        .relative_bindings
        .iter()
        .map(|binding| {
            Ok(McpBundleImportBindingMutation {
                profile_ref: binding.profile_ref.clone(),
                target_ref: binding.target_ref.clone(),
                target_type: binding.target_type,
                target_id: rebindings
                    .targets
                    .get(&binding.target_ref)
                    .ok_or_else(|| ApiError::bad_request("binding target rebinding is required"))?
                    .clone(),
            })
        })
        .collect::<Result<Vec<_>, ApiError>>()?;
    Ok(McpBundleImportCommit { profiles, bindings })
}

fn rebind_desired_server(
    server: &McpDesiredServer,
    rebindings: &McpBundleRebindings,
) -> Result<McpDesiredServer, ApiError> {
    let mut rebound = server.clone();
    rebound.runtime.clone_from(
        rebindings
            .runtimes
            .get(&format!("runtime:{}", server.runtime))
            .ok_or_else(|| ApiError::bad_request("runtime rebinding is required"))?,
    );
    for secret_ref in &mut rebound.secret_refs {
        secret_ref.reference.clone_from(
            rebindings
                .secret_refs
                .get(&secret_ref.reference)
                .ok_or_else(|| ApiError::bad_request("secret rebinding is required"))?,
        );
    }
    if let Some(definition) = &mut rebound.definition {
        if let Some(command) = &mut definition.command {
            *command = replace_rebind_placeholders(command, &rebindings.machine_local_values)?;
        }
        for arg in &mut definition.args {
            *arg = replace_rebind_placeholders(arg, &rebindings.machine_local_values)?;
        }
        if let Some(endpoint) = &mut definition.endpoint {
            *endpoint = replace_rebind_placeholders(endpoint, &rebindings.machine_local_values)?;
        }
    }
    Ok(rebound)
}

fn rebind_placeholders(value: &str) -> Vec<String> {
    let mut keys = Vec::new();
    let mut rest = value;
    while let Some(start) = rest.find("{{rebind:") {
        let after_start = &rest[start + "{{rebind:".len()..];
        let Some(end) = after_start.find("}}") else {
            break;
        };
        keys.push(after_start[..end].to_owned());
        rest = &after_start[end + 2..];
    }
    keys
}

fn replace_rebind_placeholders(
    value: &str,
    replacements: &BTreeMap<String, String>,
) -> Result<String, ApiError> {
    let mut next = value.to_owned();
    for key in rebind_placeholders(value) {
        let replacement = replacements
            .get(&key)
            .ok_or_else(|| ApiError::bad_request("machine-local rebinding is required"))?;
        next = next.replace(&format!("{{{{rebind:{key}}}}}"), replacement);
    }
    Ok(next)
}

fn import_preview_can_commit(preview: &Value) -> bool {
    preview
        .get("blockingCount")
        .and_then(Value::as_u64)
        .is_some_and(|count| count == 0)
}

fn bundle_diagnostic(
    code: &str,
    classification: McpPortabilityClass,
    profile_ref: Option<String>,
    server_id: Option<String>,
    field: Option<String>,
    rebind_key: Option<String>,
    message: &str,
) -> McpBundleDiagnostic {
    McpBundleDiagnostic {
        code: code.to_owned(),
        classification,
        profile_ref,
        server_id,
        field,
        rebind_key,
        message: message.to_owned(),
    }
}

fn bundle_error(error: McpBundleError) -> ApiError {
    match error {
        McpBundleError::UnsupportedVersion(_) | McpBundleError::HashMismatch => {
            ApiError::conflict(error.to_string())
        }
        McpBundleError::TooLarge | McpBundleError::TooDeep => ApiError {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            message: error.to_string(),
            body: None,
        },
        McpBundleError::Invalid(_) => ApiError::bad_request(error.to_string()),
    }
}

async fn resolve_state(
    state: &AppState,
    target: TargetInput,
) -> Result<McpEffectiveDesiredState, ApiError> {
    validate_target(state, &target).await?;
    let profiles = state.store.list_mcp_profiles().await?;
    let bindings = state.store.list_mcp_profile_bindings(None).await?;
    Ok(resolve_mcp_desired_state(
        &profiles,
        &bindings,
        McpDesiredTarget {
            machine_id: state.store.current_installation_id().to_owned(),
            workspace_id: target.workspace_id.map(|id| id.to_string()),
            agent_id: target.agent_id.map(|id| id.to_string()),
        },
    ))
}

async fn validate_target(state: &AppState, target: &TargetInput) -> Result<(), ApiError> {
    if let Some(workspace_id) = target.workspace_id {
        if state.store.get_workspace(workspace_id).await?.is_none() {
            return Err(ApiError::not_found("Workspace not found"));
        }
    }
    if let Some(agent_id) = target.agent_id {
        if state.store.get_agent(agent_id).await?.is_none() {
            return Err(ApiError::not_found("Agent not found"));
        }
    }
    Ok(())
}

async fn require_plan(state: &AppState, plan_id: &str) -> Result<McpPlan, ApiError> {
    state
        .store
        .get_mcp_plan(plan_id)
        .await?
        .ok_or_else(|| ApiError::not_found("MCP plan not found"))
}

async fn current_hashes(
    state: &AppState,
    plan: &McpPlan,
) -> Result<(String, String, String), ApiError> {
    let target = TargetInput {
        workspace_id: plan
            .target
            .workspace_id
            .as_deref()
            .map(Uuid::parse_str)
            .transpose()
            .map_err(|_| ApiError::bad_request("persisted MCP Workspace target is invalid"))?,
        agent_id: plan
            .target
            .agent_id
            .as_deref()
            .map(Uuid::parse_str)
            .transpose()
            .map_err(|_| ApiError::bad_request("persisted MCP Agent target is invalid"))?,
    };
    let effective = resolve_state(state, target).await?;
    let inventory = state.runtime.inspect_mcp().await?;
    let capabilities = state.runtime.inspect_mcp_capabilities().await?;
    Ok((
        hash_mcp_observation(&inventory),
        hash_mcp_config(&effective),
        capabilities.hash,
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum ApprovalStatus {
    Pending,
    Approved,
    Rejected,
    Stale,
    Expired,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct McpPlanView {
    plan: McpPlan,
    #[serde(skip_serializing_if = "Option::is_none")]
    decision: Option<McpPlanDecision>,
    approval_status: ApprovalStatus,
    stale_reasons: Vec<String>,
    approved_but_not_applied: bool,
}

impl McpPlanView {
    fn pending(plan: McpPlan) -> Self {
        Self {
            plan,
            decision: None,
            approval_status: ApprovalStatus::Pending,
            stale_reasons: Vec::new(),
            approved_but_not_applied: false,
        }
    }
}

async fn plan_view(state: &AppState, plan: McpPlan) -> Result<McpPlanView, ApiError> {
    let decision = state.store.get_mcp_plan_decision(&plan.id).await?;
    let Some(decision) = decision else {
        let (current_observation_hash, current_config_hash, current_capability_hash) =
            current_hashes(state, &plan).await?;
        let mut stale_reasons = Vec::new();
        if current_observation_hash != plan.observation_hash {
            stale_reasons.push("observation_drift".to_owned());
        }
        if current_config_hash != plan.config_hash {
            stale_reasons.push("desired_config_drift".to_owned());
        }
        if current_capability_hash != plan.capability_hash {
            stale_reasons.push("adapter_capability_or_version_drift".to_owned());
        }
        return Ok(McpPlanView {
            plan,
            decision: None,
            approval_status: if stale_reasons.is_empty() {
                ApprovalStatus::Pending
            } else {
                ApprovalStatus::Stale
            },
            stale_reasons,
            approved_but_not_applied: false,
        });
    };
    if decision.decision == McpPlanDecisionStatus::Rejected {
        return Ok(McpPlanView {
            plan,
            decision: Some(decision),
            approval_status: ApprovalStatus::Rejected,
            stale_reasons: Vec::new(),
            approved_but_not_applied: false,
        });
    }
    let mut stale_reasons = Vec::new();
    if decision.plan_hash != plan.plan_hash {
        stale_reasons.push("plan_hash_changed".to_owned());
    }
    if decision.observation_hash != plan.observation_hash {
        stale_reasons.push("approval_base_observation_mismatch".to_owned());
    }
    if decision.config_hash != plan.config_hash {
        stale_reasons.push("approval_base_config_mismatch".to_owned());
    }
    let (current_observation_hash, current_config_hash, current_capability_hash) =
        current_hashes(state, &plan).await?;
    if current_observation_hash != plan.observation_hash {
        stale_reasons.push("observation_drift".to_owned());
    }
    if current_config_hash != plan.config_hash {
        stale_reasons.push("desired_config_drift".to_owned());
    }
    if current_capability_hash != plan.capability_hash {
        stale_reasons.push("adapter_capability_or_version_drift".to_owned());
    }
    let expired = decision
        .expires_at
        .is_some_and(|expires_at| expires_at <= Utc::now());
    if expired {
        stale_reasons.push("approval_expired".to_owned());
    }
    stale_reasons.sort();
    stale_reasons.dedup();
    let approval_status = if expired {
        ApprovalStatus::Expired
    } else if stale_reasons.is_empty() {
        ApprovalStatus::Approved
    } else {
        ApprovalStatus::Stale
    };
    let approved_but_not_applied = matches!(approval_status, ApprovalStatus::Approved);
    Ok(McpPlanView {
        plan,
        decision: Some(decision),
        approval_status,
        stale_reasons,
        approved_but_not_applied,
    })
}

fn doctor_report(inventory: McpInventory) -> McpDoctorReport {
    let runtime_count = inventory
        .observations
        .iter()
        .map(|observation| observation.runtime.as_str())
        .chain(
            inventory
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.runtime.as_str()),
        )
        .filter(|runtime| !runtime.is_empty() && !matches!(*runtime, "aggregate" | "machine"))
        .collect::<BTreeSet<_>>()
        .len();
    let error_count = inventory
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == McpDiagnosticSeverity::Error)
        .count();
    let warning_count = inventory
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == McpDiagnosticSeverity::Warning)
        .count();
    let status = if error_count > 0 {
        "error"
    } else if warning_count > 0 {
        "warning"
    } else {
        "ok"
    };
    McpDoctorReport {
        summary: McpDoctorSummary {
            status: status.to_owned(),
            runtime_count,
            server_count: inventory.servers.len(),
            observation_count: inventory.observations.len(),
            diagnostic_count: inventory.diagnostics.len(),
            error_count,
            warning_count,
        },
        inventory,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cocli_driver_core::{McpDiagnostic, McpEvidence};

    #[test]
    fn doctor_summary_preserves_partial_runtime_failures() {
        let report = doctor_report(McpInventory {
            diagnostics: vec![
                McpDiagnostic {
                    code: "cli_missing".to_owned(),
                    severity: McpDiagnosticSeverity::Warning,
                    runtime: "cursor".to_owned(),
                    server_id: None,
                    message: "cursor MCP probe is unavailable".to_owned(),
                    evidence: vec![McpEvidence {
                        source: "cursor_cli".to_owned(),
                        detail: "binary was not discovered".to_owned(),
                        source_path: None,
                        proves_runtime_loaded: false,
                        proves_current_session_visibility: false,
                    }],
                    observed_at: "2026-07-19T00:00:00Z".to_owned(),
                },
                McpDiagnostic {
                    code: "mcp_duplicate_endpoint".to_owned(),
                    severity: McpDiagnosticSeverity::Info,
                    runtime: "machine".to_owned(),
                    server_id: None,
                    message: "duplicate endpoint".to_owned(),
                    evidence: Vec::new(),
                    observed_at: "2026-07-19T00:00:00Z".to_owned(),
                },
            ],
            ..McpInventory::default()
        });

        assert_eq!(report.summary.status, "warning");
        assert_eq!(report.summary.runtime_count, 1);
        assert_eq!(report.summary.warning_count, 1);
        assert_eq!(report.inventory.diagnostics[0].code, "cli_missing");
    }
}
