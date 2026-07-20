use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use chrono::Utc;
use cocli_api::{
    GovernanceScopeCapability, RuntimeError, RuntimeSkill, RuntimeSkillCompatibility,
    RuntimeSkillEvidence, RuntimeSkillFileContent, RuntimeSkillFileEntry, RuntimeSkillFinding,
    RuntimeSkillInspection, RuntimeSkillIssue, RuntimeSkillSearchPath,
};
use cocli_driver_core::types::{
    NativeSkill, NativeSkillProbe, SkillCompatibility, SkillDiscoveryEvidence,
};
use cocli_runtime_pool::RuntimeRegistry;
use cocli_store::{Agent, SkillLibraryFile};
use unicode_normalization::UnicodeNormalization;
use uuid::Uuid;

use crate::runtime::LocalRuntimeConfig;

const MAX_SKILL_BROWSER_BYTES: u64 = 1024 * 1024;
const MAX_SKILL_BROWSER_ENTRIES: usize = 5_000;
const MANAGED_SKILL_MARKER: &str = ".cocli-managed";

pub(crate) fn compatibility(
    registry: &Arc<RuntimeRegistry>,
    runtime: &str,
) -> RuntimeSkillCompatibility {
    registry
        .get(runtime)
        .map(|driver| compatibility_label(driver.skill_compatibility()))
        .unwrap_or_else(|| static_compatibility(runtime))
}

pub(crate) async fn list(
    registry: &Arc<RuntimeRegistry>,
    config: &LocalRuntimeConfig,
    agent: &Agent,
) -> Result<Vec<RuntimeSkill>, RuntimeError> {
    Ok(inspect_agent(registry, config, agent, false)
        .await?
        .skills
        .into_iter()
        .map(|finding| finding.skill)
        .collect())
}

pub(crate) async fn inspect(
    registry: &Arc<RuntimeRegistry>,
    config: &LocalRuntimeConfig,
    agent: &Agent,
) -> Result<RuntimeSkillInspection, RuntimeError> {
    inspect_agent(registry, config, agent, true).await
}

async fn inspect_agent(
    registry: &Arc<RuntimeRegistry>,
    config: &LocalRuntimeConfig,
    agent: &Agent,
    include_native: bool,
) -> Result<RuntimeSkillInspection, RuntimeError> {
    let workspace = config.workspace_root.join(agent.id.to_string());
    let paths = skill_paths(registry, &agent.runtime, &workspace);
    let compatibility = compatibility(registry, &agent.runtime);
    let driver = registry.get(&agent.runtime);
    let evidence = driver
        .as_ref()
        .map(|driver| driver.skill_discovery_evidence())
        .unwrap_or(SkillDiscoveryEvidence::FILESYSTEM);
    let runtime = agent.runtime.clone();
    let scan_workspace = workspace.clone();
    let mut inspection = tokio::task::spawn_blocking(move || {
        scan_skill_paths(&runtime, compatibility, evidence, &scan_workspace, &paths)
    })
    .await
    .map_err(|error| RuntimeError::Delivery(format!("skill scan task failed: {error}")))?
    .map_err(|error| skill_io_error("scan runtime skills", &error))?;

    if include_native {
        if let Some(driver) = driver {
            match driver.probe_skills(&workspace).await {
                Ok(Some(probe)) => merge_native_probe(&mut inspection, probe, &workspace),
                Ok(None) if agent.runtime == "cursor" => inspection.issues.push(skill_issue(
                    "native_probe_unsupported",
                    "warning",
                    "Cursor exposes filesystem-discovered Agent Skills but no stable native discovery or session-effective contract; manual verification is required".to_owned(),
                    None,
                    None,
                )),
                Ok(None) => {}
                Err(error) => inspection.issues.push(skill_issue(
                    "native_probe_failed",
                    "warning",
                    format!(
                        "native runtime skill probe failed; using filesystem evidence: {error}"
                    ),
                    None,
                    None,
                )),
            }
        }
    }
    inspection.observed_at = Utc::now();
    inspection.issues = group_issues(std::mem::take(&mut inspection.issues));
    namespace_issue_fingerprints(&mut inspection);
    Ok(inspection)
}

pub(crate) async fn inspect_machine(
    registry: &Arc<RuntimeRegistry>,
    config: &LocalRuntimeConfig,
    runtime: &str,
) -> Result<RuntimeSkillInspection, RuntimeError> {
    let workspace = &config.workspace_root;
    let paths: Vec<PathBuf> = skill_paths(registry, runtime, workspace)
        .into_iter()
        .filter(|path| !path.starts_with(workspace))
        .collect();
    let compatibility = compatibility(registry, runtime);
    let driver = registry.get(runtime);
    let scan_runtime = runtime.to_owned();
    let scan_workspace = workspace.clone();
    let mut inspection = tokio::task::spawn_blocking(move || {
        scan_skill_paths(
            &scan_runtime,
            compatibility,
            SkillDiscoveryEvidence::FILESYSTEM,
            &scan_workspace,
            &paths,
        )
    })
    .await
    .map_err(|error| RuntimeError::Delivery(format!("machine skill scan task failed: {error}")))?
    .map_err(|error| skill_io_error("scan machine runtime skills", &error))?;

    if let Some(driver) = driver {
        match driver.probe_skills(workspace).await {
            Ok(Some(probe)) => {
                merge_native_probe(&mut inspection, probe, workspace);
                inspection.skills.retain(|skill| skill.scope != "workspace");
            }
            Ok(None) if runtime == "cursor" => inspection.issues.push(skill_issue(
                "native_probe_unsupported",
                "warning",
                "Cursor exposes filesystem-discovered Agent Skills but no stable native discovery or session-effective contract; manual verification is required".to_owned(),
                None,
                None,
            )),
            Ok(None) => {}
            Err(error) => inspection.issues.push(skill_issue(
                "native_probe_failed",
                "warning",
                format!("native runtime skill probe failed; using filesystem evidence: {error}"),
                None,
                None,
            )),
        }
    }
    inspection.observed_at = Utc::now();
    inspection.issues = group_issues(std::mem::take(&mut inspection.issues));
    namespace_issue_fingerprints(&mut inspection);
    Ok(inspection)
}

pub(crate) async fn install(
    registry: &Arc<RuntimeRegistry>,
    config: &LocalRuntimeConfig,
    agent: &Agent,
    skill_name: &str,
    files: &[SkillLibraryFile],
) -> Result<String, RuntimeError> {
    validate_skill_name(skill_name)?;
    if compatibility(registry, &agent.runtime) == RuntimeSkillCompatibility::Unsupported {
        return Err(RuntimeError::Unsupported(format!(
            "{} does not support skills",
            agent.runtime
        )));
    }
    let workspace = config.workspace_root.join(agent.id.to_string());
    tokio::fs::create_dir_all(&workspace)
        .await
        .map_err(|error| skill_io_error("create agent workspace", &error))?;
    let root = workspace_skill_roots(registry, &agent.runtime, &workspace)
        .into_iter()
        .next()
        .ok_or_else(|| {
            RuntimeError::Unsupported(format!("{} exposes no workspace skill path", agent.runtime))
        })?;
    let root_relative = relative_to_workspace(&workspace, &root)?;
    let install_path = format!("{root_relative}/{skill_name}");
    let target = safe_workspace_path(&workspace, &install_path)?;
    let temporary = target.with_file_name(format!(".{skill_name}.tmp.{}", Uuid::new_v4()));
    let backup = target.with_file_name(format!(".{skill_name}.backup.{}", Uuid::new_v4()));

    if tokio::fs::try_exists(&temporary)
        .await
        .map_err(|error| skill_io_error("inspect temporary skill path", &error))?
    {
        remove_path(&temporary).await?;
    }
    tokio::fs::create_dir_all(&temporary)
        .await
        .map_err(|error| skill_io_error("create temporary skill directory", &error))?;
    for file in files {
        let path = safe_child_path(&temporary, &file.rel_path)?;
        let parent = path.parent().ok_or_else(|| {
            RuntimeError::Delivery("skill file has no parent directory".to_owned())
        })?;
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|error| skill_io_error("create skill file directory", &error))?;
        tokio::fs::write(&path, &file.content)
            .await
            .map_err(|error| skill_io_error("write skill file", &error))?;
        set_file_mode(&path, file.mode).await?;
    }
    tokio::fs::write(temporary.join(MANAGED_SKILL_MARKER), skill_name)
        .await
        .map_err(|error| skill_io_error("write managed skill marker", &error))?;

    let had_target = tokio::fs::symlink_metadata(&target).await.is_ok();
    if had_target {
        tokio::fs::rename(&target, &backup)
            .await
            .map_err(|error| skill_io_error("backup existing skill", &error))?;
    }
    if let Err(error) = tokio::fs::rename(&temporary, &target).await {
        if had_target {
            let _ = tokio::fs::rename(&backup, &target).await;
        }
        let _ = remove_path(&temporary).await;
        return Err(skill_io_error("activate installed skill", &error));
    }
    if had_target {
        remove_path(&backup).await?;
    }
    Ok(install_path)
}

pub(crate) fn governance_target(
    registry: &Arc<RuntimeRegistry>,
    config: &LocalRuntimeConfig,
    agent: &Agent,
    skill_name: &str,
) -> Result<cocli_api::GovernanceSkillTarget, RuntimeError> {
    validate_skill_name(skill_name)?;
    if compatibility(registry, &agent.runtime) == RuntimeSkillCompatibility::Unsupported {
        return Err(RuntimeError::Unsupported(format!(
            "{} does not support skills",
            agent.runtime
        )));
    }
    let workspace = config.workspace_root.join(agent.id.to_string());
    let search_root = workspace_skill_roots(registry, &agent.runtime, &workspace)
        .into_iter()
        .next()
        .ok_or_else(|| {
            RuntimeError::Unsupported(format!("{} exposes no workspace skill path", agent.runtime))
        })?;
    let entry_path = safe_child_path(&search_root, skill_name)?;
    Ok(cocli_api::GovernanceSkillTarget {
        scope_root: workspace,
        search_root,
        entry_path,
    })
}

pub(crate) fn governance_scope_capabilities(
    registry: &Arc<RuntimeRegistry>,
    config: &LocalRuntimeConfig,
    runtime: &str,
    scope: &str,
    resolved_scope_root: Option<&Path>,
) -> Result<Vec<GovernanceScopeCapability>, RuntimeError> {
    let (scope_root, paths) = match scope {
        "machine" => {
            let home = std::env::var_os("HOME").map(PathBuf::from).ok_or_else(|| {
                RuntimeError::Unsupported("HOME is unavailable for machine Skill roots".to_owned())
            })?;
            let workspace_probe = config.workspace_root.clone();
            let paths = skill_paths(registry, runtime, &workspace_probe)
                .into_iter()
                .filter(|path| !path.starts_with(&workspace_probe))
                .collect::<Vec<_>>();
            (home, paths)
        }
        "workspace" => {
            let root = resolved_scope_root.ok_or_else(|| {
                RuntimeError::Unsupported(
                    "Workspace scope requires a resolved durable binding".to_owned(),
                )
            })?;
            if !root.is_absolute() {
                return Err(RuntimeError::Unsupported(
                    "Workspace binding is not an absolute path".to_owned(),
                ));
            }
            let paths = skill_paths(registry, runtime, root)
                .into_iter()
                .filter(|path| path.starts_with(root))
                .collect::<Vec<_>>();
            (root.to_path_buf(), paths)
        }
        _ => {
            return Err(RuntimeError::Unsupported(format!(
                "unsupported governed Skill scope: {scope}"
            )))
        }
    };
    Ok(governance_scope_capabilities_for_paths(
        runtime,
        scope,
        &scope_root,
        &paths,
    ))
}

pub(crate) fn governance_target_in_scope(
    registry: &Arc<RuntimeRegistry>,
    config: &LocalRuntimeConfig,
    runtime: &str,
    scope: &str,
    resolved_scope_root: Option<&Path>,
    skill_name: &str,
) -> Result<cocli_api::GovernanceSkillTarget, RuntimeError> {
    validate_skill_name(skill_name)?;
    if compatibility(registry, runtime) == RuntimeSkillCompatibility::Unsupported {
        return Err(RuntimeError::Unsupported(format!(
            "{runtime} does not support skills"
        )));
    }
    let mut capabilities =
        governance_scope_capabilities(registry, config, runtime, scope, resolved_scope_root)?;
    capabilities.sort_by_key(|capability| capability.root_kind == "shared");
    let capability = capabilities
        .into_iter()
        .find(|capability| capability.supported)
        .ok_or_else(|| {
            RuntimeError::Unsupported(format!(
                "{runtime} exposes no automatically writable {scope} Skill root"
            ))
        })?;
    let search_root = PathBuf::from(capability.path);
    let scope_root = match scope {
        "machine" => std::env::var_os("HOME").map(PathBuf::from).ok_or_else(|| {
            RuntimeError::Unsupported("HOME is unavailable for machine Skill roots".to_owned())
        })?,
        "workspace" => resolved_scope_root
            .ok_or_else(|| {
                RuntimeError::Unsupported(
                    "Workspace scope requires a resolved durable binding".to_owned(),
                )
            })?
            .to_path_buf(),
        _ => unreachable!("scope was checked by governance_scope_capabilities"),
    };
    let entry_path = safe_child_path(&search_root, skill_name)?;
    Ok(cocli_api::GovernanceSkillTarget {
        scope_root,
        search_root,
        entry_path,
    })
}

fn governance_scope_capabilities_for_paths(
    runtime: &str,
    scope: &str,
    scope_root: &Path,
    paths: &[PathBuf],
) -> Vec<GovernanceScopeCapability> {
    let canonical_scope = fs::canonicalize(scope_root).ok();
    let mut seen = HashSet::new();
    let mut capabilities = Vec::new();
    for path in paths {
        let canonical = fs::canonicalize(path).ok();
        let lexical_key = normalized_path_alias_key(&lexical_normalize(path));
        let canonical_key = canonical
            .as_ref()
            .map(|resolved| normalized_path_alias_key(resolved));
        if seen.contains(&lexical_key)
            || canonical_key.as_ref().is_some_and(|key| seen.contains(key))
        {
            continue;
        }
        seen.insert(lexical_key);
        if let Some(key) = canonical_key {
            seen.insert(key);
        }
        let exists = fs::symlink_metadata(path).is_ok();
        let exact_symlink =
            fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink());
        let reserved = path
            .components()
            .next_back()
            .is_some_and(|component| component.as_os_str() == OsStr::new(".system"));
        let legacy_commands = path
            .components()
            .next_back()
            .is_some_and(|component| component.as_os_str() == OsStr::new("commands"));
        let within_scope = canonical.as_ref().map_or_else(
            || path.starts_with(scope_root),
            |resolved| {
                canonical_scope
                    .as_ref()
                    .is_some_and(|root| resolved.starts_with(root))
            },
        );
        let component_symlink = !exact_symlink && contains_symlink_component(scope_root, path);
        let writable = nearest_existing_directory(path)
            .as_deref()
            .is_some_and(directory_is_writable);
        let same_device = nearest_existing_directory(path)
            .as_deref()
            .is_some_and(|existing| directories_share_device(scope_root, existing));
        let (status, supported, blocked_reason) = if reserved {
            (
                "reserved",
                false,
                Some("runtime_managed_system_root".to_owned()),
            )
        } else if legacy_commands {
            ("blocked", false, Some("legacy_commands_root".to_owned()))
        } else if exact_symlink {
            (
                "blocked",
                false,
                Some("whole_root_symlink_takeover".to_owned()),
            )
        } else if component_symlink {
            ("blocked", false, Some("symlink_escape".to_owned()))
        } else if !within_scope {
            ("blocked", false, Some("root_outside_scope".to_owned()))
        } else if !writable {
            ("read_only", false, Some("root_not_writable".to_owned()))
        } else if !same_device {
            (
                "blocked",
                false,
                Some("cross_filesystem_atomic_rename".to_owned()),
            )
        } else if exists {
            ("supported", true, None)
        } else {
            ("missing", true, None)
        };
        capabilities.push(GovernanceScopeCapability {
            runtime: runtime.to_owned(),
            scope: scope.to_owned(),
            root_kind: if is_shared_skill_root(path) {
                "shared"
            } else {
                "runtime_specific"
            }
            .to_owned(),
            path: path.to_string_lossy().into_owned(),
            status: status.to_owned(),
            exists,
            writable,
            atomic_rename: supported,
            supported,
            evidence: "runtime_driver_search_path".to_owned(),
            blocked_reason,
        });
    }
    capabilities
}

fn normalized_path_alias_key(path: &Path) -> String {
    path.to_string_lossy()
        .nfc()
        .collect::<String>()
        .to_lowercase()
}

fn lexical_normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            _ => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

fn is_shared_skill_root(path: &Path) -> bool {
    path.ends_with(Path::new(".agents/skills"))
}

fn nearest_existing_directory(path: &Path) -> Option<PathBuf> {
    let mut current = path;
    loop {
        if current.is_dir() {
            return Some(current.to_path_buf());
        }
        current = current.parent()?;
    }
}

fn contains_symlink_component(scope_root: &Path, path: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(scope_root) else {
        return false;
    };
    let mut current = scope_root.to_path_buf();
    if fs::symlink_metadata(&current).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return true;
    }
    for component in relative.components() {
        let Component::Normal(component) = component else {
            continue;
        };
        current.push(component);
        if fs::symlink_metadata(&current).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
            return true;
        }
    }
    false
}

#[cfg(unix)]
fn directory_is_writable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    fs::metadata(path)
        .is_ok_and(|metadata| metadata.is_dir() && metadata.permissions().mode() & 0o222 != 0)
}

#[cfg(unix)]
fn directories_share_device(left: &Path, right: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;

    let left = nearest_existing_directory(left)
        .and_then(|path| fs::metadata(path).ok())
        .map(|metadata| metadata.dev());
    let right = nearest_existing_directory(right)
        .and_then(|path| fs::metadata(path).ok())
        .map(|metadata| metadata.dev());
    left.is_some() && left == right
}

#[cfg(not(unix))]
fn directory_is_writable(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|metadata| metadata.is_dir() && !metadata.permissions().readonly())
}

#[cfg(not(unix))]
fn directories_share_device(_left: &Path, _right: &Path) -> bool {
    true
}

pub(crate) async fn uninstall(
    registry: &Arc<RuntimeRegistry>,
    config: &LocalRuntimeConfig,
    agent: &Agent,
    install_path: &str,
) -> Result<(), RuntimeError> {
    let workspace = config.workspace_root.join(agent.id.to_string());
    tokio::fs::create_dir_all(&workspace)
        .await
        .map_err(|error| skill_io_error("create agent workspace", &error))?;
    validate_install_path(registry, &agent.runtime, &workspace, install_path)?;
    let target = safe_workspace_path(&workspace, install_path)?;
    remove_path(&target).await
}

pub(crate) async fn list_files(
    registry: &Arc<RuntimeRegistry>,
    config: &LocalRuntimeConfig,
    agent: &Agent,
    install_path: &str,
) -> Result<Vec<RuntimeSkillFileEntry>, RuntimeError> {
    let workspace = config.workspace_root.join(agent.id.to_string());
    validate_install_path(registry, &agent.runtime, &workspace, install_path)?;
    let target = safe_workspace_path(&workspace, install_path)?;
    tokio::task::spawn_blocking(move || list_skill_tree(&target))
        .await
        .map_err(|error| RuntimeError::Delivery(format!("skill file scan task failed: {error}")))?
        .map_err(|error| skill_io_error("list installed skill files", &error))
}

pub(crate) async fn read_file(
    registry: &Arc<RuntimeRegistry>,
    config: &LocalRuntimeConfig,
    agent: &Agent,
    install_path: &str,
    relative_path: &str,
) -> Result<RuntimeSkillFileContent, RuntimeError> {
    let workspace = config.workspace_root.join(agent.id.to_string());
    validate_install_path(registry, &agent.runtime, &workspace, install_path)?;
    let install_root = safe_workspace_path(&workspace, install_path)?;
    let target = safe_child_path(&install_root, relative_path)?;
    let bytes = tokio::fs::read(&target)
        .await
        .map_err(|error| skill_io_error("read installed skill file", &error))?;
    if bytes.len() as u64 > MAX_SKILL_BROWSER_BYTES || bytes.iter().take(512).any(|byte| *byte == 0)
    {
        return Ok(RuntimeSkillFileContent {
            content: String::new(),
            binary: true,
        });
    }
    match String::from_utf8(bytes) {
        Ok(content) => Ok(RuntimeSkillFileContent {
            content,
            binary: false,
        }),
        Err(_) => Ok(RuntimeSkillFileContent {
            content: String::new(),
            binary: true,
        }),
    }
}

fn compatibility_label(compatibility: SkillCompatibility) -> RuntimeSkillCompatibility {
    match compatibility {
        SkillCompatibility::Supported => RuntimeSkillCompatibility::Supported,
        SkillCompatibility::Uncertain => RuntimeSkillCompatibility::Uncertain,
        SkillCompatibility::Unsupported => RuntimeSkillCompatibility::Unsupported,
    }
}

fn static_compatibility(runtime: &str) -> RuntimeSkillCompatibility {
    match runtime {
        "claude" | "codex" | "grok" => RuntimeSkillCompatibility::Supported,
        "cursor" | "gemini" | "kimi" | "opencode" => RuntimeSkillCompatibility::Uncertain,
        "chatrs" => RuntimeSkillCompatibility::Unsupported,
        _ => RuntimeSkillCompatibility::Unknown,
    }
}

fn skill_paths(registry: &Arc<RuntimeRegistry>, runtime: &str, workspace: &Path) -> Vec<PathBuf> {
    registry
        .get(runtime)
        .map(|driver| driver.skill_search_paths(workspace))
        .unwrap_or_else(|| static_skill_paths(runtime, workspace))
}

fn workspace_skill_roots(
    registry: &Arc<RuntimeRegistry>,
    runtime: &str,
    workspace: &Path,
) -> Vec<PathBuf> {
    skill_paths(registry, runtime, workspace)
        .into_iter()
        .filter(|path| path.starts_with(workspace))
        .collect()
}

fn static_skill_paths(runtime: &str, workspace: &Path) -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut paths = match runtime {
        "claude" => vec![workspace.join(".claude/skills")],
        "codex" => vec![
            workspace.join(".codex/skills"),
            workspace.join(".agents/skills"),
        ],
        "cursor" => vec![
            workspace.join(".cursor/skills"),
            workspace.join(".agents/skills"),
            workspace.join(".claude/skills"),
            workspace.join(".codex/skills"),
        ],
        "gemini" => vec![workspace.join(".gemini/skills")],
        "grok" => vec![workspace.join(".grok/skills")],
        "kimi" => vec![workspace.join(".kimi-code/skills")],
        "opencode" => vec![workspace.join(".opencode/skills")],
        _ => Vec::new(),
    };
    if let Some(home) = home {
        match runtime {
            "claude" => paths.push(home.join(".claude/skills")),
            "codex" => {
                paths.push(home.join(".codex/skills"));
                paths.push(home.join(".codex/skills/.system"));
                paths.push(home.join(".agents/skills"));
            }
            "cursor" => {
                paths.push(home.join(".cursor/skills"));
                paths.push(home.join(".agents/skills"));
                paths.push(home.join(".claude/skills"));
                paths.push(home.join(".codex/skills"));
            }
            "gemini" => paths.push(home.join(".gemini/skills")),
            "grok" => {
                paths.push(home.join(".grok/skills"));
                paths.push(home.join(".agents/skills"));
            }
            "kimi" => paths.push(home.join(".kimi-code/skills")),
            _ => {}
        }
    }
    paths
}

fn scan_skill_paths(
    runtime: &str,
    compatibility: RuntimeSkillCompatibility,
    driver_evidence: SkillDiscoveryEvidence,
    workspace: &Path,
    paths: &[PathBuf],
) -> io::Result<RuntimeSkillInspection> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let evidence = RuntimeSkillEvidence {
        source: driver_evidence.source.to_owned(),
        detail: driver_evidence.detail.to_owned(),
        proves_session_visibility: driver_evidence.proves_session_visibility,
    };
    let mut seen_roots = HashSet::new();
    let mut seen_targets: HashMap<PathBuf, String> = HashMap::new();
    let mut seen_names: HashMap<String, String> = HashMap::new();
    let mut search_paths = Vec::with_capacity(paths.len());
    let mut skills = Vec::new();
    let mut issues = Vec::new();

    for root in paths {
        let scope = if root.starts_with(workspace) {
            "workspace"
        } else {
            "user"
        };
        let short_root = shorten_path(root, home.as_deref());
        let symlink_metadata = match fs::symlink_metadata(root) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                search_paths.push(RuntimeSkillSearchPath {
                    path: short_root,
                    scope: scope.to_owned(),
                    exists: false,
                    readable: false,
                    symlink: false,
                    resolved_path: None,
                    issue: None,
                });
                continue;
            }
            Err(error) => {
                let issue = skill_issue(
                    "path_unreadable",
                    "error",
                    format!("cannot inspect runtime skill search path: {error}"),
                    Some(short_root.clone()),
                    None,
                );
                search_paths.push(RuntimeSkillSearchPath {
                    path: short_root,
                    scope: scope.to_owned(),
                    exists: false,
                    readable: false,
                    symlink: false,
                    resolved_path: None,
                    issue: Some(issue.message.clone()),
                });
                issues.push(issue);
                continue;
            }
        };
        let is_symlink = symlink_metadata.file_type().is_symlink();
        let resolved_root = match fs::canonicalize(root) {
            Ok(path) => path,
            Err(error) => {
                let code = if is_symlink {
                    "broken_symlink"
                } else {
                    "path_unreadable"
                };
                let issue = skill_issue(
                    code,
                    "error",
                    format!("cannot resolve runtime skill search path: {error}"),
                    Some(short_root.clone()),
                    None,
                );
                search_paths.push(RuntimeSkillSearchPath {
                    path: short_root,
                    scope: scope.to_owned(),
                    exists: true,
                    readable: false,
                    symlink: is_symlink,
                    resolved_path: None,
                    issue: Some(issue.message.clone()),
                });
                issues.push(issue);
                continue;
            }
        };
        let resolved_short = shorten_path(&resolved_root, home.as_deref());
        if !seen_roots.insert(resolved_root) {
            let issue = skill_issue(
                "duplicate_search_path",
                "warning",
                "search path resolves to a target already scanned".to_owned(),
                Some(short_root.clone()),
                None,
            );
            search_paths.push(RuntimeSkillSearchPath {
                path: short_root,
                scope: scope.to_owned(),
                exists: true,
                readable: true,
                symlink: is_symlink,
                resolved_path: Some(resolved_short),
                issue: Some(issue.message.clone()),
            });
            issues.push(issue);
            continue;
        }
        let entries = match fs::read_dir(root) {
            Ok(entries) => entries,
            Err(error) => {
                let issue = skill_issue(
                    "path_unreadable",
                    "error",
                    format!("cannot read runtime skill search path: {error}"),
                    Some(short_root.clone()),
                    None,
                );
                search_paths.push(RuntimeSkillSearchPath {
                    path: short_root,
                    scope: scope.to_owned(),
                    exists: true,
                    readable: false,
                    symlink: is_symlink,
                    resolved_path: Some(resolved_short),
                    issue: Some(issue.message.clone()),
                });
                issues.push(issue);
                continue;
            }
        };
        search_paths.push(RuntimeSkillSearchPath {
            path: short_root,
            scope: scope.to_owned(),
            exists: true,
            readable: true,
            symlink: is_symlink,
            resolved_path: Some(resolved_short),
            issue: None,
        });

        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    issues.push(skill_issue(
                        "path_unreadable",
                        "error",
                        format!("cannot read entry in runtime skill search path: {error}"),
                        Some(shorten_path(root, home.as_deref())),
                        None,
                    ));
                    continue;
                }
            };
            let path = entry.path();
            let source_short = shorten_path(&path, home.as_deref());
            let link_metadata = match fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(error) => {
                    issues.push(skill_issue(
                        "path_unreadable",
                        "error",
                        format!("cannot inspect skill candidate: {error}"),
                        Some(source_short),
                        None,
                    ));
                    continue;
                }
            };
            let candidate_is_symlink = link_metadata.file_type().is_symlink();
            let resolved_candidate = match fs::canonicalize(&path) {
                Ok(path) => path,
                Err(error) => {
                    let code = if candidate_is_symlink {
                        "broken_symlink"
                    } else {
                        "path_unreadable"
                    };
                    issues.push(skill_issue(
                        code,
                        "error",
                        format!("cannot resolve skill candidate: {error}"),
                        Some(source_short),
                        Some(entry.file_name().to_string_lossy().into_owned()),
                    ));
                    continue;
                }
            };
            let metadata = match fs::metadata(&resolved_candidate) {
                Ok(metadata) => metadata,
                Err(error) => {
                    issues.push(skill_issue(
                        "path_unreadable",
                        "error",
                        format!("cannot inspect resolved skill candidate: {error}"),
                        Some(source_short),
                        None,
                    ));
                    continue;
                }
            };
            let (name, metadata_path, install_path, validates_frontmatter) = if metadata.is_dir() {
                let skill_md = resolved_candidate.join("SKILL.md");
                if !skill_md.is_file() {
                    continue;
                }
                (
                    entry.file_name().to_string_lossy().into_owned(),
                    skill_md,
                    relative_to_workspace_optional(workspace, &path),
                    true,
                )
            } else {
                if resolved_candidate.extension() != Some(OsStr::new("md")) {
                    continue;
                }
                (
                    path.file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                    resolved_candidate.clone(),
                    relative_to_workspace_optional(workspace, &path),
                    false,
                )
            };
            let resolved_short = shorten_path(&resolved_candidate, home.as_deref());
            let mut finding_issues = Vec::new();
            let duplicate_target = seen_targets
                .insert(resolved_candidate.clone(), source_short.clone())
                .is_some();
            if duplicate_target {
                finding_issues.push(skill_issue(
                    "duplicate_target",
                    "warning",
                    "skill path resolves to a target already discovered".to_owned(),
                    Some(source_short.clone()),
                    Some(name.clone()),
                ));
            }
            let shadowed = seen_names
                .insert(name.clone(), source_short.clone())
                .is_some();
            if shadowed {
                finding_issues.push(skill_issue(
                    "shadowed_skill",
                    "warning",
                    "a higher-priority search path already provides this skill name".to_owned(),
                    Some(source_short.clone()),
                    Some(name.clone()),
                ));
            }
            let metadata = parse_skill_frontmatter(&metadata_path, validates_frontmatter);
            if let Some(message) = metadata.invalid_reason.clone() {
                finding_issues.push(skill_issue(
                    "invalid_frontmatter",
                    "error",
                    message,
                    Some(shorten_path(&metadata_path, home.as_deref())),
                    Some(name.clone()),
                ));
            }
            issues.extend(finding_issues.iter().cloned());
            let fingerprint = skill_fingerprint(Some(&resolved_candidate), runtime, &name);
            skills.push(RuntimeSkillFinding {
                skill: RuntimeSkill {
                    display_name: metadata.name.unwrap_or_else(|| name.clone()),
                    name,
                    description: metadata.description.unwrap_or_default(),
                    user_invocable: metadata.user_invocable,
                    skill_type: scope.to_owned(),
                    path: shorten_path(&metadata_path, home.as_deref()),
                    install_path,
                },
                runtime: runtime.to_owned(),
                fingerprint,
                scope: scope.to_owned(),
                source_path: source_short,
                resolved_path: Some(resolved_short),
                presence: "discovered".to_owned(),
                evidence: evidence.clone(),
                enabled: None,
                valid: metadata.valid,
                duplicate: duplicate_target || shadowed,
                shadowed,
                issues: finding_issues,
            });
        }
    }
    Ok(RuntimeSkillInspection {
        observed_at: Utc::now(),
        runtime: runtime.to_owned(),
        compatibility,
        evidence,
        search_paths,
        skills,
        issues,
    })
}

fn merge_native_probe(
    inspection: &mut RuntimeSkillInspection,
    probe: NativeSkillProbe,
    workspace: &Path,
) {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let evidence = RuntimeSkillEvidence {
        source: probe.evidence.source.to_owned(),
        detail: probe.evidence.detail.to_owned(),
        proves_session_visibility: probe.evidence.proves_session_visibility,
    };
    inspection.evidence = evidence.clone();

    let native_skills: Vec<(Option<PathBuf>, NativeSkill)> = probe
        .skills
        .into_iter()
        .map(|skill| {
            let path = skill.path.as_deref().map(comparable_path);
            (path, skill)
        })
        .collect();
    let mut matched = HashSet::new();
    for finding in &mut inspection.skills {
        let reported_path = expand_short_path(&finding.skill.path, home.as_deref());
        let comparable = comparable_path(&reported_path);
        if let Some((index, (_, native))) =
            native_skills.iter().enumerate().find(|(index, (path, _))| {
                !matched.contains(index) && path.as_ref() == Some(&comparable)
            })
        {
            matched.insert(index);
            "discovered".clone_into(&mut finding.presence);
            finding.evidence = evidence.clone();
            finding.enabled = native.enabled;
            if let Some(user_invocable) = native.user_invocable {
                finding.skill.user_invocable = user_invocable;
            }
            if finding.skill.description.is_empty() {
                finding.skill.description.clone_from(&native.description);
            }
        } else {
            "installed".clone_into(&mut finding.presence);
            let issue = skill_issue(
                "not_runtime_discovered",
                "warning",
                "filesystem candidate was not returned by the native runtime probe".to_owned(),
                Some(finding.source_path.clone()),
                Some(finding.skill.name.clone()),
            );
            finding.issues.push(issue.clone());
            inspection.issues.push(issue);
        }
    }

    for (index, (_, native)) in native_skills.into_iter().enumerate() {
        if matched.contains(&index) {
            continue;
        }
        inspection.skills.push(native_skill_finding(
            &inspection.runtime,
            native,
            &evidence,
            workspace,
            home.as_deref(),
        ));
    }
    inspection
        .issues
        .extend(probe.issues.into_iter().map(|issue| {
            skill_issue(
                "native_probe_error",
                "error",
                issue.message,
                issue.path.map(|path| shorten_path(&path, home.as_deref())),
                None,
            )
        }));
}

fn native_skill_finding(
    runtime: &str,
    native: NativeSkill,
    evidence: &RuntimeSkillEvidence,
    workspace: &Path,
    home: Option<&Path>,
) -> RuntimeSkillFinding {
    let scope = normalize_native_scope(&native.scope);
    let fingerprint_path = native
        .path
        .as_deref()
        .map(|path| path.parent().unwrap_or(path).to_path_buf());
    let (path, source_path, resolved_path, install_path) = match native.path {
        Some(metadata_path) => {
            let source_path = metadata_path.parent().unwrap_or(&metadata_path);
            (
                shorten_path(&metadata_path, home),
                shorten_path(source_path, home),
                Some(shorten_path(&comparable_path(source_path), home)),
                relative_to_workspace_optional(workspace, source_path),
            )
        }
        None => {
            let source = format!("runtime:{}", native.source);
            (source.clone(), source, None, None)
        }
    };
    let fingerprint = skill_fingerprint(fingerprint_path.as_deref(), runtime, &native.name);
    RuntimeSkillFinding {
        skill: RuntimeSkill {
            display_name: native.name.clone(),
            name: native.name,
            description: native.description,
            user_invocable: native.user_invocable.unwrap_or(false),
            skill_type: scope.to_owned(),
            path,
            install_path,
        },
        runtime: runtime.to_owned(),
        fingerprint,
        scope: scope.to_owned(),
        source_path,
        resolved_path,
        presence: "discovered".to_owned(),
        evidence: evidence.clone(),
        enabled: native.enabled,
        valid: None,
        duplicate: false,
        shadowed: false,
        issues: Vec::new(),
    }
}

fn normalize_native_scope(scope: &str) -> &str {
    match scope {
        "repo" | "workspace" => "workspace",
        "system" | "admin" | "global" => "global",
        _ => "user",
    }
}

fn comparable_path(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn expand_short_path(path: &str, home: Option<&Path>) -> PathBuf {
    match (path.strip_prefix("~/"), home) {
        (Some(relative), Some(home)) => home.join(relative),
        _ => PathBuf::from(path),
    }
}

fn skill_issue(
    code: &str,
    severity: &str,
    message: String,
    path: Option<String>,
    skill_name: Option<String>,
) -> RuntimeSkillIssue {
    let root_code = match code {
        "shadowed_skill" | "not_runtime_discovered" => "skill_visibility",
        other => other,
    };
    let basis = skill_name
        .as_deref()
        .map(|name| format!("{root_code}|skill|{}", name.to_ascii_lowercase()))
        .or_else(|| {
            path.as_deref()
                .map(|path| format!("{root_code}|path|{path}"))
        })
        .unwrap_or_else(|| root_code.to_owned());
    RuntimeSkillIssue {
        fingerprint: stable_fingerprint(&basis),
        code: code.to_owned(),
        severity: severity.to_owned(),
        message,
        related_paths: path.clone().into_iter().collect(),
        related_codes: vec![code.to_owned()],
        path,
        skill_name,
    }
}

fn skill_fingerprint(path: Option<&Path>, runtime: &str, name: &str) -> String {
    let basis = path.map_or_else(
        || format!("runtime|{runtime}|{}", name.to_ascii_lowercase()),
        |path| format!("path|{}", comparable_path(path).to_string_lossy()),
    );
    stable_fingerprint(&basis)
}

fn stable_fingerprint(value: &str) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn group_issues(issues: Vec<RuntimeSkillIssue>) -> Vec<RuntimeSkillIssue> {
    let mut grouped: Vec<RuntimeSkillIssue> = Vec::new();
    let mut indexes = HashMap::new();
    for issue in issues {
        if let Some(index) = indexes.get(&issue.fingerprint).copied() {
            let existing: &mut RuntimeSkillIssue = &mut grouped[index];
            if issue.severity == "error" {
                "error".clone_into(&mut existing.severity);
            }
            for path in issue.related_paths {
                if !existing.related_paths.contains(&path) {
                    existing.related_paths.push(path);
                }
            }
            for code in issue.related_codes {
                if !existing.related_codes.contains(&code) {
                    existing.related_codes.push(code);
                }
            }
            continue;
        }
        indexes.insert(issue.fingerprint.clone(), grouped.len());
        grouped.push(issue);
    }
    grouped
}

fn namespace_issue_fingerprints(inspection: &mut RuntimeSkillInspection) {
    for issue in &mut inspection.issues {
        issue.fingerprint = stable_fingerprint(&format!(
            "runtime|{}|{}",
            inspection.runtime, issue.fingerprint
        ));
    }
    for finding in &mut inspection.skills {
        for issue in &mut finding.issues {
            issue.fingerprint = stable_fingerprint(&format!(
                "runtime|{}|{}",
                inspection.runtime, issue.fingerprint
            ));
        }
    }
}

#[derive(Default)]
struct SkillFrontmatter {
    name: Option<String>,
    description: Option<String>,
    user_invocable: bool,
    valid: Option<bool>,
    invalid_reason: Option<String>,
}

fn parse_skill_frontmatter(path: &Path, validate: bool) -> SkillFrontmatter {
    let Ok(content) = fs::read_to_string(path) else {
        return SkillFrontmatter {
            valid: validate.then_some(false),
            invalid_reason: validate.then(|| "SKILL.md is not readable UTF-8".to_owned()),
            ..SkillFrontmatter::default()
        };
    };
    let mut lines = content.lines();
    if lines.next().map(str::trim) != Some("---") {
        return SkillFrontmatter {
            valid: validate.then_some(false),
            invalid_reason: validate.then(|| "SKILL.md has no YAML frontmatter".to_owned()),
            ..SkillFrontmatter::default()
        };
    }
    let mut metadata = SkillFrontmatter {
        valid: validate.then_some(true),
        ..SkillFrontmatter::default()
    };
    let mut closed = false;
    for line in lines {
        let line = line.trim();
        if line == "---" {
            closed = true;
            break;
        }
        let Some((key, value)) = line.split_once(':') else {
            if validate && !line.is_empty() && !line.starts_with('#') {
                metadata.valid = Some(false);
                metadata.invalid_reason = Some("frontmatter contains a malformed field".to_owned());
            }
            continue;
        };
        let value = value.trim().trim_matches('"').trim_matches('\'').to_owned();
        match key.trim().to_ascii_lowercase().as_str() {
            "name" | "display-name" | "display_name" => metadata.name = Some(value),
            "description" => metadata.description = Some(value),
            "user-invocable" | "user_invocable" => {
                if value.eq_ignore_ascii_case("true") {
                    metadata.user_invocable = true;
                } else if !value.eq_ignore_ascii_case("false") && validate {
                    metadata.valid = Some(false);
                    metadata.invalid_reason =
                        Some("user-invocable must be true or false".to_owned());
                }
            }
            _ => {}
        }
    }
    if validate && !closed {
        metadata.valid = Some(false);
        metadata.invalid_reason = Some("frontmatter has no closing delimiter".to_owned());
    } else if validate
        && (matches!(metadata.name.as_deref(), None | Some(""))
            || matches!(metadata.description.as_deref(), None | Some("")))
    {
        metadata.valid = Some(false);
        metadata.invalid_reason =
            Some("frontmatter requires non-empty name and description".to_owned());
    }
    metadata
}

fn list_skill_tree(root: &Path) -> io::Result<Vec<RuntimeSkillFileEntry>> {
    let mut entries = Vec::new();
    collect_skill_tree(root, root, &mut entries)?;
    entries.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(entries)
}

fn collect_skill_tree(
    root: &Path,
    current: &Path,
    output: &mut Vec<RuntimeSkillFileEntry>,
) -> io::Result<()> {
    if output.len() >= MAX_SKILL_BROWSER_ENTRIES {
        return Ok(());
    }
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        }
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .map_err(|_| io::Error::new(io::ErrorKind::PermissionDenied, "skill path escaped"))?
            .to_string_lossy()
            .replace('\\', "/");
        if relative == MANAGED_SKILL_MARKER {
            continue;
        }
        let is_dir = file_type.is_dir();
        let size = if is_dir {
            0
        } else {
            entry.metadata()?.len() as i64
        };
        output.push(RuntimeSkillFileEntry {
            name: relative,
            is_dir,
            size,
        });
        if is_dir {
            collect_skill_tree(root, &path, output)?;
        }
        if output.len() >= MAX_SKILL_BROWSER_ENTRIES {
            break;
        }
    }
    Ok(())
}

fn validate_skill_name(name: &str) -> Result<(), RuntimeError> {
    if name.is_empty()
        || !name.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        })
    {
        return Err(RuntimeError::Delivery(format!(
            "invalid skill name: {name}"
        )));
    }
    Ok(())
}

fn validate_install_path(
    registry: &Arc<RuntimeRegistry>,
    runtime: &str,
    workspace: &Path,
    install_path: &str,
) -> Result<(), RuntimeError> {
    let path = Path::new(install_path);
    let parent = path.parent().ok_or_else(|| {
        RuntimeError::Delivery("skill install path must have a parent".to_owned())
    })?;
    let allowed = workspace_skill_roots(registry, runtime, workspace)
        .into_iter()
        .filter_map(|root| relative_to_workspace(workspace, &root).ok())
        .any(|root| Path::new(&root) == parent);
    if !allowed {
        return Err(RuntimeError::Delivery(format!(
            "skill install path is outside runtime skill roots: {install_path}"
        )));
    }
    safe_workspace_path(workspace, install_path).map(|_| ())
}

fn relative_to_workspace(workspace: &Path, path: &Path) -> Result<String, RuntimeError> {
    path.strip_prefix(workspace)
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        .map_err(|_| RuntimeError::Delivery("runtime skill path escaped workspace".to_owned()))
}

fn relative_to_workspace_optional(workspace: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(workspace)
        .ok()
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
}

fn safe_workspace_path(workspace: &Path, relative_path: &str) -> Result<PathBuf, RuntimeError> {
    validate_relative_path(relative_path)?;
    let relative = Path::new(relative_path);
    let file_name = relative.file_name().ok_or_else(|| {
        RuntimeError::Delivery("skill path must name a file or directory".to_owned())
    })?;
    let parent = relative.parent().unwrap_or(Path::new(""));
    let parent = cocli_agent::workspace::resolve_within(
        workspace,
        parent
            .to_str()
            .ok_or_else(|| RuntimeError::Delivery("skill path must be valid UTF-8".to_owned()))?,
    )
    .map_err(|error| skill_io_error("resolve skill path", &error))?;
    Ok(parent.join(file_name))
}

fn safe_child_path(root: &Path, relative_path: &str) -> Result<PathBuf, RuntimeError> {
    validate_relative_path(relative_path)?;
    let relative = Path::new(relative_path);
    let file_name = relative
        .file_name()
        .ok_or_else(|| RuntimeError::Delivery("skill file path must name a file".to_owned()))?;
    let parent = relative.parent().unwrap_or(Path::new(""));
    let parent = cocli_agent::workspace::resolve_within(
        root,
        parent.to_str().ok_or_else(|| {
            RuntimeError::Delivery("skill file path must be valid UTF-8".to_owned())
        })?,
    )
    .map_err(|error| skill_io_error("resolve skill file path", &error))?;
    Ok(parent.join(file_name))
}

fn validate_relative_path(path: &str) -> Result<(), RuntimeError> {
    let path = Path::new(path);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(RuntimeError::Delivery(
            "skill path must be a safe relative path".to_owned(),
        ));
    }
    Ok(())
}

fn shorten_path(path: &Path, home: Option<&Path>) -> String {
    if let Some(home) = home {
        if let Ok(relative) = path.strip_prefix(home) {
            return format!("~/{}", relative.to_string_lossy().replace('\\', "/"));
        }
    }
    path.to_string_lossy().into_owned()
}

async fn remove_path(path: &Path) -> Result<(), RuntimeError> {
    let metadata = match tokio::fs::symlink_metadata(path).await {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(skill_io_error("inspect skill path", &error)),
    };
    if metadata.is_dir() {
        tokio::fs::remove_dir_all(path)
            .await
            .map_err(|error| skill_io_error("remove skill directory", &error))
    } else {
        tokio::fs::remove_file(path)
            .await
            .map_err(|error| skill_io_error("remove skill file", &error))
    }
}

#[cfg(unix)]
async fn set_file_mode(path: &Path, mode: i64) -> Result<(), RuntimeError> {
    use std::os::unix::fs::PermissionsExt;

    let mode = if mode == 0 {
        0o644
    } else {
        mode as u32 & 0o777
    };
    tokio::fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .await
        .map_err(|error| skill_io_error("set skill file permissions", &error))
}

#[cfg(not(unix))]
async fn set_file_mode(_path: &Path, _mode: i64) -> Result<(), RuntimeError> {
    Ok(())
}

fn skill_io_error(action: &str, error: &io::Error) -> RuntimeError {
    RuntimeError::Delivery(format!("{action}: {error}"))
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use cocli_store::AgentStatus;
    use tempfile::tempdir;

    use super::*;

    fn test_agent(runtime: &str) -> Agent {
        Agent {
            id: Uuid::new_v4(),
            channel_id: Uuid::new_v4(),
            name: "builder".to_owned(),
            description: None,
            instructions: None,
            runtime: runtime.to_owned(),
            model: None,
            status: AgentStatus::Stopped,
            lifecycle_status: cocli_store::AgentLifecycleStatus::Active,
            created_by_agent_id: None,
            created_by_channel_id: None,
            created_at: Utc::now(),
        }
    }

    fn test_config(root: &Path) -> LocalRuntimeConfig {
        LocalRuntimeConfig::new(root.join("workspaces"), "http://127.0.0.1:8090".to_owned())
    }

    fn test_file(path: &str, content: &str) -> SkillLibraryFile {
        SkillLibraryFile {
            rel_path: path.to_owned(),
            mode: 0o644,
            content: content.as_bytes().to_vec(),
            size: content.len() as i64,
        }
    }

    #[tokio::test]
    async fn install_should_materialize_scan_browse_refresh_and_remove_managed_skill() {
        let temp = tempdir().expect("temp directory");
        let registry = Arc::new(RuntimeRegistry::new());
        let config = test_config(temp.path());
        let agent = test_agent("claude");
        let first = vec![
            test_file(
                "SKILL.md",
                "---\nname: Wiki Compiler\ndescription: Builds a wiki.\nuser-invocable: true\n---\n",
            ),
            test_file("scripts/run.sh", "v1"),
        ];

        let install_path = install(&registry, &config, &agent, "wikic", &first)
            .await
            .expect("skill should install");

        assert_eq!(install_path, ".claude/skills/wikic");
        let scanned = list(&registry, &config, &agent)
            .await
            .expect("skills should scan");
        let installed = scanned
            .iter()
            .find(|skill| skill.name == "wikic")
            .expect("installed skill should scan");
        assert_eq!(installed.display_name, "Wiki Compiler");
        assert!(installed.user_invocable);
        let entries = list_files(&registry, &config, &agent, &install_path)
            .await
            .expect("skill files should list");
        assert!(entries.iter().any(|entry| entry.name == "scripts/run.sh"));
        let content = read_file(&registry, &config, &agent, &install_path, "scripts/run.sh")
            .await
            .expect("skill file should read");
        assert_eq!(content.content, "v1");

        install(
            &registry,
            &config,
            &agent,
            "wikic",
            &[test_file("SKILL.md", "v2")],
        )
        .await
        .expect("skill should refresh");
        assert_eq!(
            read_file(&registry, &config, &agent, &install_path, "SKILL.md")
                .await
                .expect("refreshed file should read")
                .content,
            "v2"
        );
        uninstall(&registry, &config, &agent, &install_path)
            .await
            .expect("skill should uninstall");
        assert!(!list(&registry, &config, &agent)
            .await
            .expect("skills should rescan")
            .iter()
            .any(|skill| skill.install_path.as_deref() == Some(".claude/skills/wikic")));
    }

    #[tokio::test]
    async fn install_should_reject_file_path_traversal_without_writing_outside_target() {
        let temp = tempdir().expect("temp directory");
        let registry = Arc::new(RuntimeRegistry::new());
        let config = test_config(temp.path());
        let agent = test_agent("claude");

        let error = install(
            &registry,
            &config,
            &agent,
            "wikic",
            &[test_file("../escape", "bad")],
        )
        .await
        .expect_err("path traversal should fail");

        assert!(matches!(error, RuntimeError::Delivery(_)));
        assert!(!config
            .workspace_root
            .join(agent.id.to_string())
            .join(".claude/skills/escape")
            .exists());
    }

    #[test]
    fn compatibility_should_cover_supported_uncertain_and_unsupported_runtimes_offline() {
        let registry = Arc::new(RuntimeRegistry::new());

        assert_eq!(
            compatibility(&registry, "claude"),
            RuntimeSkillCompatibility::Supported
        );
        assert_eq!(
            compatibility(&registry, "gemini"),
            RuntimeSkillCompatibility::Uncertain
        );
        assert_eq!(
            compatibility(&registry, "chatrs"),
            RuntimeSkillCompatibility::Unsupported
        );
    }

    #[test]
    fn native_probe_distinguishes_runtime_discovery_from_filesystem_installation() {
        let temp = tempdir().expect("temp directory");
        let workspace = temp.path().join("workspace");
        let root = workspace.join(".codex/skills");
        let discovered = root.join("discovered");
        let installed_only = root.join("installed-only");
        let native_only = temp.path().join("native-only");
        for path in [&discovered, &installed_only, &native_only] {
            fs::create_dir_all(path).expect("skill directory");
            fs::write(
                path.join("SKILL.md"),
                "---\nname: Test\ndescription: Test skill.\n---\n",
            )
            .expect("skill manifest");
        }

        let mut report = scan_skill_paths(
            "codex",
            RuntimeSkillCompatibility::Supported,
            SkillDiscoveryEvidence::FILESYSTEM,
            &workspace,
            &[root],
        )
        .expect("filesystem scan");
        merge_native_probe(
            &mut report,
            NativeSkillProbe {
                evidence: SkillDiscoveryEvidence {
                    source: "codex_app_server",
                    detail: "skills/list(forceReload)",
                    proves_session_visibility: false,
                },
                skills: vec![
                    NativeSkill {
                        name: "discovered".to_owned(),
                        description: "Native discovered".to_owned(),
                        path: Some(discovered.join("SKILL.md")),
                        source: "codex_app_server".to_owned(),
                        scope: "repo".to_owned(),
                        enabled: Some(false),
                        user_invocable: None,
                    },
                    NativeSkill {
                        name: "native-only".to_owned(),
                        description: "Native only".to_owned(),
                        path: Some(native_only.join("SKILL.md")),
                        source: "codex_app_server".to_owned(),
                        scope: "system".to_owned(),
                        enabled: Some(true),
                        user_invocable: None,
                    },
                    NativeSkill {
                        name: "builtin".to_owned(),
                        description: "Runtime builtin".to_owned(),
                        path: None,
                        source: "builtin".to_owned(),
                        scope: "system".to_owned(),
                        enabled: Some(true),
                        user_invocable: Some(false),
                    },
                ],
                issues: Vec::new(),
            },
            &workspace,
        );

        assert_eq!(report.evidence.source, "codex_app_server");
        assert!(!report.evidence.proves_session_visibility);
        let discovered = report
            .skills
            .iter()
            .find(|skill| skill.skill.name == "discovered")
            .expect("native-discovered skill");
        assert_eq!(discovered.presence, "discovered");
        assert_eq!(discovered.enabled, Some(false));
        let installed_only = report
            .skills
            .iter()
            .find(|skill| skill.skill.name == "installed-only")
            .expect("filesystem-only skill");
        assert_eq!(installed_only.presence, "installed");
        assert!(installed_only
            .issues
            .iter()
            .any(|issue| issue.code == "not_runtime_discovered"));
        let native_only = report
            .skills
            .iter()
            .find(|skill| skill.skill.name == "native-only")
            .expect("native-only skill");
        assert_eq!(native_only.scope, "global");
        assert_eq!(native_only.evidence.source, "codex_app_server");
        let builtin = report
            .skills
            .iter()
            .find(|skill| skill.skill.name == "builtin")
            .expect("pathless runtime builtin");
        assert_eq!(builtin.source_path, "runtime:builtin");
        assert_eq!(builtin.resolved_path, None);
    }

    #[tokio::test]
    async fn failed_native_probe_keeps_filesystem_inventory() {
        let temp = tempdir().expect("temp directory");
        let config = test_config(temp.path());
        let agent = test_agent("codex");
        let workspace = config.workspace_root.join(agent.id.to_string());
        let skill = workspace.join(".codex/skills/fallback");
        fs::create_dir_all(&skill).expect("skill directory");
        fs::write(
            skill.join("SKILL.md"),
            "---\nname: Fallback\ndescription: Filesystem fallback.\n---\n",
        )
        .expect("skill manifest");
        let mut registry = RuntimeRegistry::new();
        registry.register(Arc::new(cocli_driver_codex::CodexDriver::new(
            temp.path().join("missing-codex"),
            temp.path().join("missing-bridge"),
        )));

        let report = inspect(&Arc::new(registry), &config, &agent)
            .await
            .expect("filesystem fallback should succeed");

        assert!(report
            .skills
            .iter()
            .any(|finding| finding.skill.name == "fallback"));
        assert_eq!(report.evidence.source, "filesystem");
        assert!(report
            .issues
            .iter()
            .any(|issue| issue.code == "native_probe_failed"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn cursor_unsupported_native_contract_keeps_filesystem_inventory_and_manual_diagnostic() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempdir().expect("temp directory");
        let config = test_config(temp.path());
        let agent = test_agent("cursor");
        let workspace = config.workspace_root.join(agent.id.to_string());
        let skill = workspace.join(".cursor/skills/fallback");
        fs::create_dir_all(&skill).expect("skill directory");
        fs::write(
            skill.join("SKILL.md"),
            "---\nname: Cursor fallback\ndescription: Filesystem evidence.\n---\n",
        )
        .expect("skill manifest");
        let cursor = temp.path().join("cursor-agent");
        fs::write(
            &cursor,
            "#!/bin/sh\nprintf '%s\\n' 'Usage: cursor-agent [options]' 'Start the Cursor Agent'\n",
        )
        .expect("fake cursor");
        fs::set_permissions(&cursor, fs::Permissions::from_mode(0o755))
            .expect("cursor permissions");
        let mut registry = RuntimeRegistry::new();
        registry.register(Arc::new(cocli_driver_cursor::CursorDriver::new(
            cursor,
            temp.path().join("missing-bridge"),
        )));

        let report = inspect(&Arc::new(registry), &config, &agent)
            .await
            .expect("filesystem fallback should succeed");

        assert!(report
            .skills
            .iter()
            .any(|finding| finding.skill.name == "fallback"));
        assert_eq!(report.evidence.source, "filesystem");
        assert!(report
            .issues
            .iter()
            .any(|issue| issue.code == "native_probe_unsupported"));
    }

    #[tokio::test]
    async fn missing_cursor_cli_keeps_filesystem_inventory() {
        let temp = tempdir().expect("temp directory");
        let config = test_config(temp.path());
        let agent = test_agent("cursor");
        let workspace = config.workspace_root.join(agent.id.to_string());
        let skill = workspace.join(".cursor/skills/fallback");
        fs::create_dir_all(&skill).expect("skill directory");
        fs::write(
            skill.join("SKILL.md"),
            "---\nname: Cursor fallback\ndescription: Filesystem evidence.\n---\n",
        )
        .expect("skill manifest");
        let mut registry = RuntimeRegistry::new();
        registry.register(Arc::new(cocli_driver_cursor::CursorDriver::new(
            temp.path().join("missing-cursor"),
            temp.path().join("missing-bridge"),
        )));

        let report = inspect(&Arc::new(registry), &config, &agent)
            .await
            .expect("filesystem fallback should succeed");

        assert!(report
            .skills
            .iter()
            .any(|finding| finding.skill.name == "fallback"));
        assert!(report
            .issues
            .iter()
            .any(|issue| issue.code == "native_probe_failed"));
    }

    #[tokio::test]
    async fn machine_inventory_reports_user_roots_without_creating_an_agent_workspace() {
        let temp = tempdir().expect("temp directory");
        let config = test_config(temp.path());
        let registry = Arc::new(RuntimeRegistry::new());

        let report = inspect_machine(&registry, &config, "cursor")
            .await
            .expect("machine inspection");

        assert!(report.search_paths.iter().all(|path| path.scope == "user"));
        assert!(report
            .search_paths
            .iter()
            .any(|path| { Path::new(&path.path).ends_with(Path::new(".cursor").join("skills")) }));
        assert!(!config.workspace_root.exists());
    }

    #[tokio::test]
    async fn agent_inventory_does_not_create_a_missing_runtime_workspace() {
        let temp = tempdir().expect("temp directory");
        let config = test_config(temp.path());
        let agent = test_agent("cursor");
        let workspace = config.workspace_root.join(agent.id.to_string());

        let report = inspect(&Arc::new(RuntimeRegistry::new()), &config, &agent)
            .await
            .expect("read-only agent inspection");

        assert!(report
            .skills
            .iter()
            .all(|finding| finding.scope != "workspace"));
        assert!(!workspace.exists());
    }

    #[cfg(unix)]
    #[test]
    fn scan_follows_skill_symlinks_and_reports_shadowing_invalid_and_broken_entries() {
        use std::os::unix::fs::symlink;

        let temp = tempdir().expect("temp directory");
        let workspace = temp.path().join("workspace");
        let primary = workspace.join(".cursor/skills");
        let shared = workspace.join(".agents/skills");
        let target = temp.path().join("targets/reviewer");
        fs::create_dir_all(&primary).expect("primary search path");
        fs::create_dir_all(&shared).expect("shared search path");
        fs::create_dir_all(&target).expect("symlink target");
        fs::write(
            target.join("SKILL.md"),
            "---\nname: Reviewer\ndescription: Reviews changes.\n---\n",
        )
        .expect("valid skill");
        symlink(&target, primary.join("reviewer")).expect("skill directory symlink");

        let shadowed = shared.join("reviewer");
        fs::create_dir_all(&shadowed).expect("shadowed skill");
        fs::write(
            shadowed.join("SKILL.md"),
            "---\nname: Other reviewer\ndescription: Lower priority.\n---\n",
        )
        .expect("shadowed manifest");
        let invalid = shared.join("invalid");
        fs::create_dir_all(&invalid).expect("invalid skill");
        fs::write(invalid.join("SKILL.md"), "---\nname: Invalid\n---\n").expect("invalid manifest");
        symlink(temp.path().join("missing"), shared.join("broken")).expect("broken skill symlink");

        let report = scan_skill_paths(
            "cursor",
            RuntimeSkillCompatibility::Uncertain,
            SkillDiscoveryEvidence::FILESYSTEM,
            &workspace,
            &[primary, shared],
        )
        .expect("skill scan");

        assert_eq!(report.skills.len(), 3);
        let linked = report
            .skills
            .iter()
            .find(|finding| finding.skill.name == "reviewer" && !finding.shadowed)
            .expect("linked skill");
        assert_eq!(linked.valid, Some(true));
        assert!(linked
            .resolved_path
            .as_deref()
            .is_some_and(|path| { path.ends_with("targets/reviewer") }));
        assert!(!linked.evidence.proves_session_visibility);
        assert!(report
            .skills
            .iter()
            .any(|finding| finding.skill.name == "reviewer" && finding.shadowed));
        assert!(report.skills.iter().any(|finding| {
            finding.skill.name == "invalid"
                && finding.valid == Some(false)
                && finding
                    .issues
                    .iter()
                    .any(|issue| issue.code == "invalid_frontmatter")
        }));
        assert!(report
            .issues
            .iter()
            .any(|issue| issue.code == "broken_symlink"));
    }

    #[cfg(unix)]
    #[test]
    fn aliases_to_one_skill_target_share_a_fingerprint() {
        use std::os::unix::fs::symlink;

        let temp = tempdir().expect("temp directory");
        let workspace = temp.path().join("workspace");
        let primary = workspace.join(".codex/skills");
        let shared = workspace.join(".agents/skills");
        let target = temp.path().join("targets/reviewer");
        fs::create_dir_all(&primary).expect("primary path");
        fs::create_dir_all(&shared).expect("shared path");
        fs::create_dir_all(&target).expect("target");
        fs::write(
            target.join("SKILL.md"),
            "---\nname: Reviewer\ndescription: Reviews.\n---\n",
        )
        .expect("manifest");
        symlink(&target, primary.join("reviewer")).expect("first alias");
        symlink(&target, shared.join("reviewer-alias")).expect("second alias");

        let report = scan_skill_paths(
            "codex",
            RuntimeSkillCompatibility::Supported,
            SkillDiscoveryEvidence::FILESYSTEM,
            &workspace,
            &[primary, shared],
        )
        .expect("scan");

        assert_eq!(report.skills.len(), 2);
        assert_eq!(report.skills[0].fingerprint, report.skills[1].fingerprint);
        assert!(report.skills[1].duplicate);
        assert!(report.skills[1]
            .issues
            .iter()
            .any(|issue| issue.code == "duplicate_target"));
    }

    #[test]
    fn visibility_issues_for_one_skill_are_grouped_without_losing_paths() {
        let grouped = group_issues(vec![
            skill_issue(
                "shadowed_skill",
                "warning",
                "shadowed".to_owned(),
                Some("~/.agents/skills/quickbox-qb".to_owned()),
                Some("quickbox-qb".to_owned()),
            ),
            skill_issue(
                "not_runtime_discovered",
                "warning",
                "not discovered".to_owned(),
                Some("~/.codex/skills/quickbox-qb".to_owned()),
                Some("quickbox-qb".to_owned()),
            ),
        ]);

        assert_eq!(grouped.len(), 1);
        assert_eq!(grouped[0].related_paths.len(), 2);
        assert_eq!(grouped[0].related_codes.len(), 2);
    }

    #[test]
    fn governance_scope_capabilities_classify_runtime_shared_and_reserved_roots() {
        let temp = tempdir().expect("temp directory");
        let workspace = temp.path().join("workspace");
        let paths = vec![
            workspace.join(".codex/skills"),
            workspace.join(".agents/skills"),
            workspace.join(".codex/skills/.system"),
        ];

        let capabilities =
            governance_scope_capabilities_for_paths("codex", "workspace", &workspace, &paths);

        assert_eq!(capabilities.len(), 3);
        assert_eq!(capabilities[0].root_kind, "runtime_specific");
        assert!(capabilities[0].supported);
        assert_eq!(capabilities[1].root_kind, "shared");
        assert!(capabilities[1].supported);
        assert_eq!(capabilities[2].status, "reserved");
        assert!(!capabilities[2].supported);
        assert_eq!(
            capabilities[2].blocked_reason.as_deref(),
            Some("runtime_managed_system_root")
        );
        assert!(!workspace.exists(), "capability inspection is read-only");
    }

    #[test]
    fn governance_scope_capabilities_reject_unsupported_scope() {
        let temp = tempdir().expect("temp directory");
        let registry = Arc::new(RuntimeRegistry::new());
        let config = test_config(temp.path());
        let error = governance_scope_capabilities(
            &registry,
            &config,
            "codex",
            "channel",
            Some(temp.path()),
        )
        .expect_err("unsupported scope should fail");

        assert!(
            matches!(error, RuntimeError::Unsupported(message) if message.contains("unsupported governed Skill scope"))
        );
    }

    #[cfg(unix)]
    #[test]
    fn governance_scope_capabilities_report_read_only_roots_without_creating_targets() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempdir().expect("temp directory");
        let workspace = temp.path().join("workspace");
        let root = workspace.join(".fake/skills");
        fs::create_dir_all(&root).expect("skill root");
        fs::set_permissions(&root, fs::Permissions::from_mode(0o555))
            .expect("read-only root permissions");

        let capabilities =
            governance_scope_capabilities_for_paths("fake", "workspace", &workspace, &[root]);

        assert_eq!(capabilities.len(), 1);
        assert_eq!(capabilities[0].status, "read_only");
        assert!(!capabilities[0].supported);
        assert_eq!(
            capabilities[0].blocked_reason.as_deref(),
            Some("root_not_writable")
        );
    }

    #[cfg(unix)]
    #[test]
    fn governance_scope_capabilities_deduplicate_aliases_and_block_root_takeover() {
        use std::os::unix::fs::symlink;

        let temp = tempdir().expect("temp directory");
        let workspace = temp.path().join("workspace");
        let outside = temp.path().join("outside/skills");
        fs::create_dir_all(&outside).expect("outside root");
        fs::create_dir_all(&workspace).expect("workspace root");
        symlink(&outside, workspace.join("linked-skills")).expect("root alias");
        let paths = vec![
            workspace.join("linked-skills"),
            workspace.join("linked-skills/../linked-skills"),
        ];

        let capabilities =
            governance_scope_capabilities_for_paths("fake", "workspace", &workspace, &paths);

        assert_eq!(capabilities.len(), 1, "canonical aliases are deduplicated");
        assert_eq!(capabilities[0].status, "blocked");
        assert!(!capabilities[0].supported);
        assert_eq!(
            capabilities[0].blocked_reason.as_deref(),
            Some("whole_root_symlink_takeover")
        );
    }

    #[cfg(unix)]
    #[test]
    fn governance_scope_capabilities_block_component_symlink_escape() {
        use std::os::unix::fs::symlink;

        let temp = tempdir().expect("temp directory");
        let workspace = temp.path().join("workspace");
        let outside = temp.path().join("outside");
        fs::create_dir_all(&workspace).expect("workspace root");
        fs::create_dir_all(&outside).expect("outside root");
        symlink(&outside, workspace.join("linked")).expect("component symlink");

        let capabilities = governance_scope_capabilities_for_paths(
            "fake",
            "workspace",
            &workspace,
            &[workspace.join("linked/skills")],
        );

        assert_eq!(capabilities.len(), 1);
        assert_eq!(capabilities[0].status, "blocked");
        assert!(!capabilities[0].supported);
        assert_eq!(
            capabilities[0].blocked_reason.as_deref(),
            Some("symlink_escape")
        );
    }

    #[test]
    fn governance_scope_capabilities_deduplicate_unicode_and_case_aliases() {
        let temp = tempdir().expect("temp directory");
        let workspace = temp.path().join("workspace");
        let composed = workspace.join("Caf\u{e9}/Skills");
        let decomposed = workspace.join("Cafe\u{301}/skills");

        let capabilities = governance_scope_capabilities_for_paths(
            "fake",
            "workspace",
            &workspace,
            &[composed, decomposed],
        );

        assert_eq!(
            capabilities.len(),
            1,
            "Unicode/case aliases are deduplicated"
        );
    }
}
