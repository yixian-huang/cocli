use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Component, Path as FsPath, PathBuf};

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{Duration as ChronoDuration, Utc};
use cocli_store::{
    NewSkillGovernanceApplyAction, NewSkillGovernanceApplyRun, NewSkillGovernanceManagedArtifact,
    NewSkillGovernanceMaterialization, SkillGovernanceApplyAction,
    SkillGovernanceApplyActionStatus, SkillGovernanceApplyRun, SkillGovernanceApplyRunStatus,
    SkillGovernanceGcCandidate, SkillGovernanceInstallationMode, SkillGovernanceManagedArtifact,
    SkillGovernanceMaterialization, SkillGovernanceMaterializationOwnership,
    SkillGovernanceMaterializationRootKind, SkillGovernanceRecoveryStatus,
    SkillGovernanceScope as StoreGovernanceScope, SkillGovernanceVerifyStatus,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::skill_apply::{
    activate_atomic_file_mutation, activate_atomic_mutation, backup_atomic_file_mutation,
    backup_atomic_mutation, fingerprint_path, load_local_artifact, load_vendored_artifact,
    prepare_atomic_file_mutation, prepare_atomic_mutation, prepare_managed_artifact_mutation,
    rollback_atomic_mutation, stage_atomic_file_mutation, stage_atomic_mutation, ArtifactBundle,
    MutationMode, MutationReceipt, PreparedMutation,
};
use crate::skill_governance::{canonical_hash, sha256_hex};
use crate::{ApiError, AppState, GovernanceScopeCapability, GovernanceSkillTarget, RuntimeError};

const WORKSPACE_LOCKFILE_PATH: &str = ".cocli/skills.lock.json";
const MAX_LOCKFILE_BYTES: usize = 2 * 1024 * 1024;
const MANAGED_OPERATION_LEASE: ChronoDuration = ChronoDuration::minutes(15);

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/skills/governance/scopes", get(scope_capabilities))
        .route(
            "/api/skills/governance/managed/artifacts",
            get(list_managed_artifacts),
        )
        .route(
            "/api/skills/governance/managed/artifacts/preview",
            post(preview_managed_artifact),
        )
        .route(
            "/api/skills/governance/managed/artifacts/commit",
            post(commit_managed_artifact),
        )
        .route(
            "/api/skills/governance/materializations",
            get(list_materializations),
        )
        .route(
            "/api/skills/governance/adoption/preview",
            post(preview_adoption),
        )
        .route(
            "/api/skills/governance/adoption/commit",
            post(commit_adoption),
        )
        .route(
            "/api/skills/governance/workspace-lockfile",
            get(inspect_workspace_lockfile),
        )
        .route(
            "/api/skills/governance/workspace-lockfile/restore/preview",
            post(preview_lockfile_restore),
        )
        .route(
            "/api/skills/governance/workspace-lockfile/restore",
            post(commit_lockfile_restore),
        )
        .route("/api/skills/governance/gc/preview", post(preview_gc))
        .route("/api/skills/governance/gc/commit", post(commit_gc))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScopeCapabilityQuery {
    runtime: Option<String>,
    scope: Option<String>,
    workspace_id: Option<Uuid>,
    agent_id: Option<Uuid>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScopeCapabilityResponse {
    observed_at: chrono::DateTime<Utc>,
    capabilities: Vec<GovernanceScopeCapability>,
    diagnostics: Vec<GovernanceDiagnostic>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GovernanceDiagnostic {
    subject: String,
    phase: String,
    error_type: String,
    message: String,
    observed_at: chrono::DateTime<Utc>,
}

async fn scope_capabilities(
    State(state): State<AppState>,
    Query(query): Query<ScopeCapabilityQuery>,
) -> Result<Json<ScopeCapabilityResponse>, ApiError> {
    let observed_at = Utc::now();
    let runtimes = if let Some(runtime) = query.runtime.clone() {
        vec![runtime]
    } else {
        state
            .runtime
            .list()
            .await
            .into_iter()
            .map(|runtime| runtime.name)
            .collect()
    };
    let scopes = if let Some(scope) = query.scope.clone() {
        vec![normalize_scope_name(&scope)?]
    } else {
        vec![
            "machine".to_owned(),
            "workspace".to_owned(),
            "agent".to_owned(),
        ]
    };
    let workspace_root = if scopes.iter().any(|scope| scope == "workspace") {
        match query.workspace_id {
            Some(workspace_id) => Some(resolve_workspace_root(&state, workspace_id).await?),
            None => None,
        }
    } else {
        None
    };
    let agent = if scopes.iter().any(|scope| scope == "agent") {
        match query.agent_id {
            Some(agent_id) => state.store.get_agent(agent_id).await?,
            None => None,
        }
    } else {
        None
    };

    let mut capabilities = Vec::new();
    let mut diagnostics = Vec::new();
    for runtime in runtimes {
        for scope in &scopes {
            let scope_root = match scope.as_str() {
                "workspace" => workspace_root.as_deref(),
                "agent" => None,
                _ => None,
            };
            if scope == "agent" {
                let Some(agent) = agent.as_ref() else {
                    diagnostics.push(diagnostic(
                        "agent",
                        "capability",
                        "missing_scope_id",
                        "agentId is required for Agent scope capabilities",
                        observed_at,
                    ));
                    continue;
                };
                match state
                    .runtime
                    .governance_skill_target(agent, "__capability_probe__")
                    .await
                {
                    Ok(target) => capabilities.push(target_capability(&runtime, scope, target)),
                    Err(error) => diagnostics.push(runtime_diagnostic(
                        &runtime,
                        scope,
                        "capability",
                        error,
                        observed_at,
                    )),
                }
                continue;
            }
            match state
                .runtime
                .governance_scope_capabilities(&runtime, scope, scope_root)
                .await
            {
                Ok(mut runtime_capabilities) => capabilities.append(&mut runtime_capabilities),
                Err(error) => diagnostics.push(runtime_diagnostic(
                    &runtime,
                    scope,
                    "capability",
                    error,
                    observed_at,
                )),
            }
        }
    }
    capabilities.sort_by(|left, right| {
        (
            &left.runtime,
            &left.scope,
            &left.root_kind,
            &left.path,
            &left.status,
        )
            .cmp(&(
                &right.runtime,
                &right.scope,
                &right.root_kind,
                &right.path,
                &right.status,
            ))
    });
    Ok(Json(ScopeCapabilityResponse {
        observed_at,
        capabilities,
        diagnostics,
    }))
}

async fn list_managed_artifacts(
    State(state): State<AppState>,
) -> Result<Json<Vec<SkillGovernanceManagedArtifact>>, ApiError> {
    Ok(Json(
        state
            .store
            .list_skill_governance_managed_artifacts()
            .await?,
    ))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManagedArtifactPreviewRequest {
    source_kind: String,
    local_path: Option<String>,
    library_id: Option<Uuid>,
    expected_content_digest: Option<String>,
    expected_manifest_digest: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ManagedArtifactPreview {
    source_kind: String,
    source: Value,
    artifact_key: String,
    content_digest: String,
    manifest_digest: String,
    revision: String,
    store_relative_path: String,
    preview_hash: String,
    idempotency_key: String,
    confirmation_nonce: String,
    hazards: Vec<String>,
    blocked: bool,
}

async fn preview_managed_artifact(
    State(state): State<AppState>,
    Json(request): Json<ManagedArtifactPreviewRequest>,
) -> Result<Json<ManagedArtifactPreview>, ApiError> {
    let (bundle, source) = load_artifact_for_request(&state, &request).await?;
    if let Some(expected) = &request.expected_content_digest {
        require_match("contentDigest", expected, &bundle.content_digest)?;
    }
    if let Some(expected) = &request.expected_manifest_digest {
        require_match("manifestDigest", expected, &bundle.manifest_digest)?;
    }
    let store_relative_path = artifact_store_relative_path(&bundle, &source);
    let artifact_key = artifact_key(&bundle, &source)?;
    let hazards = artifact_hazards(&bundle, bundle.canonical_source.as_deref());
    let preview = preview_from_bundle(
        request.source_kind,
        source,
        artifact_key,
        store_relative_path,
        &bundle,
        hazards,
    )?;
    Ok(Json(preview))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManagedArtifactCommitRequest {
    #[serde(flatten)]
    preview: ManagedArtifactPreviewRequest,
    expected_preview_hash: String,
    confirmation_nonce: String,
    idempotency_key: String,
}

async fn commit_managed_artifact(
    State(state): State<AppState>,
    Json(request): Json<ManagedArtifactCommitRequest>,
) -> Result<Json<SkillGovernanceManagedArtifact>, ApiError> {
    let preview = preview_managed_artifact(
        State(state.clone()),
        Json(ManagedArtifactPreviewRequest {
            source_kind: request.preview.source_kind.clone(),
            local_path: request.preview.local_path.clone(),
            library_id: request.preview.library_id,
            expected_content_digest: request.preview.expected_content_digest.clone(),
            expected_manifest_digest: request.preview.expected_manifest_digest.clone(),
        }),
    )
    .await?
    .0;
    require_match(
        "previewHash",
        &request.expected_preview_hash,
        &preview.preview_hash,
    )?;
    require_confirmation_nonce(
        "managed-artifact-commit",
        &preview.preview_hash,
        &request.idempotency_key,
        &request.confirmation_nonce,
    )?;
    if preview.blocked {
        return Err(ApiError::json(
            StatusCode::CONFLICT,
            json!({"error": "managed artifact preview is blocked", "hazards": preview.hazards}),
        ));
    }
    let (bundle, source) = load_artifact_for_request(&state, &request.preview).await?;
    require_match(
        "contentDigest",
        &preview.content_digest,
        &bundle.content_digest,
    )?;
    require_match(
        "manifestDigest",
        &preview.manifest_digest,
        &bundle.manifest_digest,
    )?;
    require_match(
        "artifactKey",
        &preview.artifact_key,
        &artifact_key(&bundle, &source)?,
    )?;
    require_match(
        "storeRelativePath",
        &preview.store_relative_path,
        &artifact_store_relative_path(&bundle, &source),
    )?;
    let store_root = state.runtime.governance_managed_artifact_root().await?;
    materialize_artifact_store(&store_root, &preview.store_relative_path, &bundle)
        .map_err(ApiError::conflict)?;
    let artifact = state
        .store
        .create_skill_governance_managed_artifact(NewSkillGovernanceManagedArtifact {
            artifact_key: preview.artifact_key,
            artifact_kind: "local_skill".to_owned(),
            source_provenance: preview.source,
            content_digest: preview.content_digest.clone(),
            manifest_digest: preview.manifest_digest.clone(),
            schema_version: 1,
            revision: preview.revision,
            store_relative_path: preview.store_relative_path,
            artifact: json!({
                "contentDigest": preview.content_digest,
                "manifestDigest": preview.manifest_digest,
                "immutable": true
            }),
            metadata: json!({"createdVia": "managed_artifact_commit", "immutable": true}),
        })
        .await?;
    Ok(Json(artifact))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MaterializationQuery {
    scope: String,
    scope_id: String,
}

async fn list_materializations(
    State(state): State<AppState>,
    Query(query): Query<MaterializationQuery>,
) -> Result<Json<Vec<SkillGovernanceMaterialization>>, ApiError> {
    Ok(Json(
        state
            .store
            .list_skill_governance_materializations(
                parse_store_scope(&query.scope)?,
                &query.scope_id,
            )
            .await?,
    ))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AdoptionRequest {
    runtime: String,
    scope: String,
    scope_id: String,
    skill_name: String,
    mode: Option<String>,
    expected_fingerprint: Option<String>,
    expected_version: Option<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AdoptionPreview {
    runtime: String,
    scope: String,
    scope_id: String,
    skill_name: String,
    target_path: String,
    target_fingerprint: String,
    content_digest: Option<String>,
    manifest_digest: Option<String>,
    existing_ownership: Option<SkillGovernanceMaterializationOwnership>,
    hazards: Vec<String>,
    blocked: bool,
    preview_hash: String,
    idempotency_key: String,
    confirmation_nonce: String,
}

async fn preview_adoption(
    State(state): State<AppState>,
    Json(request): Json<AdoptionRequest>,
) -> Result<Json<AdoptionPreview>, ApiError> {
    Ok(Json(build_adoption_preview(&state, &request).await?))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AdoptionCommitRequest {
    #[serde(flatten)]
    adoption: AdoptionRequest,
    expected_preview_hash: String,
    confirmation_nonce: String,
    idempotency_key: String,
}

async fn commit_adoption(
    State(state): State<AppState>,
    Json(request): Json<AdoptionCommitRequest>,
) -> Result<Json<SkillGovernanceMaterialization>, ApiError> {
    let preview = build_adoption_preview(&state, &request.adoption).await?;
    require_match(
        "previewHash",
        &request.expected_preview_hash,
        &preview.preview_hash,
    )?;
    require_confirmation_nonce(
        "adoption-commit",
        &preview.preview_hash,
        &request.idempotency_key,
        &request.confirmation_nonce,
    )?;
    if preview.blocked {
        return Err(ApiError::json(
            StatusCode::CONFLICT,
            json!({"error": "adoption preview is blocked", "hazards": preview.hazards}),
        ));
    }
    if let Some(expected) = &request.adoption.expected_fingerprint {
        require_match("targetFingerprint", expected, &preview.target_fingerprint)?;
    }
    let mode = request.adoption.mode.as_deref().unwrap_or("record_only");
    if !matches!(mode, "record_only" | "import_copy" | "keep_foreign") {
        return Err(ApiError::bad_request(format!(
            "unsupported adoption mode: {mode}"
        )));
    }
    let scope = parse_store_scope(&request.adoption.scope)?;
    let persisted_scope_id = if scope == StoreGovernanceScope::Machine {
        "machine".to_owned()
    } else {
        request.adoption.scope_id.clone()
    };
    let target = resolve_target(
        &state,
        &request.adoption.runtime,
        scope,
        &request.adoption.scope_id,
        &request.adoption.skill_name,
    )
    .await?;
    let bundle = load_existing_target_artifact(&target.entry_path).map_err(ApiError::conflict)?;
    let committed_target_fingerprint =
        fingerprint_path(&target.entry_path).map_err(ApiError::conflict)?;
    require_match(
        "targetFingerprint",
        &preview.target_fingerprint,
        &committed_target_fingerprint,
    )?;
    if let Some(expected) = preview.content_digest.as_deref() {
        require_match("contentDigest", expected, &bundle.content_digest)?;
    }
    if let Some(expected) = preview.manifest_digest.as_deref() {
        require_match("manifestDigest", expected, &bundle.manifest_digest)?;
    }
    let source = json!({
        "kind": "adoption",
        "targetHash": hash_secret(&preview.target_path),
        "runtime": request.adoption.runtime,
        "scope": request.adoption.scope,
    });
    let store_relative_path = if mode == "import_copy" {
        artifact_store_relative_path(&bundle, &source)
    } else {
        format!(
            "record-only/{}/{}",
            digest_segment(&bundle.content_digest)?,
            safe_skill_name(&request.adoption.skill_name)?
        )
    };
    let known_artifact = state
        .store
        .list_skill_governance_managed_artifacts()
        .await?
        .into_iter()
        .find(|artifact| {
            artifact.content_digest == bundle.content_digest
                && artifact.manifest_digest == bundle.manifest_digest
        });
    if mode != "keep_foreign" && known_artifact.is_none() {
        return Err(ApiError::conflict(
            "adoption requires a matching verified managed or Library artifact",
        ));
    }
    let artifact = if let Some(artifact) = known_artifact {
        artifact
    } else {
        state
            .store
            .create_skill_governance_managed_artifact(NewSkillGovernanceManagedArtifact {
                artifact_key: artifact_key(&bundle, &source)?,
                artifact_kind: "foreign_observation".to_owned(),
                source_provenance: source,
                content_digest: bundle.content_digest.clone(),
                manifest_digest: bundle.manifest_digest.clone(),
                schema_version: 1,
                revision: bundle.content_digest.clone(),
                store_relative_path,
                artifact: json!({"adoptionMode": mode, "materialized": false}),
                metadata: json!({"previewHash": preview.preview_hash, "observedOnly": true}),
            })
            .await?
    };
    let target_path = target.entry_path.to_string_lossy().into_owned();
    let input = adoption_materialization_input(
        artifact.id,
        scope,
        persisted_scope_id.clone(),
        &target_path,
        &request.adoption.runtime,
        root_kind_for_scope(scope),
        if mode == "import_copy" {
            SkillGovernanceInstallationMode::Copy
        } else {
            SkillGovernanceInstallationMode::InPlace
        },
        if mode == "keep_foreign" {
            SkillGovernanceMaterializationOwnership::Foreign
        } else {
            SkillGovernanceMaterializationOwnership::Adopted
        },
        &bundle.content_digest,
        &preview.target_fingerprint,
        json!({
            "mode": mode,
            "previewHash": preview.preview_hash,
            "newSessionRequired": true,
            "sessionEffective": "unknown"
        }),
    );

    if mode == "keep_foreign" {
        return Ok(Json(
            state
                .store
                .upsert_skill_governance_materialization(input, request.adoption.expected_version)
                .await?,
        ));
    }

    if mode == "record_only" {
        let foreign = state
            .store
            .upsert_skill_governance_materialization(
                adoption_materialization_input(
                    artifact.id,
                    scope,
                    persisted_scope_id,
                    &target_path,
                    &request.adoption.runtime,
                    root_kind_for_scope(scope),
                    SkillGovernanceInstallationMode::InPlace,
                    SkillGovernanceMaterializationOwnership::Foreign,
                    &bundle.content_digest,
                    &preview.target_fingerprint,
                    json!({"mode": "record_only", "previewHash": preview.preview_hash, "adoptionPending": true}),
                ),
                request.adoption.expected_version,
            )
            .await?;
        return Ok(Json(
            state
                .store
                .adopt_skill_governance_materialization(
                    foreign.id,
                    foreign.version,
                    json!({
                        "mode": "record_only",
                        "previewHash": preview.preview_hash,
                        "expectedFingerprint": preview.target_fingerprint,
                        "diskMutation": false
                    }),
                )
                .await?,
        ));
    }

    let materialization = execute_import_copy_adoption(
        &state,
        &request,
        &preview,
        artifact.id,
        target,
        bundle,
        input,
    )
    .await?;
    Ok(Json(materialization))
}

#[allow(clippy::too_many_arguments)]
async fn execute_import_copy_adoption(
    state: &AppState,
    request: &AdoptionCommitRequest,
    preview: &AdoptionPreview,
    artifact_id: Uuid,
    target: GovernanceSkillTarget,
    bundle: ArtifactBundle,
    mut materialization_input: NewSkillGovernanceMaterialization,
) -> Result<SkillGovernanceMaterialization, ApiError> {
    let scope = parse_store_scope(&request.adoption.scope)?;
    let scope_id = if scope == StoreGovernanceScope::Machine {
        "machine".to_owned()
    } else {
        request.adoption.scope_id.clone()
    };
    let operation_key = format!("adoption:{}", request.idempotency_key);
    if let Some(existing_run) = state
        .store
        .get_skill_governance_apply_run_by_idempotency(scope, &scope_id, &operation_key)
        .await?
    {
        if existing_run.nonce != request.confirmation_nonce
            || existing_run.observation_hash != preview.preview_hash
        {
            return Err(ApiError::conflict(
                "adoption idempotency key was used for different evidence",
            ));
        }
        if existing_run.status == SkillGovernanceApplyRunStatus::Succeeded {
            return state
                .store
                .get_skill_governance_materialization_for_target(
                    scope,
                    &scope_id,
                    &target.entry_path.to_string_lossy(),
                )
                .await?
                .ok_or_else(|| {
                    ApiError::conflict("completed adoption has no materialization receipt")
                });
        }
        return Err(ApiError::conflict(
            "adoption idempotency key has an incomplete recovery run",
        ));
    }

    let lease_nonce = Uuid::new_v4().to_string();
    let acquired = state
        .store
        .acquire_skill_governance_lock(
            scope,
            &scope_id,
            &format!("adoption:{operation_key}"),
            Some(i64::from(std::process::id())),
            None,
            &lease_nonce,
            Utc::now() + MANAGED_OPERATION_LEASE,
        )
        .await?;
    let run = state
        .store
        .create_skill_governance_apply_run(NewSkillGovernanceApplyRun {
            scope,
            scope_id: scope_id.clone(),
            plan_id: None,
            lock_id: Some(acquired.lock.id),
            idempotency_key: operation_key,
            nonce: request.confirmation_nonce.clone(),
            observation_hash: preview.preview_hash.clone(),
            desired_hash: preview.preview_hash.clone(),
            lock_hash: preview.preview_hash.clone(),
            backup_path: None,
            quarantine_path: None,
            evidence: json!({
                "phase": "preflight",
                "operation": "adoption_import_copy",
                "previewHash": preview.preview_hash,
                "applied": false,
                "highRisk": true
            }),
        })
        .await;
    let mut run = match run {
        Ok(run) => run,
        Err(error) => {
            release_managed_operation_lock(state, acquired.lock.id, &lease_nonce).await;
            return Err(error.into());
        }
    };
    if let Err(error) = state
        .store
        .attach_skill_governance_lock_run(
            acquired.lock.id,
            acquired.lock.version,
            &lease_nonce,
            run.id,
            Utc::now() + MANAGED_OPERATION_LEASE,
        )
        .await
    {
        release_managed_operation_lock(state, acquired.lock.id, &lease_nonce).await;
        return Err(error.into());
    }
    run = state
        .store
        .transition_skill_governance_apply_run(
            run.id,
            run.version,
            SkillGovernanceApplyRunStatus::Running,
            SkillGovernanceRecoveryStatus::NotRequired,
            None,
            None,
            json!({
                "phase": "locked",
                "operation": "adoption_import_copy",
                "applied": false,
                "highRisk": true
            }),
            None,
        )
        .await?;

    let result = execute_import_copy_adoption_locked(
        state,
        request,
        preview,
        artifact_id,
        target,
        bundle,
        &mut materialization_input,
        run.clone(),
    )
    .await;
    let result = match result {
        Ok((materialization, current_run)) => Ok((materialization, current_run)),
        Err(error) => {
            recover_import_copy_adoption(state, &run, &materialization_input.target_path, &error)
                .await;
            Err(ApiError::conflict(error))
        }
    };
    release_managed_operation_lock(state, acquired.lock.id, &lease_nonce).await;
    result.map(|(materialization, _)| materialization)
}

#[allow(clippy::too_many_arguments)]
async fn execute_import_copy_adoption_locked(
    state: &AppState,
    request: &AdoptionCommitRequest,
    preview: &AdoptionPreview,
    artifact_id: Uuid,
    target: GovernanceSkillTarget,
    bundle: ArtifactBundle,
    materialization_input: &mut NewSkillGovernanceMaterialization,
    run: SkillGovernanceApplyRun,
) -> Result<(SkillGovernanceMaterialization, SkillGovernanceApplyRun), String> {
    let action = state
        .store
        .create_skill_governance_apply_action(NewSkillGovernanceApplyAction {
            run_id: run.id,
            sequence: 0,
            action_key: "adoption:import_copy".to_owned(),
            request_hash: preview.preview_hash.clone(),
            backup_path: None,
            quarantine_path: None,
            evidence: json!({"phase": "pending", "operation": "adoption_import_copy"}),
        })
        .await
        .map_err(|_| "persist adoption action journal".to_owned())?;
    let mut action = transition_managed_action(
        state,
        action,
        SkillGovernanceApplyActionStatus::Preflight,
        None,
        "preflight",
    )
    .await?;
    let managed_root = state
        .runtime
        .governance_managed_artifact_root()
        .await
        .map_err(|_| "resolve managed artifact root".to_owned())?;
    let mut prepared =
        prepare_import_copy_adoption(managed_root, target, bundle.clone(), run.id, action.id)
            .await
            .map_err(|_| "prepare import-copy adoption".to_owned())?;
    if prepared.target_mutation.receipt().before_fingerprint != preview.target_fingerprint {
        return Err("adoption target changed after preview".to_owned());
    }
    let previous = state
        .store
        .get_skill_governance_materialization_for_target(
            materialization_input.scope,
            &materialization_input.scope_id,
            &materialization_input.target_path,
        )
        .await
        .map_err(|_| "load prior adoption ownership".to_owned())?;
    prepared
        .target_mutation
        .receipt_mut()
        .governance_state_before = previous
        .as_ref()
        .and_then(|row| serde_json::to_value(row).ok());
    let receipt = prepared.target_mutation.receipt().clone();
    action = transition_managed_action(
        state,
        action,
        SkillGovernanceApplyActionStatus::Locked,
        Some(&receipt),
        "locked",
    )
    .await?;

    if let Some(managed_mutation) = prepared.managed_mutation.as_ref() {
        backup_atomic_mutation(managed_mutation)?;
        stage_atomic_mutation(managed_mutation, Some(&bundle))?;
        activate_atomic_mutation(managed_mutation)?;
    }
    backup_atomic_mutation(&prepared.target_mutation)?;
    action = transition_managed_action(
        state,
        action,
        SkillGovernanceApplyActionStatus::BackedUp,
        Some(&receipt),
        "backed_up",
    )
    .await?;
    stage_atomic_mutation(&prepared.target_mutation, Some(&prepared.stored_artifact))?;
    action = transition_managed_action(
        state,
        action,
        SkillGovernanceApplyActionStatus::Staged,
        Some(&receipt),
        "staged",
    )
    .await?;
    activate_atomic_mutation(&prepared.target_mutation)?;
    action = transition_managed_action(
        state,
        action,
        SkillGovernanceApplyActionStatus::Written,
        Some(&receipt),
        "written",
    )
    .await?;

    materialization_input.artifact_id = artifact_id;
    materialization_input.ownership = SkillGovernanceMaterializationOwnership::Foreign;
    materialization_input.installation_mode = SkillGovernanceInstallationMode::Copy;
    materialization_input
        .expected_fingerprint
        .clone_from(&receipt.after_fingerprint);
    materialization_input
        .content_digest
        .clone_from(&prepared.stored_artifact.content_digest);
    materialization_input.receipt = json!({
        "mode": "import_copy",
        "previewHash": preview.preview_hash,
        "idempotencyKeyHash": hash_secret(&request.idempotency_key),
        "receipt": receipt,
        "newSessionRequired": true,
        "sessionEffective": "unknown"
    });
    let foreign = state
        .store
        .upsert_skill_governance_materialization(
            materialization_input.clone(),
            request.adoption.expected_version,
        )
        .await
        .map_err(|_| "persist import-copy materialization receipt".to_owned())?;
    let materialization = state
        .store
        .adopt_skill_governance_materialization(
            foreign.id,
            foreign.version,
            json!({
                "mode": "import_copy",
                "previewHash": preview.preview_hash,
                "expectedFingerprint": receipt.after_fingerprint,
                "backupRef": receipt.backup_ref,
                "journalRunId": run.id
            }),
        )
        .await
        .map_err(|_| "audit import-copy ownership transition".to_owned())?;
    action = transition_managed_action(
        state,
        action,
        SkillGovernanceApplyActionStatus::Refreshing,
        Some(&receipt),
        "refreshing",
    )
    .await?;
    state.skill_snapshots.invalidate_all().await;
    crate::skill_http::governance_observation(state, true)
        .await
        .map_err(|_| "fresh inventory unavailable after import-copy adoption".to_owned())?;
    let actual = fingerprint_path(FsPath::new(&receipt.target))?;
    if actual != receipt.after_fingerprint {
        return Err("import-copy adoption verification mismatch".to_owned());
    }
    let _ = transition_managed_action(
        state,
        action,
        SkillGovernanceApplyActionStatus::Verified,
        Some(&receipt),
        "verified",
    )
    .await?;
    let run = state
        .store
        .transition_skill_governance_apply_run(
            run.id,
            run.version,
            SkillGovernanceApplyRunStatus::Succeeded,
            SkillGovernanceRecoveryStatus::NotRequired,
            receipt.backup_ref.as_deref(),
            receipt.quarantine_ref.as_deref(),
            json!({
                "phase": "verified",
                "operation": "adoption_import_copy",
                "materializationId": materialization.id,
                "applied": true,
                "highRisk": true,
                "artifactStored": true,
                "materializedOnDisk": true,
                "runtimeDiscovered": "unknown",
                "sessionEffective": "unknown",
                "newSessionRequired": true
            }),
            None,
        )
        .await
        .map_err(|_| "complete adoption journal run".to_owned())?;
    Ok((materialization, run))
}

async fn transition_managed_action(
    state: &AppState,
    action: SkillGovernanceApplyAction,
    status: SkillGovernanceApplyActionStatus,
    receipt: Option<&MutationReceipt>,
    phase: &str,
) -> Result<SkillGovernanceApplyAction, String> {
    let evidence = receipt.map_or_else(
        || json!({"phase": phase, "operation": "adoption_import_copy"}),
        |receipt| {
            json!({
                "phase": phase,
                "operation": "adoption_import_copy",
                "receipt": receipt
            })
        },
    );
    let result_hash = matches!(
        status,
        SkillGovernanceApplyActionStatus::Written
            | SkillGovernanceApplyActionStatus::Refreshing
            | SkillGovernanceApplyActionStatus::Verified
    )
    .then(|| receipt.map(|value| value.after_fingerprint.as_str()))
    .flatten();
    state
        .store
        .transition_skill_governance_apply_action(
            action.id,
            action.version,
            status,
            result_hash,
            receipt.and_then(|value| value.backup_ref.as_deref()),
            receipt.and_then(|value| value.quarantine_ref.as_deref()),
            evidence,
            None,
        )
        .await
        .map_err(|_| format!("persist adoption journal boundary {phase}"))
}

async fn recover_import_copy_adoption(
    state: &AppState,
    run: &SkillGovernanceApplyRun,
    target_path: &str,
    error: &str,
) {
    let action = state
        .store
        .list_skill_governance_apply_actions(run.id)
        .await
        .ok()
        .and_then(|mut actions| actions.pop());
    let receipt = action.as_ref().and_then(|action| {
        action
            .evidence
            .get("receipt")
            .cloned()
            .and_then(|value| serde_json::from_value::<MutationReceipt>(value).ok())
    });
    let disk_recovered = receipt
        .as_ref()
        .map_or(true, |receipt| rollback_atomic_mutation(receipt).is_ok());
    let metadata_recovered = if disk_recovered {
        reconcile_adoption_metadata(
            state,
            run.scope,
            &run.scope_id,
            target_path,
            receipt.as_ref(),
        )
        .await
        .is_ok()
    } else {
        false
    };
    let recovered = disk_recovered && metadata_recovered;
    if let Some(action) = action {
        let _ = state
            .store
            .transition_skill_governance_apply_action(
                action.id,
                action.version,
                if recovered {
                    SkillGovernanceApplyActionStatus::RolledBack
                } else {
                    SkillGovernanceApplyActionStatus::RecoveryRequired
                },
                action.result_hash.as_deref(),
                action.backup_path.as_deref(),
                action.quarantine_path.as_deref(),
                json!({
                    "phase": if recovered { "rolled_back" } else { "recovery_required" },
                    "operation": "adoption_import_copy",
                    "receipt": receipt,
                    "errorType": "adoption_apply_failure"
                }),
                Some(error),
            )
            .await;
    }
    if let Ok(Some(current)) = state.store.get_skill_governance_apply_run(run.id).await {
        let _ = state
            .store
            .transition_skill_governance_apply_run(
                current.id,
                current.version,
                if recovered {
                    SkillGovernanceApplyRunStatus::RolledBack
                } else {
                    SkillGovernanceApplyRunStatus::RecoveryRequired
                },
                if recovered {
                    SkillGovernanceRecoveryStatus::Recovered
                } else {
                    SkillGovernanceRecoveryStatus::Failed
                },
                receipt
                    .as_ref()
                    .and_then(|value| value.backup_ref.as_deref()),
                receipt
                    .as_ref()
                    .and_then(|value| value.quarantine_ref.as_deref()),
                json!({
                    "phase": if recovered { "rolled_back" } else { "recovery_required" },
                    "operation": "adoption_import_copy",
                    "errorType": "adoption_apply_failure"
                }),
                Some(error),
            )
            .await;
    }
}

async fn reconcile_adoption_metadata(
    state: &AppState,
    scope: StoreGovernanceScope,
    scope_id: &str,
    target_path: &str,
    receipt: Option<&MutationReceipt>,
) -> Result<(), String> {
    let current = state
        .store
        .get_skill_governance_materialization_for_target(scope, scope_id, target_path)
        .await
        .map_err(|_| "load adoption recovery metadata".to_owned())?;
    let previous = receipt
        .and_then(|value| value.governance_state_before.clone())
        .and_then(|value| serde_json::from_value::<SkillGovernanceMaterialization>(value).ok());
    if let Some(previous) = previous {
        state
            .store
            .upsert_skill_governance_materialization(
                NewSkillGovernanceMaterialization {
                    artifact_id: previous.artifact_id,
                    scope: previous.scope,
                    scope_id: previous.scope_id,
                    target_path: previous.target_path,
                    target_runtime: previous.target_runtime,
                    root_kind: previous.root_kind,
                    installation_mode: previous.installation_mode,
                    ownership: previous.ownership,
                    content_digest: previous.content_digest,
                    expected_destination: previous.expected_destination,
                    expected_fingerprint: previous.expected_fingerprint,
                    verify_status: previous.verify_status,
                    receipt: previous.receipt,
                },
                current.map(|value| value.version),
            )
            .await
            .map_err(|_| "restore adoption recovery metadata".to_owned())?;
    } else if let Some(mut current) = current {
        if current.ownership == SkillGovernanceMaterializationOwnership::Foreign {
            current = state
                .store
                .adopt_skill_governance_materialization(
                    current.id,
                    current.version,
                    json!({"mode": "recovery_cleanup", "diskMutation": false}),
                )
                .await
                .map_err(|_| "prepare adoption metadata cleanup".to_owned())?;
        }
        state
            .store
            .delete_skill_governance_materialization_if_safe(
                current.id,
                current.version,
                Some(&current.expected_fingerprint),
            )
            .await
            .map_err(|_| "delete adoption recovery metadata".to_owned())?;
    }
    Ok(())
}

async fn release_managed_operation_lock(state: &AppState, lock_id: Uuid, lease_nonce: &str) {
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceLockfileQuery {
    workspace_id: Uuid,
    lockfile_path: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceLockfileInspect {
    workspace_id: String,
    lockfile_path: String,
    disk_hash: String,
    disk_fingerprint: String,
    stored: Option<cocli_store::SkillGovernanceWorkspaceLockfile>,
    exists: bool,
}

async fn inspect_workspace_lockfile(
    State(state): State<AppState>,
    Query(query): Query<WorkspaceLockfileQuery>,
) -> Result<Json<WorkspaceLockfileInspect>, ApiError> {
    let path = normalized_lockfile_path(query.lockfile_path.as_deref())?;
    let workspace_root = resolve_workspace_root(&state, query.workspace_id).await?;
    let disk_path = workspace_root.join(&path);
    let disk_hash = hash_file_or_missing(&disk_path).map_err(ApiError::conflict)?;
    let disk_fingerprint = fingerprint_path(&disk_path).map_err(ApiError::conflict)?;
    let stored = state
        .store
        .get_skill_governance_workspace_lockfile(&query.workspace_id.to_string(), &path)
        .await?;
    Ok(Json(WorkspaceLockfileInspect {
        workspace_id: query.workspace_id.to_string(),
        lockfile_path: path,
        exists: disk_path.exists(),
        disk_hash,
        disk_fingerprint,
        stored,
    }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LockfileRestoreRequest {
    workspace_id: Uuid,
    lockfile_path: Option<String>,
    expected_version: i64,
    expected_disk_hash: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LockfileRestorePreview {
    workspace_id: String,
    lockfile_path: String,
    before_hash: String,
    after_hash: String,
    bytes: usize,
    preview_hash: String,
    idempotency_key: String,
    confirmation_nonce: String,
}

async fn preview_lockfile_restore(
    State(state): State<AppState>,
    Json(request): Json<LockfileRestoreRequest>,
) -> Result<Json<LockfileRestorePreview>, ApiError> {
    Ok(Json(
        build_lockfile_restore_preview(&state, &request).await?,
    ))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LockfileRestoreCommitRequest {
    #[serde(flatten)]
    restore: LockfileRestoreRequest,
    expected_preview_hash: String,
    confirmation_nonce: String,
    idempotency_key: String,
}

async fn commit_lockfile_restore(
    State(state): State<AppState>,
    Json(request): Json<LockfileRestoreCommitRequest>,
) -> Result<Json<LockfileRestorePreview>, ApiError> {
    let preview = build_lockfile_restore_preview(&state, &request.restore).await?;
    require_match(
        "previewHash",
        &request.expected_preview_hash,
        &preview.preview_hash,
    )?;
    require_confirmation_nonce(
        "lockfile-restore",
        &preview.preview_hash,
        &request.idempotency_key,
        &request.confirmation_nonce,
    )?;
    let workspace_root = resolve_workspace_root(&state, request.restore.workspace_id).await?;
    let disk_path = workspace_root.join(&preview.lockfile_path);
    let stored = state
        .store
        .get_skill_governance_workspace_lockfile(
            &request.restore.workspace_id.to_string(),
            &preview.lockfile_path,
        )
        .await?
        .ok_or_else(|| ApiError::not_found("workspace lockfile snapshot not found"))?;
    if stored.version != request.restore.expected_version {
        return Err(ApiError::conflict("workspace lockfile version changed"));
    }
    let (restore_document, restore_lock_hash, bytes) =
        lockfile_restore_document_and_bytes(&stored)?;
    let next_restore_document = stored.document.clone();
    let next_restore_lock_hash = stored.lock_hash.clone();
    let current_fingerprint = fingerprint_path(&disk_path).map_err(ApiError::conflict)?;
    require_match(
        "diskFingerprint",
        &stored.expected_disk_fingerprint,
        &current_fingerprint,
    )?;
    let run_id = Uuid::new_v4();
    let action_id = Uuid::new_v4();
    let prepared = tokio::task::spawn_blocking({
        let workspace_root = workspace_root.clone();
        let disk_path = disk_path.clone();
        let bytes = bytes.clone();
        let expected = current_fingerprint.clone();
        move || {
            prepare_atomic_file_mutation(
                &workspace_root,
                &disk_path,
                &expected,
                &bytes,
                run_id,
                action_id,
            )
        }
    })
    .await
    .map_err(|_| ApiError::conflict("lockfile restore preparation task failed"))?
    .map_err(ApiError::conflict)?;
    let receipt = prepared.receipt().clone();
    let result = tokio::task::spawn_blocking(move || {
        backup_atomic_file_mutation(&prepared)
            .and_then(|()| stage_atomic_file_mutation(&prepared))
            .and_then(|()| activate_atomic_file_mutation(&prepared))
    })
    .await
    .map_err(|_| ApiError::conflict("lockfile restore mutation task failed"))?;
    if let Err(error) = result {
        let _ = rollback_atomic_mutation(&receipt);
        return Err(ApiError::conflict(error));
    }
    state
        .store
        .upsert_skill_governance_workspace_lockfile(
            &request.restore.workspace_id.to_string(),
            &preview.lockfile_path,
            &restore_lock_hash,
            &receipt.after_fingerprint,
            &receipt.after_fingerprint,
            restore_document,
            receipt.backup_ref.as_deref(),
            (receipt.before_fingerprint != "missing")
                .then_some(receipt.before_fingerprint.as_str()),
            serde_json::to_value(&receipt)
                .map_err(|_| ApiError::bad_request("encode lockfile restore receipt"))?,
            json!({
                "restoredFrom": stored.id,
                "restorePreviewHash": preview.preview_hash,
                "restoreDocument": next_restore_document,
                "restoreLockHash": next_restore_lock_hash,
                "casProtected": true
            }),
            Some(stored.version),
        )
        .await?;
    Ok(Json(preview))
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GcCandidateView {
    entity_type: String,
    entity_id: Uuid,
    reason: String,
    expected_version: i64,
    expected_fingerprint: Option<String>,
    ownership: Option<SkillGovernanceMaterializationOwnership>,
    reference_state_hash: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GcPreviewResponse {
    candidates: Vec<GcCandidateView>,
    preview_hash: String,
    idempotency_key: String,
    confirmation_nonce: String,
}

async fn preview_gc(State(state): State<AppState>) -> Result<Json<GcPreviewResponse>, ApiError> {
    let candidates =
        gc_candidate_views(&state, state.store.preview_skill_governance_gc().await?).await?;
    let preview_hash = canonical_hash(&candidates).map_err(ApiError::bad_request)?;
    let auth = preview_auth("gc-commit", &preview_hash)?;
    Ok(Json(GcPreviewResponse {
        candidates,
        preview_hash,
        idempotency_key: auth.idempotency_key,
        confirmation_nonce: auth.confirmation_nonce,
    }))
}

async fn gc_candidate_views(
    state: &AppState,
    candidates: Vec<SkillGovernanceGcCandidate>,
) -> Result<Vec<GcCandidateView>, ApiError> {
    let mut views = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        match candidate.entity_type.as_str() {
            "managed_artifact" => {
                let Some(artifact) = state
                    .store
                    .get_skill_governance_managed_artifact(candidate.entity_id)
                    .await?
                else {
                    continue;
                };
                views.push(GcCandidateView {
                    entity_type: candidate.entity_type,
                    entity_id: candidate.entity_id,
                    reason: candidate.reason,
                    expected_version: artifact.version,
                    expected_fingerprint: Some(artifact.content_digest.clone()),
                    ownership: None,
                    reference_state_hash: canonical_hash(&(
                        "unreferenced_managed_artifact",
                        artifact.id,
                        artifact.version,
                        &artifact.content_digest,
                        &artifact.manifest_digest,
                        &artifact.store_relative_path,
                    ))
                    .map_err(ApiError::bad_request)?,
                });
            }
            "materialization" => {
                let Some(materialization) = state
                    .store
                    .get_skill_governance_materialization(candidate.entity_id)
                    .await?
                else {
                    continue;
                };
                views.push(GcCandidateView {
                    entity_type: candidate.entity_type,
                    entity_id: candidate.entity_id,
                    reason: candidate.reason,
                    expected_version: materialization.version,
                    expected_fingerprint: Some(materialization.expected_fingerprint.clone()),
                    ownership: Some(materialization.ownership),
                    reference_state_hash: canonical_hash(&(
                        "unreferenced_materialization",
                        materialization.id,
                        materialization.version,
                        materialization.ownership,
                        &materialization.expected_fingerprint,
                    ))
                    .map_err(ApiError::bad_request)?,
                });
            }
            _ => {}
        }
    }
    Ok(views)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GcCommitRequest {
    expected_preview_hash: String,
    confirmation_nonce: String,
    idempotency_key: String,
}

async fn commit_gc(
    State(state): State<AppState>,
    Json(request): Json<GcCommitRequest>,
) -> Result<Json<GcPreviewResponse>, ApiError> {
    let preview = preview_gc(State(state.clone())).await?.0;
    require_match(
        "previewHash",
        &request.expected_preview_hash,
        &preview.preview_hash,
    )?;
    require_confirmation_nonce(
        "gc-commit",
        &preview.preview_hash,
        &request.idempotency_key,
        &request.confirmation_nonce,
    )?;
    let store_root = state.runtime.governance_managed_artifact_root().await?;
    let canonical_store_root =
        canonicalize_or_create_store_root(&store_root).map_err(ApiError::conflict)?;
    for candidate in &preview.candidates {
        match candidate.entity_type.as_str() {
            "managed_artifact" => {
                let Some(artifact) = state
                    .store
                    .get_skill_governance_managed_artifact(candidate.entity_id)
                    .await?
                else {
                    continue;
                };
                require_entity_version(
                    "managed artifact",
                    artifact.version,
                    candidate.expected_version,
                )?;
                if candidate.expected_fingerprint.as_deref()
                    != Some(artifact.content_digest.as_str())
                {
                    return Err(ApiError::conflict("managed artifact fingerprint changed"));
                }
                verify_managed_artifact_store_entry(&canonical_store_root, &artifact)
                    .map_err(ApiError::conflict)?;
                let quarantine = quarantine_artifact_store_path(
                    &canonical_store_root,
                    &artifact.store_relative_path,
                )
                .map_err(ApiError::conflict)?;
                let delete = state
                    .store
                    .delete_skill_governance_managed_artifact(
                        artifact.id,
                        candidate.expected_version,
                    )
                    .await;
                if let Err(error) = delete {
                    if let Some((original, quarantine)) = quarantine {
                        let _ = fs::rename(quarantine, original);
                    }
                    return Err(error.into());
                }
                if let Some((_, quarantine)) = quarantine {
                    remove_quarantine_path(&quarantine).map_err(ApiError::conflict)?;
                }
            }
            "materialization" => {
                let Some(materialization) = state
                    .store
                    .get_skill_governance_materialization(candidate.entity_id)
                    .await?
                else {
                    continue;
                };
                require_entity_version(
                    "materialization",
                    materialization.version,
                    candidate.expected_version,
                )?;
                if candidate.ownership != Some(materialization.ownership)
                    || candidate.expected_fingerprint.as_deref()
                        != Some(materialization.expected_fingerprint.as_str())
                {
                    return Err(ApiError::conflict("materialization CAS changed"));
                }
                let _ = state
                    .store
                    .delete_skill_governance_materialization_if_safe(
                        materialization.id,
                        candidate.expected_version,
                        candidate.expected_fingerprint.as_deref(),
                    )
                    .await?;
            }
            _ => {}
        }
    }
    preview_gc(State(state)).await
}

async fn load_artifact_for_request(
    state: &AppState,
    request: &ManagedArtifactPreviewRequest,
) -> Result<(ArtifactBundle, Value), ApiError> {
    match request.source_kind.to_ascii_lowercase().as_str() {
        "local" => {
            let path = request.local_path.as_ref().ok_or_else(|| {
                ApiError::bad_request("localPath is required for local artifacts")
            })?;
            let path = PathBuf::from(path);
            if !path.is_absolute() {
                return Err(ApiError::bad_request(
                    "local managed artifact source must be absolute",
                ));
            }
            let bundle = tokio::task::spawn_blocking(move || load_local_artifact(&path))
                .await
                .map_err(|_| ApiError::bad_request("local artifact load task failed"))?
                .map_err(ApiError::bad_request)?;
            let canonical = bundle
                .canonical_source
                .as_deref()
                .unwrap_or(FsPath::new(""));
            let redacted = redacted_path(canonical);
            Ok((
                bundle,
                json!({"kind": "local", "redactedPath": redacted.redacted, "pathHash": redacted.hash}),
            ))
        }
        "library" => {
            let library_id = request.library_id.ok_or_else(|| {
                ApiError::bad_request("libraryId is required for library artifacts")
            })?;
            let library = state
                .store
                .get_skill_library(library_id)
                .await?
                .ok_or_else(|| ApiError::not_found("Skill Library entry not found"))?;
            let files = state.store.load_skill_library_files(library_id).await?;
            let bundle = load_vendored_artifact(&files).map_err(ApiError::bad_request)?;
            Ok((
                bundle,
                json!({"kind": "library", "libraryId": library.id, "name": library.name, "sourceRefHash": library.source_ref.map(|value| hash_secret(&value))}),
            ))
        }
        other => Err(ApiError::bad_request(format!(
            "unsupported managed artifact source kind: {other}"
        ))),
    }
}

fn preview_from_bundle(
    source_kind: String,
    source: Value,
    artifact_key: String,
    store_relative_path: String,
    bundle: &ArtifactBundle,
    hazards: Vec<String>,
) -> Result<ManagedArtifactPreview, ApiError> {
    let mut preview = ManagedArtifactPreview {
        source_kind,
        source,
        artifact_key,
        content_digest: bundle.content_digest.clone(),
        manifest_digest: bundle.manifest_digest.clone(),
        revision: bundle.content_digest.clone(),
        store_relative_path,
        preview_hash: String::new(),
        idempotency_key: String::new(),
        confirmation_nonce: String::new(),
        blocked: !hazards.is_empty(),
        hazards,
    };
    preview.preview_hash = canonical_hash(&(
        &preview.source_kind,
        &preview.source,
        &preview.artifact_key,
        &preview.content_digest,
        &preview.manifest_digest,
        &preview.revision,
        &preview.store_relative_path,
        &preview.hazards,
    ))
    .map_err(ApiError::bad_request)?;
    let auth = preview_auth("managed-artifact-commit", &preview.preview_hash)?;
    preview.idempotency_key = auth.idempotency_key;
    preview.confirmation_nonce = auth.confirmation_nonce;
    Ok(preview)
}

async fn build_adoption_preview(
    state: &AppState,
    request: &AdoptionRequest,
) -> Result<AdoptionPreview, ApiError> {
    let scope = parse_store_scope(&request.scope)?;
    let target = resolve_target(
        state,
        &request.runtime,
        scope,
        &request.scope_id,
        &request.skill_name,
    )
    .await?;
    let target_path = target.entry_path.to_string_lossy().into_owned();
    let target_fingerprint = fingerprint_path(&target.entry_path).map_err(ApiError::conflict)?;
    let mut hazards = target_hazards(&target).map_err(ApiError::conflict)?;
    let (content_digest, manifest_digest) = if target_fingerprint == "missing" {
        hazards.push("target_missing".to_owned());
        (None, None)
    } else {
        match load_existing_target_artifact(&target.entry_path) {
            Ok(bundle) => (Some(bundle.content_digest), Some(bundle.manifest_digest)),
            Err(error) => {
                hazards.push(format!("manual_review_required:{error}"));
                (None, None)
            }
        }
    };
    let existing = state
        .store
        .get_skill_governance_materialization_for_target(scope, &request.scope_id, &target_path)
        .await?;
    let existing_ownership = existing.as_ref().map(|row| row.ownership);
    let source_known = match (&content_digest, &manifest_digest) {
        (Some(content), Some(manifest)) => state
            .store
            .list_skill_governance_managed_artifacts()
            .await?
            .iter()
            .any(|artifact| {
                artifact.content_digest == *content && artifact.manifest_digest == *manifest
            }),
        _ => false,
    };
    if !source_known {
        hazards.push("unknown_source_provenance".to_owned());
    }
    hazards.sort();
    hazards.dedup();
    let mode = request.mode.as_deref().unwrap_or("record_only");
    let blocked = hazards
        .iter()
        .any(|hazard| mode != "keep_foreign" || hazard.as_str() != "unknown_source_provenance");
    let mut preview = AdoptionPreview {
        runtime: request.runtime.clone(),
        scope: request.scope.clone(),
        scope_id: request.scope_id.clone(),
        skill_name: request.skill_name.clone(),
        target_path,
        target_fingerprint,
        content_digest,
        manifest_digest,
        existing_ownership,
        blocked,
        hazards,
        preview_hash: String::new(),
        idempotency_key: String::new(),
        confirmation_nonce: String::new(),
    };
    preview.preview_hash = canonical_hash(&(
        &preview.runtime,
        &preview.scope,
        &preview.scope_id,
        &preview.skill_name,
        &preview.target_path,
        &preview.target_fingerprint,
        &preview.content_digest,
        &preview.manifest_digest,
        &preview.existing_ownership,
        &preview.hazards,
    ))
    .map_err(ApiError::bad_request)?;
    let auth = preview_auth("adoption-commit", &preview.preview_hash)?;
    preview.idempotency_key = auth.idempotency_key;
    preview.confirmation_nonce = auth.confirmation_nonce;
    Ok(preview)
}

async fn resolve_target(
    state: &AppState,
    runtime: &str,
    scope: StoreGovernanceScope,
    scope_id: &str,
    skill_name: &str,
) -> Result<GovernanceSkillTarget, ApiError> {
    match scope {
        StoreGovernanceScope::Agent => {
            let agent_id = scope_id
                .parse::<Uuid>()
                .map_err(|_| ApiError::bad_request("Agent scopeId must be a UUID"))?;
            let agent = state
                .store
                .get_agent(agent_id)
                .await?
                .ok_or_else(|| ApiError::not_found("Agent not found"))?;
            state
                .runtime
                .governance_skill_target(&agent, skill_name)
                .await
                .map_err(runtime_api_error)
        }
        StoreGovernanceScope::Workspace => {
            let workspace_id = scope_id
                .parse::<Uuid>()
                .map_err(|_| ApiError::bad_request("Workspace scopeId must be a UUID"))?;
            let root = resolve_workspace_root(state, workspace_id).await?;
            state
                .runtime
                .governance_skill_target_in_scope(runtime, "workspace", Some(&root), skill_name)
                .await
                .map_err(runtime_api_error)
        }
        StoreGovernanceScope::Machine => state
            .runtime
            .governance_skill_target_in_scope(runtime, "machine", None, skill_name)
            .await
            .map_err(runtime_api_error),
    }
}

async fn resolve_workspace_root(state: &AppState, workspace_id: Uuid) -> Result<PathBuf, ApiError> {
    let locator = state
        .store
        .resolve_workspace(workspace_id)
        .await?
        .ok_or_else(|| ApiError::conflict("Workspace has no ready local directory binding"))?;
    let root = PathBuf::from(locator);
    tokio::task::spawn_blocking(move || root.canonicalize())
        .await
        .map_err(|_| ApiError::conflict("Workspace binding resolution task failed"))?
        .map_err(|_| ApiError::conflict("Workspace binding cannot be canonicalized"))
}

fn target_capability(
    runtime: &str,
    scope: &str,
    target: GovernanceSkillTarget,
) -> GovernanceScopeCapability {
    let exists = target.search_root.exists();
    let writable = fs::metadata(&target.search_root)
        .map(|metadata| !metadata.permissions().readonly())
        .unwrap_or(false);
    GovernanceScopeCapability {
        runtime: runtime.to_owned(),
        scope: scope.to_owned(),
        root_kind: if scope == "agent" {
            "agent"
        } else {
            "runtime_specific"
        }
        .to_owned(),
        path: target.search_root.to_string_lossy().into_owned(),
        status: if exists && writable {
            "supported"
        } else {
            "blocked"
        }
        .to_owned(),
        exists,
        writable,
        atomic_rename: exists && writable,
        supported: exists && writable,
        evidence: "runtime-derived canonical target".to_owned(),
        blocked_reason: (!exists)
            .then_some("agent Skill root does not exist".to_owned())
            .or_else(|| (!writable).then_some("agent Skill root is not writable".to_owned())),
    }
}

fn runtime_diagnostic(
    runtime: &str,
    scope: &str,
    phase: &str,
    error: RuntimeError,
    observed_at: chrono::DateTime<Utc>,
) -> GovernanceDiagnostic {
    diagnostic(
        &format!("{runtime}:{scope}"),
        phase,
        match error {
            RuntimeError::Unsupported(_) => "unsupported",
            RuntimeError::NotFound(_) => "not_found",
            RuntimeError::Busy(_) => "busy",
            RuntimeError::Delivery(_) => "runtime_error",
        },
        error.to_string(),
        observed_at,
    )
}

fn diagnostic(
    subject: &str,
    phase: &str,
    error_type: &str,
    message: impl Into<String>,
    observed_at: chrono::DateTime<Utc>,
) -> GovernanceDiagnostic {
    GovernanceDiagnostic {
        subject: subject.to_owned(),
        phase: phase.to_owned(),
        error_type: error_type.to_owned(),
        message: message.into(),
        observed_at,
    }
}

fn parse_store_scope(value: &str) -> Result<StoreGovernanceScope, ApiError> {
    match normalize_scope_name(value)?.as_str() {
        "machine" => Ok(StoreGovernanceScope::Machine),
        "workspace" => Ok(StoreGovernanceScope::Workspace),
        "agent" => Ok(StoreGovernanceScope::Agent),
        _ => unreachable!(),
    }
}

fn normalize_scope_name(value: &str) -> Result<String, ApiError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "machine" | "user" => Ok("machine".to_owned()),
        "workspace" | "project" => Ok("workspace".to_owned()),
        "agent" => Ok("agent".to_owned()),
        other => Err(ApiError::bad_request(format!(
            "unsupported governance scope: {other}"
        ))),
    }
}

fn root_kind_for_scope(scope: StoreGovernanceScope) -> SkillGovernanceMaterializationRootKind {
    match scope {
        StoreGovernanceScope::Machine => SkillGovernanceMaterializationRootKind::Machine,
        StoreGovernanceScope::Workspace => SkillGovernanceMaterializationRootKind::Workspace,
        StoreGovernanceScope::Agent => SkillGovernanceMaterializationRootKind::Agent,
    }
}

#[allow(clippy::too_many_arguments)]
fn adoption_materialization_input(
    artifact_id: Uuid,
    scope: StoreGovernanceScope,
    scope_id: String,
    target_path: &str,
    target_runtime: &str,
    root_kind: SkillGovernanceMaterializationRootKind,
    installation_mode: SkillGovernanceInstallationMode,
    ownership: SkillGovernanceMaterializationOwnership,
    content_digest: &str,
    expected_fingerprint: &str,
    receipt: Value,
) -> NewSkillGovernanceMaterialization {
    NewSkillGovernanceMaterialization {
        artifact_id,
        scope,
        scope_id,
        target_path: target_path.to_owned(),
        target_runtime: target_runtime.to_owned(),
        root_kind,
        installation_mode,
        ownership,
        content_digest: content_digest.to_owned(),
        expected_destination: target_path.to_owned(),
        expected_fingerprint: expected_fingerprint.to_owned(),
        verify_status: SkillGovernanceVerifyStatus::Verified,
        receipt,
    }
}

struct PreparedImportCopyAdoption {
    managed_mutation: Option<PreparedMutation>,
    target_mutation: PreparedMutation,
    stored_artifact: ArtifactBundle,
}

async fn prepare_import_copy_adoption(
    managed_root: PathBuf,
    target: GovernanceSkillTarget,
    artifact: ArtifactBundle,
    run_id: Uuid,
    action_id: Uuid,
) -> Result<PreparedImportCopyAdoption, ApiError> {
    tokio::task::spawn_blocking(move || {
        let (managed_mutation, stored_artifact, managed_key) =
            prepare_managed_artifact_mutation(&managed_root, &artifact, run_id, action_id)?;
        let mut mutation = prepare_atomic_mutation(
            &target,
            MutationMode::Copy,
            Some(&stored_artifact),
            run_id,
            action_id,
        )?;
        mutation.receipt_mut().managed_artifact_ref = Some(format!("managed:{managed_key}"));
        mutation.receipt_mut().managed_artifact_fingerprint =
            Some(stored_artifact.content_digest.clone());
        Ok::<_, String>(PreparedImportCopyAdoption {
            managed_mutation,
            target_mutation: mutation,
            stored_artifact,
        })
    })
    .await
    .map_err(|_| ApiError::conflict("import-copy adoption preparation task failed"))?
    .map_err(ApiError::conflict)
}

fn artifact_key(bundle: &ArtifactBundle, source: &Value) -> Result<String, ApiError> {
    canonical_hash(&(
        "managed-artifact",
        &bundle.content_digest,
        &bundle.manifest_digest,
        source,
    ))
    .map_err(ApiError::bad_request)
}

fn artifact_store_relative_path(bundle: &ArtifactBundle, source: &Value) -> String {
    let _ = source;
    bundle
        .content_digest
        .strip_prefix("sha256:")
        .unwrap_or(&bundle.content_digest)
        .to_owned()
}

fn digest_segment(digest: &str) -> Result<String, ApiError> {
    digest
        .strip_prefix("sha256:")
        .filter(|value| {
            value.len() >= 16
                && value
                    .chars()
                    .all(|ch| ch.is_ascii_hexdigit() && !ch.is_ascii_uppercase())
        })
        .map(ToOwned::to_owned)
        .ok_or_else(|| ApiError::bad_request("digest is not a supported sha256 value"))
}

fn materialize_artifact_store(
    store_root: &FsPath,
    relative_path: &str,
    bundle: &ArtifactBundle,
) -> Result<PathBuf, String> {
    let root = canonicalize_or_create_store_root(store_root)?;
    let target = safe_join(&root, relative_path)?;
    if target.exists() {
        let existing = load_local_artifact(&target)?;
        if existing.content_digest == bundle.content_digest
            && existing.manifest_digest == bundle.manifest_digest
        {
            return Ok(target);
        }
        return Err("managed artifact store path already contains different content".to_owned());
    }
    let staging = root.join(".staging").join(format!(
        "{}-{}",
        Utc::now().timestamp_nanos_opt().unwrap_or_default(),
        Uuid::new_v4()
    ));
    write_bundle(&root, &staging, bundle)?;
    if let Some(parent) = target.parent() {
        create_dir_all_guarded(&root, parent)?;
    }
    fs::rename(&staging, &target)
        .map_err(|error| safe_io_error("activate managed artifact", &error))?;
    sync_directory(target.parent().unwrap_or(&root))?;
    Ok(target)
}

fn verify_managed_artifact_store_entry(
    store_root: &FsPath,
    artifact: &SkillGovernanceManagedArtifact,
) -> Result<(), String> {
    let target = safe_join(store_root, &artifact.store_relative_path)?;
    let actual = load_local_artifact(&target)?;
    if actual.content_digest != artifact.content_digest
        || actual.manifest_digest != artifact.manifest_digest
    {
        return Err("managed artifact store content changed after preview".to_owned());
    }
    Ok(())
}

fn write_bundle(root: &FsPath, target: &FsPath, bundle: &ArtifactBundle) -> Result<(), String> {
    create_dir_all_guarded(root, target)?;
    for file in &bundle.files {
        let path = safe_join(target, &file.relative_path)?;
        if let Some(parent) = path.parent() {
            create_dir_all_guarded(target, parent)?;
        }
        let mut output = File::create(&path)
            .map_err(|error| safe_io_error("write managed artifact file", &error))?;
        output
            .write_all(&file.content)
            .map_err(|error| safe_io_error("write managed artifact file", &error))?;
        output
            .sync_all()
            .map_err(|error| safe_io_error("sync managed artifact file", &error))?;
    }
    Ok(())
}

fn canonicalize_or_create_store_root(store_root: &FsPath) -> Result<PathBuf, String> {
    fs::create_dir_all(store_root)
        .map_err(|error| safe_io_error("create managed artifact root", &error))?;
    store_root
        .canonicalize()
        .map_err(|error| safe_io_error("canonicalize managed artifact root", &error))
}

fn quarantine_artifact_store_path(
    root: &FsPath,
    relative_path: &str,
) -> Result<Option<(PathBuf, PathBuf)>, String> {
    let target = safe_join(root, relative_path)?;
    if !target.exists() {
        return Ok(None);
    }
    if fs::symlink_metadata(&target)
        .map_err(|error| safe_io_error("inspect managed artifact target", &error))?
        .file_type()
        .is_symlink()
    {
        return Err("managed artifact GC refuses symlink targets".to_owned());
    }
    let quarantine_root = root.join(".gc-quarantine");
    create_dir_all_guarded(root, &quarantine_root)?;
    let quarantine = quarantine_root.join(format!(
        "{}-{}",
        Utc::now().timestamp_nanos_opt().unwrap_or_default(),
        Uuid::new_v4()
    ));
    fs::rename(&target, &quarantine)
        .map_err(|error| safe_io_error("quarantine managed artifact", &error))?;
    sync_directory(target.parent().unwrap_or(root))?;
    Ok(Some((target, quarantine)))
}

fn remove_quarantine_path(path: &FsPath) -> Result<(), String> {
    if fs::symlink_metadata(path)
        .map_err(|error| safe_io_error("inspect managed artifact quarantine", &error))?
        .file_type()
        .is_symlink()
    {
        return Err("managed artifact GC refuses symlink quarantine".to_owned());
    }
    if path.is_dir() {
        fs::remove_dir_all(path)
            .map_err(|error| safe_io_error("remove managed artifact quarantine", &error))?;
    } else {
        fs::remove_file(path)
            .map_err(|error| safe_io_error("remove managed artifact quarantine", &error))?;
    }
    Ok(())
}

fn load_existing_target_artifact(path: &FsPath) -> Result<ArtifactBundle, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| safe_io_error("inspect adoption target", &error))?;
    if metadata.file_type().is_symlink() {
        let resolved = path
            .canonicalize()
            .map_err(|error| safe_io_error("resolve adoption symlink", &error))?;
        return load_local_artifact(&resolved);
    }
    load_local_artifact(path)
}

fn target_hazards(target: &GovernanceSkillTarget) -> Result<Vec<String>, String> {
    let mut hazards = Vec::new();
    if !target.entry_path.starts_with(&target.search_root) {
        hazards.push("target_outside_search_root".to_owned());
    }
    if !target.search_root.starts_with(&target.scope_root) {
        hazards.push("search_root_outside_scope_root".to_owned());
    }
    if target
        .entry_path
        .strip_prefix(&target.search_root)
        .map_or(true, |relative| {
            relative.components().any(is_forbidden_component)
        })
    {
        hazards.push("path_traversal".to_owned());
    }
    let metadata = match fs::symlink_metadata(&target.entry_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(hazards),
        Err(error) => return Err(safe_io_error("inspect adoption target hazards", &error)),
    };
    if metadata.file_type().is_symlink() {
        match target.entry_path.canonicalize() {
            Ok(resolved) if !resolved.starts_with(&target.search_root) => {
                hazards.push("symlink_escape".to_owned());
            }
            Ok(_) => {}
            Err(_) => hazards.push("broken_or_circular_symlink".to_owned()),
        }
    }
    collect_sensitive_hazards(&target.entry_path, &mut hazards)?;
    hazards.sort();
    hazards.dedup();
    Ok(hazards)
}

fn artifact_hazards(bundle: &ArtifactBundle, root: Option<&FsPath>) -> Vec<String> {
    let mut hazards = Vec::new();
    for file in &bundle.files {
        if file.relative_path.starts_with(".git/")
            || file.relative_path == ".git"
            || file.relative_path.contains("/.git/")
        {
            hazards.push("nested_git_metadata".to_owned());
        }
        if file.relative_path.ends_with("postinstall")
            || file.relative_path.ends_with(".hook")
            || file.relative_path.contains("/hooks/")
        {
            hazards.push("executable_hook_or_script".to_owned());
        }
        if file.relative_path.contains("id_rsa")
            || file.relative_path.ends_with(".pem")
            || file.relative_path.ends_with(".key")
            || file.relative_path.contains("secrets")
        {
            hazards.push("sensitive_file".to_owned());
        }
    }
    if let Some(root) = root {
        if root.join(".git").exists() {
            hazards.push("nested_git_worktree_or_submodule".to_owned());
        }
    }
    hazards.sort();
    hazards.dedup();
    hazards
}

fn collect_sensitive_hazards(path: &FsPath, hazards: &mut Vec<String>) -> Result<(), String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(safe_io_error("inspect adoption target", &error)),
    };
    if metadata.file_type().is_symlink() || metadata.is_file() {
        return Ok(());
    }
    let entries = fs::read_dir(path)
        .map_err(|error| safe_io_error("read adoption target", &error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| safe_io_error("read adoption target", &error))?;
    for entry in entries {
        let file_name = entry.file_name().to_string_lossy().to_string();
        let child = entry.path();
        if file_name == ".git" {
            hazards.push("nested_git_worktree_or_submodule".to_owned());
            continue;
        }
        if matches!(
            file_name.as_str(),
            "postinstall" | "preinstall" | "install.sh"
        ) || child
            .components()
            .any(|component| component.as_os_str() == "hooks")
        {
            hazards.push("executable_hook_or_script".to_owned());
        }
        if file_name.contains("secret")
            || file_name.ends_with(".pem")
            || file_name.ends_with(".key")
            || file_name == "id_rsa"
        {
            hazards.push("sensitive_file".to_owned());
        }
        let child_metadata = fs::symlink_metadata(&child)
            .map_err(|error| safe_io_error("inspect adoption target", &error))?;
        if child_metadata.is_dir() && !child_metadata.file_type().is_symlink() {
            collect_sensitive_hazards(&child, hazards)?;
        }
    }
    Ok(())
}

async fn build_lockfile_restore_preview(
    state: &AppState,
    request: &LockfileRestoreRequest,
) -> Result<LockfileRestorePreview, ApiError> {
    let lockfile_path = normalized_lockfile_path(request.lockfile_path.as_deref())?;
    let workspace_root = resolve_workspace_root(state, request.workspace_id).await?;
    let disk_path = workspace_root.join(&lockfile_path);
    let before_hash = hash_file_or_missing(&disk_path).map_err(ApiError::conflict)?;
    require_match("diskHash", &request.expected_disk_hash, &before_hash)?;
    let stored = state
        .store
        .get_skill_governance_workspace_lockfile(&request.workspace_id.to_string(), &lockfile_path)
        .await?
        .ok_or_else(|| ApiError::not_found("workspace lockfile snapshot not found"))?;
    if stored.version != request.expected_version {
        return Err(ApiError::conflict("workspace lockfile version changed"));
    }
    let (restore_document, restore_lock_hash, bytes) =
        lockfile_restore_document_and_bytes(&stored)?;
    if bytes.len() > MAX_LOCKFILE_BYTES {
        return Err(ApiError::bad_request(
            "workspace lockfile snapshot exceeds size limit",
        ));
    }
    let after_hash = format!("sha256:{}", sha256_hex(&bytes));
    let mut preview = LockfileRestorePreview {
        workspace_id: request.workspace_id.to_string(),
        lockfile_path,
        before_hash,
        after_hash,
        bytes: bytes.len(),
        preview_hash: String::new(),
        idempotency_key: String::new(),
        confirmation_nonce: String::new(),
    };
    preview.preview_hash = canonical_hash(&(
        &preview.workspace_id,
        &preview.lockfile_path,
        &preview.before_hash,
        &preview.after_hash,
        preview.bytes,
        &restore_document,
        &restore_lock_hash,
    ))
    .map_err(ApiError::bad_request)?;
    let auth = preview_auth("lockfile-restore", &preview.preview_hash)?;
    preview.idempotency_key = auth.idempotency_key;
    preview.confirmation_nonce = auth.confirmation_nonce;
    Ok(preview)
}

fn lockfile_restore_document_and_bytes(
    stored: &cocli_store::SkillGovernanceWorkspaceLockfile,
) -> Result<(Value, String, Vec<u8>), ApiError> {
    let document = stored
        .restore_metadata
        .get("restoreDocument")
        .or_else(|| stored.restore_metadata.get("candidateDocument"))
        .cloned()
        .ok_or_else(|| {
            ApiError::conflict("workspace lockfile restore requires a persisted restore candidate")
        })?;
    let lock_hash = stored
        .restore_metadata
        .get("restoreLockHash")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or(canonical_hash(&document).map_err(ApiError::bad_request)?);
    let bytes = stable_lockfile_bytes(&document).map_err(ApiError::bad_request)?;
    Ok((document, lock_hash, bytes))
}

fn stable_lockfile_bytes(document: &Value) -> Result<Vec<u8>, String> {
    let mut bytes =
        serde_json::to_vec_pretty(document).map_err(|_| "encode lockfile".to_owned())?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn normalized_lockfile_path(value: Option<&str>) -> Result<String, ApiError> {
    let value = value.unwrap_or(WORKSPACE_LOCKFILE_PATH).trim();
    if value != WORKSPACE_LOCKFILE_PATH {
        return Err(ApiError::bad_request(
            "workspace lockfile path must be .cocli/skills.lock.json",
        ));
    }
    Ok(value.to_owned())
}

fn safe_join(root: &FsPath, relative: &str) -> Result<PathBuf, String> {
    let relative_path = FsPath::new(relative);
    if relative_path.is_absolute() {
        return Err("relative path must not be absolute".to_owned());
    }
    if relative_path.components().any(is_forbidden_component) {
        return Err("relative path contains traversal or prefix components".to_owned());
    }
    Ok(root.join(relative_path))
}

fn is_forbidden_component(component: Component<'_>) -> bool {
    matches!(
        component,
        Component::ParentDir | Component::RootDir | Component::Prefix(_) | Component::CurDir
    )
}

fn create_dir_all_guarded(root: &FsPath, path: &FsPath) -> Result<(), String> {
    if !path.starts_with(root) {
        return Err("managed path escaped canonical root".to_owned());
    }
    let relative = path
        .strip_prefix(root)
        .map_err(|_| "managed path escaped canonical root".to_owned())?;
    let root_metadata = fs::symlink_metadata(root)
        .map_err(|error| safe_io_error("inspect managed directory root", &error))?;
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return Err("managed directory root is not a real directory".to_owned());
    }

    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(segment) = component else {
            return Err("managed path contains unsupported components".to_owned());
        };
        current.push(segment);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err("managed directory path contains a symlink".to_owned());
            }
            Ok(metadata) if metadata.is_dir() => {}
            Ok(_) => return Err("managed directory path is not a directory".to_owned()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                fs::create_dir(&current)
                    .map_err(|error| safe_io_error("create managed directory", &error))?;
            }
            Err(error) => {
                return Err(safe_io_error("inspect managed directory", &error));
            }
        }
    }
    Ok(())
}

fn hash_file_or_missing(path: &FsPath) -> Result<String, String> {
    match fs::read(path) {
        Ok(bytes) => Ok(format!("sha256:{}", sha256_hex(&bytes))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok("missing".to_owned()),
        Err(error) => Err(safe_io_error("read file hash", &error)),
    }
}

fn safe_skill_name(name: &str) -> Result<String, ApiError> {
    let trimmed = name.trim();
    if trimmed.is_empty()
        || trimmed.contains('/')
        || trimmed.contains('\\')
        || trimmed == "."
        || trimmed == ".."
    {
        return Err(ApiError::bad_request(
            "skillName is not a safe path segment",
        ));
    }
    Ok(trimmed.to_owned())
}

fn require_match(field: &str, expected: &str, actual: &str) -> Result<(), ApiError> {
    if expected == actual {
        Ok(())
    } else {
        Err(ApiError::conflict(format!("{field} changed")))
    }
}

fn require_entity_version(entity: &str, current: i64, expected: i64) -> Result<(), ApiError> {
    if current == expected {
        Ok(())
    } else {
        Err(ApiError::conflict(format!("{entity} version changed")))
    }
}

struct PreviewAuth {
    idempotency_key: String,
    confirmation_nonce: String,
}

fn preview_auth(purpose: &str, preview_hash: &str) -> Result<PreviewAuth, ApiError> {
    let idempotency_key = Uuid::new_v4().to_string();
    let confirmation_nonce = canonical_hash(&(purpose, preview_hash, &idempotency_key))
        .map_err(ApiError::bad_request)?;
    Ok(PreviewAuth {
        idempotency_key,
        confirmation_nonce,
    })
}

fn require_confirmation_nonce(
    purpose: &str,
    preview_hash: &str,
    idempotency_key: &str,
    provided: &str,
) -> Result<(), ApiError> {
    let expected =
        canonical_hash(&(purpose, preview_hash, idempotency_key)).map_err(ApiError::bad_request)?;
    require_match("confirmationNonce", &expected, provided)
}

fn runtime_api_error(error: RuntimeError) -> ApiError {
    match error {
        RuntimeError::Unsupported(message) => ApiError::json(
            StatusCode::CONFLICT,
            json!({"error": message, "errorType": "unsupported"}),
        ),
        RuntimeError::NotFound(message) => ApiError::not_found(message),
        RuntimeError::Busy(message) | RuntimeError::Delivery(message) => {
            ApiError::conflict(message)
        }
    }
}

fn hash_secret(value: &str) -> String {
    format!("sha256:{}", sha256_hex(value.as_bytes()))
}

#[derive(Debug)]
struct RedactedPath {
    redacted: String,
    hash: String,
}

fn redacted_path(path: &FsPath) -> RedactedPath {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("<unknown>");
    RedactedPath {
        redacted: format!("…/{name}"),
        hash: hash_secret(&path.to_string_lossy()),
    }
}

fn safe_io_error(action: &str, error: &io::Error) -> String {
    format!("{action}: {}", error.kind())
}

fn sync_directory(path: &FsPath) -> Result<(), String> {
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|error| safe_io_error("sync directory", &error))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirmation_nonce_is_bound_to_preview_and_idempotency_key() {
        let first = canonical_hash(&("managed-artifact-commit", "sha256:a", "nonce-a")).unwrap();
        let second = canonical_hash(&("managed-artifact-commit", "sha256:a", "nonce-b")).unwrap();
        assert_ne!(first, second);
        require_confirmation_nonce("managed-artifact-commit", "sha256:a", "nonce-a", &first)
            .unwrap();
        assert!(require_confirmation_nonce(
            "managed-artifact-commit",
            "sha256:a",
            "nonce-a",
            &second
        )
        .is_err());
    }

    #[test]
    fn preview_auth_returns_commit_ready_key_and_nonce_without_changing_hash() {
        let preview_hash = "sha256:preview";
        let first = preview_auth("gc-commit", preview_hash).unwrap();
        let second = preview_auth("gc-commit", preview_hash).unwrap();
        assert_ne!(first.idempotency_key, second.idempotency_key);
        require_confirmation_nonce(
            "gc-commit",
            preview_hash,
            &first.idempotency_key,
            &first.confirmation_nonce,
        )
        .unwrap();
    }

    #[test]
    fn local_paths_are_redacted_with_stable_hash() {
        let path = PathBuf::from("/very/private/source/secret-skill");
        let redacted = redacted_path(&path);
        assert_eq!(redacted.redacted, "…/secret-skill");
        assert!(redacted.hash.starts_with("sha256:"));
        assert!(!redacted.redacted.contains("/very/private"));
    }

    #[cfg(unix)]
    #[test]
    fn local_source_aliases_use_canonical_source_identity() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().expect("temp");
        let source = temp.path().join("skill");
        fs::create_dir_all(&source).expect("source dir");
        fs::write(source.join("SKILL.md"), "# Alias Skill\n").expect("manifest");
        let alias = temp.path().join("alias");
        symlink(&source, &alias).expect("symlink alias");

        let canonical = load_local_artifact(&source).expect("canonical artifact");
        let via_alias = load_local_artifact(&alias).expect("alias artifact");
        assert_eq!(canonical.content_digest, via_alias.content_digest);
        assert_eq!(canonical.canonical_source, via_alias.canonical_source);

        let redacted = redacted_path(canonical.canonical_source.as_deref().unwrap());
        let source_json =
            json!({"kind": "local", "redactedPath": redacted.redacted, "pathHash": redacted.hash});
        assert_eq!(
            artifact_key(&canonical, &source_json).unwrap(),
            artifact_key(&via_alias, &source_json).unwrap()
        );
    }

    #[test]
    fn managed_store_relative_path_uses_full_content_digest() {
        let digest = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let bundle = ArtifactBundle {
            files: Vec::new(),
            canonical_source: None,
            content_digest: digest.to_owned(),
            manifest_digest: digest.to_owned(),
        };
        assert_eq!(
            artifact_store_relative_path(&bundle, &json!({"kind": "test"})),
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
    }

    #[test]
    fn gc_preview_hash_changes_when_candidate_cas_changes() {
        let entity_id = Uuid::new_v4();
        let first = vec![GcCandidateView {
            entity_type: "managed_artifact".to_owned(),
            entity_id,
            reason: "unreferenced".to_owned(),
            expected_version: 1,
            expected_fingerprint: Some("sha256:a".to_owned()),
            ownership: None,
            reference_state_hash: "sha256:refs-a".to_owned(),
        }];
        let mut second = first.clone();
        second[0].expected_version = 2;
        assert_ne!(
            canonical_hash(&first).unwrap(),
            canonical_hash(&second).unwrap()
        );
    }

    #[test]
    fn lockfile_path_is_fixed_to_workspace_contract() {
        assert_eq!(
            normalized_lockfile_path(None).unwrap(),
            ".cocli/skills.lock.json"
        );
        assert!(normalized_lockfile_path(Some("../skills.lock.json")).is_err());
        assert!(normalized_lockfile_path(Some(".cocli/other.json")).is_err());
    }

    #[test]
    fn safe_join_rejects_traversal_and_absolute_paths() {
        let root = PathBuf::from("/tmp/root");
        assert!(safe_join(&root, "artifacts/abc/skill").is_ok());
        assert!(safe_join(&root, "../escape").is_err());
        assert!(safe_join(&root, "/escape").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn guarded_directory_creation_rejects_internal_symlink_components() {
        use std::os::unix::fs::symlink;

        for internal in [".staging", ".gc-quarantine"] {
            let temp = tempfile::tempdir().expect("temp");
            let root = temp.path().join("store");
            let outside = temp.path().join("outside");
            fs::create_dir_all(&root).expect("store root");
            fs::create_dir_all(&outside).expect("outside root");
            symlink(&outside, root.join(internal)).expect("internal symlink");

            let escaped = root.join(internal).join("unexpected");
            let error = create_dir_all_guarded(&root, &escaped)
                .expect_err("internal symlink must be rejected");

            assert!(error.contains("symlink"));
            assert!(!outside.join("unexpected").exists());
        }
    }
}
