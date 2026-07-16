use std::collections::{BTreeMap, HashMap};
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
use tokio::sync::OwnedMutexGuard;
use uuid::Uuid;

use super::skill_import::{
    canonical_skill_name, derive_source_name, fetch_skill, FetchedSkill, SkillImportError,
};
use super::{require_agent, ApiError, AppState, RuntimeSkill, RuntimeSkillCompatibility};

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/runtimes/compatibility", get(runtime_compatibility))
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

#[derive(Debug, Serialize)]
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
    state: &'static str,
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

async fn list_agent_skills(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
) -> Result<Json<AgentSkillsResponse>, ApiError> {
    let agent = require_agent(&state.store, agent_id).await?;
    let scanned = state.runtime.list_skills(&agent).await?;
    let installs = state.store.list_agent_skill_installs(agent_id).await?;
    let mut by_path: HashMap<String, AgentSkillInstall> = installs
        .into_iter()
        .map(|install| (install.install_path.clone(), install))
        .collect();
    let mut skills = Vec::with_capacity(scanned.len() + by_path.len());
    for scanned_skill in scanned {
        let managed = scanned_skill
            .install_path
            .as_ref()
            .filter(|path| !path.is_empty())
            .and_then(|path| by_path.remove(path));
        skills.push(scanned_skill_view(scanned_skill, managed));
    }
    for install in by_path.into_values() {
        skills.push(broken_skill_view(install));
    }
    skills.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(Json(AgentSkillsResponse { skills }))
}

fn scanned_skill_view(skill: RuntimeSkill, install: Option<AgentSkillInstall>) -> SkillView {
    let state = if install.is_some() {
        "managed"
    } else {
        "external"
    };
    SkillView {
        name: skill.name,
        display_name: skill.display_name,
        description: skill.description,
        user_invocable: skill.user_invocable,
        skill_type: skill.skill_type,
        path: Some(skill.path),
        install_path: skill.install_path,
        state,
        install_id: install.as_ref().map(|value| value.id),
        library_id: install.as_ref().map(|value| value.library_id),
        source_url: install.as_ref().map(|value| value.source_url.clone()),
        source_ref: install.and_then(|value| value.source_ref),
    }
}

fn broken_skill_view(install: AgentSkillInstall) -> SkillView {
    SkillView {
        name: install.library_name,
        display_name: String::new(),
        description: String::new(),
        user_invocable: false,
        skill_type: "workspace".to_owned(),
        path: None,
        install_path: Some(install.install_path),
        state: "broken",
        install_id: Some(install.id),
        library_id: Some(install.library_id),
        source_url: Some(install.source_url),
        source_ref: install.source_ref,
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
