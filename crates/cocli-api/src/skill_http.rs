use std::collections::{BTreeMap, HashMap, HashSet};
use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use cocli_store::{
    Agent, AgentSkillInstall, NewSkillLibrary, SkillLibraryEntry, SkillLibraryFileMeta, Store,
    StoreError,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::{Mutex, OwnedMutexGuard, Semaphore};
use tokio::task::JoinSet;
use uuid::Uuid;

use super::skill_import::{
    canonical_skill_name, derive_source_name, fetch_skill, FetchedSkill, SkillImportError,
};
use super::{
    require_agent, ApiError, AppState, RuntimeError, RuntimeSkillCompatibility,
    RuntimeSkillEvidence, RuntimeSkillFinding, RuntimeSkillInspection, RuntimeSkillIssue,
    RuntimeSkillSearchPath,
};

const SKILL_SNAPSHOT_TTL: Duration = Duration::from_secs(3);

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
enum SkillSnapshotKey {
    Agent(Uuid),
    Machine(String),
}

#[derive(Clone)]
struct CachedSkillInspection {
    inspection: RuntimeSkillInspection,
    stored_at: Instant,
    expires_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone)]
struct SkillSnapshot {
    inspection: RuntimeSkillInspection,
    cache_status: &'static str,
    expires_at: chrono::DateTime<chrono::Utc>,
}

pub(super) struct SkillSnapshotCoordinator {
    ttl: Duration,
    cache: Mutex<HashMap<SkillSnapshotKey, CachedSkillInspection>>,
    locks: Mutex<HashMap<SkillSnapshotKey, Arc<Mutex<()>>>>,
}

impl SkillSnapshotCoordinator {
    pub(super) fn new() -> Arc<Self> {
        Self::with_ttl(SKILL_SNAPSHOT_TTL)
    }

    fn with_ttl(ttl: Duration) -> Arc<Self> {
        Arc::new(Self {
            ttl,
            cache: Mutex::new(HashMap::new()),
            locks: Mutex::new(HashMap::new()),
        })
    }

    async fn get_or_refresh<F, Fut>(
        &self,
        key: SkillSnapshotKey,
        force: bool,
        refresh: F,
    ) -> Result<SkillSnapshot, RuntimeError>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<RuntimeSkillInspection, RuntimeError>>,
    {
        let requested_at = Instant::now();
        if !force {
            if let Some(snapshot) = self.cached(&key).await {
                return Ok(snapshot);
            }
        }
        let lock = {
            let mut locks = self.locks.lock().await;
            Arc::clone(
                locks
                    .entry(key.clone())
                    .or_insert_with(|| Arc::new(Mutex::new(()))),
            )
        };
        let _guard = lock.lock().await;
        if let Some(cached) = self.cache.lock().await.get(&key).cloned() {
            let fresh = cached.stored_at.elapsed() < self.ttl;
            let force_already_satisfied = cached.stored_at >= requested_at;
            if fresh && (!force || force_already_satisfied) {
                return Ok(SkillSnapshot {
                    inspection: cached.inspection,
                    cache_status: "cached",
                    expires_at: cached.expires_at,
                });
            }
        }
        let inspection = refresh().await?;
        let expires_at = chrono::Utc::now()
            + chrono::Duration::from_std(self.ttl).unwrap_or_else(|_| chrono::Duration::seconds(3));
        self.cache.lock().await.insert(
            key,
            CachedSkillInspection {
                inspection: inspection.clone(),
                stored_at: Instant::now(),
                expires_at,
            },
        );
        Ok(SkillSnapshot {
            inspection,
            cache_status: "fresh",
            expires_at,
        })
    }

    async fn cached(&self, key: &SkillSnapshotKey) -> Option<SkillSnapshot> {
        let mut cache = self.cache.lock().await;
        if cache
            .get(key)
            .is_some_and(|cached| cached.stored_at.elapsed() >= self.ttl)
        {
            cache.remove(key);
            return None;
        }
        cache.get(key).cloned().map(|cached| SkillSnapshot {
            inspection: cached.inspection,
            cache_status: "cached",
            expires_at: cached.expires_at,
        })
    }

    async fn invalidate_agent(&self, agent_id: Uuid) {
        self.cache
            .lock()
            .await
            .retain(|key, _| !matches!(key, SkillSnapshotKey::Agent(id) if *id == agent_id));
    }
}

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
            Ok(agent) => {
                state.skill_snapshots.invalidate_agent(agent.id).await;
                refreshed.push((agent, install));
            }
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
            Ok(path) if path == install.install_path => {
                state.skill_snapshots.invalidate_agent(agent.id).await;
            }
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
            state.skill_snapshots.invalidate_agent(agent.id).await;
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
    fingerprint: String,
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
    session_effective: SessionEffectiveEvidence,
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

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionEffectiveEvidence {
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    effective: Option<bool>,
    reason: String,
    evidence: RuntimeSkillEvidence,
    observed_at: chrono::DateTime<chrono::Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct AgentSkillsResponse {
    skills: Vec<SkillView>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AgentSkillInventoryResponse {
    observed_at: chrono::DateTime<chrono::Utc>,
    cache_status: &'static str,
    expires_at: chrono::DateTime<chrono::Utc>,
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
    observed_at: chrono::DateTime<chrono::Utc>,
    cache_status: &'static str,
    expires_at: chrono::DateTime<chrono::Utc>,
    runtime: String,
    compatibility: RuntimeSkillCompatibility,
    agent_count: usize,
    skill_count: usize,
    issue_count: usize,
    evidence_sources: Vec<String>,
    evidence: RuntimeSkillEvidence,
    search_paths: Vec<RuntimeSkillSearchPath>,
    skills: Vec<SkillView>,
    issues: Vec<RuntimeSkillIssue>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MachineSkillInventoryResponse {
    observed_at: chrono::DateTime<chrono::Utc>,
    cache_status: &'static str,
    force_refresh: bool,
    runtimes: Vec<RuntimeSkillInventorySummary>,
    agents: Vec<AgentSkillInventoryResponse>,
    diagnostics: Vec<SkillInspectionDiagnostic>,
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
#[serde(rename_all = "camelCase")]
struct MachineSkillDoctorResponse {
    observed_at: chrono::DateTime<chrono::Utc>,
    cache_status: &'static str,
    force_refresh: bool,
    summary: SkillDoctorSummary,
    runtimes: Vec<RuntimeSkillInventorySummary>,
    agents: Vec<AgentSkillInventoryResponse>,
    diagnostics: Vec<SkillInspectionDiagnostic>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SkillInspectionDiagnostic {
    fingerprint: String,
    subject: &'static str,
    runtime: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    agent_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    agent_name: Option<String>,
    stage: &'static str,
    error_type: &'static str,
    message: String,
    observed_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Default, Deserialize)]
struct SkillRefreshQuery {
    #[serde(default)]
    force: bool,
}

async fn list_agent_skills(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
) -> Result<Json<AgentSkillsResponse>, ApiError> {
    let agent = require_agent(&state.store, agent_id).await?;
    let skills = build_agent_skill_list(&state, agent).await?;
    Ok(Json(AgentSkillsResponse { skills }))
}

async fn agent_skill_inventory(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Query(query): Query<SkillRefreshQuery>,
) -> Result<Json<AgentSkillInventoryResponse>, ApiError> {
    let agent = require_agent(&state.store, agent_id).await?;
    Ok(Json(
        build_agent_skill_inventory(&state, agent, query.force).await?,
    ))
}

async fn agent_skill_doctor(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Query(query): Query<SkillRefreshQuery>,
) -> Result<Json<AgentSkillDoctorResponse>, ApiError> {
    let agent = require_agent(&state.store, agent_id).await?;
    let inventory = build_agent_skill_inventory(&state, agent, query.force).await?;
    let summary = doctor_summary(&[], std::slice::from_ref(&inventory), &[]);
    Ok(Json(AgentSkillDoctorResponse { summary, inventory }))
}

async fn machine_skill_inventory(
    State(state): State<AppState>,
    Query(query): Query<SkillRefreshQuery>,
) -> Result<Json<MachineSkillInventoryResponse>, ApiError> {
    let machine = build_machine_skill_inventory(&state, query.force).await?;
    Ok(Json(MachineSkillInventoryResponse {
        observed_at: machine.observed_at,
        cache_status: machine.cache_status,
        force_refresh: query.force,
        runtimes: machine.runtimes,
        agents: machine.agents,
        diagnostics: machine.diagnostics,
    }))
}

async fn machine_skill_doctor(
    State(state): State<AppState>,
    Query(query): Query<SkillRefreshQuery>,
) -> Result<Json<MachineSkillDoctorResponse>, ApiError> {
    let machine = build_machine_skill_inventory(&state, query.force).await?;
    let summary = doctor_summary(&machine.runtimes, &machine.agents, &machine.diagnostics);
    Ok(Json(MachineSkillDoctorResponse {
        observed_at: machine.observed_at,
        cache_status: machine.cache_status,
        force_refresh: query.force,
        summary,
        runtimes: machine.runtimes,
        agents: machine.agents,
        diagnostics: machine.diagnostics,
    }))
}

struct MachineSkillInventory {
    observed_at: chrono::DateTime<chrono::Utc>,
    cache_status: &'static str,
    runtimes: Vec<RuntimeSkillInventorySummary>,
    agents: Vec<AgentSkillInventoryResponse>,
    diagnostics: Vec<SkillInspectionDiagnostic>,
}

async fn build_machine_skill_inventory(
    state: &AppState,
    force: bool,
) -> Result<MachineSkillInventory, ApiError> {
    const MAX_CONCURRENT_INSPECTIONS: usize = 4;

    let agents = state.store.list_agents().await?;
    let mut runtime_names = vec![
        "chatrs".to_owned(),
        "claude".to_owned(),
        "codex".to_owned(),
        "cursor".to_owned(),
        "gemini".to_owned(),
        "grok".to_owned(),
        "kimi".to_owned(),
        "opencode".to_owned(),
    ];
    runtime_names.extend(
        state
            .runtime
            .list()
            .await
            .into_iter()
            .map(|runtime| runtime.name),
    );
    runtime_names.extend(agents.iter().map(|agent| agent.runtime.clone()));
    runtime_names.sort();
    runtime_names.dedup();

    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_INSPECTIONS));
    let mut tasks = JoinSet::new();
    for (index, runtime) in runtime_names.into_iter().enumerate() {
        let state = state.clone();
        let semaphore = Arc::clone(&semaphore);
        tasks.spawn(async move {
            let _permit = semaphore.acquire_owned().await.ok();
            let result = build_runtime_skill_inventory(&state, &runtime, force).await;
            (index, runtime, result)
        });
    }
    let mut runtimes = Vec::new();
    let mut diagnostics = Vec::new();
    while let Some(result) = tasks.join_next().await {
        match result {
            Ok((index, _runtime, Ok(inventory))) => runtimes.push((index, inventory)),
            Ok((index, runtime, Err(error))) => {
                diagnostics.push(skill_diagnostic(
                    "runtime",
                    runtime.clone(),
                    None,
                    None,
                    "inspection",
                    "runtime_error",
                    error.message.clone(),
                ));
                runtimes.push((
                    index,
                    failed_runtime_inventory(state, runtime, error.message),
                ));
            }
            Err(error) => diagnostics.push(skill_diagnostic(
                "runtime",
                "unknown".to_owned(),
                None,
                None,
                "task",
                "join_error",
                error.to_string(),
            )),
        }
    }
    runtimes.sort_by_key(|(index, _)| *index);
    let mut runtimes: Vec<_> = runtimes
        .into_iter()
        .map(|(_, inventory)| inventory)
        .collect();

    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_INSPECTIONS));
    let mut tasks = JoinSet::new();
    for (index, agent) in agents.into_iter().enumerate() {
        let state = state.clone();
        let semaphore = Arc::clone(&semaphore);
        tasks.spawn(async move {
            let _permit = semaphore.acquire_owned().await.ok();
            let runtime = agent.runtime.clone();
            let agent_id = agent.id;
            let agent_name = agent.name.clone();
            let result = build_agent_skill_inventory(&state, agent, force).await;
            (index, runtime, agent_id, agent_name, result)
        });
    }
    let mut inventories = Vec::new();
    while let Some(result) = tasks.join_next().await {
        match result {
            Ok((index, _runtime, _agent_id, _agent_name, Ok(inventory))) => {
                inventories.push((index, inventory));
            }
            Ok((_index, runtime, agent_id, agent_name, Err(error))) => {
                diagnostics.push(skill_diagnostic(
                    "agent",
                    runtime,
                    Some(agent_id),
                    Some(agent_name),
                    "inspection",
                    "runtime_or_store_error",
                    error.message,
                ));
            }
            Err(error) => diagnostics.push(skill_diagnostic(
                "agent",
                "unknown".to_owned(),
                None,
                None,
                "task",
                "join_error",
                error.to_string(),
            )),
        }
    }
    inventories.sort_by_key(|(index, _)| *index);
    let agents: Vec<_> = inventories
        .into_iter()
        .map(|(_, inventory)| inventory)
        .collect();
    for runtime in &mut runtimes {
        let matching: Vec<_> = agents
            .iter()
            .filter(|agent| agent.runtime == runtime.runtime)
            .collect();
        runtime.agent_count = matching.len();
        let unique_skills: HashSet<&str> = runtime
            .skills
            .iter()
            .chain(matching.iter().flat_map(|agent| &agent.skills))
            .map(|skill| skill.fingerprint.as_str())
            .collect();
        runtime.skill_count = unique_skills.len();
        let unique_issues: HashSet<&str> = runtime
            .issues
            .iter()
            .chain(matching.iter().flat_map(|agent| &agent.issues))
            .map(|issue| issue.fingerprint.as_str())
            .collect();
        runtime.issue_count = unique_issues.len();
        runtime
            .evidence_sources
            .extend(matching.iter().map(|agent| agent.evidence.source.clone()));
        runtime.evidence_sources.sort();
        runtime.evidence_sources.dedup();
    }
    let mut observed_at = chrono::DateTime::<chrono::Utc>::UNIX_EPOCH;
    for value in runtimes
        .iter()
        .map(|runtime| runtime.observed_at)
        .chain(agents.iter().map(|agent| agent.observed_at))
    {
        if value > observed_at {
            observed_at = value;
        }
    }
    let statuses: Vec<_> = runtimes
        .iter()
        .map(|runtime| runtime.cache_status)
        .chain(agents.iter().map(|agent| agent.cache_status))
        .collect();
    let cache_status = if statuses.iter().all(|status| *status == "cached") {
        "cached"
    } else if statuses.iter().all(|status| *status == "fresh") {
        "fresh"
    } else {
        "mixed"
    };
    Ok(MachineSkillInventory {
        observed_at,
        cache_status,
        runtimes,
        agents,
        diagnostics,
    })
}

pub(super) async fn governance_observation(
    state: &AppState,
    force: bool,
) -> Result<super::skill_governance::GovernanceObservation, ApiError> {
    use super::skill_governance::{finalize_observation, ObservationDiagnostic, ObservedSkill};

    let machine = build_machine_skill_inventory(state, force).await?;
    let mut skills = Vec::new();
    for runtime in &machine.runtimes {
        let native_unsupported = runtime
            .issues
            .iter()
            .any(|issue| issue.code == "native_probe_unsupported");
        for skill in &runtime.skills {
            skills.push(ObservedSkill {
                logical_identity: skill.name.clone(),
                runtime: runtime.runtime.clone(),
                scope: governance_scope(&skill.scope),
                scope_id: Some("machine".to_owned()),
                source_provenance: Some(format!(
                    "filesystem:{}",
                    skill.resolved_path.as_deref().unwrap_or(&skill.source_path)
                )),
                version: skill.source_ref.clone(),
                content_digest: None,
                manifest_digest: None,
                installation_mode: skill
                    .install_id
                    .map(|_| super::skill_governance::InstallationMode::Copy),
                destination: skill.install_path.clone(),
                fingerprint: skill.fingerprint.clone(),
                enabled: skill.enabled,
                shadowed: skill.shadowed,
                broken_symlink: skill
                    .issues
                    .iter()
                    .any(|issue| issue.code == "broken_symlink"),
                evidence_status: evidence_status(skill, false),
                evidence_source: skill.evidence.source.clone(),
                session_effective: skill.session_effective.status.to_owned(),
                session_reason: skill.session_effective.reason.clone(),
                observed_at: runtime.observed_at,
                supported: runtime.compatibility != RuntimeSkillCompatibility::Unsupported
                    && !native_unsupported,
            });
        }
    }
    for agent in &machine.agents {
        let native_unsupported = agent
            .issues
            .iter()
            .any(|issue| issue.code == "native_probe_unsupported");
        for skill in agent
            .skills
            .iter()
            .filter(|skill| matches!(skill.scope.as_str(), "workspace" | "repo"))
        {
            skills.push(ObservedSkill {
                logical_identity: skill.name.clone(),
                runtime: agent.runtime.clone(),
                scope: super::skill_governance::GovernanceScope::Agent,
                scope_id: Some(agent.agent_id.to_string()),
                source_provenance: skill.source_url.clone().or_else(|| {
                    Some(format!(
                        "filesystem:{}",
                        skill.resolved_path.as_deref().unwrap_or(&skill.source_path)
                    ))
                }),
                version: skill.source_ref.clone(),
                content_digest: None,
                manifest_digest: None,
                installation_mode: skill
                    .install_id
                    .map(|_| super::skill_governance::InstallationMode::Copy),
                destination: skill.install_path.clone(),
                fingerprint: skill.fingerprint.clone(),
                enabled: skill.enabled,
                shadowed: skill.shadowed,
                broken_symlink: skill
                    .issues
                    .iter()
                    .any(|issue| issue.code == "broken_symlink"),
                evidence_status: evidence_status(skill, true),
                evidence_source: skill.evidence.source.clone(),
                session_effective: skill.session_effective.status.to_owned(),
                session_reason: skill.session_effective.reason.clone(),
                observed_at: agent.observed_at,
                supported: agent.compatibility != RuntimeSkillCompatibility::Unsupported
                    && !native_unsupported,
            });
        }
    }
    let diagnostics = machine
        .diagnostics
        .into_iter()
        .map(|diagnostic| ObservationDiagnostic {
            fingerprint: diagnostic.fingerprint,
            runtime: diagnostic.runtime,
            subject: diagnostic.subject.to_owned(),
            stage: diagnostic.stage.to_owned(),
            error_type: diagnostic.error_type.to_owned(),
            message: diagnostic.message,
            observed_at: diagnostic.observed_at,
        })
        .collect();
    finalize_observation(machine.observed_at, skills, diagnostics).map_err(ApiError::bad_request)
}

fn governance_scope(scope: &str) -> super::skill_governance::GovernanceScope {
    if matches!(scope, "workspace" | "repo") {
        super::skill_governance::GovernanceScope::Workspace
    } else {
        super::skill_governance::GovernanceScope::Machine
    }
}

fn evidence_status(skill: &SkillView, agent_workspace: bool) -> String {
    if skill.session_effective.status != "unknown" {
        "session_effective".to_owned()
    } else if skill.evidence.source != "filesystem" && skill.presence == "discovered" {
        "runtime_discovered".to_owned()
    } else if agent_workspace {
        "agent_workspace".to_owned()
    } else {
        "machine_discovered".to_owned()
    }
}

async fn build_agent_skill_inventory(
    state: &AppState,
    agent: Agent,
    force: bool,
) -> Result<AgentSkillInventoryResponse, ApiError> {
    let agent_for_probe = agent.clone();
    let runtime = Arc::clone(&state.runtime);
    let snapshot = state
        .skill_snapshots
        .get_or_refresh(
            SkillSnapshotKey::Agent(agent.id),
            force,
            move || async move { runtime.inspect_skills(&agent_for_probe).await },
        )
        .await?;
    build_agent_skill_inventory_from_inspection(state, agent, snapshot).await
}

async fn build_agent_skill_list(
    state: &AppState,
    agent: Agent,
) -> Result<Vec<SkillView>, ApiError> {
    let runtime_name = agent.runtime.clone();
    let evidence = RuntimeSkillEvidence::default();
    let observed_at = chrono::Utc::now();
    let scanned = state
        .runtime
        .list_skills(&agent)
        .await?
        .into_iter()
        .map(|skill| RuntimeSkillFinding {
            fingerprint: stable_api_fingerprint(&format!("{}|{}", runtime_name, skill.path)),
            scope: skill.skill_type.clone(),
            source_path: skill.path.clone(),
            resolved_path: None,
            presence: "installed".to_owned(),
            runtime: runtime_name.clone(),
            evidence: evidence.clone(),
            enabled: None,
            valid: None,
            duplicate: false,
            shadowed: false,
            issues: Vec::new(),
            skill,
        })
        .collect();
    let snapshot = SkillSnapshot {
        inspection: RuntimeSkillInspection {
            observed_at,
            runtime: runtime_name,
            compatibility: state.runtime.skill_compatibility(&agent.runtime),
            evidence,
            search_paths: Vec::new(),
            skills: scanned,
            issues: Vec::new(),
        },
        cache_status: "fresh",
        expires_at: observed_at,
    };
    Ok(
        build_agent_skill_inventory_from_inspection(state, agent, snapshot)
            .await?
            .skills,
    )
}

async fn build_agent_skill_inventory_from_inspection(
    state: &AppState,
    agent: Agent,
    snapshot: SkillSnapshot,
) -> Result<AgentSkillInventoryResponse, ApiError> {
    let SkillSnapshot {
        inspection,
        cache_status,
        expires_at,
    } = snapshot;
    let RuntimeSkillInspection {
        observed_at,
        runtime,
        compatibility,
        evidence,
        search_paths,
        skills: scanned,
        mut issues,
    } = inspection;
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
        skills.push(scanned_skill_view(scanned_skill, managed, observed_at));
    }
    for install in by_path.into_values() {
        let issue = RuntimeSkillIssue {
            fingerprint: stable_api_fingerprint(&format!(
                "missing_managed_install|{}|{}",
                agent.id, install.install_path
            )),
            code: "missing_managed_install".to_owned(),
            severity: "error".to_owned(),
            message: "SQLite install record has no discovered filesystem skill".to_owned(),
            path: Some(install.install_path.clone()),
            skill_name: Some(install.library_name.clone()),
            related_paths: vec![install.install_path.clone()],
            related_codes: vec!["missing_managed_install".to_owned()],
        };
        issues.push(issue.clone());
        skills.push(broken_skill_view(
            &runtime,
            evidence.clone(),
            install,
            issue,
            observed_at,
        ));
    }
    skills.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(AgentSkillInventoryResponse {
        observed_at,
        cache_status,
        expires_at,
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

fn scanned_skill_view(
    skill: RuntimeSkillFinding,
    install: Option<AgentSkillInstall>,
    observed_at: chrono::DateTime<chrono::Utc>,
) -> SkillView {
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
        fingerprint: skill.fingerprint,
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
        session_effective: session_effective_evidence(&skill.evidence, observed_at),
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
    observed_at: chrono::DateTime<chrono::Utc>,
) -> SkillView {
    SkillView {
        fingerprint: stable_api_fingerprint(&format!(
            "missing_managed_install|{}",
            install.install_path
        )),
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
        session_effective: session_effective_evidence(&evidence, observed_at),
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

fn session_effective_evidence(
    evidence: &RuntimeSkillEvidence,
    observed_at: chrono::DateTime<chrono::Utc>,
) -> SessionEffectiveEvidence {
    let reason = if evidence.proves_session_visibility {
        "native discovery is not bound to a concrete runtime session"
    } else {
        "discovery evidence does not prove that a concrete runtime session loaded this skill"
    };
    SessionEffectiveEvidence {
        status: "unknown",
        effective: None,
        reason: reason.to_owned(),
        evidence: evidence.clone(),
        observed_at,
        session_id: None,
    }
}

async fn build_runtime_skill_inventory(
    state: &AppState,
    runtime: &str,
    force: bool,
) -> Result<RuntimeSkillInventorySummary, ApiError> {
    let runtime_name = runtime.to_owned();
    let runtime_service = Arc::clone(&state.runtime);
    let snapshot = state
        .skill_snapshots
        .get_or_refresh(
            SkillSnapshotKey::Machine(runtime.to_owned()),
            force,
            move || async move { runtime_service.inspect_machine_skills(&runtime_name).await },
        )
        .await?;
    let SkillSnapshot {
        inspection,
        cache_status,
        expires_at,
    } = snapshot;
    let RuntimeSkillInspection {
        observed_at,
        runtime,
        compatibility,
        evidence,
        search_paths,
        skills,
        issues,
    } = inspection;
    let skills: Vec<_> = skills
        .into_iter()
        .map(|skill| scanned_skill_view(skill, None, observed_at))
        .collect();
    Ok(RuntimeSkillInventorySummary {
        observed_at,
        cache_status,
        expires_at,
        runtime,
        compatibility,
        agent_count: 0,
        skill_count: skills.len(),
        issue_count: issues.len(),
        evidence_sources: vec![evidence.source.clone()],
        evidence,
        search_paths,
        skills,
        issues,
    })
}

fn doctor_summary(
    runtimes: &[RuntimeSkillInventorySummary],
    inventories: &[AgentSkillInventoryResponse],
    diagnostics: &[SkillInspectionDiagnostic],
) -> SkillDoctorSummary {
    let runtime_count = runtimes
        .iter()
        .map(|runtime| runtime.runtime.as_str())
        .chain(
            inventories
                .iter()
                .map(|inventory| inventory.runtime.as_str()),
        )
        .collect::<HashSet<_>>()
        .len();
    let mut issues: HashMap<&str, &RuntimeSkillIssue> = HashMap::new();
    for issue in runtimes
        .iter()
        .flat_map(|runtime| &runtime.issues)
        .chain(inventories.iter().flat_map(|inventory| &inventory.issues))
    {
        issues.entry(&issue.fingerprint).or_insert(issue);
    }
    let error_count = issues
        .values()
        .filter(|issue| issue.severity == "error")
        .count()
        + diagnostics.len();
    let warning_count = issues
        .values()
        .filter(|issue| issue.severity == "warning")
        .count();
    let unique_skills: HashSet<&str> = runtimes
        .iter()
        .flat_map(|runtime| &runtime.skills)
        .chain(inventories.iter().flat_map(|inventory| &inventory.skills))
        .map(|skill| skill.fingerprint.as_str())
        .collect();
    SkillDoctorSummary {
        status: if error_count > 0 {
            "error"
        } else if warning_count > 0 {
            "warning"
        } else {
            "ok"
        },
        runtime_count,
        agent_count: inventories.len(),
        skill_count: unique_skills.len(),
        issue_count: issues.len() + diagnostics.len(),
        error_count,
        warning_count,
    }
}

fn skill_diagnostic(
    subject: &'static str,
    runtime: String,
    agent_id: Option<Uuid>,
    agent_name: Option<String>,
    stage: &'static str,
    error_type: &'static str,
    message: String,
) -> SkillInspectionDiagnostic {
    let fingerprint = stable_api_fingerprint(&format!(
        "{subject}|{runtime}|{}|{stage}|{error_type}",
        agent_id.map_or_else(String::new, |id| id.to_string())
    ));
    SkillInspectionDiagnostic {
        fingerprint,
        subject,
        runtime,
        agent_id,
        agent_name,
        stage,
        error_type,
        message,
        observed_at: chrono::Utc::now(),
    }
}

fn stable_api_fingerprint(value: &str) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn failed_runtime_inventory(
    state: &AppState,
    runtime: String,
    message: String,
) -> RuntimeSkillInventorySummary {
    let observed_at = chrono::Utc::now();
    RuntimeSkillInventorySummary {
        observed_at,
        cache_status: "fresh",
        expires_at: observed_at,
        compatibility: state.runtime.skill_compatibility(&runtime),
        runtime,
        agent_count: 0,
        skill_count: 0,
        issue_count: 0,
        evidence_sources: vec!["unavailable".to_owned()],
        evidence: RuntimeSkillEvidence {
            source: "unavailable".to_owned(),
            detail: message,
            proves_session_visibility: false,
        },
        search_paths: Vec::new(),
        skills: Vec::new(),
        issues: Vec::new(),
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
        Ok(install) => {
            state.skill_snapshots.invalidate_agent(agent_id).await;
            Ok(Json(InstallAgentSkillResponse {
                install_id: install.id,
                install_path,
                bytes,
            }))
        }
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
    state.skill_snapshots.invalidate_agent(agent_id).await;
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
    use std::sync::atomic::{AtomicUsize, Ordering};

    use chrono::Utc;

    use super::*;
    use crate::RuntimeSkill;

    fn empty_inspection(runtime: &str) -> RuntimeSkillInspection {
        RuntimeSkillInspection {
            observed_at: Utc::now(),
            runtime: runtime.to_owned(),
            compatibility: RuntimeSkillCompatibility::Supported,
            evidence: RuntimeSkillEvidence::default(),
            search_paths: Vec::new(),
            skills: Vec::new(),
            issues: Vec::new(),
        }
    }

    #[tokio::test]
    async fn snapshot_ttl_expires_and_force_bypasses_an_older_entry() {
        let coordinator = SkillSnapshotCoordinator::with_ttl(Duration::from_millis(20));
        let calls = Arc::new(AtomicUsize::new(0));
        let key = SkillSnapshotKey::Machine("fake".to_owned());

        for expected_status in ["fresh", "cached"] {
            let calls = Arc::clone(&calls);
            let snapshot = coordinator
                .get_or_refresh(key.clone(), false, move || async move {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Ok(empty_inspection("fake"))
                })
                .await
                .expect("snapshot");
            assert_eq!(snapshot.cache_status, expected_status);
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        tokio::time::sleep(Duration::from_millis(25)).await;
        let refresh_calls = Arc::clone(&calls);
        let expired = coordinator
            .get_or_refresh(key.clone(), false, move || async move {
                refresh_calls.fetch_add(1, Ordering::SeqCst);
                Ok(empty_inspection("fake"))
            })
            .await
            .expect("expired refresh");
        assert_eq!(expired.cache_status, "fresh");
        assert_eq!(calls.load(Ordering::SeqCst), 2);

        let force_calls = Arc::clone(&calls);
        let forced = coordinator
            .get_or_refresh(key, true, move || async move {
                force_calls.fetch_add(1, Ordering::SeqCst);
                Ok(empty_inspection("fake"))
            })
            .await
            .expect("forced refresh");
        assert_eq!(forced.cache_status, "fresh");
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn managed_skill_preserves_native_discovery_presence() {
        let finding = RuntimeSkillFinding {
            fingerprint: "skill-reviewer".to_owned(),
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

        let view = scanned_skill_view(finding, Some(install), Utc::now());

        assert_eq!(view.state, "managed");
        assert_eq!(view.presence, "discovered");
        assert_eq!(view.evidence.source, "codex_app_server");
        assert_eq!(view.session_effective.status, "unknown");
        assert_eq!(view.session_effective.effective, None);
    }
}
