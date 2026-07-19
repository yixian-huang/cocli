use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use cocli_store::{
    Agent, AgentSkillInstall, NewSkillLibrary, SkillLibraryEntry, SkillLibraryFileMeta, Store,
    StoreError,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::{OwnedMutexGuard, Semaphore};
use tokio::task::JoinSet;
use uuid::Uuid;

use super::skill_import::{
    canonical_skill_name, derive_source_name, fetch_skill, FetchedSkill, SkillImportError,
};
use super::{
    require_agent, ApiError, AppState, RuntimeSkillCompatibility, RuntimeSkillEvidence,
    RuntimeSkillFinding, RuntimeSkillInspection, RuntimeSkillIssue, RuntimeSkillSearchPath,
};

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/runtimes/compatibility", get(runtime_compatibility))
        .route(
            "/api/runtimes/skills/inventory",
            get(machine_skill_inventory),
        )
        .route("/api/runtimes/skills/doctor", get(machine_skill_doctor))
        .route(
            "/api/zones/:zone_id/skills/library",
            get(list_library).post(import_library),
        )
        .route(
            "/api/zones/:zone_id/skills/library/:library_id",
            get(get_library).delete(delete_library),
        )
        .route(
            "/api/zones/:zone_id/skills/library/:library_id/reinstall",
            post(reinstall_library),
        )
        .route(
            "/api/zones/:zone_id/skills/library/:library_id/files/*rel_path",
            get(get_library_file),
        )
        .route(
            "/api/agents/:agent_id/skills",
            get(list_agent_skills).post(install_agent_skill),
        )
        .route(
            "/api/agents/:agent_id/skills/inventory",
            get(agent_skill_inventory),
        )
        .route(
            "/api/agents/:agent_id/skills/doctor",
            get(agent_skill_doctor),
        )
        .route(
            "/api/agents/:agent_id/skills/:install_id",
            axum::routing::delete(uninstall_agent_skill),
        )
        .route(
            "/api/agents/:agent_id/skills/:install_id/files",
            get(list_agent_skill_files),
        )
        .route(
            "/api/agents/:agent_id/skills/:install_id/files/*rel_path",
            get(get_agent_skill_file),
        )
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SkillLibraryView {
    id: Uuid,
    zone_id: String,
    name: String,
    display_name: String,
    description: String,
    user_invocable: bool,
    source_kind: String,
    source_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_subpath: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_ref: Option<String>,
    total_bytes: i64,
    file_count: i64,
    imported_by: &'static str,
    imported_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    in_use_count: i64,
}

impl SkillLibraryView {
    fn local(zone_id: &str, entry: SkillLibraryEntry) -> Self {
        Self {
            id: entry.id,
            zone_id: zone_id.to_owned(),
            name: entry.name,
            display_name: entry.display_name,
            description: entry.description,
            user_invocable: entry.user_invocable,
            source_kind: entry.source_kind,
            source_url: entry.source_url,
            source_subpath: entry.source_subpath,
            source_ref: entry.source_ref,
            total_bytes: entry.total_bytes,
            file_count: entry.file_count,
            imported_by: "local",
            imported_at: entry.imported_at,
            updated_at: entry.updated_at,
            in_use_count: entry.in_use_count,
        }
    }
}

#[derive(Debug, Serialize)]
struct LibraryListResponse {
    entries: Vec<SkillLibraryView>,
}

async fn list_library(
    State(state): State<AppState>,
    Path(zone_id): Path<String>,
) -> Result<Json<LibraryListResponse>, ApiError> {
    let entries = state
        .store
        .list_skill_library()
        .await?
        .into_iter()
        .map(|entry| SkillLibraryView::local(&zone_id, entry))
        .collect();
    Ok(Json(LibraryListResponse { entries }))
}

#[derive(Debug, Serialize)]
struct LibraryDetailResponse {
    entry: SkillLibraryView,
    files: Vec<SkillLibraryFileMeta>,
}

async fn get_library(
    State(state): State<AppState>,
    Path((zone_id, library_id)): Path<(String, Uuid)>,
) -> Result<Json<LibraryDetailResponse>, ApiError> {
    let entry = require_library(&state, library_id).await?;
    let files = state.store.list_skill_library_files(library_id).await?;
    Ok(Json(LibraryDetailResponse {
        entry: SkillLibraryView::local(&zone_id, entry),
        files,
    }))
}

#[derive(Debug, Serialize)]
struct FileContentResponse {
    content: String,
    binary: bool,
    size: i64,
    mode: i64,
    #[serde(rename = "relPath")]
    rel_path: String,
}

async fn get_library_file(
    State(state): State<AppState>,
    Path((_zone_id, library_id, rel_path)): Path<(String, Uuid, String)>,
) -> Result<Json<FileContentResponse>, ApiError> {
    require_library(&state, library_id).await?;
    let file = state
        .store
        .get_skill_library_file(library_id, trim_wildcard(&rel_path)?)
        .await?
        .ok_or_else(|| ApiError::not_found("skill library file not found"))?;
    let (content, binary) = utf8_file_content(&file.content);
    Ok(Json(FileContentResponse {
        content,
        binary,
        size: file.size,
        mode: file.mode,
        rel_path: file.rel_path,
    }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportLibraryRequest {
    url: String,
    sub_path: Option<String>,
    name: Option<String>,
}

#[derive(Debug, Serialize)]
struct ImportLibraryResponse {
    library_id: Uuid,
    files: usize,
    size: i64,
}

async fn import_library(
    State(state): State<AppState>,
    Path(_zone_id): Path<String>,
    Json(request): Json<ImportLibraryRequest>,
) -> Result<Json<ImportLibraryResponse>, ApiError> {
    let fetched = fetch_skill(&request.url, request.sub_path.as_deref())
        .await
        .map_err(import_error)?;
    let source_name = request
        .name
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            (!fetched.metadata.name.trim().is_empty()).then_some(fetched.metadata.name.as_str())
        })
        .map(str::to_owned)
        .unwrap_or_else(|| derive_source_name(&request.url, request.sub_path.as_deref()));
    let name = canonical_skill_name(&source_name).map_err(import_error)?;
    if let Some(existing) = state.store.get_skill_library_by_name(&name).await? {
        return Err(ApiError::json(
            axum::http::StatusCode::CONFLICT,
            json!({
                "error": "name already imported in the local catalog",
                "existing_id": existing.id,
                "existing_source": existing.source_url,
            }),
        ));
    }
    let total = fetched
        .files
        .iter()
        .map(|file| file.size)
        .try_fold(0_i64, i64::checked_add)
        .ok_or_else(|| ApiError::bad_request("skill byte count overflowed"))?;
    let file_count = fetched.files.len();
    let display_name = if !fetched.metadata.display_name.trim().is_empty() {
        fetched.metadata.display_name.clone()
    } else if !fetched.metadata.name.trim().is_empty() {
        fetched.metadata.name.clone()
    } else if !source_name.trim().is_empty() {
        source_name.trim().to_owned()
    } else {
        name.clone()
    };
    let entry = state
        .store
        .create_skill_library(NewSkillLibrary {
            name,
            display_name,
            description: fetched.metadata.description,
            user_invocable: fetched.metadata.user_invocable,
            source_kind: fetched.source_kind,
            source_url: fetched.source_url,
            source_subpath: request.sub_path.filter(|value| !value.trim().is_empty()),
            source_ref: fetched.source_ref,
            files: fetched.files,
        })
        .await?;
    Ok(Json(ImportLibraryResponse {
        library_id: entry.id,
        files: file_count,
        size: total,
    }))
}

#[derive(Debug, Serialize)]
struct ReinstallLibraryResponse {
    updated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_ref: Option<String>,
    files: usize,
    size: i64,
}

async fn reinstall_library(
    State(state): State<AppState>,
    Path((_zone_id, library_id)): Path<(String, Uuid)>,
) -> Result<Json<ReinstallLibraryResponse>, ApiError> {
    let _mutation_guard = skill_mutation_guard(&state, library_id).await;
    let current = require_library(&state, library_id).await?;
    let fetched = fetch_skill(&current.source_url, current.source_subpath.as_deref())
        .await
        .map_err(import_gateway_error)?;
    let total = fetched
        .files
        .iter()
        .map(|file| file.size)
        .try_fold(0_i64, i64::checked_add)
        .ok_or_else(|| ApiError::bad_request("skill byte count overflowed"))?;
    let previous_files = state.store.load_skill_library_files(library_id).await?;
    let updated = fetched.source_ref != current.source_ref || fetched.files != previous_files;
    let refreshed = refresh_installed_skills(&state, &current, &fetched, &previous_files).await?;
    if updated {
        if let Err(error) = state
            .store
            .replace_skill_library_files(library_id, fetched.source_ref.as_deref(), &fetched.files)
            .await
        {
            let rollback_errors =
                rollback_runtime_refreshes(&state, &current, &previous_files, &refreshed).await;
            if !rollback_errors.is_empty() {
                return Err(ApiError::conflict(format!(
                    "catalog update failed ({error}); runtime rollback also failed: {}",
                    rollback_errors.join("; ")
                )));
            }
            return Err(error.into());
        }
    }
    Ok(Json(ReinstallLibraryResponse {
        updated,
        source_ref: fetched.source_ref,
        files: fetched.files.len(),
        size: total,
    }))
}

pub(crate) async fn reconcile_skill_state(
    store: &Store,
    runtime: &Arc<dyn super::RuntimeService>,
) -> Result<(), super::RuntimeError> {
    let agents = store
        .list_agents()
        .await
        .map_err(skill_reconcile_store_error)?;
    for agent in agents {
        let installs = store
            .list_agent_skill_installs(agent.id)
            .await
            .map_err(skill_reconcile_store_error)?;
        let managed_paths: std::collections::HashSet<&str> = installs
            .iter()
            .map(|install| install.install_path.as_str())
            .collect();
        for skill in runtime.list_skills(&agent).await? {
            let Some(install_path) = skill.install_path.as_deref() else {
                continue;
            };
            if skill.skill_type != "workspace" || managed_paths.contains(install_path) {
                continue;
            }
            let marker = runtime
                .read_skill_file(&agent, install_path, ".cocli-managed")
                .await;
            if marker
                .as_ref()
                .is_ok_and(|marker| !marker.binary && marker.content.trim() == skill.name)
            {
                runtime.uninstall_skill(&agent, install_path).await?;
            }
        }
        for install in installs {
            let library = store
                .get_skill_library(install.library_id)
                .await
                .map_err(skill_reconcile_store_error)?
                .ok_or_else(|| {
                    super::RuntimeError::Delivery(format!(
                        "managed skill install {} references missing library {}",
                        install.id, install.library_id
                    ))
                })?;
            let files = store
                .load_skill_library_files(install.library_id)
                .await
                .map_err(skill_reconcile_store_error)?;
            let installed_path = runtime.install_skill(&agent, &library.name, &files).await?;
            if installed_path != install.install_path {
                return Err(super::RuntimeError::Delivery(format!(
                    "managed skill {} reconciled to {} instead of {}",
                    library.name, installed_path, install.install_path
                )));
            }
        }
    }
    Ok(())
}

fn skill_reconcile_store_error(error: StoreError) -> super::RuntimeError {
    super::RuntimeError::Delivery(format!("failed to reconcile managed skills: {error}"))
}

async fn refresh_installed_skills(
    state: &AppState,
    library: &SkillLibraryEntry,
    fetched: &FetchedSkill,
    previous_files: &[cocli_store::SkillLibraryFile],
) -> Result<Vec<(Agent, AgentSkillInstall)>, ApiError> {
    let mut refreshed: Vec<(Agent, AgentSkillInstall)> = Vec::new();
    for install in state.store.list_skill_library_installs(library.id).await? {
        let refresh = async {
            let agent = require_agent(&state.store, install.agent_id).await?;
            let install_path = state
                .runtime
                .install_skill(&agent, &library.name, &fetched.files)
                .await?;
            if install_path != install.install_path {
                if let Err(error) = state.runtime.uninstall_skill(&agent, &install_path).await {
                    tracing::warn!(
                        %error,
                        %install_path,
                        "failed to remove skill written to an unexpected runtime path"
                    );
                }
                return Err(ApiError::conflict(format!(
                    "runtime install path changed from {} to {}; remove and reinstall the skill",
                    install.install_path, install_path
                )));
            }
            Ok(agent)
        }
        .await;
        match refresh {
            Ok(agent) => refreshed.push((agent, install)),
            Err(error) => {
                let rollback_errors =
                    rollback_runtime_refreshes(state, library, previous_files, &refreshed).await;
                if !rollback_errors.is_empty() {
                    return Err(ApiError::conflict(format!(
                        "{}; runtime rollback also failed: {}",
                        error.message,
                        rollback_errors.join("; ")
                    )));
                }
                return Err(error);
            }
        }
    }
    Ok(refreshed)
}

async fn rollback_runtime_refreshes(
    state: &AppState,
    library: &SkillLibraryEntry,
    previous_files: &[cocli_store::SkillLibraryFile],
    refreshed: &[(Agent, AgentSkillInstall)],
) -> Vec<String> {
    let mut errors = Vec::new();
    for (agent, install) in refreshed {
        match state
            .runtime
            .install_skill(agent, &library.name, previous_files)
            .await
        {
            Ok(path) if path == install.install_path => {}
            Ok(path) => errors.push(format!(
                "agent {} restored to unexpected path {} (expected {})",
                agent.id, path, install.install_path
            )),
            Err(error) => errors.push(format!("agent {}: {error}", agent.id)),
        }
    }
    errors
}

#[derive(Debug, Serialize)]
struct DeleteLibraryResponse {
    deleted: Uuid,
}

async fn delete_library(
    State(state): State<AppState>,
    Path((_zone_id, library_id)): Path<(String, Uuid)>,
) -> Result<Json<DeleteLibraryResponse>, ApiError> {
    let _mutation_guard = skill_mutation_guard(&state, library_id).await;
    let library = require_library(&state, library_id).await?;
    let files = state.store.load_skill_library_files(library_id).await?;
    let mut removed: Vec<(Agent, AgentSkillInstall)> = Vec::new();
    for install in state.store.list_skill_library_installs(library_id).await? {
        if let Some(agent) = state.store.get_agent(install.agent_id).await? {
            if let Err(error) = state
                .runtime
                .uninstall_skill(&agent, &install.install_path)
                .await
            {
                let rollback_errors =
                    rollback_runtime_refreshes(&state, &library, &files, &removed).await;
                let rollback_detail = if rollback_errors.is_empty() {
                    String::new()
                } else {
                    format!(
                        "; previously removed installs could not be restored: {}",
                        rollback_errors.join("; ")
                    )
                };
                return Err(ApiError::conflict(format!(
                    "could not remove skill from agent {}: {error}{rollback_detail}",
                    agent.id
                )));
            }
            removed.push((agent, install));
        }
    }
    if let Err(error) = state.store.delete_skill_library(library_id).await {
        let rollback_errors = rollback_runtime_refreshes(&state, &library, &files, &removed).await;
        if !rollback_errors.is_empty() {
            return Err(ApiError::conflict(format!(
                "catalog delete failed ({error}); runtime rollback also failed: {}",
                rollback_errors.join("; ")
            )));
        }
        return Err(error.into());
    }
    Ok(Json(DeleteLibraryResponse {
        deleted: library_id,
    }))
}

async fn runtime_compatibility(
    State(state): State<AppState>,
) -> Json<BTreeMap<String, RuntimeSkillCompatibility>> {
    let mut names = vec![
        "chatrs".to_owned(),
        "claude".to_owned(),
        "codex".to_owned(),
        "cursor".to_owned(),
        "gemini".to_owned(),
        "grok".to_owned(),
        "kimi".to_owned(),
        "opencode".to_owned(),
    ];
    names.extend(
        state
            .runtime
            .list()
            .await
            .into_iter()
            .map(|runtime| runtime.name),
    );
    names.sort();
    names.dedup();
    Json(
        names
            .into_iter()
            .map(|name| {
                let compatibility = state.runtime.skill_compatibility(&name);
                (name, compatibility)
            })
            .collect(),
    )
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SkillView {
    name: String,
    display_name: String,
    description: String,
    user_invocable: bool,
    #[serde(rename = "type")]
    skill_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    install_path: Option<String>,
    presence: String,
    state: String,
    runtime: String,
    scope: String,
    source_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    resolved_path: Option<String>,
    evidence: RuntimeSkillEvidence,
    #[serde(skip_serializing_if = "Option::is_none")]
    enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    valid: Option<bool>,
    duplicate: bool,
    shadowed: bool,
    issues: Vec<RuntimeSkillIssue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    install_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    library_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_ref: Option<String>,
}

#[derive(Debug, Serialize)]
struct AgentSkillsResponse {
    skills: Vec<SkillView>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AgentSkillInventoryResponse {
    agent_id: Uuid,
    agent_name: String,
    runtime: String,
    compatibility: RuntimeSkillCompatibility,
    evidence: RuntimeSkillEvidence,
    search_paths: Vec<RuntimeSkillSearchPath>,
    skills: Vec<SkillView>,
    issues: Vec<RuntimeSkillIssue>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeSkillInventorySummary {
    runtime: String,
    compatibility: RuntimeSkillCompatibility,
    agent_count: usize,
    skill_count: usize,
    issue_count: usize,
    evidence_sources: Vec<String>,
}

#[derive(Debug, Serialize)]
struct MachineSkillInventoryResponse {
    runtimes: Vec<RuntimeSkillInventorySummary>,
    agents: Vec<AgentSkillInventoryResponse>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SkillDoctorSummary {
    status: &'static str,
    runtime_count: usize,
    agent_count: usize,
    skill_count: usize,
    issue_count: usize,
    error_count: usize,
    warning_count: usize,
}

#[derive(Debug, Serialize)]
struct AgentSkillDoctorResponse {
    summary: SkillDoctorSummary,
    inventory: AgentSkillInventoryResponse,
}

#[derive(Debug, Serialize)]
struct MachineSkillDoctorResponse {
    summary: SkillDoctorSummary,
    runtimes: Vec<RuntimeSkillInventorySummary>,
    agents: Vec<AgentSkillInventoryResponse>,
}

async fn list_agent_skills(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
) -> Result<Json<AgentSkillsResponse>, ApiError> {
    let agent = require_agent(&state.store, agent_id).await?;
    let inventory = build_agent_skill_inventory(&state, agent).await?;
    Ok(Json(AgentSkillsResponse {
        skills: inventory.skills,
    }))
}

async fn agent_skill_inventory(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
) -> Result<Json<AgentSkillInventoryResponse>, ApiError> {
    let agent = require_agent(&state.store, agent_id).await?;
    Ok(Json(build_agent_skill_inventory(&state, agent).await?))
}

async fn agent_skill_doctor(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
) -> Result<Json<AgentSkillDoctorResponse>, ApiError> {
    let agent = require_agent(&state.store, agent_id).await?;
    let inventory = build_agent_skill_inventory(&state, agent).await?;
    let summary = doctor_summary(std::slice::from_ref(&inventory));
    Ok(Json(AgentSkillDoctorResponse { summary, inventory }))
}

async fn machine_skill_inventory(
    State(state): State<AppState>,
) -> Result<Json<MachineSkillInventoryResponse>, ApiError> {
    let agents = build_machine_skill_inventory(&state).await?;
    let runtimes = runtime_inventory_summaries(&state, &agents).await;
    Ok(Json(MachineSkillInventoryResponse { runtimes, agents }))
}

async fn machine_skill_doctor(
    State(state): State<AppState>,
) -> Result<Json<MachineSkillDoctorResponse>, ApiError> {
    let agents = build_machine_skill_inventory(&state).await?;
    let runtimes = runtime_inventory_summaries(&state, &agents).await;
    let summary = doctor_summary(&agents);
    Ok(Json(MachineSkillDoctorResponse {
        summary,
        runtimes,
        agents,
    }))
}

async fn build_machine_skill_inventory(
    state: &AppState,
) -> Result<Vec<AgentSkillInventoryResponse>, ApiError> {
    const MAX_CONCURRENT_INSPECTIONS: usize = 4;

    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_INSPECTIONS));
    let mut tasks = JoinSet::new();
    for (index, agent) in state.store.list_agents().await?.into_iter().enumerate() {
        let state = state.clone();
        let semaphore = Arc::clone(&semaphore);
        tasks.spawn(async move {
            let _permit = semaphore.acquire_owned().await.map_err(|error| {
                ApiError::json(
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    json!({"error": format!("skill inspection semaphore closed: {error}")}),
                )
            })?;
            build_agent_skill_inventory(&state, agent)
                .await
                .map(|inventory| (index, inventory))
        });
    }
    let mut inventories = Vec::new();
    while let Some(result) = tasks.join_next().await {
        let inventory = result.map_err(|error| {
            ApiError::json(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                json!({"error": format!("skill inspection task failed: {error}")}),
            )
        })??;
        inventories.push(inventory);
    }
    inventories.sort_by_key(|(index, _)| *index);
    Ok(inventories
        .into_iter()
        .map(|(_, inventory)| inventory)
        .collect())
}

async fn build_agent_skill_inventory(
    state: &AppState,
    agent: Agent,
) -> Result<AgentSkillInventoryResponse, ApiError> {
    let RuntimeSkillInspection {
        runtime,
        compatibility,
        evidence,
        search_paths,
        skills: scanned,
        mut issues,
    } = state.runtime.inspect_skills(&agent).await?;
    let installs = state.store.list_agent_skill_installs(agent.id).await?;
    let mut by_path: HashMap<String, AgentSkillInstall> = installs
        .into_iter()
        .map(|install| (install.install_path.clone(), install))
        .collect();
    let mut skills = Vec::with_capacity(scanned.len() + by_path.len());
    for scanned_skill in scanned {
        let managed = scanned_skill
            .skill
            .install_path
            .as_ref()
            .filter(|path| !path.is_empty())
            .and_then(|path| by_path.remove(path));
        skills.push(scanned_skill_view(scanned_skill, managed));
    }
    for install in by_path.into_values() {
        let issue = RuntimeSkillIssue {
            code: "missing_managed_install".to_owned(),
            severity: "error".to_owned(),
            message: "SQLite install record has no discovered filesystem skill".to_owned(),
            path: Some(install.install_path.clone()),
            skill_name: Some(install.library_name.clone()),
        };
        issues.push(issue.clone());
        skills.push(broken_skill_view(
            &runtime,
            evidence.clone(),
            install,
            issue,
        ));
    }
    skills.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(AgentSkillInventoryResponse {
        agent_id: agent.id,
        agent_name: agent.name,
        runtime,
        compatibility,
        evidence,
        search_paths,
        skills,
        issues,
    })
}

fn scanned_skill_view(skill: RuntimeSkillFinding, install: Option<AgentSkillInstall>) -> SkillView {
    let state = if install.is_some() {
        "managed"
    } else {
        "external"
    };
    let presence = if install.is_some() && skill.evidence.source == "filesystem" {
        "installed".to_owned()
    } else {
        skill.presence
    };
    SkillView {
        name: skill.skill.name,
        display_name: skill.skill.display_name,
        description: skill.skill.description,
        user_invocable: skill.skill.user_invocable,
        skill_type: skill.skill.skill_type,
        path: Some(skill.skill.path),
        install_path: skill.skill.install_path,
        presence,
        state: state.to_owned(),
        runtime: skill.runtime,
        scope: skill.scope,
        source_path: skill.source_path,
        resolved_path: skill.resolved_path,
        evidence: skill.evidence,
        enabled: skill.enabled,
        valid: skill.valid,
        duplicate: skill.duplicate,
        shadowed: skill.shadowed,
        issues: skill.issues,
        install_id: install.as_ref().map(|value| value.id),
        library_id: install.as_ref().map(|value| value.library_id),
        source_url: install.as_ref().map(|value| value.source_url.clone()),
        source_ref: install.and_then(|value| value.source_ref),
    }
}

fn broken_skill_view(
    runtime: &str,
    evidence: RuntimeSkillEvidence,
    install: AgentSkillInstall,
    issue: RuntimeSkillIssue,
) -> SkillView {
    SkillView {
        name: install.library_name,
        display_name: String::new(),
        description: String::new(),
        user_invocable: false,
        skill_type: "workspace".to_owned(),
        path: None,
        install_path: Some(install.install_path.clone()),
        presence: "installed".to_owned(),
        state: "broken".to_owned(),
        runtime: runtime.to_owned(),
        scope: "workspace".to_owned(),
        source_path: install.install_path.clone(),
        resolved_path: None,
        evidence,
        enabled: None,
        valid: None,
        duplicate: false,
        shadowed: false,
        issues: vec![issue],
        install_id: Some(install.id),
        library_id: Some(install.library_id),
        source_url: Some(install.source_url),
        source_ref: install.source_ref,
    }
}

async fn runtime_inventory_summaries(
    state: &AppState,
    inventories: &[AgentSkillInventoryResponse],
) -> Vec<RuntimeSkillInventorySummary> {
    let mut names = vec![
        "chatrs".to_owned(),
        "claude".to_owned(),
        "codex".to_owned(),
        "cursor".to_owned(),
        "gemini".to_owned(),
        "grok".to_owned(),
        "kimi".to_owned(),
        "opencode".to_owned(),
    ];
    names.extend(
        state
            .runtime
            .list()
            .await
            .into_iter()
            .map(|runtime| runtime.name),
    );
    names.extend(
        inventories
            .iter()
            .map(|inventory| inventory.runtime.clone()),
    );
    names.sort();
    names.dedup();
    names
        .into_iter()
        .map(|runtime| {
            let matching: Vec<&AgentSkillInventoryResponse> = inventories
                .iter()
                .filter(|inventory| inventory.runtime == runtime)
                .collect();
            let mut evidence_sources: Vec<String> = matching
                .iter()
                .map(|inventory| inventory.evidence.source.clone())
                .collect();
            evidence_sources.sort();
            evidence_sources.dedup();
            let unique_skills: HashSet<&str> = matching
                .iter()
                .flat_map(|inventory| &inventory.skills)
                .map(|skill| skill.source_path.as_str())
                .collect();
            let unique_issues: HashSet<(&str, Option<&str>)> = matching
                .iter()
                .flat_map(|inventory| &inventory.issues)
                .map(|issue| (issue.code.as_str(), issue.path.as_deref()))
                .collect();
            RuntimeSkillInventorySummary {
                compatibility: state.runtime.skill_compatibility(&runtime),
                runtime,
                agent_count: matching.len(),
                skill_count: unique_skills.len(),
                issue_count: unique_issues.len(),
                evidence_sources,
            }
        })
        .collect()
}

fn doctor_summary(inventories: &[AgentSkillInventoryResponse]) -> SkillDoctorSummary {
    let mut runtimes: Vec<&str> = inventories
        .iter()
        .map(|inventory| inventory.runtime.as_str())
        .collect();
    runtimes.sort_unstable();
    runtimes.dedup();
    let issue_count = inventories
        .iter()
        .map(|inventory| inventory.issues.len())
        .sum();
    let error_count = inventories
        .iter()
        .flat_map(|inventory| &inventory.issues)
        .filter(|issue| issue.severity == "error")
        .count();
    let warning_count = inventories
        .iter()
        .flat_map(|inventory| &inventory.issues)
        .filter(|issue| issue.severity == "warning")
        .count();
    SkillDoctorSummary {
        status: if error_count > 0 {
            "error"
        } else if warning_count > 0 {
            "warning"
        } else {
            "ok"
        },
        runtime_count: runtimes.len(),
        agent_count: inventories.len(),
        skill_count: inventories
            .iter()
            .map(|inventory| inventory.skills.len())
            .sum(),
        issue_count,
        error_count,
        warning_count,
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstallAgentSkillRequest {
    library_id: Uuid,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InstallAgentSkillResponse {
    install_id: Uuid,
    install_path: String,
    bytes: i64,
}

async fn install_agent_skill(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Json(request): Json<InstallAgentSkillRequest>,
) -> Result<Json<InstallAgentSkillResponse>, ApiError> {
    let _mutation_guard = skill_mutation_guard(&state, request.library_id).await;
    let agent = require_agent(&state.store, agent_id).await?;
    if state.runtime.skill_compatibility(&agent.runtime) == RuntimeSkillCompatibility::Unsupported {
        return Err(ApiError::bad_request(format!(
            "{} does not support skills",
            agent.runtime
        )));
    }
    let library = require_library(&state, request.library_id).await?;
    let installs = state.store.list_agent_skill_installs(agent_id).await?;
    if installs
        .iter()
        .any(|install| install.library_id == request.library_id)
    {
        return Err(StoreError::SkillAlreadyInstalled {
            agent_id,
            library_id: request.library_id,
        }
        .into());
    }
    if state
        .runtime
        .list_skills(&agent)
        .await?
        .into_iter()
        .any(|skill| {
            skill.name == library.name
                && skill
                    .install_path
                    .as_deref()
                    .is_some_and(|path| !path.is_empty())
        })
    {
        return Err(ApiError::conflict(format!(
            "workspace skill {} already exists outside the local catalog",
            library.name
        )));
    }
    let files = state
        .store
        .load_skill_library_files(request.library_id)
        .await?;
    let bytes = files.iter().map(|file| file.size).sum();
    let install_path = state
        .runtime
        .install_skill(&agent, &library.name, &files)
        .await?;
    match state
        .store
        .create_agent_skill_install(agent_id, request.library_id, &install_path)
        .await
    {
        Ok(install) => Ok(Json(InstallAgentSkillResponse {
            install_id: install.id,
            install_path,
            bytes,
        })),
        Err(error) => {
            if !matches!(error, StoreError::SkillAlreadyInstalled { .. }) {
                if let Err(rollback_error) =
                    state.runtime.uninstall_skill(&agent, &install_path).await
                {
                    return Err(ApiError::conflict(format!(
                        "{error}; runtime install rollback failed: {rollback_error}"
                    )));
                }
            }
            Err(error.into())
        }
    }
}

#[derive(Debug, Serialize)]
struct OkResponse {
    ok: bool,
}

async fn uninstall_agent_skill(
    State(state): State<AppState>,
    Path((agent_id, install_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<OkResponse>, ApiError> {
    let agent = require_agent(&state.store, agent_id).await?;
    let install = require_agent_install(&state, agent_id, install_id).await?;
    let _mutation_guard = skill_mutation_guard(&state, install.library_id).await;
    let install = require_agent_install(&state, agent_id, install_id).await?;
    state
        .runtime
        .uninstall_skill(&agent, &install.install_path)
        .await?;
    if let Err(error) = state
        .store
        .delete_agent_skill_install(agent_id, install_id)
        .await
    {
        let library = require_library(&state, install.library_id).await?;
        let files = state
            .store
            .load_skill_library_files(install.library_id)
            .await?;
        match state
            .runtime
            .install_skill(&agent, &library.name, &files)
            .await
        {
            Ok(path) if path == install.install_path => return Err(error.into()),
            Ok(path) => {
                return Err(ApiError::conflict(format!(
                    "{error}; runtime uninstall rollback used {path} instead of {}",
                    install.install_path
                )));
            }
            Err(rollback_error) => {
                return Err(ApiError::conflict(format!(
                    "{error}; runtime uninstall rollback failed: {rollback_error}"
                )));
            }
        }
    }
    Ok(Json(OkResponse { ok: true }))
}

async fn skill_mutation_guard(state: &AppState, library_id: Uuid) -> OwnedMutexGuard<()> {
    let lock = {
        let mut locks = state.skill_mutation_locks.lock().await;
        Arc::clone(
            locks
                .entry(library_id)
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(()))),
        )
    };
    lock.lock_owned().await
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InstalledFilesResponse {
    install_path: String,
    files: Vec<super::RuntimeSkillFileEntry>,
}

async fn list_agent_skill_files(
    State(state): State<AppState>,
    Path((agent_id, install_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<InstalledFilesResponse>, ApiError> {
    let agent = require_agent(&state.store, agent_id).await?;
    let install = require_agent_install(&state, agent_id, install_id).await?;
    let files = state
        .runtime
        .list_skill_files(&agent, &install.install_path)
        .await?;
    Ok(Json(InstalledFilesResponse {
        install_path: install.install_path,
        files,
    }))
}

async fn get_agent_skill_file(
    State(state): State<AppState>,
    Path((agent_id, install_id, rel_path)): Path<(Uuid, Uuid, String)>,
) -> Result<Json<super::RuntimeSkillFileContent>, ApiError> {
    let agent = require_agent(&state.store, agent_id).await?;
    let install = require_agent_install(&state, agent_id, install_id).await?;
    let content = state
        .runtime
        .read_skill_file(&agent, &install.install_path, trim_wildcard(&rel_path)?)
        .await?;
    Ok(Json(content))
}

async fn require_library(
    state: &AppState,
    library_id: Uuid,
) -> Result<SkillLibraryEntry, ApiError> {
    state
        .store
        .get_skill_library(library_id)
        .await?
        .ok_or_else(|| ApiError::not_found("skill library entry not found"))
}

async fn require_agent_install(
    state: &AppState,
    agent_id: Uuid,
    install_id: Uuid,
) -> Result<AgentSkillInstall, ApiError> {
    state
        .store
        .get_agent_skill_install(install_id)
        .await?
        .filter(|install| install.agent_id == agent_id)
        .ok_or_else(|| ApiError::not_found("agent skill install not found"))
}

fn trim_wildcard(path: &str) -> Result<&str, ApiError> {
    let path = path.trim_start_matches('/');
    if path.is_empty() {
        Err(ApiError::bad_request("skill file path must not be empty"))
    } else {
        Ok(path)
    }
}

fn utf8_file_content(bytes: &[u8]) -> (String, bool) {
    if bytes.iter().take(512).any(|byte| *byte == 0) {
        return (String::new(), true);
    }
    String::from_utf8(bytes.to_vec())
        .map_or_else(|_| (String::new(), true), |content| (content, false))
}

fn import_error(error: SkillImportError) -> ApiError {
    match error {
        SkillImportError::Invalid(message) => ApiError::bad_request(message),
        SkillImportError::Io(message) => ApiError::bad_request(message),
        SkillImportError::Git(message) => ApiError::bad_request(message),
    }
}

fn import_gateway_error(error: SkillImportError) -> ApiError {
    match error {
        SkillImportError::Invalid(message) => ApiError::bad_request(message),
        SkillImportError::Io(message) | SkillImportError::Git(message) => ApiError::json(
            axum::http::StatusCode::BAD_GATEWAY,
            json!({ "error": message }),
        ),
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;
    use crate::RuntimeSkill;

    #[test]
    fn managed_skill_preserves_native_discovery_presence() {
        let finding = RuntimeSkillFinding {
            skill: RuntimeSkill {
                name: "reviewer".to_owned(),
                display_name: "Reviewer".to_owned(),
                description: String::new(),
                user_invocable: false,
                skill_type: "workspace".to_owned(),
                path: "/tmp/reviewer/SKILL.md".to_owned(),
                install_path: Some(".agents/skills/reviewer".to_owned()),
            },
            runtime: "codex".to_owned(),
            scope: "workspace".to_owned(),
            source_path: "/tmp/reviewer".to_owned(),
            resolved_path: Some("/tmp/reviewer".to_owned()),
            presence: "discovered".to_owned(),
            evidence: RuntimeSkillEvidence {
                source: "codex_app_server".to_owned(),
                detail: "skills/list(forceReload)".to_owned(),
                proves_session_visibility: false,
            },
            enabled: Some(true),
            valid: Some(true),
            duplicate: false,
            shadowed: false,
            issues: Vec::new(),
        };
        let install = AgentSkillInstall {
            id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
            library_id: Uuid::new_v4(),
            install_path: ".agents/skills/reviewer".to_owned(),
            installed_at: Utc::now(),
            library_name: "reviewer".to_owned(),
            source_url: "/tmp/reviewer".to_owned(),
            source_ref: None,
        };

        let view = scanned_skill_view(finding, Some(install));

        assert_eq!(view.state, "managed");
        assert_eq!(view.presence, "discovered");
        assert_eq!(view.evidence.source, "codex_app_server");
    }
}
