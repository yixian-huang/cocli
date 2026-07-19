use std::collections::BTreeSet;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use cocli_driver_core::{
    bind_mcp_plan_capabilities, generate_mcp_plan, hash_mcp_config, hash_mcp_observation,
    resolve_mcp_desired_state, McpApplyExecutionRequest, McpApplyExecutionResult,
    McpApplyJournalEntry, McpApplyJournalPhase, McpBindingTargetType, McpCapabilitySnapshot,
    McpDesiredTarget, McpDiagnosticSeverity, McpDoctorReport, McpDoctorSummary,
    McpEffectiveDesiredState, McpInventory, McpPlan, McpPreflightReport, McpProfile,
    McpProfileBinding, McpRiskLevel, McpRollbackExecutionRequest, McpSessionEffectiveStatus,
    McpVerificationResult, McpVerificationStatus,
};
use cocli_store::{
    McpApplyRun, McpApplyRunStatus, McpPlanDecision, McpPlanDecisionStatus, NewMcpApplyRun,
    NewMcpPlanDecision, NewMcpProfile, NewMcpProfileBinding, UpdateMcpProfile,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use super::{ApiError, AppState};

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/runtimes/mcp/inventory", get(machine_mcp_inventory))
        .route("/api/runtimes/mcp/doctor", get(machine_mcp_doctor))
        .route(
            "/api/runtimes/mcp/capabilities",
            get(machine_mcp_capabilities),
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
    if observation_hash != plan.observation_hash
        || config_hash != plan.config_hash
        || capability_hash != plan.capability_hash
    {
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
    if !preflight.stale_reasons.is_empty() {
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
    let execution = match state
        .runtime
        .apply_mcp(McpApplyExecutionRequest {
            run_id: run.id.to_string(),
            plan,
            actor: request.actor,
            confirm_high_risk: request.confirm_high_risk,
            capability_hash,
            resume_journal: run.journal.clone(),
        })
        .await
    {
        Ok(result) => result,
        Err(error) => failed_execution(&run, error),
    };
    checkpoint_execution(&state, run.id, &execution).await?;
    let run = state
        .store
        .complete_mcp_apply_run(run.id, &execution)
        .await?;
    Ok(Json(ApplyRunView { run }))
}

async fn checkpoint_execution(
    state: &AppState,
    run_id: Uuid,
    execution: &McpApplyExecutionResult,
) -> Result<(), ApiError> {
    for entry in &execution.journal {
        state
            .store
            .checkpoint_mcp_apply_run(run_id, entry.phase, entry, None, None)
            .await?;
    }
    Ok(())
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
            | McpApplyRunStatus::RollingBack
    ) {
        return Err(ApiError::conflict(
            "MCP apply run must finish before rollback",
        ));
    }
    if run.rollback_status == Some(McpApplyRunStatus::RolledBack) {
        return Ok(Json(ApplyRunView { run }));
    }
    let backups = run
        .actions
        .iter()
        .filter_map(|action| action.backup.clone())
        .collect::<Vec<_>>();
    if backups.is_empty() {
        return Err(ApiError::conflict("MCP apply run has no restorable backup"));
    }
    let actor = request.actor.unwrap_or_else(|| run.actor.clone());
    if actor.trim().is_empty() {
        return Err(ApiError::bad_request("rollback actor is required"));
    }
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
