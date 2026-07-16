use std::collections::HashSet;
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use cocli_api::{
    RuntimeError, RuntimeSkill, RuntimeSkillCompatibility, RuntimeSkillFileContent,
    RuntimeSkillFileEntry,
};
use cocli_driver_core::types::SkillCompatibility;
use cocli_runtime_pool::RuntimeRegistry;
use cocli_store::{Agent, SkillLibraryFile};
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
    let workspace = config.workspace_root.join(agent.id.to_string());
    tokio::fs::create_dir_all(&workspace)
        .await
        .map_err(|error| skill_io_error("create agent workspace", &error))?;
    let paths = skill_paths(registry, &agent.runtime, &workspace);
    tokio::task::spawn_blocking(move || scan_skill_paths(&workspace, &paths))
        .await
        .map_err(|error| RuntimeError::Delivery(format!("skill scan task failed: {error}")))?
        .map_err(|error| skill_io_error("scan runtime skills", &error))
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
        "cursor" => vec![workspace.join(".cursor/rules")],
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

fn scan_skill_paths(workspace: &Path, paths: &[PathBuf]) -> io::Result<Vec<RuntimeSkill>> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut seen = HashSet::new();
    let mut skills = Vec::new();
    for root in paths {
        let skill_type = if root.starts_with(workspace) {
            "workspace"
        } else {
            "global"
        };
        let Ok(entries) = fs::read_dir(root) else {
            continue;
        };
        for entry in entries {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if file_type.is_symlink() {
                continue;
            }
            let path = entry.path();
            let (name, metadata_path, install_path) = if file_type.is_dir() {
                let skill_md = path.join("SKILL.md");
                if !skill_md.is_file() {
                    continue;
                }
                (
                    entry.file_name().to_string_lossy().into_owned(),
                    skill_md,
                    relative_to_workspace_optional(workspace, &path),
                )
            } else {
                if path.extension() != Some(OsStr::new("md")) {
                    continue;
                }
                (
                    path.file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                    path.clone(),
                    relative_to_workspace_optional(workspace, &path),
                )
            };
            if !seen.insert(name.clone()) {
                continue;
            }
            let metadata = parse_skill_frontmatter(&metadata_path);
            skills.push(RuntimeSkill {
                display_name: metadata.name.unwrap_or_else(|| name.clone()),
                name,
                description: metadata.description.unwrap_or_default(),
                user_invocable: metadata.user_invocable,
                skill_type: skill_type.to_owned(),
                path: shorten_path(&metadata_path, home.as_deref()),
                install_path,
            });
        }
    }
    Ok(skills)
}

#[derive(Default)]
struct SkillFrontmatter {
    name: Option<String>,
    description: Option<String>,
    user_invocable: bool,
}

fn parse_skill_frontmatter(path: &Path) -> SkillFrontmatter {
    let Ok(content) = fs::read_to_string(path) else {
        return SkillFrontmatter::default();
    };
    let mut lines = content.lines();
    if lines.next().map(str::trim) != Some("---") {
        return SkillFrontmatter::default();
    }
    let mut metadata = SkillFrontmatter::default();
    for line in lines {
        let line = line.trim();
        if line == "---" {
            break;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim().trim_matches('"').trim_matches('\'').to_owned();
        match key.trim().to_ascii_lowercase().as_str() {
            "name" | "display-name" | "display_name" => metadata.name = Some(value),
            "description" => metadata.description = Some(value),
            "user-invocable" | "user_invocable" => {
                metadata.user_invocable = value.eq_ignore_ascii_case("true");
            }
            _ => {}
        }
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
            runtime: runtime.to_owned(),
            model: None,
            status: AgentStatus::Stopped,
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
        assert_eq!(scanned.len(), 1);
        assert_eq!(scanned[0].display_name, "Wiki Compiler");
        assert!(scanned[0].user_invocable);
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
        assert!(list(&registry, &config, &agent)
            .await
            .expect("skills should rescan")
            .is_empty());
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
}
