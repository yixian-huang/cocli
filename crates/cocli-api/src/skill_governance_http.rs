use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use cocli_store::{
    NewSkillLockSnapshot, SkillGovernancePlan, SkillGovernancePlanStatus,
    SkillGovernanceScope as StoreGovernanceScope, SkillLockSnapshot, SkillProfile,
    SkillProfileBinding,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use super::skill_governance::{
    build_dry_run_plan, build_lockfile_preview, compare_drift, resolve_effective_desired,
    stale_plan_reasons, validate_profile_document, BoundProfile, DryRunPlanPreview,
    EffectiveDesiredState, GovernanceObservation, GovernanceScope, SkillDrift,
    SkillLockfilePreview, SkillProfileDocument,
};
use super::{skill_http, ApiError, AppState};

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/skills/governance/profiles",
            get(list_profiles).post(create_profile),
        )
        .route(
            "/api/skills/governance/profiles/:profile_id",
            get(get_profile).put(update_profile).delete(delete_profile),
        )
        .route(
            "/api/skills/governance/bindings",
            get(list_bindings).post(bind_profile),
        )
        .route(
            "/api/skills/governance/bindings/:binding_id",
            axum::routing::delete(unbind_profile),
        )
        .route(
            "/api/skills/governance/desired/effective",
            get(effective_desired),
        )
        .route("/api/skills/governance/evidence", get(evidence))
        .route(
            "/api/skills/governance/lock/preview",
            post(lockfile_preview),
        )
        .route("/api/skills/governance/locks", get(list_lock_snapshots))
        .route(
            "/api/skills/governance/plans",
            get(list_plans).post(generate_plan),
        )
        .route("/api/skills/governance/plans/:plan_id", get(get_plan))
        .route(
            "/api/skills/governance/plans/:plan_id/audit",
            get(get_plan_audit),
        )
        .route(
            "/api/skills/governance/plans/:plan_id/approve",
            post(approve_plan),
        )
        .route(
            "/api/skills/governance/plans/:plan_id/reject",
            post(reject_plan),
        )
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProfileView {
    id: Uuid,
    version: i64,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    #[serde(flatten)]
    document: SkillProfileDocument,
}

impl TryFrom<SkillProfile> for ProfileView {
    type Error = ApiError;

    fn try_from(profile: SkillProfile) -> Result<Self, Self::Error> {
        let document = serde_json::from_value(profile.document)
            .map_err(|_| ApiError::bad_request("stored SkillProfile document is invalid"))?;
        Ok(Self {
            id: profile.id,
            version: profile.version,
            created_at: profile.created_at,
            updated_at: profile.updated_at,
            document,
        })
    }
}

async fn list_profiles(State(state): State<AppState>) -> Result<Json<Vec<ProfileView>>, ApiError> {
    let profiles = state.store.list_skill_profiles().await?;
    Ok(Json(
        profiles
            .into_iter()
            .map(ProfileView::try_from)
            .collect::<Result<_, _>>()?,
    ))
}

async fn create_profile(
    State(state): State<AppState>,
    Json(document): Json<SkillProfileDocument>,
) -> Result<(StatusCode, Json<ProfileView>), ApiError> {
    validate_profile_document(&document).map_err(ApiError::bad_request)?;
    let profile = state
        .store
        .create_skill_profile(
            serde_json::to_value(document)
                .map_err(|_| ApiError::bad_request("encode SkillProfile document"))?,
        )
        .await?;
    Ok((StatusCode::CREATED, Json(profile.try_into()?)))
}

async fn get_profile(
    State(state): State<AppState>,
    Path(profile_id): Path<Uuid>,
) -> Result<Json<ProfileView>, ApiError> {
    let profile = require_profile(&state, profile_id).await?;
    Ok(Json(profile.try_into()?))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UpdateProfileRequest {
    expected_version: i64,
    document: SkillProfileDocument,
}

async fn update_profile(
    State(state): State<AppState>,
    Path(profile_id): Path<Uuid>,
    Json(request): Json<UpdateProfileRequest>,
) -> Result<Json<ProfileView>, ApiError> {
    validate_profile_document(&request.document).map_err(ApiError::bad_request)?;
    let profile = state
        .store
        .update_skill_profile(
            profile_id,
            serde_json::to_value(request.document)
                .map_err(|_| ApiError::bad_request("encode SkillProfile document"))?,
            request.expected_version,
        )
        .await?;
    Ok(Json(profile.try_into()?))
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
) -> Result<StatusCode, ApiError> {
    state
        .store
        .delete_skill_profile(profile_id, query.expected_version)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BindProfileRequest {
    profile_id: Uuid,
    scope: GovernanceScope,
    scope_id: String,
}

async fn bind_profile(
    State(state): State<AppState>,
    Json(request): Json<BindProfileRequest>,
) -> Result<(StatusCode, Json<SkillProfileBinding>), ApiError> {
    let scope_id = normalized_scope_id(request.scope, &request.scope_id)?;
    let binding = state
        .store
        .bind_skill_profile(request.scope.into(), &scope_id, request.profile_id)
        .await?;
    Ok((StatusCode::CREATED, Json(binding)))
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BindingQuery {
    scope: Option<GovernanceScope>,
    scope_id: Option<String>,
}

async fn list_bindings(
    State(state): State<AppState>,
    Query(query): Query<BindingQuery>,
) -> Result<Json<Vec<SkillProfileBinding>>, ApiError> {
    let bindings = if let (Some(scope), Some(scope_id)) = (query.scope, query.scope_id.as_deref()) {
        state
            .store
            .list_skill_profile_bindings_for_scope(
                scope.into(),
                &normalized_scope_id(scope, scope_id)?,
            )
            .await?
    } else {
        state
            .store
            .list_skill_profile_bindings(query.scope.map(Into::into))
            .await?
    };
    Ok(Json(bindings))
}

async fn unbind_profile(
    State(state): State<AppState>,
    Path(binding_id): Path<Uuid>,
    Query(query): Query<ExpectedVersionQuery>,
) -> Result<StatusCode, ApiError> {
    state
        .store
        .unbind_skill_profile(binding_id, query.expected_version)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DesiredTarget {
    workspace_id: Option<String>,
    agent_id: Option<String>,
}

async fn effective_desired(
    State(state): State<AppState>,
    Query(target): Query<DesiredTarget>,
) -> Result<Json<EffectiveDesiredState>, ApiError> {
    Ok(Json(load_effective_desired(&state, &target).await?))
}

#[derive(Debug, Default, Deserialize)]
struct ForceQuery {
    #[serde(default)]
    force: bool,
}

async fn evidence(
    State(state): State<AppState>,
    Query(query): Query<ForceQuery>,
) -> Result<Json<GovernanceObservation>, ApiError> {
    Ok(Json(
        skill_http::governance_observation(&state, query.force).await?,
    ))
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GovernancePreviewRequest {
    scope: GovernanceScope,
    scope_id: String,
    #[serde(default)]
    workspace_id: Option<String>,
    #[serde(default)]
    agent_id: Option<String>,
    #[serde(default)]
    force: bool,
}

impl GovernancePreviewRequest {
    fn desired_target(&self) -> DesiredTarget {
        DesiredTarget {
            workspace_id: self.workspace_id.clone().or_else(|| {
                (self.scope == GovernanceScope::Workspace).then(|| self.scope_id.clone())
            }),
            agent_id: self
                .agent_id
                .clone()
                .or_else(|| (self.scope == GovernanceScope::Agent).then(|| self.scope_id.clone())),
        }
    }

    fn normalized_scope_id(&self) -> Result<String, ApiError> {
        normalized_scope_id(self.scope, &self.scope_id)
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LockPreviewResponse {
    snapshot: SkillLockSnapshot,
    preview: SkillLockfilePreview,
    drift: Vec<SkillDrift>,
    #[serde(skip_serializing_if = "Option::is_none")]
    previous_lock_hash: Option<String>,
    lockfile_changed: bool,
    lockfile_boundary: &'static str,
    writes_real_directories: bool,
}

async fn lockfile_preview(
    State(state): State<AppState>,
    Json(request): Json<GovernancePreviewRequest>,
) -> Result<Json<LockPreviewResponse>, ApiError> {
    let generated = generate_lock_preview(&state, &request).await?;
    Ok(Json(generated))
}

async fn list_lock_snapshots(
    State(state): State<AppState>,
    Query(query): Query<ScopeQuery>,
) -> Result<Json<Vec<SkillLockSnapshot>>, ApiError> {
    let scope_id = normalized_scope_id(query.scope, &query.scope_id)?;
    Ok(Json(
        state
            .store
            .list_skill_lock_snapshots(query.scope.into(), &scope_id)
            .await?,
    ))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScopeQuery {
    scope: GovernanceScope,
    scope_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PlanResponse {
    plan: SkillGovernancePlan,
    preview: DryRunPlanPreview,
    drift: Vec<SkillDrift>,
    lock_snapshot_id: Uuid,
    lockfile_changed: bool,
    applied: bool,
}

async fn generate_plan(
    State(state): State<AppState>,
    Json(request): Json<GovernancePreviewRequest>,
) -> Result<(StatusCode, Json<PlanResponse>), ApiError> {
    let lock = generate_lock_preview(&state, &request).await?;
    let preview = build_dry_run_plan(
        &lock.drift,
        &lock.preview.snapshot_hash,
        &lock.preview.desired_config_hash,
        &lock.preview.lockfile_hash,
        lock.lockfile_changed,
    )
    .map_err(ApiError::bad_request)?;
    let desired_target = request.desired_target();
    let stored_document = json!({
        "schemaVersion": 1,
        "dryRun": true,
        "applied": false,
        "scope": request.scope,
        "scopeId": request.normalized_scope_id()?,
        "workspaceId": desired_target.workspace_id,
        "agentId": desired_target.agent_id,
        "lockSnapshotId": lock.snapshot.id,
        "lockfileChanged": lock.lockfile_changed,
        "drift": lock.drift,
        "preview": preview,
    });
    let plan = state
        .store
        .create_skill_governance_plan(
            request.scope.into(),
            &request.normalized_scope_id()?,
            stored_document,
            &preview.content.observation_hash,
            &preview.content.desired_config_hash,
        )
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(PlanResponse {
            plan,
            preview,
            drift: lock.drift,
            lock_snapshot_id: lock.snapshot.id,
            lockfile_changed: lock.lockfile_changed,
            applied: false,
        }),
    ))
}

async fn list_plans(
    State(state): State<AppState>,
    Query(query): Query<ScopeQuery>,
) -> Result<Json<Vec<SkillGovernancePlan>>, ApiError> {
    let scope_id = normalized_scope_id(query.scope, &query.scope_id)?;
    Ok(Json(
        state
            .store
            .list_skill_governance_plans(query.scope.into(), &scope_id)
            .await?,
    ))
}

async fn get_plan(
    State(state): State<AppState>,
    Path(plan_id): Path<Uuid>,
) -> Result<Json<SkillGovernancePlan>, ApiError> {
    Ok(Json(require_plan(&state, plan_id).await?))
}

async fn get_plan_audit(
    State(state): State<AppState>,
    Path(plan_id): Path<Uuid>,
) -> Result<Json<Vec<cocli_store::SkillGovernancePlanAudit>>, ApiError> {
    require_plan(&state, plan_id).await?;
    Ok(Json(
        state
            .store
            .list_skill_governance_plan_audit(plan_id)
            .await?,
    ))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PlanDecisionRequest {
    expected_version: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PlanDecisionResponse {
    plan: SkillGovernancePlan,
    applied: bool,
    dry_run: bool,
    stale_reasons: Vec<String>,
}

async fn approve_plan(
    State(state): State<AppState>,
    Path(plan_id): Path<Uuid>,
    Json(request): Json<PlanDecisionRequest>,
) -> Result<Json<PlanDecisionResponse>, ApiError> {
    let plan = require_plan(&state, plan_id).await?;
    if plan.status != SkillGovernancePlanStatus::Draft {
        return Err(ApiError::conflict(
            "only draft governance plans can be approved",
        ));
    }
    let context = parse_plan_context(&plan.plan)?;
    let observation = skill_http::governance_observation(&state, true).await?;
    let effective = load_effective_desired(
        &state,
        &DesiredTarget {
            workspace_id: context.workspace_id.clone(),
            agent_id: context.agent_id.clone(),
        },
    )
    .await?;
    let current_lock = build_lockfile_preview(
        &effective,
        &observation.snapshot_hash,
        observation.observed_at,
    )
    .map_err(ApiError::bad_request)?;
    let expected = parse_plan_preview(&plan.plan)?;
    let stale_reasons = stale_plan_reasons(
        &expected.content,
        &observation.snapshot_hash,
        &effective.desired_config_hash,
        &current_lock.lockfile_hash,
    );
    if !stale_reasons.is_empty() {
        let stale = state
            .store
            .mark_skill_governance_plan_stale(plan_id, request.expected_version)
            .await?;
        return Err(ApiError::json(
            StatusCode::CONFLICT,
            json!({
                "error": "governance plan is stale",
                "plan": stale,
                "staleReasons": stale_reasons,
            }),
        ));
    }
    let approved = state
        .store
        .approve_skill_governance_plan(plan_id, request.expected_version)
        .await?;
    Ok(Json(PlanDecisionResponse {
        plan: approved,
        applied: false,
        dry_run: true,
        stale_reasons,
    }))
}

async fn reject_plan(
    State(state): State<AppState>,
    Path(plan_id): Path<Uuid>,
    Json(request): Json<PlanDecisionRequest>,
) -> Result<Json<PlanDecisionResponse>, ApiError> {
    let current = require_plan(&state, plan_id).await?;
    if current.status != SkillGovernancePlanStatus::Draft {
        return Err(ApiError::conflict(
            "only draft governance plans can be rejected",
        ));
    }
    let plan = state
        .store
        .reject_skill_governance_plan(plan_id, request.expected_version)
        .await?;
    Ok(Json(PlanDecisionResponse {
        plan,
        applied: false,
        dry_run: true,
        stale_reasons: Vec::new(),
    }))
}

async fn generate_lock_preview(
    state: &AppState,
    request: &GovernancePreviewRequest,
) -> Result<LockPreviewResponse, ApiError> {
    let scope_id = request.normalized_scope_id()?;
    let previous = state
        .store
        .list_skill_lock_snapshots(request.scope.into(), &scope_id)
        .await?
        .into_iter()
        .next();
    let observation = skill_http::governance_observation(state, request.force).await?;
    let effective = load_effective_desired(state, &request.desired_target()).await?;
    if !effective.conflicts.is_empty() {
        return Err(ApiError::json(
            StatusCode::CONFLICT,
            json!({
                "error": "effective desired state has same-layer conflicts",
                "conflicts": effective.conflicts,
            }),
        ));
    }
    let mut scoped_effective = effective.clone();
    scoped_effective
        .skills
        .retain(|skill| skill.desired.install_scope == request.scope);
    let preview = build_lockfile_preview(
        &scoped_effective,
        &observation.snapshot_hash,
        observation.observed_at,
    )
    .map_err(ApiError::bad_request)?;
    let scoped_observation = observation
        .skills
        .iter()
        .filter(|skill| observation_matches_scope(skill, request.scope, &scope_id))
        .cloned()
        .collect::<Vec<_>>();
    let drift = compare_drift(&scoped_observation, &preview.content);
    let snapshot = state
        .store
        .create_skill_lock_snapshot(NewSkillLockSnapshot {
            scope: request.scope.into(),
            scope_id: scope_id.clone(),
            profile_id: None,
            snapshot: serde_json::to_value(&preview)
                .map_err(|_| ApiError::bad_request("encode lockfile preview"))?,
            observation_hash: preview.snapshot_hash.clone(),
            desired_hash: preview.desired_config_hash.clone(),
            lock_hash: preview.lockfile_hash.clone(),
        })
        .await?;
    let previous_lock_hash = previous.map(|snapshot| snapshot.lock_hash);
    let lockfile_changed = previous_lock_hash
        .as_deref()
        .map_or(true, |hash| hash != preview.lockfile_hash);
    Ok(LockPreviewResponse {
        snapshot,
        preview,
        drift,
        previous_lock_hash,
        lockfile_changed,
        lockfile_boundary: if request.scope == GovernanceScope::Workspace {
            "workspace_candidate"
        } else {
            "store_only"
        },
        writes_real_directories: false,
    })
}

fn observation_matches_scope(
    skill: &crate::skill_governance::ObservedSkill,
    scope: GovernanceScope,
    scope_id: &str,
) -> bool {
    skill.scope == scope && skill.scope_id.as_deref() == Some(scope_id)
}

async fn load_effective_desired(
    state: &AppState,
    target: &DesiredTarget,
) -> Result<EffectiveDesiredState, ApiError> {
    let bindings = state.store.list_skill_profile_bindings(None).await?;
    let mut bound = Vec::new();
    for binding in bindings.into_iter().filter(|binding| match binding.scope {
        StoreGovernanceScope::Machine => binding.scope_id == "machine",
        StoreGovernanceScope::Workspace => {
            target.workspace_id.as_deref() == Some(&binding.scope_id)
        }
        StoreGovernanceScope::Agent => target.agent_id.as_deref() == Some(&binding.scope_id),
    }) {
        let profile = require_profile(state, binding.profile_id).await?;
        let document: SkillProfileDocument = serde_json::from_value(profile.document)
            .map_err(|_| ApiError::bad_request("stored SkillProfile document is invalid"))?;
        bound.push(BoundProfile {
            binding_id: binding.id,
            profile_id: profile.id,
            profile_name: document.name.clone(),
            scope: binding.scope.into(),
            document,
        });
    }
    resolve_effective_desired(&bound).map_err(ApiError::bad_request)
}

async fn require_profile(state: &AppState, profile_id: Uuid) -> Result<SkillProfile, ApiError> {
    state
        .store
        .get_skill_profile(profile_id)
        .await?
        .ok_or_else(|| ApiError::not_found("SkillProfile not found"))
}

async fn require_plan(state: &AppState, plan_id: Uuid) -> Result<SkillGovernancePlan, ApiError> {
    state
        .store
        .get_skill_governance_plan(plan_id)
        .await?
        .ok_or_else(|| ApiError::not_found("skill governance plan not found"))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredPlanContext {
    workspace_id: Option<String>,
    agent_id: Option<String>,
}

fn parse_plan_context(value: &Value) -> Result<StoredPlanContext, ApiError> {
    serde_json::from_value(value.clone())
        .map_err(|_| ApiError::bad_request("stored governance plan context is invalid"))
}

fn parse_plan_preview(value: &Value) -> Result<DryRunPlanPreview, ApiError> {
    serde_json::from_value(
        value
            .get("preview")
            .cloned()
            .ok_or_else(|| ApiError::bad_request("stored governance plan has no preview"))?,
    )
    .map_err(|_| ApiError::bad_request("stored governance plan preview is invalid"))
}

fn normalized_scope_id(scope: GovernanceScope, value: &str) -> Result<String, ApiError> {
    if scope == GovernanceScope::Machine {
        return Ok("machine".to_owned());
    }
    let value = value.trim();
    if value.is_empty() || value.len() > 200 {
        return Err(ApiError::bad_request("governance scopeId is invalid"));
    }
    Ok(value.to_owned())
}

impl From<GovernanceScope> for StoreGovernanceScope {
    fn from(value: GovernanceScope) -> Self {
        match value {
            GovernanceScope::Machine => Self::Machine,
            GovernanceScope::Workspace => Self::Workspace,
            GovernanceScope::Agent => Self::Agent,
        }
    }
}

impl From<StoreGovernanceScope> for GovernanceScope {
    fn from(value: StoreGovernanceScope) -> Self {
        match value {
            StoreGovernanceScope::Machine => Self::Machine,
            StoreGovernanceScope::Workspace => Self::Workspace,
            StoreGovernanceScope::Agent => Self::Agent,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skill_governance::{InstallationMode, ObservedSkill};
    use chrono::{DateTime, Utc};

    fn observed_agent(scope_id: &str) -> ObservedSkill {
        ObservedSkill {
            logical_identity: "reviewer".to_owned(),
            runtime: "codex".to_owned(),
            scope: GovernanceScope::Agent,
            scope_id: Some(scope_id.to_owned()),
            source_provenance: None,
            version: None,
            content_digest: None,
            manifest_digest: None,
            installation_mode: Some(InstallationMode::Copy),
            destination: None,
            fingerprint: "reviewer".to_owned(),
            enabled: Some(true),
            shadowed: false,
            broken_symlink: false,
            evidence_status: "agent_workspace".to_owned(),
            evidence_source: "filesystem".to_owned(),
            session_effective: "unknown".to_owned(),
            session_reason: "not session-bound".to_owned(),
            observed_at: DateTime::<Utc>::UNIX_EPOCH,
            supported: true,
        }
    }

    #[test]
    fn machine_scope_is_canonical() {
        assert_eq!(
            normalized_scope_id(GovernanceScope::Machine, "anything").expect("scope"),
            "machine"
        );
    }

    #[test]
    fn agent_observation_is_scoped_to_the_requested_agent() {
        let observed = observed_agent("agent-a");
        assert!(observation_matches_scope(
            &observed,
            GovernanceScope::Agent,
            "agent-a"
        ));
        assert!(!observation_matches_scope(
            &observed,
            GovernanceScope::Agent,
            "agent-b"
        ));
        assert!(!observation_matches_scope(
            &observed,
            GovernanceScope::Workspace,
            "agent-a"
        ));
    }
}
