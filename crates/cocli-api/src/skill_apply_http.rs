use std::path::{Path, PathBuf};

use axum::extract::{Path as AxumPath, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{Duration as ChronoDuration, Utc};
use cocli_store::{
    NewSkillGovernanceApplyAction, NewSkillGovernanceApplyRun, SkillGovernanceApplyAction,
    SkillGovernanceApplyActionStatus, SkillGovernanceApplyRun, SkillGovernanceApplyRunStatus,
    SkillGovernancePlan, SkillGovernancePlanStatus, SkillGovernanceRecoveryStatus,
    SkillGovernanceScope as StoreGovernanceScope,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::skill_apply::{
    activate_atomic_mutation, backup_atomic_mutation, fingerprint_path, is_safe_removal_candidate,
    load_local_artifact, load_vendored_artifact, prepare_atomic_mutation, rollback_atomic_mutation,
    stage_atomic_mutation, ArtifactBundle, MutationMode, MutationReceipt, PreparedMutation,
};
use crate::skill_governance::{
    canonical_hash, DryRunPlanPreview, EffectiveDesiredSkill, GovernanceScope, InstallationMode,
    PlanAction, PlanActionKind, PlanRisk, RiskPolicy,
};
use crate::skill_governance_http::{
    generate_lock_preview, load_effective_desired, normalized_scope_id, parse_plan_preview,
    require_plan, DesiredTarget, GovernancePreviewRequest,
};
use crate::{skill_http, ApiError, AppState};

const APPROVAL_TTL: ChronoDuration = ChronoDuration::minutes(15);
// Staging is bounded to 5,000 files / 50 MiB and fsyncs each file. Keep one
// lease comfortably above that bounded phase while still renewing at every
// journal boundary and allowing stale-owner recovery.
const LOCK_LEASE: ChronoDuration = ChronoDuration::minutes(15);

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/skills/governance/plans/:plan_id/apply/preview",
            post(preview_apply),
        )
        .route(
            "/api/skills/governance/plans/:plan_id/apply",
            post(apply_plan),
        )
        .route("/api/skills/governance/runs", get(list_runs))
        .route("/api/skills/governance/runs/:run_id", get(get_run))
        .route(
            "/api/skills/governance/runs/:run_id/verify",
            post(verify_run),
        )
        .route(
            "/api/skills/governance/runs/:run_id/rollback/preview",
            post(preview_rollback),
        )
        .route(
            "/api/skills/governance/runs/:run_id/rollback",
            post(rollback_run),
        )
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredApplyContext {
    scope: GovernanceScope,
    scope_id: String,
    workspace_id: Option<String>,
    agent_id: Option<String>,
}

#[derive(Clone, Debug)]
struct ApplyPreflight {
    plan: SkillGovernancePlan,
    preview: DryRunPlanPreview,
    context: StoredApplyContext,
    lock_snapshot_id: Uuid,
    stale_reasons: Vec<String>,
    decisions: Vec<ActionDecision>,
    high_risk: bool,
    confirmation_nonce: String,
    current_observation_hash: String,
    current_lock_hash: String,
    current_drift: Vec<crate::skill_governance::SkillDrift>,
}

#[derive(Clone, Debug)]
struct ActionDecision {
    action: PlanAction,
    supported: bool,
    reason: String,
}

struct PreparedActionMutation {
    mutation: PreparedMutation,
    artifact: Option<ArtifactBundle>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct RunEffect {
    kind: String,
    status: String,
    label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    created_id: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ApplyPreviewResponse {
    plan: SkillGovernancePlan,
    dry_run: bool,
    applied: bool,
    high_risk: bool,
    confirmation_required: bool,
    nonce_required: bool,
    confirmation_nonce: String,
    idempotency_key: String,
    recovery_required: bool,
    recovery_reasons: Vec<String>,
    lock_snapshot_id: Uuid,
    effects: Vec<RunEffect>,
    actions: Vec<PlanAction>,
    stale_reasons: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ApplyConfirmation {
    expected_version: i64,
    idempotency_key: String,
    #[serde(default)]
    confirmation_nonce: Option<String>,
    #[serde(default)]
    confirm_high_risk: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ApplyResponse {
    run: RunView,
    applied: bool,
    recovery_required: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct VerifyResponse {
    run: RunView,
    verified: bool,
    recovery_required: bool,
    reasons: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RollbackPreviewResponse {
    run: RunView,
    dry_run: bool,
    rollback_required: bool,
    confirmation_required: bool,
    confirmation_nonce: String,
    idempotency_key: String,
    effects: Vec<RunEffect>,
    actions: Vec<PlanAction>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RollbackConfirmation {
    idempotency_key: String,
    #[serde(default)]
    confirmation_nonce: Option<String>,
    #[serde(default)]
    confirm_rollback: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RollbackResponse {
    run: RunView,
    rolled_back: bool,
    recovery_required: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RunView {
    id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    plan_id: Option<Uuid>,
    scope: StoreGovernanceScope,
    scope_id: String,
    status: String,
    phase: String,
    progress: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
    dry_run: bool,
    applied: bool,
    high_risk: bool,
    recovery_required: bool,
    recovery_reasons: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    lock_snapshot_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    backup_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    quarantine_id: Option<String>,
    effects: Vec<RunEffect>,
    actions: Vec<PlanAction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    started_at: Option<chrono::DateTime<Utc>>,
    updated_at: chrono::DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    completed_at: Option<chrono::DateTime<Utc>>,
}

async fn preview_apply(
    State(state): State<AppState>,
    AxumPath(plan_id): AxumPath<Uuid>,
) -> Result<Json<ApplyPreviewResponse>, ApiError> {
    let preflight = apply_preflight(&state, plan_id).await?;
    Ok(Json(preflight_response(preflight)?))
}

async fn apply_plan(
    State(state): State<AppState>,
    AxumPath(plan_id): AxumPath<Uuid>,
    Json(request): Json<ApplyConfirmation>,
) -> Result<Json<ApplyResponse>, ApiError> {
    validate_idempotency_key(&request.idempotency_key)?;
    let stored_plan = require_plan(&state, plan_id).await?;
    let stored_context: StoredApplyContext = serde_json::from_value(stored_plan.plan.clone())
        .map_err(|_| ApiError::bad_request("stored governance apply context is invalid"))?;
    let stored_scope_id = normalized_scope_id(stored_context.scope, &stored_context.scope_id)?;
    if let Some(existing) = state
        .store
        .get_skill_governance_apply_run_by_idempotency(
            stored_context.scope.into(),
            &stored_scope_id,
            &request.idempotency_key,
        )
        .await?
    {
        let expected = parse_plan_preview(&stored_plan.plan)?;
        if existing.plan_id != Some(plan_id)
            || existing.observation_hash != expected.content.observation_hash
            || existing.desired_hash != expected.content.desired_config_hash
            || existing.lock_hash != expected.content.lockfile_hash
            || request
                .confirmation_nonce
                .as_deref()
                .is_some_and(|nonce| nonce != existing.nonce)
        {
            return Err(ApiError::conflict(
                "idempotency key was already used for a different apply request",
            ));
        }
        let existing = recover_interrupted_run(&state, existing).await?;
        let view = run_view(&state, existing).await?;
        return Ok(Json(ApplyResponse {
            applied: view.applied,
            recovery_required: view.recovery_required,
            run: view,
        }));
    }
    let preflight = apply_preflight(&state, plan_id).await?;
    if preflight.plan.version != request.expected_version {
        return Err(ApiError::conflict(
            "governance plan version changed before apply",
        ));
    }
    ensure_apply_eligible(&preflight, &request)?;
    let scope_id = normalized_scope_id(preflight.context.scope, &preflight.context.scope_id)?;
    let lease_nonce = Uuid::new_v4().to_string();
    let acquired = state
        .store
        .acquire_skill_governance_lock(
            preflight.context.scope.into(),
            &scope_id,
            &format!("apply:{}", request.idempotency_key),
            Some(i64::from(std::process::id())),
            None,
            &lease_nonce,
            Utc::now() + LOCK_LEASE,
        )
        .await?;
    let run = state
        .store
        .create_skill_governance_apply_run(NewSkillGovernanceApplyRun {
            scope: preflight.context.scope.into(),
            scope_id,
            plan_id: Some(preflight.plan.id),
            lock_id: Some(acquired.lock.id),
            idempotency_key: request.idempotency_key.clone(),
            nonce: request
                .confirmation_nonce
                .clone()
                .unwrap_or_else(|| preflight.confirmation_nonce.clone()),
            observation_hash: preflight.preview.content.observation_hash.clone(),
            desired_hash: preflight.preview.content.desired_config_hash.clone(),
            lock_hash: preflight.preview.content.lockfile_hash.clone(),
            backup_path: None,
            quarantine_path: None,
            evidence: run_evidence(
                "preflight",
                false,
                preflight.high_risk,
                &preflight.preview.content.actions,
                Vec::new(),
                Vec::new(),
                Some("approved apply preflight passed"),
            ),
        })
        .await;
    let run = match run {
        Ok(run) => run,
        Err(error) => {
            release_governance_lock(&state, acquired.lock.id, &lease_nonce).await;
            return Err(ApiError::from(error));
        }
    };

    if let Err(error) = state
        .store
        .attach_skill_governance_lock_run(
            acquired.lock.id,
            acquired.lock.version,
            &lease_nonce,
            run.id,
            Utc::now() + LOCK_LEASE,
        )
        .await
    {
        release_governance_lock(&state, acquired.lock.id, &lease_nonce).await;
        return Err(ApiError::from(error));
    }

    let result = execute_apply(&state, run, &preflight, &lease_nonce).await;
    release_governance_lock(&state, acquired.lock.id, &lease_nonce).await;
    let run = result?;
    let view = run_view(&state, run).await?;
    Ok(Json(ApplyResponse {
        applied: view.applied,
        recovery_required: view.recovery_required,
        run: view,
    }))
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunsQuery {
    scope: Option<GovernanceScope>,
    scope_id: Option<String>,
}

async fn list_runs(
    State(state): State<AppState>,
    Query(query): Query<RunsQuery>,
) -> Result<Json<Vec<RunView>>, ApiError> {
    let (scope, scope_id) = match (query.scope, query.scope_id.as_deref()) {
        (Some(scope), Some(scope_id)) => (scope, normalized_scope_id(scope, scope_id)?),
        _ => return Ok(Json(Vec::new())),
    };
    let runs = state
        .store
        .list_skill_governance_apply_runs(scope.into(), &scope_id)
        .await?;
    let mut views = Vec::with_capacity(runs.len());
    for run in runs {
        let run = recover_interrupted_run(&state, run).await?;
        views.push(run_view(&state, run).await?);
    }
    Ok(Json(views))
}

async fn get_run(
    State(state): State<AppState>,
    AxumPath(run_id): AxumPath<Uuid>,
) -> Result<Json<RunView>, ApiError> {
    let run = require_run(&state, run_id).await?;
    Ok(Json(run_view(&state, run).await?))
}

async fn verify_run(
    State(state): State<AppState>,
    AxumPath(run_id): AxumPath<Uuid>,
) -> Result<Json<VerifyResponse>, ApiError> {
    let run = require_run(&state, run_id).await?;
    let (verified, reasons) = verify_receipts(&state, &run).await?;
    let recovery_required = !verified;
    let run = if recovery_required {
        state
            .store
            .transition_skill_governance_apply_run(
                run.id,
                run.version,
                SkillGovernanceApplyRunStatus::RecoveryRequired,
                SkillGovernanceRecoveryStatus::Pending,
                None,
                None,
                merge_run_evidence(
                    &run,
                    "recovery",
                    run_evidence_applied(&run),
                    &reasons,
                    "verify mismatch",
                ),
                Some("governed Skill verification mismatch"),
            )
            .await?
    } else {
        run
    };
    Ok(Json(VerifyResponse {
        run: run_view(&state, run).await?,
        verified,
        recovery_required,
        reasons,
    }))
}

async fn preview_rollback(
    State(state): State<AppState>,
    AxumPath(run_id): AxumPath<Uuid>,
) -> Result<Json<RollbackPreviewResponse>, ApiError> {
    let run = require_run(&state, run_id).await?;
    let idempotency_key = format!("rollback-{}", Uuid::new_v4());
    let confirmation_nonce = rollback_nonce(&run, &idempotency_key)?;
    let view = run_view(&state, run).await?;
    let rollback_required = view.applied || view.recovery_required;
    Ok(Json(RollbackPreviewResponse {
        effects: vec![RunEffect {
            kind: "rollback".to_owned(),
            status: if rollback_required {
                "pending"
            } else {
                "skipped"
            }
            .to_owned(),
            label: "CAS-safe compensating rollback".to_owned(),
            detail: Some(
                "restores backups only when post-apply fingerprints still match".to_owned(),
            ),
            created_id: None,
        }],
        actions: view.actions.clone(),
        run: view,
        dry_run: true,
        rollback_required,
        confirmation_required: rollback_required,
        confirmation_nonce,
        idempotency_key,
    }))
}

async fn rollback_run(
    State(state): State<AppState>,
    AxumPath(run_id): AxumPath<Uuid>,
    Json(request): Json<RollbackConfirmation>,
) -> Result<Json<RollbackResponse>, ApiError> {
    validate_idempotency_key(&request.idempotency_key)?;
    if !request.confirm_rollback {
        return Err(ApiError::conflict(
            "rollback requires explicit confirmation",
        ));
    }
    let run = require_run(&state, run_id).await?;
    if run.status == SkillGovernanceApplyRunStatus::RolledBack {
        return Ok(Json(RollbackResponse {
            run: run_view(&state, run).await?,
            rolled_back: true,
            recovery_required: false,
        }));
    }
    let expected_nonce = rollback_nonce(&run, &request.idempotency_key)?;
    if request.confirmation_nonce.as_deref() != Some(expected_nonce.as_str()) {
        return Err(ApiError::conflict(
            "rollback confirmation nonce is invalid or stale",
        ));
    }
    let lease_nonce = Uuid::new_v4().to_string();
    let acquired = state
        .store
        .acquire_skill_governance_lock(
            run.scope,
            &run.scope_id,
            &format!("rollback:{}", request.idempotency_key),
            Some(i64::from(std::process::id())),
            Some(run.id),
            &lease_nonce,
            Utc::now() + LOCK_LEASE,
        )
        .await?;
    let current = state
        .store
        .transition_skill_governance_apply_run(
            run.id,
            run.version,
            SkillGovernanceApplyRunStatus::RollingBack,
            SkillGovernanceRecoveryStatus::InProgress,
            None,
            None,
            merge_run_evidence(&run, "rollback", false, &[], "rollback started"),
            None,
        )
        .await;
    let mut current = match current {
        Ok(run) => run,
        Err(error) => {
            release_governance_lock(&state, acquired.lock.id, &lease_nonce).await;
            return Err(ApiError::from(error));
        }
    };
    let rollback_result = rollback_actions(&state, &current, acquired.lock.id, &lease_nonce).await;
    current = match rollback_result {
        Ok(()) => {
            state
                .store
                .transition_skill_governance_apply_run(
                    current.id,
                    current.version,
                    SkillGovernanceApplyRunStatus::RolledBack,
                    SkillGovernanceRecoveryStatus::Recovered,
                    None,
                    None,
                    merge_run_evidence(&current, "rollback", false, &[], "rollback verified"),
                    None,
                )
                .await?
        }
        Err(error) => {
            state
                .store
                .transition_skill_governance_apply_run(
                    current.id,
                    current.version,
                    SkillGovernanceApplyRunStatus::RecoveryRequired,
                    SkillGovernanceRecoveryStatus::Failed,
                    None,
                    None,
                    merge_run_evidence(
                        &current,
                        "recovery",
                        false,
                        &[error.clone()],
                        "rollback requires manual recovery",
                    ),
                    Some(&error),
                )
                .await?
        }
    };
    release_governance_lock(&state, acquired.lock.id, &lease_nonce).await;
    invalidate_run_snapshot(&state, &current).await;
    let view = run_view(&state, current).await?;
    Ok(Json(RollbackResponse {
        rolled_back: view.status == "rolled_back",
        recovery_required: view.recovery_required,
        run: view,
    }))
}

async fn apply_preflight(state: &AppState, plan_id: Uuid) -> Result<ApplyPreflight, ApiError> {
    let plan = require_plan(state, plan_id).await?;
    if plan.status != SkillGovernancePlanStatus::Approved {
        return Err(ApiError::conflict(
            "only approved governance plans can be applied",
        ));
    }
    let context: StoredApplyContext = serde_json::from_value(plan.plan.clone())
        .map_err(|_| ApiError::bad_request("stored governance apply context is invalid"))?;
    let expected = parse_plan_preview(&plan.plan)?;
    let request = GovernancePreviewRequest {
        scope: context.scope,
        scope_id: context.scope_id.clone(),
        workspace_id: context.workspace_id.clone(),
        agent_id: context.agent_id.clone(),
        force: true,
    };
    let current = generate_lock_preview(state, &request).await?;
    let mut stale_reasons = crate::skill_governance::stale_plan_reasons(
        &expected.content,
        &current.preview.snapshot_hash,
        &current.preview.desired_config_hash,
        &current.preview.lockfile_hash,
    );
    if Utc::now() - plan.updated_at > APPROVAL_TTL {
        stale_reasons.push("approval_expired".to_owned());
    }
    let effective = load_effective_desired(state, &request.desired_target()).await?;
    let decisions = expected
        .content
        .actions
        .iter()
        .cloned()
        .map(|action| action_decision(&action, &effective.skills, context.scope))
        .collect::<Vec<_>>();
    let high_risk = expected
        .content
        .actions
        .iter()
        .any(|action| action.risk == PlanRisk::High || action.approval_required);
    let confirmation_nonce = canonical_hash(&(
        "skill-governance-apply",
        plan.id,
        plan.version,
        &expected.plan_hash,
        &current.preview.lockfile_hash,
    ))
    .map_err(ApiError::bad_request)?;
    Ok(ApplyPreflight {
        plan,
        preview: expected,
        context,
        lock_snapshot_id: current.snapshot.id,
        stale_reasons,
        decisions,
        high_risk,
        confirmation_nonce,
        current_observation_hash: current.preview.snapshot_hash,
        current_lock_hash: current.preview.lockfile_hash,
        current_drift: current.drift,
    })
}

fn preflight_response(preflight: ApplyPreflight) -> Result<ApplyPreviewResponse, ApiError> {
    let blocked = preflight
        .decisions
        .iter()
        .filter(|decision| !decision.supported)
        .map(|decision| decision.reason.clone())
        .collect::<Vec<_>>();
    let effects = preflight_effects(&preflight.decisions, preflight.lock_snapshot_id);
    let actions = preflight.preview.content.actions.clone();
    let idempotency_key = format!("apply-{}", Uuid::new_v4());
    let confirmation_nonce = apply_confirmation_nonce(&preflight, &idempotency_key)?;
    Ok(ApplyPreviewResponse {
        idempotency_key,
        confirmation_required: preflight.high_risk,
        nonce_required: preflight.high_risk,
        confirmation_nonce,
        recovery_required: false,
        recovery_reasons: blocked,
        lock_snapshot_id: preflight.lock_snapshot_id,
        stale_reasons: preflight.stale_reasons,
        high_risk: preflight.high_risk,
        effects,
        actions,
        plan: preflight.plan,
        dry_run: true,
        applied: false,
    })
}

fn ensure_apply_eligible(
    preflight: &ApplyPreflight,
    request: &ApplyConfirmation,
) -> Result<(), ApiError> {
    if !preflight.stale_reasons.is_empty() {
        return Err(ApiError::json(
            StatusCode::CONFLICT,
            json!({
                "error": "approved governance plan is stale",
                "staleReasons": preflight.stale_reasons,
                "expectedObservationHash": preflight.preview.content.observation_hash,
                "actualObservationHash": preflight.current_observation_hash,
                "expectedLockHash": preflight.preview.content.lockfile_hash,
                "actualLockHash": preflight.current_lock_hash,
                "currentDrift": preflight.current_drift,
            }),
        ));
    }
    let blocked = preflight
        .decisions
        .iter()
        .filter(|decision| !decision.supported)
        .map(|decision| json!({"target": decision.action.target, "reason": decision.reason}))
        .collect::<Vec<_>>();
    if !blocked.is_empty() {
        return Err(ApiError::json(
            StatusCode::UNPROCESSABLE_ENTITY,
            json!({"error": "governance plan contains manual or unsupported actions", "blocked": blocked}),
        ));
    }
    let expected_nonce = apply_confirmation_nonce(preflight, &request.idempotency_key)?;
    if preflight.high_risk
        && (!request.confirm_high_risk
            || request.confirmation_nonce.as_deref() != Some(expected_nonce.as_str()))
    {
        return Err(ApiError::conflict(
            "high-risk governance apply requires the current confirmation nonce",
        ));
    }
    Ok(())
}

fn action_decision(
    action: &PlanAction,
    desired: &[EffectiveDesiredSkill],
    scope: GovernanceScope,
) -> ActionDecision {
    let blocked = |reason: &str| ActionDecision {
        action: action.clone(),
        supported: false,
        reason: reason.to_owned(),
    };
    if action.blocked
        || matches!(
            action.action,
            PlanActionKind::Manual | PlanActionKind::Unsupported
        )
    {
        return blocked("action is blocked by unknown or unsupported evidence");
    }
    if scope != GovernanceScope::Agent {
        return blocked("automatic Phase 3B writes are limited to canonical Agent workspace roots");
    }
    if matches!(
        action.action,
        PlanActionKind::Enable | PlanActionKind::Disable
    ) {
        return blocked("Runtime-neutral enable/disable has no native-safe write contract");
    }
    if action.action == PlanActionKind::LockfileUpdate {
        return ActionDecision {
            action: action.clone(),
            supported: true,
            reason: "immutable Store lock snapshot is already available".to_owned(),
        };
    }
    if action.action == PlanActionKind::Remove {
        return ActionDecision {
            action: action.clone(),
            supported: true,
            reason: "remove uses same-filesystem quarantine and CAS rollback".to_owned(),
        };
    }
    let Some(desired) = desired.iter().find(|skill| {
        skill
            .desired
            .target_runtime
            .eq_ignore_ascii_case(&action.runtime)
            && skill.desired.install_scope == action.scope
            && normalize_name(&skill.desired.logical_identity) == normalize_name(&action.target)
    }) else {
        return blocked("desired artifact could not be resolved for this action");
    };
    if desired.desired.source.credential_ref.is_some() {
        return blocked("private source credentials are opaque and manual in Phase 3B");
    }
    if !matches!(
        desired.desired.risk_policy,
        RiskPolicy::Trusted | RiskPolicy::Allowlisted
    ) {
        return blocked("desired artifact risk policy does not permit automatic apply");
    }
    let source_allowlisted = desired
        .desired
        .allowed_sources
        .iter()
        .any(|allowed| allowed.eq_ignore_ascii_case(&desired.desired.source.kind));
    if (desired.desired.risk_policy == RiskPolicy::Allowlisted
        || !desired.desired.allowed_sources.is_empty())
        && !source_allowlisted
    {
        return blocked("desired artifact source is outside its explicit allowlist");
    }
    if !matches!(
        desired.desired.installation_mode,
        InstallationMode::Copy | InstallationMode::Symlink
    ) {
        return blocked("installation mode has no native-safe Phase 3B mutation contract");
    }
    let kind = desired.desired.source.kind.to_ascii_lowercase();
    if !matches!(kind.as_str(), "local" | "cocli" | "library" | "vendored") {
        return blocked("remote, script, Registry, Marketplace, and Git sources remain manual");
    }
    ActionDecision {
        action: action.clone(),
        supported: true,
        reason: "trusted local or cocli-vendored artifact is digest-verifiable".to_owned(),
    }
}

async fn execute_apply(
    state: &AppState,
    run: SkillGovernanceApplyRun,
    preflight: &ApplyPreflight,
    lease_nonce: &str,
) -> Result<SkillGovernanceApplyRun, ApiError> {
    let mut run = state
        .store
        .transition_skill_governance_apply_run(
            run.id,
            run.version,
            SkillGovernanceApplyRunStatus::Running,
            SkillGovernanceRecoveryStatus::NotRequired,
            None,
            None,
            run_evidence(
                "lock",
                false,
                preflight.high_risk,
                &preflight.preview.content.actions,
                Vec::new(),
                Vec::new(),
                Some("scoped lease acquired"),
            ),
            None,
        )
        .await?;
    let target_agent = require_target_agent(state, &preflight.context).await?;
    let effective = load_effective_desired(
        state,
        &DesiredTarget {
            workspace_id: preflight.context.workspace_id.clone(),
            agent_id: preflight.context.agent_id.clone(),
        },
    )
    .await?;
    let mut receipts = Vec::new();
    let mut effects = vec![RunEffect {
        kind: "lock".to_owned(),
        status: "succeeded".to_owned(),
        label: "Scoped apply lease acquired".to_owned(),
        detail: None,
        created_id: run.lock_id.map(|id| id.to_string()),
    }];

    for (sequence, decision) in preflight.decisions.iter().enumerate() {
        let request_hash = canonical_hash(&decision.action).map_err(ApiError::bad_request)?;
        let action_row = state
            .store
            .create_skill_governance_apply_action(NewSkillGovernanceApplyAction {
                run_id: run.id,
                sequence: sequence as i64,
                action_key: format!(
                    "{}:{}:{}",
                    sequence, decision.action.runtime, decision.action.target
                ),
                request_hash,
                backup_path: None,
                quarantine_path: None,
                evidence: json!({"phase": "preflight", "reason": decision.reason}),
            })
            .await?;
        let action_row = state
            .store
            .transition_skill_governance_apply_action(
                action_row.id,
                action_row.version,
                SkillGovernanceApplyActionStatus::Preflight,
                None,
                None,
                None,
                json!({"phase": "preflight", "reason": decision.reason}),
                None,
            )
            .await?;
        let action_result = execute_journaled_action(
            state,
            run.id,
            action_row,
            &decision.action,
            &effective.skills,
            target_agent.as_ref(),
            &preflight.preview.content.lockfile_hash,
            run.lock_id,
            lease_nonce,
        )
        .await;
        match action_result {
            Ok(Some(receipt)) => {
                effects.push(effect_from_receipt(&receipt));
                receipts.push(receipt);
            }
            Ok(None) => {
                effects.push(RunEffect {
                    kind: "apply".to_owned(),
                    status: "succeeded".to_owned(),
                    label: "Lock snapshot committed in the apply journal".to_owned(),
                    detail: Some("no workspace lockfile was written".to_owned()),
                    created_id: Some(preflight.lock_snapshot_id.to_string()),
                });
            }
            Err(error) => {
                let recovery_errors =
                    compensate_journaled_actions(state, run.id, run.lock_id, Some(lease_nonce))
                        .await;
                let recovery_required = !recovery_errors.is_empty();
                return state
                    .store
                    .transition_skill_governance_apply_run(
                        run.id,
                        run.version,
                        if recovery_required {
                            SkillGovernanceApplyRunStatus::RecoveryRequired
                        } else {
                            SkillGovernanceApplyRunStatus::RolledBack
                        },
                        if recovery_required {
                            SkillGovernanceRecoveryStatus::Failed
                        } else {
                            SkillGovernanceRecoveryStatus::Recovered
                        },
                        receipts
                            .first()
                            .and_then(|receipt| receipt.backup_ref.as_deref()),
                        receipts
                            .first()
                            .and_then(|receipt| receipt.quarantine_ref.as_deref()),
                        run_evidence(
                            if recovery_required {
                                "recovery"
                            } else {
                                "rollback"
                            },
                            recovery_required,
                            preflight.high_risk,
                            &preflight.preview.content.actions,
                            effects,
                            recovery_errors,
                            Some("apply failed; compensating rollback attempted"),
                        ),
                        Some(&error),
                    )
                    .await
                    .map_err(ApiError::from);
            }
        }
    }

    invalidate_run_snapshot(state, &run).await;
    let (verified, reasons) = verify_receipt_values(state, &run, &receipts).await?;
    finalize_action_verification(state, run.id, verified, &reasons).await?;
    effects.push(RunEffect {
        kind: "verify".to_owned(),
        status: if verified { "succeeded" } else { "failed" }.to_owned(),
        label: if verified {
            "Verified on disk; new Session may be required"
        } else {
            "Post-apply verification mismatch"
        }
        .to_owned(),
        detail: Some("session-effective remains unknown without session-bound evidence".to_owned()),
        created_id: None,
    });
    run = state
        .store
        .transition_skill_governance_apply_run(
            run.id,
            run.version,
            if verified {
                SkillGovernanceApplyRunStatus::Succeeded
            } else {
                SkillGovernanceApplyRunStatus::RecoveryRequired
            },
            if verified {
                SkillGovernanceRecoveryStatus::NotRequired
            } else {
                SkillGovernanceRecoveryStatus::Pending
            },
            receipts
                .first()
                .and_then(|receipt| receipt.backup_ref.as_deref()),
            receipts
                .first()
                .and_then(|receipt| receipt.quarantine_ref.as_deref()),
            run_evidence(
                if verified { "verify" } else { "recovery" },
                !receipts.is_empty(),
                preflight.high_risk,
                &preflight.preview.content.actions,
                effects,
                reasons,
                Some(if verified {
                    "filesystem apply verified; session-effective unknown"
                } else {
                    "verification mismatch requires rollback or recovery"
                }),
            ),
            (!verified).then_some("post-apply verification mismatch"),
        )
        .await?;
    Ok(run)
}

async fn prepare_action_mutation(
    state: &AppState,
    run_id: Uuid,
    action_id: Uuid,
    action: &PlanAction,
    desired: &[EffectiveDesiredSkill],
    agent: Option<&cocli_store::Agent>,
) -> Result<PreparedActionMutation, String> {
    let agent = agent.ok_or_else(|| "automatic apply requires an Agent target".to_owned())?;
    if !agent.runtime.eq_ignore_ascii_case(&action.runtime) {
        return Err("plan Runtime does not match the target Agent Runtime".to_owned());
    }
    let target = state
        .runtime
        .governance_skill_target(agent, &action.target)
        .await
        .map_err(|error| format!("Runtime target resolution failed: {error}"))?;
    let (mode, artifact) = if action.action == PlanActionKind::Remove {
        let removal_target = target.entry_path.clone();
        let safe = tokio::task::spawn_blocking(move || is_safe_removal_candidate(&removal_target))
            .await
            .map_err(|_| "governed Skill removal preflight task failed".to_owned())??;
        if !safe {
            return Err(
                "automatic removal is limited to cocli-managed entries or symbolic links"
                    .to_owned(),
            );
        }
        (MutationMode::Remove, None)
    } else {
        let desired = desired
            .iter()
            .find(|skill| {
                skill
                    .desired
                    .target_runtime
                    .eq_ignore_ascii_case(&action.runtime)
                    && normalize_name(&skill.desired.logical_identity)
                        == normalize_name(&action.target)
            })
            .ok_or_else(|| "desired artifact is unavailable during apply".to_owned())?;
        let artifact = resolve_artifact(state, desired).await?;
        if artifact.content_digest != desired.desired.content_digest
            || artifact.manifest_digest != desired.desired.manifest_digest
        {
            return Err("trusted artifact digest does not match desired state".to_owned());
        }
        let mode = match desired.desired.installation_mode {
            InstallationMode::Copy => MutationMode::Copy,
            InstallationMode::Symlink => MutationMode::Symlink,
            InstallationMode::Native | InstallationMode::Manual => {
                return Err("installation mode is not automatically supported".to_owned())
            }
        };
        (mode, Some(artifact))
    };
    let artifact_for_prepare = artifact.clone();
    let mutation = tokio::task::spawn_blocking(move || {
        prepare_atomic_mutation(
            &target,
            mode,
            artifact_for_prepare.as_ref(),
            run_id,
            action_id,
        )
    })
    .await
    .map_err(|_| "governed Skill preparation task failed".to_owned())??;
    Ok(PreparedActionMutation { mutation, artifact })
}

#[allow(clippy::too_many_arguments)]
async fn execute_journaled_action(
    state: &AppState,
    run_id: Uuid,
    mut action_row: SkillGovernanceApplyAction,
    action: &PlanAction,
    desired: &[EffectiveDesiredSkill],
    agent: Option<&cocli_store::Agent>,
    lock_hash: &str,
    lock_id: Option<Uuid>,
    lease_nonce: &str,
) -> Result<Option<MutationReceipt>, String> {
    renew_apply_lease(state, lock_id, lease_nonce).await?;
    if action.action == PlanActionKind::LockfileUpdate {
        action_row = state
            .store
            .transition_skill_governance_apply_action(
                action_row.id,
                action_row.version,
                SkillGovernanceApplyActionStatus::Locked,
                None,
                None,
                None,
                json!({"phase": "locked", "storeOnly": true}),
                None,
            )
            .await
            .map_err(|_| "persist lockfile action lock boundary".to_owned())?;
        state
            .store
            .transition_skill_governance_apply_action(
                action_row.id,
                action_row.version,
                SkillGovernanceApplyActionStatus::LockfileWritten,
                Some(lock_hash),
                None,
                None,
                json!({"phase": "lockfile_written", "storeOnly": true}),
                None,
            )
            .await
            .map_err(|_| "persist lockfile journal boundary".to_owned())?;
        return Ok(None);
    }

    let prepared =
        match prepare_action_mutation(state, run_id, action_row.id, action, desired, agent).await {
            Ok(prepared) => prepared,
            Err(error) => {
                fail_journal_action(state, action_row, None, &error).await;
                return Err(error);
            }
        };
    let receipt = prepared.mutation.receipt().clone();
    action_row = transition_action_receipt(
        state,
        action_row,
        SkillGovernanceApplyActionStatus::Locked,
        &receipt,
        None,
        "locked",
    )
    .await
    .map_err(|_| "persist prepared mutation receipt".to_owned())?;

    renew_apply_lease(state, lock_id, lease_nonce).await?;
    let mutation = prepared.mutation.clone();
    if let Err(error) = tokio::task::spawn_blocking(move || backup_atomic_mutation(&mutation))
        .await
        .map_err(|_| "governed Skill backup task failed".to_owned())?
    {
        fail_journal_action(state, action_row, Some(&receipt), &error).await;
        return Err(error);
    }
    if receipt.backup_ref.is_some() || receipt.quarantine_ref.is_some() {
        action_row = transition_action_receipt(
            state,
            action_row,
            SkillGovernanceApplyActionStatus::BackedUp,
            &receipt,
            None,
            "backed_up",
        )
        .await
        .map_err(|_| "persist backup journal boundary".to_owned())?;
    }

    renew_apply_lease(state, lock_id, lease_nonce).await?;
    let mutation = prepared.mutation.clone();
    let artifact = prepared.artifact.clone();
    if let Err(error) =
        tokio::task::spawn_blocking(move || stage_atomic_mutation(&mutation, artifact.as_ref()))
            .await
            .map_err(|_| "governed Skill staging task failed".to_owned())?
    {
        fail_journal_action(state, action_row, Some(&receipt), &error).await;
        return Err(error);
    }
    if receipt.staging_ref.is_some() {
        action_row = transition_action_receipt(
            state,
            action_row,
            SkillGovernanceApplyActionStatus::Staged,
            &receipt,
            None,
            "staged",
        )
        .await
        .map_err(|_| "persist staging journal boundary".to_owned())?;
    }

    renew_apply_lease(state, lock_id, lease_nonce).await?;
    let mutation = prepared.mutation;
    if let Err(error) = tokio::task::spawn_blocking(move || activate_atomic_mutation(&mutation))
        .await
        .map_err(|_| "governed Skill activation task failed".to_owned())?
    {
        fail_journal_action(state, action_row, Some(&receipt), &error).await;
        return Err(error);
    }
    transition_action_receipt(
        state,
        action_row,
        SkillGovernanceApplyActionStatus::Written,
        &receipt,
        Some(&receipt.after_fingerprint),
        "written",
    )
    .await
    .map_err(|_| "persist written journal boundary".to_owned())?;
    Ok(Some(receipt))
}

async fn fail_journal_action(
    state: &AppState,
    action: SkillGovernanceApplyAction,
    receipt: Option<&MutationReceipt>,
    error: &str,
) {
    let evidence = receipt.map_or_else(
        || json!({"phase": "failed", "errorType": "safe_apply_error"}),
        |receipt| json!({"phase": "failed", "errorType": "safe_apply_error", "receipt": receipt}),
    );
    let _ = state
        .store
        .transition_skill_governance_apply_action(
            action.id,
            action.version,
            SkillGovernanceApplyActionStatus::Failed,
            action.result_hash.as_deref(),
            receipt.and_then(|receipt| receipt.backup_ref.as_deref()),
            receipt.and_then(|receipt| receipt.quarantine_ref.as_deref()),
            evidence,
            Some(error),
        )
        .await;
}

async fn resolve_artifact(
    state: &AppState,
    desired: &EffectiveDesiredSkill,
) -> Result<ArtifactBundle, String> {
    let source = &desired.desired.source;
    if source.credential_ref.is_some() {
        return Err("private source credentials remain manual".to_owned());
    }
    match source.kind.to_ascii_lowercase().as_str() {
        "local" => {
            let path = PathBuf::from(&source.location);
            if !path.is_absolute() {
                return Err("local governance source must be an absolute path".to_owned());
            }
            tokio::task::spawn_blocking(move || load_local_artifact(&path))
                .await
                .map_err(|_| "local artifact verification task failed".to_owned())?
        }
        "cocli" | "library" | "vendored" => {
            let entry = if let Ok(id) = Uuid::parse_str(&source.location) {
                state
                    .store
                    .get_skill_library(id)
                    .await
                    .map_err(|_| "load vendored artifact metadata".to_owned())?
            } else {
                state
                    .store
                    .get_skill_library_by_name(&source.location)
                    .await
                    .map_err(|_| "load vendored artifact metadata".to_owned())?
            }
            .ok_or_else(|| "cocli-vendored artifact was not found".to_owned())?;
            let files = state
                .store
                .load_skill_library_files(entry.id)
                .await
                .map_err(|_| "load vendored artifact files".to_owned())?;
            load_vendored_artifact(&files)
        }
        _ => Err(
            "remote, private, script, Registry, Marketplace, and Git sources are blocked"
                .to_owned(),
        ),
    }
}

async fn require_target_agent(
    state: &AppState,
    context: &StoredApplyContext,
) -> Result<Option<cocli_store::Agent>, ApiError> {
    if context.scope != GovernanceScope::Agent {
        return Ok(None);
    }
    let id = context
        .agent_id
        .as_deref()
        .unwrap_or(&context.scope_id)
        .parse::<Uuid>()
        .map_err(|_| ApiError::bad_request("governance Agent scope id is invalid"))?;
    state
        .store
        .get_agent(id)
        .await?
        .ok_or_else(|| ApiError::not_found("governance target Agent not found"))
        .map(Some)
}

async fn transition_action_receipt(
    state: &AppState,
    action: SkillGovernanceApplyAction,
    status: SkillGovernanceApplyActionStatus,
    receipt: &MutationReceipt,
    result_hash: Option<&str>,
    phase: &str,
) -> Result<SkillGovernanceApplyAction, ApiError> {
    state
        .store
        .transition_skill_governance_apply_action(
            action.id,
            action.version,
            status,
            result_hash,
            receipt.backup_ref.as_deref(),
            receipt.quarantine_ref.as_deref(),
            json!({"phase": phase, "receipt": receipt}),
            None,
        )
        .await
        .map_err(ApiError::from)
}

async fn finalize_action_verification(
    state: &AppState,
    run_id: Uuid,
    verified: bool,
    reasons: &[String],
) -> Result<(), ApiError> {
    let actions = state
        .store
        .list_skill_governance_apply_actions(run_id)
        .await?;
    for action in actions {
        if !matches!(
            action.status,
            SkillGovernanceApplyActionStatus::Written
                | SkillGovernanceApplyActionStatus::LockfileWritten
        ) {
            continue;
        }
        let receipt = receipt_value(&action);
        let evidence = receipt.as_ref().map_or_else(
            || json!({"phase": "refreshing", "storeOnly": true}),
            |receipt| json!({"phase": "refreshing", "receipt": receipt}),
        );
        let refreshing = state
            .store
            .transition_skill_governance_apply_action(
                action.id,
                action.version,
                SkillGovernanceApplyActionStatus::Refreshing,
                action.result_hash.as_deref(),
                action.backup_path.as_deref(),
                action.quarantine_path.as_deref(),
                evidence,
                None,
            )
            .await?;
        let evidence = receipt.as_ref().map_or_else(
            || json!({"phase": if verified { "verified" } else { "recovery_required" }, "storeOnly": true, "reasons": reasons}),
            |receipt| json!({"phase": if verified { "verified" } else { "recovery_required" }, "receipt": receipt, "reasons": reasons}),
        );
        state
            .store
            .transition_skill_governance_apply_action(
                refreshing.id,
                refreshing.version,
                if verified {
                    SkillGovernanceApplyActionStatus::Verified
                } else {
                    SkillGovernanceApplyActionStatus::RecoveryRequired
                },
                refreshing.result_hash.as_deref(),
                refreshing.backup_path.as_deref(),
                refreshing.quarantine_path.as_deref(),
                evidence,
                (!verified).then_some("post-apply verification mismatch"),
            )
            .await?;
    }
    Ok(())
}

async fn verify_receipts(
    state: &AppState,
    run: &SkillGovernanceApplyRun,
) -> Result<(bool, Vec<String>), ApiError> {
    let actions = state
        .store
        .list_skill_governance_apply_actions(run.id)
        .await?;
    let receipts = receipt_values(&actions);
    verify_receipt_values(state, run, &receipts).await
}

async fn verify_receipt_values(
    state: &AppState,
    run: &SkillGovernanceApplyRun,
    receipts: &[MutationReceipt],
) -> Result<(bool, Vec<String>), ApiError> {
    let mut reasons = Vec::new();
    for receipt in receipts {
        let receipt = receipt.clone();
        let actual =
            tokio::task::spawn_blocking(move || fingerprint_path(Path::new(&receipt.target)))
                .await
                .map_err(|_| ApiError::bad_request("verify governed Skill task failed"))?
                .map_err(ApiError::bad_request)?;
        if actual != receipt.after_fingerprint {
            reasons.push("disk_fingerprint_mismatch".to_owned());
        }
    }
    invalidate_run_snapshot(state, run).await;
    if skill_http::governance_observation(state, true)
        .await
        .is_err()
    {
        reasons.push("fresh_inventory_unavailable".to_owned());
    }
    reasons.sort();
    reasons.dedup();
    Ok((reasons.is_empty(), reasons))
}

async fn rollback_actions(
    state: &AppState,
    run: &SkillGovernanceApplyRun,
    lock_id: Uuid,
    lease_nonce: &str,
) -> Result<(), String> {
    compensate_journaled_actions(state, run.id, Some(lock_id), Some(lease_nonce))
        .await
        .into_iter()
        .next()
        .map_or(Ok(()), Err)
}

async fn compensate_journaled_actions(
    state: &AppState,
    run_id: Uuid,
    lock_id: Option<Uuid>,
    lease_nonce: Option<&str>,
) -> Vec<String> {
    let mut errors = Vec::new();
    let mut actions = match state
        .store
        .list_skill_governance_apply_actions(run_id)
        .await
    {
        Ok(actions) => actions,
        Err(_) => return vec!["load apply journal for rollback".to_owned()],
    };
    actions.reverse();
    for action in actions {
        if let Some(lease_nonce) = lease_nonce {
            if let Err(error) = renew_apply_lease(state, lock_id, lease_nonce).await {
                errors.push(error);
                break;
            }
        }
        if action.status == SkillGovernanceApplyActionStatus::RolledBack {
            continue;
        }
        let Some(receipt) = receipt_value(&action) else {
            continue;
        };
        let rolling_back = match state
            .store
            .transition_skill_governance_apply_action(
                action.id,
                action.version,
                SkillGovernanceApplyActionStatus::RollingBack,
                action.result_hash.as_deref(),
                action.backup_path.as_deref(),
                action.quarantine_path.as_deref(),
                json!({"phase": "rolling_back", "receipt": receipt}),
                None,
            )
            .await
        {
            Ok(action) => action,
            Err(_) => {
                errors.push("persist rollback journal transition".to_owned());
                continue;
            }
        };
        let receipt = receipt.clone();
        let result = tokio::task::spawn_blocking(move || rollback_atomic_mutation(&receipt)).await;
        let (status, error) = match result {
            Ok(Ok(())) => (SkillGovernanceApplyActionStatus::RolledBack, None),
            Ok(Err(error)) => {
                errors.push(error.clone());
                (
                    SkillGovernanceApplyActionStatus::RecoveryRequired,
                    Some(error),
                )
            }
            Err(_) => {
                let error = "rollback task failed".to_owned();
                errors.push(error.clone());
                (
                    SkillGovernanceApplyActionStatus::RecoveryRequired,
                    Some(error),
                )
            }
        };
        let receipt = receipt_value(&rolling_back);
        let evidence = receipt.as_ref().map_or_else(
            || json!({"phase": status.as_str()}),
            |receipt| json!({"phase": status.as_str(), "receipt": receipt}),
        );
        if state
            .store
            .transition_skill_governance_apply_action(
                rolling_back.id,
                rolling_back.version,
                status,
                rolling_back.result_hash.as_deref(),
                rolling_back.backup_path.as_deref(),
                rolling_back.quarantine_path.as_deref(),
                evidence,
                error.as_deref(),
            )
            .await
            .is_err()
        {
            errors.push("persist rollback completion".to_owned());
        }
    }
    errors
}

async fn renew_apply_lease(
    state: &AppState,
    lock_id: Option<Uuid>,
    lease_nonce: &str,
) -> Result<(), String> {
    let lock_id = lock_id.ok_or_else(|| "governance run has no scoped lock".to_owned())?;
    let lock = state
        .store
        .get_skill_governance_lock(lock_id)
        .await
        .map_err(|_| "load scoped governance lease".to_owned())?
        .ok_or_else(|| "scoped governance lease is unavailable".to_owned())?;
    if lock.released_at.is_some() || lock.lease_nonce != lease_nonce {
        return Err("scoped governance lease ownership changed".to_owned());
    }
    state
        .store
        .renew_skill_governance_lock(lock.id, lock.version, lease_nonce, Utc::now() + LOCK_LEASE)
        .await
        .map_err(|_| "renew scoped governance lease".to_owned())?;
    Ok(())
}

fn receipt_values(actions: &[SkillGovernanceApplyAction]) -> Vec<MutationReceipt> {
    actions.iter().filter_map(receipt_value).collect()
}

fn receipt_value(action: &SkillGovernanceApplyAction) -> Option<MutationReceipt> {
    serde_json::from_value(action.evidence.clone())
        .ok()
        .or_else(|| {
            action
                .evidence
                .get("receipt")
                .cloned()
                .and_then(|receipt| serde_json::from_value(receipt).ok())
        })
}

async fn invalidate_run_snapshot(state: &AppState, run: &SkillGovernanceApplyRun) {
    if run.scope == StoreGovernanceScope::Agent {
        if let Ok(agent_id) = run.scope_id.parse::<Uuid>() {
            state.skill_snapshots.invalidate_agent(agent_id).await;
        }
    }
}

async fn release_governance_lock(state: &AppState, lock_id: Uuid, lease_nonce: &str) {
    let Ok(Some(lock)) = state.store.get_skill_governance_lock(lock_id).await else {
        return;
    };
    if lock.released_at.is_none() && lock.lease_nonce == lease_nonce {
        let _ = state
            .store
            .release_skill_governance_lock(lock.id, lock.version, lease_nonce)
            .await;
    }
}

async fn require_run(state: &AppState, run_id: Uuid) -> Result<SkillGovernanceApplyRun, ApiError> {
    let run = state
        .store
        .get_skill_governance_apply_run(run_id)
        .await?
        .ok_or_else(|| ApiError::not_found("skill governance apply run not found"))?;
    recover_interrupted_run(state, run).await
}

async fn recover_interrupted_run(
    state: &AppState,
    run: SkillGovernanceApplyRun,
) -> Result<SkillGovernanceApplyRun, ApiError> {
    if !matches!(
        run.status,
        SkillGovernanceApplyRunStatus::Pending
            | SkillGovernanceApplyRunStatus::Running
            | SkillGovernanceApplyRunStatus::RollingBack
    ) {
        return Ok(run);
    }
    let lock_active = if let Some(lock_id) = run.lock_id {
        state
            .store
            .get_skill_governance_lock(lock_id)
            .await?
            .is_some_and(|lock| lock.released_at.is_none() && lock.lease_expires_at > Utc::now())
    } else {
        false
    };
    if lock_active {
        return Ok(run);
    }
    let actions = state
        .store
        .list_skill_governance_apply_actions(run.id)
        .await?;
    let applied = actions.iter().any(|action| receipt_value(action).is_some());
    for action in actions {
        if matches!(
            action.status,
            SkillGovernanceApplyActionStatus::Verified
                | SkillGovernanceApplyActionStatus::Failed
                | SkillGovernanceApplyActionStatus::RolledBack
                | SkillGovernanceApplyActionStatus::RecoveryRequired
                | SkillGovernanceApplyActionStatus::Skipped
        ) {
            continue;
        }
        let evidence = receipt_value(&action).map_or_else(
            || json!({"phase": "recovery_required", "reason": "lease_expired_after_restart"}),
            |receipt| json!({"phase": "recovery_required", "reason": "lease_expired_after_restart", "receipt": receipt}),
        );
        let _ = state
            .store
            .transition_skill_governance_apply_action(
                action.id,
                action.version,
                SkillGovernanceApplyActionStatus::RecoveryRequired,
                action.result_hash.as_deref(),
                action.backup_path.as_deref(),
                action.quarantine_path.as_deref(),
                evidence,
                Some("apply was interrupted after its scoped lease expired"),
            )
            .await;
    }
    state
        .store
        .transition_skill_governance_apply_run(
            run.id,
            run.version,
            SkillGovernanceApplyRunStatus::RecoveryRequired,
            SkillGovernanceRecoveryStatus::Pending,
            run.backup_path.as_deref(),
            run.quarantine_path.as_deref(),
            merge_run_evidence(
                &run,
                "recovery",
                applied,
                &["lease_expired_after_restart".to_owned()],
                "interrupted apply requires verified rollback or manual recovery",
            ),
            Some("apply was interrupted after its scoped lease expired"),
        )
        .await
        .map_err(ApiError::from)
}

async fn run_view(state: &AppState, run: SkillGovernanceApplyRun) -> Result<RunView, ApiError> {
    let actions = if let Some(plan_id) = run.plan_id {
        let plan = require_plan(state, plan_id).await?;
        parse_plan_preview(&plan.plan)?.content.actions
    } else {
        Vec::new()
    };
    let evidence = run.evidence.as_object();
    let phase = evidence
        .and_then(|value| value.get("phase"))
        .and_then(Value::as_str)
        .unwrap_or("preview")
        .to_owned();
    let effects = evidence
        .and_then(|value| value.get("effects"))
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default();
    let recovery_reasons = evidence
        .and_then(|value| value.get("recoveryReasons"))
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default();
    let high_risk = evidence
        .and_then(|value| value.get("highRisk"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let applied = evidence
        .and_then(|value| value.get("applied"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let message = evidence
        .and_then(|value| value.get("message"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    let (status, progress) = run_status(&run);
    Ok(RunView {
        id: run.id,
        plan_id: run.plan_id,
        scope: run.scope,
        scope_id: run.scope_id,
        status: status.to_owned(),
        phase,
        progress,
        message,
        dry_run: false,
        applied,
        high_risk,
        recovery_required: run.recovery_status != SkillGovernanceRecoveryStatus::NotRequired,
        recovery_reasons,
        lock_snapshot_id: run.lock_id,
        backup_id: run.backup_path,
        quarantine_id: run.quarantine_path,
        effects,
        actions,
        started_at: run.started_at,
        updated_at: run.updated_at,
        completed_at: run.completed_at,
    })
}

fn run_status(run: &SkillGovernanceApplyRun) -> (&'static str, u8) {
    match run.status {
        SkillGovernanceApplyRunStatus::Pending => ("queued", 5),
        SkillGovernanceApplyRunStatus::Running => ("running", 55),
        SkillGovernanceApplyRunStatus::Succeeded => ("succeeded", 100),
        SkillGovernanceApplyRunStatus::Failed => ("failed", 100),
        SkillGovernanceApplyRunStatus::RollingBack => ("recovery_required", 80),
        SkillGovernanceApplyRunStatus::RolledBack => ("rolled_back", 100),
        SkillGovernanceApplyRunStatus::RecoveryRequired => ("recovery_required", 100),
    }
}

fn run_evidence(
    phase: &str,
    applied: bool,
    high_risk: bool,
    actions: &[PlanAction],
    effects: Vec<RunEffect>,
    recovery_reasons: Vec<String>,
    message: Option<&str>,
) -> Value {
    json!({
        "phase": phase,
        "applied": applied,
        "highRisk": high_risk,
        "actions": actions,
        "effects": effects,
        "recoveryReasons": recovery_reasons,
        "message": message,
        "sessionEffective": "unknown",
        "newSessionRequired": applied,
    })
}

fn merge_run_evidence(
    run: &SkillGovernanceApplyRun,
    phase: &str,
    applied: bool,
    reasons: &[String],
    message: &str,
) -> Value {
    let mut evidence = run.evidence.clone();
    if !evidence.is_object() {
        evidence = json!({});
    }
    let Some(object) = evidence.as_object_mut() else {
        return evidence;
    };
    object.insert("phase".to_owned(), Value::String(phase.to_owned()));
    object.insert("applied".to_owned(), Value::Bool(applied));
    object.insert("message".to_owned(), Value::String(message.to_owned()));
    object.insert("recoveryReasons".to_owned(), json!(reasons));
    evidence
}

fn run_evidence_applied(run: &SkillGovernanceApplyRun) -> bool {
    run.evidence
        .get("applied")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn preflight_effects(decisions: &[ActionDecision], lock_id: Uuid) -> Vec<RunEffect> {
    let mut effects = vec![RunEffect {
        kind: "lock".to_owned(),
        status: "pending".to_owned(),
        label: "Acquire scoped lease".to_owned(),
        detail: Some("lease has owner, PID, nonce, expiry, and stale takeover audit".to_owned()),
        created_id: Some(lock_id.to_string()),
    }];
    effects.extend(decisions.iter().map(|decision| {
        RunEffect {
            kind: if decision.action.action == PlanActionKind::Remove {
                "quarantine"
            } else {
                "apply"
            }
            .to_owned(),
            status: if decision.supported {
                "pending"
            } else {
                "skipped"
            }
            .to_owned(),
            label: format!("{:?}: {}", decision.action.action, decision.action.target),
            detail: Some(decision.reason.clone()),
            created_id: None,
        }
    }));
    effects.push(RunEffect {
        kind: "verify".to_owned(),
        status: "pending".to_owned(),
        label: "Force refresh and verify disk evidence".to_owned(),
        detail: Some("does not restart active Runtime Sessions".to_owned()),
        created_id: None,
    });
    effects
}

fn effect_from_receipt(receipt: &MutationReceipt) -> RunEffect {
    RunEffect {
        kind: if receipt.installation_mode == "remove" {
            "quarantine"
        } else {
            "backup"
        }
        .to_owned(),
        status: "succeeded".to_owned(),
        label: format!(
            "{} mutation committed atomically",
            receipt.installation_mode
        ),
        detail: Some(format!(
            "before={} after={}",
            receipt.before_fingerprint, receipt.after_fingerprint
        )),
        created_id: receipt
            .quarantine_ref
            .clone()
            .or_else(|| receipt.backup_ref.clone()),
    }
}

fn apply_confirmation_nonce(
    preflight: &ApplyPreflight,
    idempotency_key: &str,
) -> Result<String, ApiError> {
    canonical_hash(&(
        "skill-governance-apply-confirmation",
        &preflight.confirmation_nonce,
        idempotency_key,
    ))
    .map_err(ApiError::bad_request)
}

fn rollback_nonce(
    run: &SkillGovernanceApplyRun,
    idempotency_key: &str,
) -> Result<String, ApiError> {
    canonical_hash(&(
        "skill-governance-rollback",
        run.id,
        run.version,
        &run.lock_hash,
        idempotency_key,
    ))
    .map_err(ApiError::bad_request)
}

fn validate_idempotency_key(key: &str) -> Result<(), ApiError> {
    if !(8..=128).contains(&key.len())
        || !key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err(ApiError::bad_request(
            "governance idempotency key is invalid",
        ));
    }
    Ok(())
}

fn normalize_name(value: &str) -> String {
    value.trim().to_ascii_lowercase().replace(' ', "-")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn action(kind: PlanActionKind, scope: GovernanceScope) -> PlanAction {
        PlanAction {
            action: kind,
            runtime: "codex".to_owned(),
            scope,
            target: "reviewer".to_owned(),
            skill_fingerprint: "fingerprint".to_owned(),
            before: "before".to_owned(),
            after: "after".to_owned(),
            risk: PlanRisk::Low,
            reason: "drift".to_owned(),
            evidence: "filesystem".to_owned(),
            expected_observation_hash: "observation".to_owned(),
            expected_config_hash: "config".to_owned(),
            expected_lock_hash: "lock".to_owned(),
            approval_required: false,
            blocked: false,
        }
    }

    #[test]
    fn automatic_matrix_blocks_non_agent_unknown_and_runtime_neutral_enable() {
        let machine = action_decision(
            &action(PlanActionKind::Install, GovernanceScope::Machine),
            &[],
            GovernanceScope::Machine,
        );
        assert!(!machine.supported);
        let enable = action_decision(
            &action(PlanActionKind::Enable, GovernanceScope::Agent),
            &[],
            GovernanceScope::Agent,
        );
        assert!(!enable.supported);
        let mut unknown = action(PlanActionKind::Install, GovernanceScope::Agent);
        unknown.blocked = true;
        assert!(!action_decision(&unknown, &[], GovernanceScope::Agent).supported);
        assert!(
            action_decision(
                &action(PlanActionKind::Remove, GovernanceScope::Agent),
                &[],
                GovernanceScope::Agent
            )
            .supported
        );
    }

    #[test]
    fn idempotency_keys_are_strict_and_secret_free() {
        assert!(validate_idempotency_key("apply:12345678").is_ok());
        assert!(validate_idempotency_key("short").is_err());
        assert!(validate_idempotency_key("secret/value").is_err());
    }
}
