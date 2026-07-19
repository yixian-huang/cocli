use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::Utc;
use cocli_api::RuntimeInfo;
use cocli_driver_core::{
    hash_mcp_observation, McpApplyActionResult, McpApplyActionStatus, McpApplyExecutionRequest,
    McpApplyExecutionResult, McpApprovalMode, McpBackupDescriptor, McpBinding,
    McpCanonicalDefinition, McpDiagnostic, McpDiagnosticSeverity, McpEvidence, McpInventory,
    McpPlanAction, McpPlanActionKind, McpReloadResult, McpReloadStatus, McpRiskLevel,
    McpRollbackExecutionRequest, McpRollbackExecutionResult, McpSecretRef, McpServer,
    McpStartupState, McpTransport, McpVerificationResult, McpVerificationStatus,
    ObservedMcpInstance,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use uuid::Uuid;

use crate::runtime::LocalRuntimeConfig;

const PROBE_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_CONFIG_BYTES: u64 = 1024 * 1024;
const STALE_APPLY_LOCK: chrono::Duration = chrono::Duration::minutes(15);

#[derive(Clone, Debug)]
struct CommandOutput {
    success: bool,
    stdout: String,
    stderr: String,
}

#[derive(Clone, Debug)]
enum CommandOutcome {
    Missing,
    Timeout,
    StartFailed(std::io::ErrorKind),
    Output(CommandOutput),
}

#[async_trait::async_trait]
trait CommandRunner: Send + Sync {
    async fn run(
        &self,
        binary: &Path,
        args: &[&str],
        workspace: &Path,
        timeout: Duration,
    ) -> CommandOutcome;
}

struct SystemCommandRunner;

#[async_trait::async_trait]
impl CommandRunner for SystemCommandRunner {
    async fn run(
        &self,
        binary: &Path,
        args: &[&str],
        workspace: &Path,
        timeout: Duration,
    ) -> CommandOutcome {
        let output = tokio::time::timeout(
            timeout,
            Command::new(binary)
                .args(args)
                .current_dir(workspace)
                .output(),
        )
        .await;
        match output {
            Ok(Ok(output)) => CommandOutcome::Output(CommandOutput {
                success: output.status.success(),
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            }),
            Ok(Err(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                CommandOutcome::Missing
            }
            Ok(Err(error)) => CommandOutcome::StartFailed(error.kind()),
            Err(_) => CommandOutcome::Timeout,
        }
    }
}

pub async fn inspect(catalog: &[RuntimeInfo], config: &LocalRuntimeConfig) -> McpInventory {
    let observed_at = timestamp();
    let home_dir = std::env::var_os("HOME").map(PathBuf::from);
    let mut aggregate = Aggregate::new(observed_at.clone());

    let runtimes = target_runtimes(catalog);
    for runtime in &runtimes {
        let paths = config_paths(runtime, home_dir.as_deref(), &config.workspace_root);
        for path in paths {
            let workspace_scope = path
                .starts_with(&config.workspace_root)
                .then_some(config.workspace_root.as_path());
            match discover_config(runtime, &path, workspace_scope, &observed_at).await {
                ConfigRead::Missing => {}
                ConfigRead::Snapshot(snapshot) => aggregate.extend(snapshot),
                ConfigRead::Diagnostic(diagnostic) => aggregate.diagnostics.push(diagnostic),
            }
        }
    }

    let runner = SystemCommandRunner;
    let probe_snapshots = run_probes(
        catalog,
        &runtimes,
        &config.workspace_root,
        &observed_at,
        &runner,
    )
    .await;
    for snapshot in probe_snapshots {
        aggregate.extend(snapshot);
    }

    aggregate.finalize()
}

pub async fn apply(
    catalog: &[RuntimeInfo],
    config: &LocalRuntimeConfig,
    request: McpApplyExecutionRequest,
) -> McpApplyExecutionResult {
    let before = inspect(catalog, config).await;
    if hash_mcp_observation(&before) != request.plan.observation_hash {
        return blocked_execution(
            &request,
            "observation changed before the configuration lock was acquired",
        );
    }

    let backup_root = config
        .workspace_root
        .parent()
        .unwrap_or(&config.workspace_root)
        .join("mcp-backups")
        .join(&request.run_id);
    let mut actions = Vec::with_capacity(request.plan.actions.len());
    let mut changed_runtimes = HashSet::new();
    for (action_index, action) in request.plan.actions.iter().enumerate() {
        let result = apply_action(config, &request, action_index, action, &backup_root).await;
        if result.status == McpApplyActionStatus::Applied {
            changed_runtimes.insert(result.runtime.clone());
        }
        actions.push(result);
    }

    let mut reloads = changed_runtimes
        .into_iter()
        .map(|runtime| McpReloadResult {
            runtime,
            status: McpReloadStatus::Deferred,
            reason: "configuration is visible to new Runtime sessions; active sessions were not restarted"
                .to_owned(),
        })
        .collect::<Vec<_>>();
    reloads.sort_by(|left, right| left.runtime.cmp(&right.runtime));
    let after = inspect(catalog, config).await;
    let verification = verify_plan(&request, &actions, &after);
    if verification.status == McpVerificationStatus::Matched {
        for action in &mut actions {
            if action.status == McpApplyActionStatus::Applied {
                action.status = McpApplyActionStatus::Verified;
            }
        }
    }
    McpApplyExecutionResult {
        actions,
        reloads,
        verification,
    }
}

pub async fn rollback(
    catalog: &[RuntimeInfo],
    config: &LocalRuntimeConfig,
    request: McpRollbackExecutionRequest,
) -> McpRollbackExecutionResult {
    let mut actions = Vec::with_capacity(request.backups.len());
    for (action_index, backup) in request.backups.iter().enumerate() {
        let status = rollback_backup(backup).await;
        actions.push(McpApplyActionResult {
            action_index,
            runtime: backup.runtime.clone(),
            server_id: String::new(),
            status: status.0,
            reason: status.1,
            backup: Some(backup.clone()),
            before_source_hash: None,
            after_source_hash: None,
        });
    }
    let inventory = inspect(catalog, config).await;
    let failed = actions
        .iter()
        .any(|action| action.status != McpApplyActionStatus::RolledBack);
    McpRollbackExecutionResult {
        actions,
        verification: McpVerificationResult {
            status: if failed {
                McpVerificationStatus::Failed
            } else {
                McpVerificationStatus::Matched
            },
            observation_hash: hash_mcp_observation(&inventory),
            mismatches: if failed {
                vec!["one or more backups could not be restored".to_owned()]
            } else {
                Vec::new()
            },
        },
    }
}

fn blocked_execution(request: &McpApplyExecutionRequest, reason: &str) -> McpApplyExecutionResult {
    McpApplyExecutionResult {
        actions: request
            .plan
            .actions
            .iter()
            .enumerate()
            .map(|(action_index, action)| McpApplyActionResult {
                action_index,
                runtime: action.runtime.clone(),
                server_id: action.server_id.clone(),
                status: McpApplyActionStatus::Blocked,
                reason: reason.to_owned(),
                backup: None,
                before_source_hash: None,
                after_source_hash: None,
            })
            .collect(),
        reloads: Vec::new(),
        verification: McpVerificationResult {
            status: McpVerificationStatus::Blocked,
            observation_hash: String::new(),
            mismatches: vec![reason.to_owned()],
        },
    }
}

async fn apply_action(
    config: &LocalRuntimeConfig,
    request: &McpApplyExecutionRequest,
    action_index: usize,
    action: &McpPlanAction,
    backup_root: &Path,
) -> McpApplyActionResult {
    let mut result = McpApplyActionResult {
        action_index,
        runtime: action.runtime.clone(),
        server_id: action.server_id.clone(),
        status: McpApplyActionStatus::Blocked,
        reason: String::new(),
        backup: None,
        before_source_hash: None,
        after_source_hash: None,
    };
    if action.blocked
        || matches!(
            action.kind,
            McpPlanActionKind::ApprovalRequired
                | McpPlanActionKind::AuthenticationRequired
                | McpPlanActionKind::ManualUnsupported
        )
    {
        result.status = McpApplyActionStatus::Skipped;
        "plan action is manual, blocked, authentication-required, or unsupported"
            .clone_into(&mut result.reason);
        return result;
    }
    if action.risk >= McpRiskLevel::High && !request.confirm_high_risk {
        "high-risk action requires explicit second confirmation".clone_into(&mut result.reason);
        return result;
    }
    if !matches!(action.runtime.as_str(), "cursor" | "claude") {
        "Runtime writer is not supported without a reliable native adapter"
            .clone_into(&mut result.reason);
        return result;
    }
    let desired = request
        .plan
        .effective_desired_state
        .servers
        .iter()
        .find(|server| {
            server.desired.runtime == action.runtime && server.desired.server_id == action.server_id
        });
    if desired.is_some_and(|server| !server.desired.secret_refs.is_empty()) {
        "secure secret resolver is unavailable; secret reference action is blocked"
            .clone_into(&mut result.reason);
        return result;
    }
    if desired.is_some_and(|server| {
        !server.desired.allow_tools.is_empty()
            || !server.desired.deny_tools.is_empty()
            || server.desired.approval_mode != McpApprovalMode::Manual
    }) {
        "Runtime tool policy writer is unavailable".clone_into(&mut result.reason);
        return result;
    }

    let path = action_config_path(config, action);
    if path.extension().and_then(|value| value.to_str()) != Some("json") {
        "only isolated JSON Runtime adapters are supported in this phase"
            .clone_into(&mut result.reason);
        return result;
    }
    let Some(parent) = path.parent() else {
        "configuration path has no parent".clone_into(&mut result.reason);
        return result;
    };
    if tokio::fs::create_dir_all(parent).await.is_err() {
        "configuration parent could not be prepared".clone_into(&mut result.reason);
        return result;
    }
    let lock_path = path.with_extension("json.cocli-mcp.lock");
    if acquire_apply_lock(&lock_path).await.is_err() {
        "configuration is locked by another apply operation".clone_into(&mut result.reason);
        return result;
    }

    let outcome = mutate_json_config(&path, backup_root, action, desired).await;
    let _ = tokio::fs::remove_file(&lock_path).await;
    match outcome {
        Ok((backup, before_hash, after_hash)) => {
            result.status = McpApplyActionStatus::Applied;
            "configuration subtree updated atomically; reload is handled separately"
                .clone_into(&mut result.reason);
            result.backup = Some(backup);
            result.before_source_hash = Some(before_hash);
            result.after_source_hash = Some(after_hash);
        }
        Err(reason) => result.reason = reason,
    }
    result
}

fn action_config_path(config: &LocalRuntimeConfig, action: &McpPlanAction) -> PathBuf {
    if let Some(path) = action
        .evidence
        .iter()
        .find(|item| item.source == "config")
        .and_then(|item| item.source_path.as_deref())
    {
        return PathBuf::from(path);
    }
    match action.runtime.as_str() {
        "claude" => config.workspace_root.join(".claude").join("mcp.json"),
        _ => config.workspace_root.join(".cursor").join("mcp.json"),
    }
}

async fn mutate_json_config(
    path: &Path,
    backup_root: &Path,
    action: &McpPlanAction,
    desired: Option<&cocli_driver_core::McpEffectiveServer>,
) -> Result<(McpBackupDescriptor, String, String), String> {
    let source = match tokio::fs::read(path).await {
        Ok(source) => source,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(_) => return Err("configuration could not be read".to_owned()),
    };
    if source.len() as u64 > MAX_CONFIG_BYTES {
        return Err("configuration exceeds the safe write limit".to_owned());
    }
    let source_existed = !source.is_empty() || tokio::fs::metadata(path).await.is_ok();
    let before_hash = sha256_bytes(&source);
    let mut root: Value = if source.is_empty() {
        serde_json::json!({})
    } else {
        serde_json::from_slice(&source)
            .map_err(|_| "configuration contains invalid JSON; no write was attempted".to_owned())?
    };
    if json_contains_secret_material(&root) {
        return Err(
            "source configuration contains inline credential material; backup and apply are blocked"
                .to_owned(),
        );
    }
    let root_object = root
        .as_object_mut()
        .ok_or_else(|| "configuration root must be a JSON object".to_owned())?;
    let key = ["mcpServers", "mcp_servers", "mcp"]
        .into_iter()
        .find(|key| root_object.get(*key).is_some())
        .unwrap_or("mcpServers")
        .to_owned();
    if !root_object.contains_key(&key) {
        root_object.insert(key.clone(), serde_json::json!({}));
    }
    let servers = root_object
        .get_mut(&key)
        .and_then(Value::as_object_mut)
        .ok_or_else(|| "MCP configuration entry must be a JSON object".to_owned())?;
    if action.kind == McpPlanActionKind::AddConfigure
        && action.before.configured == Some(false)
        && desired.is_some_and(|server| servers.contains_key(&server.desired.alias))
    {
        return Err(
            "configuration alias appeared after planning; CAS rejected add/configure".to_owned(),
        );
    }
    let current_alias = find_server_alias(servers, action).or_else(|| {
        desired
            .map(|server| server.desired.alias.as_str())
            .filter(|alias| servers.contains_key(*alias))
            .map(ToOwned::to_owned)
    });
    if let (Some(alias), Some(expected)) = (
        current_alias.as_deref(),
        action.before.endpoint_fingerprint.as_deref(),
    ) {
        let current = servers
            .get(alias)
            .and_then(|value| server_from_value(alias, value))
            .map(|server| endpoint_fingerprint(&server.definition));
        if current.as_deref() != Some(expected) {
            return Err(
                "configuration source changed after planning; CAS rejected the write".to_owned(),
            );
        }
    }
    match action.kind {
        McpPlanActionKind::Remove => {
            let alias = current_alias.ok_or_else(|| {
                "configured server no longer exists; CAS rejected removal".to_owned()
            })?;
            servers.remove(&alias);
        }
        McpPlanActionKind::AddConfigure | McpPlanActionKind::Update => {
            let desired = desired.ok_or_else(|| "desired server is unavailable".to_owned())?;
            let definition = desired
                .desired
                .definition
                .as_ref()
                .ok_or_else(|| "desired definition is unavailable".to_owned())?;
            let alias = current_alias.unwrap_or_else(|| desired.desired.alias.clone());
            servers.insert(
                alias,
                definition_json(definition, desired.desired.desired_enabled),
            );
        }
        McpPlanActionKind::Enable | McpPlanActionKind::Disable => {
            let alias =
                current_alias.ok_or_else(|| "configured server no longer exists".to_owned())?;
            let object = servers
                .get_mut(&alias)
                .and_then(Value::as_object_mut)
                .ok_or_else(|| "configured server entry is invalid".to_owned())?;
            object.remove("enabled");
            object.insert(
                "disabled".to_owned(),
                Value::Bool(action.kind == McpPlanActionKind::Disable),
            );
        }
        McpPlanActionKind::ApprovalRequired
        | McpPlanActionKind::AuthenticationRequired
        | McpPlanActionKind::ManualUnsupported => {
            return Err("non-executable action reached the writer".to_owned());
        }
    }
    let next = serde_json::to_vec_pretty(&root)
        .map_err(|_| "configuration could not be serialized".to_owned())?;
    let after_hash = sha256_bytes(&next);
    if before_hash == after_hash {
        return Err("configuration already matches the requested action".to_owned());
    }
    let mut backup = write_backup(backup_root, path, &source, source_existed, &before_hash).await?;
    let current = match tokio::fs::read(path).await {
        Ok(current) => current,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(_) => return Err("configuration could not be re-read before replace".to_owned()),
    };
    if sha256_bytes(&current) != before_hash {
        return Err("configuration changed after backup; CAS rejected atomic replace".to_owned());
    }
    atomic_write(path, &next).await?;
    backup.applied_hash.clone_from(&after_hash);
    Ok((backup, before_hash, after_hash))
}

fn json_contains_secret_material(value: &Value) -> bool {
    match value {
        Value::Object(object) => object.iter().any(|(key, value)| {
            (key.eq_ignore_ascii_case("env")
                && value.as_object().is_some_and(|env| !env.is_empty()))
                || (secret_like(key) && value.as_str().is_some_and(|secret| !secret.is_empty()))
                || json_contains_secret_material(value)
        }),
        Value::Array(values) => values.iter().any(json_contains_secret_material),
        Value::String(value) => secret_like(value) || redact_url(value) != *value,
        Value::Null | Value::Bool(_) | Value::Number(_) => false,
    }
}

fn find_server_alias(
    servers: &serde_json::Map<String, Value>,
    action: &McpPlanAction,
) -> Option<String> {
    servers.iter().find_map(|(alias, value)| {
        let definition = server_from_value(alias, value)?;
        let fingerprint = endpoint_fingerprint(&definition.definition);
        let id = server_id(&fingerprint);
        (id == action.server_id
            || action.before.endpoint_fingerprint.as_deref() == Some(fingerprint.as_str()))
        .then(|| alias.clone())
    })
}

fn definition_json(definition: &McpCanonicalDefinition, enabled: bool) -> Value {
    let mut value = serde_json::Map::new();
    match definition.transport {
        McpTransport::Stdio => {
            value.insert("type".to_owned(), Value::String("stdio".to_owned()));
        }
        McpTransport::Sse => {
            value.insert("type".to_owned(), Value::String("sse".to_owned()));
        }
        McpTransport::StreamableHttp => {
            value.insert(
                "type".to_owned(),
                Value::String("streamable-http".to_owned()),
            );
        }
        McpTransport::Http => {
            value.insert("type".to_owned(), Value::String("http".to_owned()));
        }
        McpTransport::Unknown => {}
    }
    if let Some(command) = &definition.command {
        value.insert("command".to_owned(), Value::String(command.clone()));
    }
    if !definition.args.is_empty() {
        value.insert(
            "args".to_owned(),
            Value::Array(
                definition
                    .args
                    .iter()
                    .map(|value| Value::String(value.clone()))
                    .collect(),
            ),
        );
    }
    if let Some(endpoint) = &definition.endpoint {
        value.insert("url".to_owned(), Value::String(endpoint.clone()));
    }
    value.insert("disabled".to_owned(), Value::Bool(!enabled));
    Value::Object(value)
}

async fn write_backup(
    backup_root: &Path,
    source_path: &Path,
    source: &[u8],
    source_existed: bool,
    source_hash: &str,
) -> Result<McpBackupDescriptor, String> {
    tokio::fs::create_dir_all(backup_root)
        .await
        .map_err(|_| "backup directory could not be prepared".to_owned())?;
    let id = Uuid::new_v4().to_string();
    let backup_path = backup_root.join(format!("{id}.backup"));
    atomic_write(&backup_path, source).await?;
    Ok(McpBackupDescriptor {
        id,
        runtime: runtime_for_path(source_path).to_owned(),
        source_path: source_path.display().to_string(),
        backup_path: backup_path.display().to_string(),
        source_hash: source_hash.to_owned(),
        backup_hash: sha256_bytes(source),
        applied_hash: String::new(),
        source_existed,
    })
}

fn runtime_for_path(path: &Path) -> &'static str {
    let text = path.to_string_lossy();
    if text.contains("claude") || text.ends_with(".mcp-config.json") {
        "claude"
    } else {
        "cursor"
    }
}

async fn atomic_write(path: &Path, content: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "atomic write target has no parent".to_owned())?;
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(|_| "atomic write parent could not be prepared".to_owned())?;
    let temporary = parent.join(format!(".cocli-mcp-{}.tmp", Uuid::new_v4()));
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .await
        .map_err(|_| "atomic write temporary file could not be created".to_owned())?;
    file.write_all(content)
        .await
        .map_err(|_| "atomic write temporary file failed".to_owned())?;
    file.sync_all()
        .await
        .map_err(|_| "atomic write temporary file could not be synced".to_owned())?;
    drop(file);
    set_private_permissions(&temporary).await?;
    if let Err(error) = tokio::fs::rename(&temporary, path).await {
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err(format!("atomic replace failed: {}", error.kind()));
    }
    if let Ok(directory) = tokio::fs::File::open(parent).await {
        let _ = directory.sync_all().await;
    }
    Ok(())
}

#[cfg(unix)]
async fn set_private_permissions(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .await
        .map_err(|_| "atomic write permissions could not be secured".to_owned())
}

#[cfg(not(unix))]
async fn set_private_permissions(path: &Path) -> Result<(), String> {
    let mut permissions = tokio::fs::metadata(path)
        .await
        .map_err(|_| "atomic write permissions could not be read".to_owned())?
        .permissions();
    permissions.set_readonly(false);
    tokio::fs::set_permissions(path, permissions)
        .await
        .map_err(|_| "atomic write permissions could not be secured".to_owned())
}

async fn acquire_apply_lock(path: &Path) -> Result<(), ()> {
    for attempt in 0..2 {
        match tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .await
        {
            Ok(mut file) => {
                let marker = Utc::now().to_rfc3339();
                file.write_all(marker.as_bytes()).await.map_err(|_| ())?;
                file.sync_all().await.map_err(|_| ())?;
                return Ok(());
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists && attempt == 0 => {
                let stale = tokio::fs::read_to_string(path)
                    .await
                    .ok()
                    .and_then(|value| chrono::DateTime::parse_from_rfc3339(value.trim()).ok())
                    .is_some_and(|created_at| {
                        Utc::now().signed_duration_since(created_at.with_timezone(&Utc))
                            > STALE_APPLY_LOCK
                    });
                if stale {
                    let _ = tokio::fs::remove_file(path).await;
                    continue;
                }
                return Err(());
            }
            Err(_) => return Err(()),
        }
    }
    Err(())
}

async fn rollback_backup(backup: &McpBackupDescriptor) -> (McpApplyActionStatus, String) {
    let backup_path = Path::new(&backup.backup_path);
    let source_path = Path::new(&backup.source_path);
    let bytes = match tokio::fs::read(backup_path).await {
        Ok(bytes) => bytes,
        Err(_) => {
            return (
                McpApplyActionStatus::Blocked,
                "backup is unavailable".to_owned(),
            )
        }
    };
    if sha256_bytes(&bytes) != backup.backup_hash {
        return (
            McpApplyActionStatus::Blocked,
            "backup checksum mismatch".to_owned(),
        );
    }
    let lock_path = source_path.with_extension("json.cocli-mcp.lock");
    if acquire_apply_lock(&lock_path).await.is_err() {
        return (
            McpApplyActionStatus::Blocked,
            "configuration is locked by another apply or rollback operation".to_owned(),
        );
    }
    let current = match tokio::fs::read(source_path).await {
        Ok(current) => current,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(_) => {
            let _ = tokio::fs::remove_file(&lock_path).await;
            return (
                McpApplyActionStatus::Blocked,
                "configuration could not be read before rollback".to_owned(),
            );
        }
    };
    let current_hash = sha256_bytes(&current);
    if current_hash == backup.source_hash {
        let _ = tokio::fs::remove_file(&lock_path).await;
        return (
            McpApplyActionStatus::RolledBack,
            "backup was already restored".to_owned(),
        );
    }
    if current_hash != backup.applied_hash {
        let _ = tokio::fs::remove_file(&lock_path).await;
        return (
            McpApplyActionStatus::Blocked,
            "configuration changed after apply; rollback CAS rejected restore".to_owned(),
        );
    }
    let restored = if backup.source_existed {
        atomic_write(source_path, &bytes).await
    } else {
        match tokio::fs::remove_file(source_path).await {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!("rollback remove failed: {}", error.kind())),
        }
    };
    let _ = tokio::fs::remove_file(&lock_path).await;
    match restored {
        Ok(()) => (
            McpApplyActionStatus::RolledBack,
            "backup restored atomically".to_owned(),
        ),
        Err(reason) => (McpApplyActionStatus::Failed, reason),
    }
}

fn verify_plan(
    request: &McpApplyExecutionRequest,
    results: &[McpApplyActionResult],
    inventory: &McpInventory,
) -> McpVerificationResult {
    let mut mismatches = results
        .iter()
        .filter(|result| {
            matches!(
                result.status,
                McpApplyActionStatus::Skipped
                    | McpApplyActionStatus::Blocked
                    | McpApplyActionStatus::Failed
            )
        })
        .map(|result| {
            format!(
                "{}/{} was not applied: {}",
                result.runtime, result.server_id, result.reason
            )
        })
        .collect::<Vec<_>>();
    for result in results.iter().filter(|result| {
        matches!(
            result.status,
            McpApplyActionStatus::Applied | McpApplyActionStatus::Verified
        )
    }) {
        let action = &request.plan.actions[result.action_index];
        let desired = request
            .plan
            .effective_desired_state
            .servers
            .iter()
            .find(|server| {
                server.desired.runtime == action.runtime
                    && server.desired.server_id == action.server_id
            });
        let observation = inventory.observations.iter().find(|observation| {
            observation.runtime == action.runtime
                && (observation.server_id == action.server_id
                    || desired.is_some_and(|server| observation.alias == server.desired.alias))
        });
        match action.kind {
            McpPlanActionKind::Remove if observation.is_some_and(|item| item.configured) => {
                mismatches.push(format!(
                    "{}/{} remains configured",
                    action.runtime, action.server_id
                ));
            }
            McpPlanActionKind::Remove => {}
            _ => {
                let expected_enabled = desired.map(|server| server.desired.desired_enabled);
                if observation.map_or(true, |item| {
                    !item.configured
                        || expected_enabled.is_some_and(|expected| item.enabled != Some(expected))
                }) {
                    mismatches.push(format!(
                        "{}/{} does not match desired configuration",
                        action.runtime, action.server_id
                    ));
                }
            }
        }
    }
    let applied = results.iter().any(|result| {
        matches!(
            result.status,
            McpApplyActionStatus::Applied | McpApplyActionStatus::Verified
        )
    });
    let failed = results
        .iter()
        .any(|result| result.status == McpApplyActionStatus::Failed);
    McpVerificationResult {
        status: if failed {
            McpVerificationStatus::Failed
        } else if !applied && !mismatches.is_empty() {
            McpVerificationStatus::Blocked
        } else if mismatches.is_empty() {
            McpVerificationStatus::Matched
        } else {
            McpVerificationStatus::Mismatched
        },
        observation_hash: hash_mcp_observation(inventory),
        mismatches,
    }
}

fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex(hasher.finalize().as_slice())
}

fn target_runtimes(catalog: &[RuntimeInfo]) -> Vec<String> {
    let mut names: Vec<String> = catalog
        .iter()
        .map(|runtime| runtime.name.as_str())
        .filter(|name| matches!(*name, "codex" | "cursor" | "claude" | "grok"))
        .map(ToOwned::to_owned)
        .collect();
    for runtime in ["codex", "cursor", "claude", "grok"] {
        if !names.iter().any(|name| name == runtime) {
            names.push(runtime.to_owned());
        }
    }
    names.sort();
    names.dedup();
    names
}

fn config_paths(runtime: &str, home: Option<&Path>, workspace: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    match runtime {
        "codex" => {
            if let Some(home) = home {
                paths.push(home.join(".codex").join("config.toml"));
            }
            paths.push(workspace.join(".codex").join("config.toml"));
        }
        "cursor" => {
            if let Some(home) = home {
                paths.push(home.join(".cursor").join("mcp.json"));
            }
            paths.push(workspace.join(".cursor").join("mcp.json"));
        }
        "claude" => {
            if let Some(home) = home {
                paths.push(home.join(".claude.json"));
                paths.push(home.join(".claude").join("mcp.json"));
            }
            paths.push(workspace.join(".mcp-config.json"));
            paths.push(workspace.join(".claude").join("mcp.json"));
        }
        "grok" => {
            if let Some(home) = home {
                paths.push(home.join(".grok").join("config.toml"));
            }
            paths.push(workspace.join(".grok").join("config.toml"));
        }
        _ => {}
    }
    paths
}

enum ConfigRead {
    Missing,
    Snapshot(Snapshot),
    Diagnostic(McpDiagnostic),
}

#[derive(Default)]
struct Snapshot {
    servers: Vec<McpServer>,
    bindings: Vec<McpBinding>,
    observations: Vec<ObservedMcpInstance>,
    diagnostics: Vec<McpDiagnostic>,
}

impl Snapshot {
    fn extend(&mut self, other: Snapshot) {
        self.servers.extend(other.servers);
        self.bindings.extend(other.bindings);
        for observation in other.observations {
            if let Some(existing) = self.observations.iter_mut().find(|existing| {
                existing.runtime == observation.runtime && existing.alias == observation.alias
            }) {
                merge_observation(existing, observation);
            } else {
                self.observations.push(observation);
            }
        }
        self.diagnostics.extend(other.diagnostics);
    }
}

fn merge_observation(existing: &mut ObservedMcpInstance, next: ObservedMcpInstance) {
    existing.discoverable |= next.discoverable;
    existing.configured |= next.configured;
    if next.loaded.is_some() {
        existing.loaded = next.loaded;
    }
    if next.enabled.is_some() {
        existing.enabled = next.enabled;
    }
    if next.approved.is_some() {
        existing.approved = next.approved;
    }
    if next.authenticated.is_some() {
        existing.authenticated = next.authenticated;
    }
    if next.healthy.is_some() {
        existing.healthy = next.healthy;
    }
    if next.startup.is_some() {
        existing.startup = next.startup;
    }
    if next.current_session_visible.is_some() {
        existing.current_session_visible = next.current_session_visible;
    }
    if next.invoked.is_some() {
        existing.invoked = next.invoked;
    }
    if next.tool_count.is_some() {
        existing.tool_count = next.tool_count;
    }
    if next.schema_hash.is_some() {
        existing.schema_hash = next.schema_hash;
    }
    existing.evidence.extend(next.evidence);
}

async fn discover_config(
    runtime: &str,
    path: &Path,
    workspace_scope: Option<&Path>,
    observed_at: &str,
) -> ConfigRead {
    let metadata = match tokio::fs::metadata(path).await {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return ConfigRead::Missing,
        Err(error) => {
            return ConfigRead::Diagnostic(diagnostic(
                "mcp_config_unreadable",
                McpDiagnosticSeverity::Warning,
                runtime,
                None,
                format!("MCP config could not be read: {}", error.kind()),
                vec![evidence(
                    "config",
                    "metadata failed",
                    Some(path),
                    false,
                    false,
                )],
                observed_at,
            ));
        }
    };
    if !metadata.is_file() {
        return ConfigRead::Missing;
    }
    if metadata.len() > MAX_CONFIG_BYTES {
        return ConfigRead::Diagnostic(diagnostic(
            "mcp_config_too_large",
            McpDiagnosticSeverity::Warning,
            runtime,
            None,
            "MCP config was skipped because it is larger than the read-only inventory limit",
            vec![evidence(
                "config",
                "file exceeds 1 MiB",
                Some(path),
                false,
                false,
            )],
            observed_at,
        ));
    }

    let bytes = match tokio::fs::read(path).await {
        Ok(bytes) => bytes,
        Err(error) => {
            return ConfigRead::Diagnostic(diagnostic(
                "mcp_config_unreadable",
                McpDiagnosticSeverity::Warning,
                runtime,
                None,
                format!("MCP config could not be read: {}", error.kind()),
                vec![evidence("config", "read failed", Some(path), false, false)],
                observed_at,
            ));
        }
    };
    let text = String::from_utf8_lossy(&bytes);
    let parsed = if path.extension().and_then(|ext| ext.to_str()) == Some("toml") {
        parse_toml_servers(&text)
    } else {
        parse_json_servers(&text)
    };

    match parsed {
        Ok(definitions) => ConfigRead::Snapshot(snapshot_config(
            runtime,
            path,
            workspace_scope,
            definitions,
            observed_at,
        )),
        Err(message) => ConfigRead::Diagnostic(diagnostic(
            "mcp_config_bad_json",
            McpDiagnosticSeverity::Warning,
            runtime,
            None,
            message,
            vec![evidence("config", "parse failed", Some(path), false, false)],
            observed_at,
        )),
    }
}

#[derive(Clone, Debug)]
struct ServerDefinition {
    alias: String,
    definition: McpCanonicalDefinition,
    desired_enabled: Option<bool>,
    policy: Option<String>,
    secret_refs: Vec<McpSecretRef>,
    plaintext_secret: bool,
}

fn parse_json_servers(text: &str) -> Result<Vec<ServerDefinition>, String> {
    let value: Value =
        serde_json::from_str(text).map_err(|_| "MCP config contains invalid JSON".to_owned())?;
    let Some(servers) = value
        .get("mcpServers")
        .or_else(|| value.get("mcp_servers"))
        .or_else(|| value.get("mcp"))
        .and_then(Value::as_object)
    else {
        return Ok(Vec::new());
    };
    Ok(servers
        .iter()
        .filter_map(|(alias, value)| server_from_value(alias, value))
        .collect())
}

fn server_from_value(alias: &str, value: &Value) -> Option<ServerDefinition> {
    let object = value.as_object()?;
    let safe_alias = redact_text(alias);
    let transport = match object
        .get("transport")
        .or_else(|| object.get("type"))
        .and_then(Value::as_str)
    {
        Some("sse") => McpTransport::Sse,
        Some("streamable-http" | "streamableHttp") => McpTransport::StreamableHttp,
        Some("http" | "remote") => McpTransport::Http,
        Some("stdio" | "local") => McpTransport::Stdio,
        _ if object
            .get("url")
            .or_else(|| object.get("endpoint"))
            .is_some() =>
        {
            McpTransport::Http
        }
        _ if object.get("command").is_some() => McpTransport::Stdio,
        _ => McpTransport::Unknown,
    };
    let command = object
        .get("command")
        .and_then(Value::as_str)
        .map(redact_text);
    let endpoint = object
        .get("url")
        .or_else(|| object.get("endpoint"))
        .and_then(Value::as_str)
        .map(redact_url);
    let args = object
        .get("args")
        .and_then(Value::as_array)
        .map(|args| {
            let raw = args
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>();
            redact_args(&raw)
        })
        .unwrap_or_default();
    let desired_enabled = object.get("enabled").and_then(Value::as_bool).or_else(|| {
        object
            .get("disabled")
            .and_then(Value::as_bool)
            .map(|disabled| !disabled)
    });
    let policy = object
        .get("approval")
        .or_else(|| object.get("policy"))
        .or_else(|| object.get("required"))
        .map(|value| redact_text(&value.to_string()));
    let secret_refs = object
        .get("env")
        .and_then(Value::as_object)
        .map(|env| secret_refs(&safe_alias, env.keys()))
        .unwrap_or_default();
    let plaintext_secret = !secret_refs.is_empty() || args.iter().any(|arg| arg == "<redacted>");
    Some(ServerDefinition {
        alias: safe_alias,
        definition: McpCanonicalDefinition {
            transport,
            command,
            args,
            endpoint,
        },
        desired_enabled,
        policy,
        secret_refs,
        plaintext_secret,
    })
}

fn parse_toml_servers(text: &str) -> Result<Vec<ServerDefinition>, String> {
    let mut raw: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    let mut current: Option<String> = None;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(section) = line
            .strip_prefix('[')
            .and_then(|line| line.strip_suffix(']'))
        {
            current = toml_server_alias(section);
            continue;
        }
        let Some(alias) = current.as_ref() else {
            continue;
        };
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        raw.entry(alias.clone())
            .or_default()
            .insert(key.trim().to_owned(), value.trim().to_owned());
    }
    Ok(raw
        .into_iter()
        .map(|(alias, values)| {
            let command = values
                .get("command")
                .and_then(|value| toml_string(value))
                .map(|value| redact_text(&value));
            let endpoint = values
                .get("url")
                .or_else(|| values.get("endpoint"))
                .and_then(|value| toml_string(value))
                .map(|value| redact_url(&value));
            let args = values
                .get("args")
                .map(|value| parse_toml_string_array(value))
                .unwrap_or_default();
            let transport = if endpoint.is_some() {
                McpTransport::Http
            } else if command.is_some() {
                McpTransport::Stdio
            } else {
                McpTransport::Unknown
            };
            ServerDefinition {
                alias,
                definition: McpCanonicalDefinition {
                    transport,
                    command,
                    args: redact_args(&args),
                    endpoint,
                },
                desired_enabled: values
                    .get("enabled")
                    .and_then(|value| value.parse::<bool>().ok()),
                policy: values
                    .get("required")
                    .or_else(|| values.get("approval"))
                    .cloned(),
                secret_refs: values
                    .keys()
                    .filter(|key| secret_like(key))
                    .map(|key| McpSecretRef {
                        location: "config".to_owned(),
                        kind: "inline".to_owned(),
                        reference: key.clone(),
                    })
                    .collect(),
                plaintext_secret: values.keys().any(|key| secret_like(key))
                    || args.iter().any(|arg| secret_like(arg)),
            }
        })
        .collect())
}

fn toml_server_alias(section: &str) -> Option<String> {
    for prefix in ["mcp_servers.", "mcpServers.", "mcp."] {
        if let Some(alias) = section.strip_prefix(prefix) {
            return Some(alias.trim_matches('"').to_owned());
        }
    }
    None
}

fn toml_string(value: &str) -> Option<String> {
    let value = value.trim();
    if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
        Some(value[1..value.len() - 1].to_owned())
    } else {
        None
    }
}

fn parse_toml_string_array(value: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(value).unwrap_or_default()
}

fn snapshot_config(
    runtime: &str,
    path: &Path,
    workspace_scope: Option<&Path>,
    definitions: Vec<ServerDefinition>,
    observed_at: &str,
) -> Snapshot {
    let mut snapshot = Snapshot::default();
    for definition in definitions {
        let fingerprint = endpoint_fingerprint(&definition.definition);
        let server_id = server_id(&fingerprint);
        let evidence = evidence("config", "configured MCP server", Some(path), false, false);
        if definition.plaintext_secret {
            snapshot.diagnostics.push(diagnostic(
                "mcp_plaintext_secret",
                McpDiagnosticSeverity::Warning,
                runtime,
                Some(server_id.clone()),
                "MCP config contains inline secret material; values were redacted",
                vec![evidence.clone()],
                observed_at,
            ));
        }
        snapshot.servers.push(McpServer {
            id: server_id.clone(),
            canonical_name: definition.alias.clone(),
            definition: definition.definition,
            endpoint_fingerprint: fingerprint,
            aliases: vec![definition.alias.clone()],
            provenance: vec![evidence.clone()],
            secret_refs: definition.secret_refs,
        });
        snapshot.bindings.push(McpBinding {
            server_id: server_id.clone(),
            runtime: runtime.to_owned(),
            agent_id: None,
            workspace: workspace_scope.map(|workspace| workspace.display().to_string()),
            profile: Some(
                if workspace_scope.is_some() {
                    "workspace"
                } else {
                    "user"
                }
                .to_owned(),
            ),
            desired_enabled: definition.desired_enabled,
            policy: definition.policy,
        });
        snapshot.observations.push(ObservedMcpInstance {
            runtime: runtime.to_owned(),
            server_id,
            alias: definition.alias,
            source_path: Some(path.display().to_string()),
            discoverable: true,
            configured: true,
            loaded: None,
            enabled: definition.desired_enabled,
            approved: None,
            authenticated: None,
            healthy: None,
            startup: Some(McpStartupState::NotAttempted),
            current_session_visible: None,
            invoked: None,
            tool_count: None,
            schema_hash: None,
            evidence: vec![evidence],
            observed_at: observed_at.to_owned(),
        });
    }
    snapshot
}

async fn run_probes(
    catalog: &[RuntimeInfo],
    runtimes: &[String],
    workspace: &Path,
    observed_at: &str,
    runner: &dyn CommandRunner,
) -> Vec<Snapshot> {
    let mut snapshots = Vec::new();
    for runtime in runtimes {
        let binary = catalog
            .iter()
            .find(|entry| entry.name == *runtime)
            .and_then(|entry| entry.binary.as_ref())
            .map(PathBuf::from);
        snapshots
            .push(probe_runtime(runtime, binary.as_deref(), workspace, observed_at, runner).await);
    }
    snapshots
}

async fn probe_runtime(
    runtime: &str,
    binary: Option<&Path>,
    workspace: &Path,
    observed_at: &str,
    runner: &dyn CommandRunner,
) -> Snapshot {
    let mut snapshot = Snapshot::default();
    let Some(binary) = binary else {
        snapshot.diagnostics.push(diagnostic(
            "mcp_probe_command_missing",
            McpDiagnosticSeverity::Info,
            runtime,
            None,
            "Runtime MCP probe skipped because the runtime command is not installed",
            Vec::new(),
            observed_at,
        ));
        return snapshot;
    };
    if runtime == "codex" {
        match probe_codex_app_server(binary, workspace, observed_at).await {
            Ok(app_server) => return app_server,
            Err(diagnostic) => snapshot.diagnostics.push(diagnostic),
        }
    }
    let args = probe_args(runtime);
    let output = match runner.run(binary, args, workspace, PROBE_TIMEOUT).await {
        CommandOutcome::Output(output) => output,
        CommandOutcome::Missing => {
            snapshot.diagnostics.push(diagnostic(
                "mcp_probe_command_missing",
                McpDiagnosticSeverity::Info,
                runtime,
                None,
                "Runtime MCP probe command was not found",
                Vec::new(),
                observed_at,
            ));
            return snapshot;
        }
        CommandOutcome::StartFailed(kind) => {
            snapshot.diagnostics.push(diagnostic(
                "mcp_probe_failed",
                McpDiagnosticSeverity::Warning,
                runtime,
                None,
                format!("Runtime MCP probe failed to start: {kind}"),
                Vec::new(),
                observed_at,
            ));
            return snapshot;
        }
        CommandOutcome::Timeout => {
            snapshot.diagnostics.push(diagnostic(
                "mcp_probe_timeout",
                McpDiagnosticSeverity::Warning,
                runtime,
                None,
                "Runtime MCP probe timed out",
                Vec::new(),
                observed_at,
            ));
            return snapshot;
        }
    };

    if !output.success {
        let code = if looks_unauthorized(&output.stdout) || looks_unauthorized(&output.stderr) {
            "mcp_probe_unauthorized"
        } else {
            "mcp_probe_failed"
        };
        snapshot.diagnostics.push(diagnostic(
            code,
            McpDiagnosticSeverity::Warning,
            runtime,
            None,
            "Runtime MCP probe exited unsuccessfully",
            Vec::new(),
            observed_at,
        ));
        return snapshot;
    }

    snapshot.observations = if probe_outputs_json(runtime) {
        let value: Value = match serde_json::from_str(&output.stdout) {
            Ok(value) => value,
            Err(_) => {
                snapshot.diagnostics.push(diagnostic(
                    "mcp_probe_bad_json",
                    McpDiagnosticSeverity::Warning,
                    runtime,
                    None,
                    "Runtime MCP probe returned invalid JSON",
                    Vec::new(),
                    observed_at,
                ));
                if runtime == "grok" {
                    Value::Null
                } else {
                    return snapshot;
                }
            }
        };
        if value.is_null() {
            Vec::new()
        } else {
            observations_from_json_probe(runtime, &value, observed_at, false)
        }
    } else {
        observations_from_text_probe(runtime, &output.stdout, observed_at)
    };
    for alias in snapshot
        .observations
        .iter()
        .map(|observation| observation.alias.clone())
        .collect::<Vec<_>>()
    {
        snapshot.extend(
            probe_alias_detail(runtime, binary, workspace, observed_at, runner, &alias).await,
        );
    }
    if runtime == "grok" {
        snapshot.extend(probe_grok_doctor(binary, workspace, observed_at, runner).await);
    }
    snapshot
}

async fn probe_alias_detail(
    runtime: &str,
    binary: &Path,
    workspace: &Path,
    observed_at: &str,
    runner: &dyn CommandRunner,
    alias: &str,
) -> Snapshot {
    let Some(args) = detail_probe_args(runtime, alias) else {
        return Snapshot::default();
    };
    let mut snapshot = Snapshot::default();
    match runner
        .run(
            binary,
            &args.iter().map(String::as_str).collect::<Vec<_>>(),
            workspace,
            PROBE_TIMEOUT,
        )
        .await
    {
        CommandOutcome::Output(output) if output.success => {
            if let Ok(value) = serde_json::from_str::<Value>(&output.stdout) {
                snapshot.observations =
                    observations_from_json_probe(runtime, &value, observed_at, true);
            } else {
                snapshot.observations.push(observation_from_detail_text(
                    runtime,
                    alias,
                    &output.stdout,
                    observed_at,
                ));
            }
        }
        CommandOutcome::Output(output) => {
            let code = if looks_unauthorized(&output.stdout) || looks_unauthorized(&output.stderr) {
                "mcp_probe_unauthorized"
            } else {
                "mcp_probe_detail_failed"
            };
            snapshot.diagnostics.push(diagnostic(
                code,
                McpDiagnosticSeverity::Warning,
                runtime,
                None,
                format!(
                    "Runtime MCP detail probe failed for alias `{}`",
                    redact_text(alias)
                ),
                Vec::new(),
                observed_at,
            ));
        }
        CommandOutcome::Missing => {}
        CommandOutcome::Timeout => snapshot.diagnostics.push(diagnostic(
            "mcp_probe_timeout",
            McpDiagnosticSeverity::Warning,
            runtime,
            None,
            format!(
                "Runtime MCP detail probe timed out for alias `{}`",
                redact_text(alias)
            ),
            Vec::new(),
            observed_at,
        )),
        CommandOutcome::StartFailed(kind) => snapshot.diagnostics.push(diagnostic(
            "mcp_probe_detail_failed",
            McpDiagnosticSeverity::Warning,
            runtime,
            None,
            format!(
                "Runtime MCP detail probe failed to start for alias `{}`: {kind}",
                redact_text(alias)
            ),
            Vec::new(),
            observed_at,
        )),
    }
    snapshot
}

fn observation_from_detail_text(
    runtime: &str,
    alias: &str,
    output: &str,
    observed_at: &str,
) -> ObservedMcpInstance {
    let lower = output.to_ascii_lowercase();
    let tool_count = (runtime == "cursor").then(|| {
        output
            .lines()
            .filter(|line| {
                let line = line.trim();
                !line.is_empty()
                    && !line.starts_with("MCP")
                    && !line.starts_with("Tool")
                    && !line.starts_with("---")
            })
            .count() as u32
    });
    let schema_hash = tool_count.map(|_| sha256_text(output));
    let approved = if lower.contains("not approved") || lower.contains("approval required") {
        Some(false)
    } else if lower.contains("approved") {
        Some(true)
    } else {
        None
    };
    let authenticated = if looks_unauthorized(output) || lower.contains("login required") {
        Some(false)
    } else if lower.contains("authenticated") {
        Some(true)
    } else {
        None
    };
    let healthy = if lower.contains("failed")
        || lower.contains("disconnected")
        || lower.contains("unhealthy")
    {
        Some(false)
    } else if lower.contains("connected") || lower.contains("healthy") {
        Some(true)
    } else {
        None
    };
    let cursor_tools_visible = runtime == "cursor" && tool_count.is_some();
    ObservedMcpInstance {
        runtime: runtime.to_owned(),
        server_id: format!("runtime:{runtime}:{alias}"),
        alias: alias.to_owned(),
        source_path: None,
        discoverable: true,
        configured: runtime == "claude",
        loaded: if cursor_tools_visible {
            Some(true)
        } else {
            healthy
        },
        enabled: None,
        approved: if cursor_tools_visible {
            Some(true)
        } else {
            approved
        },
        authenticated,
        healthy: if cursor_tools_visible {
            Some(true)
        } else {
            healthy
        },
        startup: if cursor_tools_visible {
            Some(McpStartupState::Ready)
        } else {
            healthy.map(|healthy| {
                if healthy {
                    McpStartupState::Ready
                } else {
                    McpStartupState::Failed
                }
            })
        },
        current_session_visible: None,
        invoked: None,
        tool_count,
        schema_hash,
        evidence: vec![evidence(
            "runtime_probe",
            if runtime == "cursor" {
                "cursor-agent mcp list-tools"
            } else {
                "claude mcp get"
            },
            None,
            cursor_tools_visible,
            false,
        )],
        observed_at: observed_at.to_owned(),
    }
}

async fn probe_grok_doctor(
    binary: &Path,
    workspace: &Path,
    observed_at: &str,
    runner: &dyn CommandRunner,
) -> Snapshot {
    let mut snapshot = Snapshot::default();
    match runner
        .run(
            binary,
            &["mcp", "doctor", "--json"],
            workspace,
            PROBE_TIMEOUT,
        )
        .await
    {
        CommandOutcome::Output(output) if output.success => {
            if let Ok(value) = serde_json::from_str::<Value>(&output.stdout) {
                snapshot.observations =
                    observations_from_json_probe("grok", &value, observed_at, true);
            } else {
                snapshot.diagnostics.push(diagnostic(
                    "mcp_doctor_bad_json",
                    McpDiagnosticSeverity::Warning,
                    "grok",
                    None,
                    "Runtime MCP doctor returned invalid JSON",
                    Vec::new(),
                    observed_at,
                ));
            }
        }
        CommandOutcome::Output(output) => {
            let code = if looks_unauthorized(&output.stdout) || looks_unauthorized(&output.stderr) {
                "mcp_probe_unauthorized"
            } else {
                "mcp_doctor_failed"
            };
            snapshot.diagnostics.push(diagnostic(
                code,
                McpDiagnosticSeverity::Warning,
                "grok",
                None,
                "Runtime MCP doctor exited unsuccessfully",
                Vec::new(),
                observed_at,
            ));
        }
        CommandOutcome::Missing => {}
        CommandOutcome::Timeout => snapshot.diagnostics.push(diagnostic(
            "mcp_probe_timeout",
            McpDiagnosticSeverity::Warning,
            "grok",
            None,
            "Runtime MCP doctor timed out",
            Vec::new(),
            observed_at,
        )),
        CommandOutcome::StartFailed(kind) => snapshot.diagnostics.push(diagnostic(
            "mcp_doctor_failed",
            McpDiagnosticSeverity::Warning,
            "grok",
            None,
            format!("Runtime MCP doctor failed to start: {kind}"),
            Vec::new(),
            observed_at,
        )),
    }
    snapshot
}

async fn probe_codex_app_server(
    binary: &Path,
    workspace: &Path,
    observed_at: &str,
) -> Result<Snapshot, McpDiagnostic> {
    match tokio::time::timeout(PROBE_TIMEOUT, codex_app_server_request(binary, workspace)).await {
        Ok(Ok(value)) => Ok(Snapshot {
            observations: observations_from_json_probe("codex", &value, observed_at, true),
            ..Snapshot::default()
        }),
        Ok(Err(_)) | Err(_) => Err(diagnostic(
            "mcp_codex_app_server_probe_fallback",
            McpDiagnosticSeverity::Info,
            "codex",
            None,
            "Codex app-server MCP probe was unavailable; falling back to codex mcp list --json",
            vec![evidence(
                "codex_app_server",
                "attempted mcpServerStatus/list JSON-RPC",
                None,
                false,
                false,
            )],
            observed_at,
        )),
    }
}

async fn codex_app_server_request(binary: &Path, workspace: &Path) -> std::io::Result<Value> {
    let mut child = Command::new(binary)
        .arg("app-server")
        .arg("--listen")
        .arg("stdio://")
        .current_dir(workspace)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| std::io::Error::other("missing stdin"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| std::io::Error::other("missing stdout"))?;
    let mut stdout = BufReader::new(stdout);
    write_jsonrpc(
        &mut stdin,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "clientInfo": {"name": "cocli-mcp-inventory", "version": env!("CARGO_PKG_VERSION")},
                "capabilities": {"experimentalApi": true}
            }
        }),
    )
    .await?;
    read_jsonrpc_response(&mut stdout, 1).await?;
    write_jsonrpc(
        &mut stdin,
        &serde_json::json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}),
    )
    .await?;
    write_jsonrpc(
        &mut stdin,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "mcpServerStatus/list",
            "params": {"cwd": workspace}
        }),
    )
    .await?;
    let response = read_jsonrpc_response(&mut stdout, 2).await;
    drop(stdin);
    let _ = child.kill().await;
    let _ = child.wait().await;
    response
}

async fn write_jsonrpc(
    stdin: &mut tokio::process::ChildStdin,
    value: &Value,
) -> std::io::Result<()> {
    let mut line = serde_json::to_vec(value).map_err(std::io::Error::other)?;
    line.push(b'\n');
    stdin.write_all(&line).await?;
    stdin.flush().await
}

async fn read_jsonrpc_response(
    stdout: &mut BufReader<tokio::process::ChildStdout>,
    id: u64,
) -> std::io::Result<Value> {
    loop {
        let mut line = String::new();
        let read = stdout.read_line(&mut line).await?;
        if read == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "codex app-server exited",
            ));
        }
        let value: Value = serde_json::from_str(&line).map_err(std::io::Error::other)?;
        if value.get("id").and_then(Value::as_u64) != Some(id) {
            continue;
        }
        if value.get("error").is_some() {
            return Err(std::io::Error::other("codex app-server request failed"));
        }
        return Ok(value.get("result").cloned().unwrap_or(value));
    }
}

fn detail_probe_args(runtime: &str, alias: &str) -> Option<Vec<String>> {
    match runtime {
        "cursor" => Some(vec![
            "mcp".to_owned(),
            "list-tools".to_owned(),
            alias.to_owned(),
        ]),
        "claude" => Some(vec!["mcp".to_owned(), "get".to_owned(), alias.to_owned()]),
        _ => None,
    }
}

fn probe_args(runtime: &str) -> &'static [&'static str] {
    match runtime {
        "codex" | "grok" => &["mcp", "list", "--json"],
        "claude" | "cursor" => &["mcp", "list"],
        _ => &[],
    }
}

fn probe_outputs_json(runtime: &str) -> bool {
    matches!(runtime, "codex" | "grok")
}

fn observations_from_json_probe(
    runtime: &str,
    value: &Value,
    observed_at: &str,
    state_is_loaded_evidence: bool,
) -> Vec<ObservedMcpInstance> {
    let servers = value
        .get("servers")
        .or_else(|| value.get("mcpServers"))
        .or_else(|| value.get("mcp_servers"))
        .unwrap_or(value);
    let entries: Vec<(String, &Value)> = if let Some(object) = servers.as_object() {
        object
            .iter()
            .map(|(alias, value)| (alias.clone(), value))
            .collect()
    } else {
        servers
            .as_array()
            .into_iter()
            .flat_map(|array| array.iter())
            .enumerate()
            .map(|(index, value)| {
                let alias = value
                    .get("name")
                    .or_else(|| value.get("alias"))
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(|| format!("server-{index}"));
                (alias, value)
            })
            .collect()
    };

    entries
        .into_iter()
        .map(|(alias, value)| {
            let definition = server_from_value(&alias, value)
                .map(|server| server.definition)
                .unwrap_or(McpCanonicalDefinition {
                    transport: McpTransport::Unknown,
                    command: None,
                    args: Vec::new(),
                    endpoint: None,
                });
            let fingerprint = endpoint_fingerprint(&definition);
            let alias = redact_text(&alias);
            let server_id = server_id(&fingerprint);
            let approved = value.get("approved").and_then(Value::as_bool);
            let authenticated = value.get("authenticated").and_then(Value::as_bool);
            let loaded = state_is_loaded_evidence
                .then(|| {
                    value
                        .get("loaded")
                        .or_else(|| value.get("connected"))
                        .and_then(Value::as_bool)
                })
                .flatten();
            let healthy = value
                .get("healthy")
                .or_else(|| value.get("ok"))
                .and_then(Value::as_bool);
            let current_session_visible = value
                .get("currentSessionVisible")
                .or_else(|| value.get("current_session_visible"))
                .and_then(Value::as_bool);
            ObservedMcpInstance {
                runtime: runtime.to_owned(),
                server_id,
                alias,
                source_path: None,
                discoverable: true,
                configured: value
                    .get("configured")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                loaded,
                enabled: value.get("enabled").and_then(Value::as_bool),
                approved,
                authenticated,
                healthy,
                startup: startup_state(value),
                current_session_visible,
                invoked: value.get("invoked").and_then(Value::as_bool),
                tool_count: value
                    .get("toolCount")
                    .or_else(|| value.get("tool_count"))
                    .and_then(Value::as_u64)
                    .map(|count| count as u32),
                schema_hash: value
                    .get("schemaHash")
                    .or_else(|| value.get("schema_hash"))
                    .and_then(Value::as_str)
                    .map(sanitize_schema_hash),
                evidence: vec![evidence(
                    "runtime_probe",
                    "runtime-reported MCP state",
                    None,
                    state_is_loaded_evidence,
                    current_session_visible.is_some(),
                )],
                observed_at: observed_at.to_owned(),
            }
        })
        .collect()
}

fn observations_from_text_probe(
    runtime: &str,
    stdout: &str,
    observed_at: &str,
) -> Vec<ObservedMcpInstance> {
    stdout
        .lines()
        .filter_map(text_probe_alias)
        .map(|alias| {
            let definition = McpCanonicalDefinition {
                transport: McpTransport::Unknown,
                command: None,
                args: Vec::new(),
                endpoint: None,
            };
            let fingerprint = endpoint_fingerprint(&definition);
            ObservedMcpInstance {
                runtime: runtime.to_owned(),
                server_id: format!(
                    "runtime:{runtime}:{alias}:{}",
                    &fingerprint[..12.min(fingerprint.len())]
                ),
                alias,
                source_path: None,
                discoverable: true,
                configured: false,
                loaded: None,
                enabled: None,
                approved: None,
                authenticated: None,
                healthy: None,
                startup: Some(McpStartupState::Unknown),
                current_session_visible: None,
                invoked: None,
                tool_count: None,
                schema_hash: None,
                evidence: vec![evidence(
                    "runtime_probe",
                    "runtime text MCP list output",
                    None,
                    false,
                    false,
                )],
                observed_at: observed_at.to_owned(),
            }
        })
        .collect()
}

fn text_probe_alias(line: &str) -> Option<String> {
    let line = line.trim();
    if line.is_empty()
        || line.starts_with("No MCP")
        || line.starts_with("MCP")
        || line.starts_with("Name")
        || line.starts_with("---")
    {
        return None;
    }
    let candidate = line
        .trim_start_matches(|char: char| {
            char == '-' || char == '*' || char == '•' || char.is_whitespace()
        })
        .split_whitespace()
        .next()?
        .trim_matches(|char: char| char == ':' || char == ',' || char == '"' || char == '\'');
    if candidate.is_empty() {
        None
    } else {
        Some(redact_text(candidate))
    }
}

fn startup_state(value: &Value) -> Option<McpStartupState> {
    match value
        .get("startup")
        .or_else(|| value.get("status"))
        .and_then(Value::as_str)
    {
        Some("ready" | "connected" | "ok") => Some(McpStartupState::Ready),
        Some("failed" | "error") => Some(McpStartupState::Failed),
        Some("starting") => Some(McpStartupState::Starting),
        Some("not_attempted" | "notAttempted") => Some(McpStartupState::NotAttempted),
        Some(_) => Some(McpStartupState::Unknown),
        None => None,
    }
}

struct Aggregate {
    servers: BTreeMap<String, McpServer>,
    bindings: Vec<McpBinding>,
    observations: Vec<ObservedMcpInstance>,
    diagnostics: Vec<McpDiagnostic>,
    observed_at: String,
}

impl Aggregate {
    fn new(observed_at: String) -> Self {
        Self {
            servers: BTreeMap::new(),
            bindings: Vec::new(),
            observations: Vec::new(),
            diagnostics: Vec::new(),
            observed_at,
        }
    }

    fn extend(&mut self, snapshot: Snapshot) {
        for server in snapshot.servers {
            self.servers
                .entry(server.id.clone())
                .and_modify(|existing| {
                    for alias in &server.aliases {
                        if !existing.aliases.contains(alias) {
                            existing.aliases.push(alias.clone());
                        }
                    }
                    existing.provenance.extend(server.provenance.clone());
                    existing.secret_refs.extend(server.secret_refs.clone());
                })
                .or_insert(server);
        }
        self.bindings.extend(snapshot.bindings);
        self.observations.extend(snapshot.observations);
        self.diagnostics.extend(snapshot.diagnostics);
    }

    fn finalize(mut self) -> McpInventory {
        self.align_probe_observations();
        self.add_native_only_servers();
        self.detect_duplicates();
        self.detect_configuration_drift();
        self.detect_runtime_state_issues();
        let mut servers: Vec<_> = self.servers.into_values().collect();
        servers.sort_by(|a, b| a.id.cmp(&b.id));
        self.bindings
            .sort_by(|a, b| (&a.runtime, &a.server_id).cmp(&(&b.runtime, &b.server_id)));
        self.observations.sort_by(|a, b| {
            (&a.runtime, &a.alias, &a.server_id).cmp(&(&b.runtime, &b.alias, &b.server_id))
        });
        self.diagnostics.sort_by(|a, b| {
            (&a.runtime, &a.code, &a.server_id).cmp(&(&b.runtime, &b.code, &b.server_id))
        });
        McpInventory {
            servers,
            bindings: self.bindings,
            observations: self.observations,
            diagnostics: self.diagnostics,
            observed_at: self.observed_at,
        }
    }

    fn align_probe_observations(&mut self) {
        let configured: HashMap<(String, String), String> = self
            .observations
            .iter()
            .filter(|observation| observation.configured && observation.source_path.is_some())
            .map(|observation| {
                (
                    (observation.runtime.clone(), observation.alias.clone()),
                    observation.server_id.clone(),
                )
            })
            .collect();
        for observation in &mut self.observations {
            if observation.source_path.is_none() {
                if let Some(server_id) =
                    configured.get(&(observation.runtime.clone(), observation.alias.clone()))
                {
                    observation.server_id.clone_from(server_id);
                    observation.configured = true;
                }
            }
        }
    }

    fn add_native_only_servers(&mut self) {
        for observation in &self.observations {
            self.servers
                .entry(observation.server_id.clone())
                .or_insert_with(|| {
                    let definition = McpCanonicalDefinition {
                        transport: McpTransport::Unknown,
                        command: None,
                        args: Vec::new(),
                        endpoint: None,
                    };
                    McpServer {
                        id: observation.server_id.clone(),
                        canonical_name: observation.alias.clone(),
                        endpoint_fingerprint: endpoint_fingerprint(&definition),
                        definition,
                        aliases: vec![observation.alias.clone()],
                        provenance: observation.evidence.clone(),
                        secret_refs: Vec::new(),
                    }
                });
        }
    }

    fn detect_duplicates(&mut self) {
        let mut aliases: HashMap<(String, String), Vec<String>> = HashMap::new();
        for observation in &self.observations {
            aliases
                .entry((observation.runtime.clone(), observation.alias.clone()))
                .or_default()
                .push(observation.server_id.clone());
        }
        for ((runtime, alias), ids) in aliases {
            let unique: HashSet<_> = ids.iter().collect();
            if unique.len() > 1 {
                self.diagnostics.push(diagnostic(
                    "mcp_duplicate_alias",
                    McpDiagnosticSeverity::Warning,
                    &runtime,
                    None,
                    format!("MCP alias `{alias}` maps to multiple endpoints"),
                    Vec::new(),
                    &self.observed_at,
                ));
            }
        }
        for server in self.servers.values() {
            let distinct_sources = server
                .provenance
                .iter()
                .map(|item| (&item.source, &item.source_path))
                .collect::<HashSet<_>>()
                .len();
            if server.aliases.len() > 1 || distinct_sources > 1 {
                self.diagnostics.push(diagnostic(
                    "mcp_duplicate_endpoint",
                    McpDiagnosticSeverity::Info,
                    "machine",
                    Some(server.id.clone()),
                    "Multiple MCP server aliases point at the same endpoint",
                    server.provenance.clone(),
                    &self.observed_at,
                ));
            }
        }
    }

    fn detect_configuration_drift(&mut self) {
        let desired = self
            .bindings
            .iter()
            .filter_map(|binding| {
                binding.desired_enabled.map(|enabled| {
                    (
                        (binding.runtime.clone(), binding.server_id.clone()),
                        enabled,
                    )
                })
            })
            .collect::<HashMap<_, _>>();
        for observation in &self.observations {
            if observation.source_path.is_none() {
                if let (Some(desired), Some(actual)) = (
                    desired.get(&(observation.runtime.clone(), observation.server_id.clone())),
                    observation.enabled,
                ) {
                    if *desired != actual {
                        self.diagnostics.push(diagnostic(
                            "mcp_config_drift",
                            McpDiagnosticSeverity::Warning,
                            &observation.runtime,
                            Some(observation.server_id.clone()),
                            "Configured MCP enabled state differs from Runtime-observed state",
                            observation.evidence.clone(),
                            &self.observed_at,
                        ));
                    }
                }
            }
        }

        let mut aliases: HashMap<String, HashSet<String>> = HashMap::new();
        for observation in self
            .observations
            .iter()
            .filter(|observation| observation.source_path.is_some())
        {
            aliases
                .entry(observation.alias.clone())
                .or_default()
                .insert(observation.server_id.clone());
        }
        for (alias, server_ids) in aliases {
            if server_ids.len() > 1 {
                self.diagnostics.push(diagnostic(
                    "mcp_config_drift",
                    McpDiagnosticSeverity::Warning,
                    "machine",
                    None,
                    format!("MCP alias `{alias}` has different canonical definitions"),
                    Vec::new(),
                    &self.observed_at,
                ));
            }
        }
    }

    fn detect_runtime_state_issues(&mut self) {
        for observation in &self.observations {
            let code = if observation.approved == Some(false) {
                Some("mcp_not_approved")
            } else if observation.authenticated == Some(false) {
                Some("mcp_not_authenticated")
            } else if observation.startup == Some(McpStartupState::Failed)
                || observation.healthy == Some(false)
            {
                Some("mcp_startup_failed")
            } else if observation.configured && observation.loaded == Some(false) {
                Some("mcp_config_runtime_drift")
            } else {
                None
            };
            if let Some(code) = code {
                self.diagnostics.push(diagnostic(
                    code,
                    McpDiagnosticSeverity::Warning,
                    &observation.runtime,
                    Some(observation.server_id.clone()),
                    "Runtime MCP state needs attention",
                    observation.evidence.clone(),
                    &self.observed_at,
                ));
            }
        }
    }
}

fn server_id(fingerprint: &str) -> String {
    format!("mcp:{}", &fingerprint[..24.min(fingerprint.len())])
}

fn endpoint_fingerprint(definition: &McpCanonicalDefinition) -> String {
    cocli_driver_core::mcp_definition_fingerprint(definition)
}

fn sha256_text(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    hex(hasher.finalize().as_slice())
}

fn sanitize_schema_hash(value: &str) -> String {
    if (16..=128).contains(&value.len()) && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        value.to_ascii_lowercase()
    } else {
        sha256_text(value)
    }
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(DIGITS[(byte >> 4) as usize] as char);
        output.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    output
}

fn evidence(
    source: &str,
    detail: &str,
    source_path: Option<&Path>,
    proves_runtime_loaded: bool,
    proves_current_session_visibility: bool,
) -> McpEvidence {
    McpEvidence {
        source: source.to_owned(),
        detail: detail.to_owned(),
        source_path: source_path.map(|path| path.display().to_string()),
        proves_runtime_loaded,
        proves_current_session_visibility,
    }
}

fn diagnostic(
    code: &str,
    severity: McpDiagnosticSeverity,
    runtime: &str,
    server_id: Option<String>,
    message: impl Into<String>,
    evidence: Vec<McpEvidence>,
    observed_at: &str,
) -> McpDiagnostic {
    McpDiagnostic {
        code: code.to_owned(),
        severity,
        runtime: runtime.to_owned(),
        server_id,
        message: message.into(),
        evidence,
        observed_at: observed_at.to_owned(),
    }
}

fn timestamp() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn redact_args(args: &[String]) -> Vec<String> {
    let mut redacted = Vec::with_capacity(args.len());
    let mut redact_next = false;
    for arg in args {
        if redact_next {
            redacted.push("<redacted>".to_owned());
            redact_next = false;
            continue;
        }
        if let Some((key, value)) = arg.split_once('=') {
            if secret_like(key) {
                redacted.push(format!("{key}=<redacted>"));
            } else {
                redacted.push(format!("{key}={}", redact_url(value)));
            }
        } else if secret_like(arg) {
            if arg.starts_with("--") {
                redacted.push(arg.clone());
                redact_next = true;
            } else {
                redacted.push("<redacted>".to_owned());
            }
        } else {
            redacted.push(redact_url(arg));
        }
    }
    redacted
}

fn redact_text(text: &str) -> String {
    if secret_like(text) {
        "<redacted>".to_owned()
    } else {
        redact_url(text)
    }
}

fn redact_url(text: &str) -> String {
    let (base, query) = text.split_once('?').unwrap_or((text, ""));
    let base = if let Some((scheme, remainder)) = base.split_once("://") {
        if let Some((_, host)) = remainder.rsplit_once('@') {
            format!("{scheme}://<redacted>@{host}")
        } else {
            base.to_owned()
        }
    } else {
        base.to_owned()
    };
    if query.is_empty() {
        return base;
    }
    let redacted_query = query
        .split('&')
        .map(|part| {
            let Some((key, value)) = part.split_once('=') else {
                return part.to_owned();
            };
            if secret_like(key) {
                format!("{key}=<redacted>")
            } else {
                format!("{key}={value}")
            }
        })
        .collect::<Vec<_>>()
        .join("&");
    format!("{base}?{redacted_query}")
}

fn secret_refs<'a>(alias: &str, keys: impl Iterator<Item = &'a String>) -> Vec<McpSecretRef> {
    keys.filter(|key| secret_like(key))
        .map(|key| McpSecretRef {
            location: format!("mcpServers.{alias}.env"),
            kind: "env".to_owned(),
            reference: key.clone(),
        })
        .collect()
}

fn secret_like(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("token")
        || lower.contains("secret")
        || lower.contains("api_key")
        || lower.contains("api-key")
        || lower.contains("apikey")
        || lower.contains("access_key")
        || lower.contains("access-key")
        || lower.contains("client-secret")
        || lower.contains("password")
        || lower.contains("authorization")
        || lower.contains("bearer")
        || lower.starts_with("sk-")
        || lower.starts_with("sk_")
        || lower.starts_with("ghp_")
        || lower.starts_with("github_pat_")
        || lower.starts_with("xox")
        || lower.starts_with("eyj")
}

fn looks_unauthorized(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("unauthorized")
        || lower.contains("not authenticated")
        || lower.contains("permission denied")
        || lower.contains("login")
}

#[cfg(test)]
mod tests {
    use super::*;
    use cocli_driver_core::{
        McpBindingTargetType, McpDesiredServer, McpEffectiveServer, McpStateSummary,
    };
    use std::collections::VecDeque;
    use std::sync::Mutex;

    #[test]
    fn json_config_is_canonicalized_and_redacted() {
        let parsed = parse_json_servers(
            r#"{
              "mcpServers": {
                "docs": {
                  "command": "/bin/server",
                  "args": ["--auth-token", "super-secret", "--url=https://x.test/path?api_key=secret&ok=1", "--api-key", "hyphen-secret"],
                  "env": {"API_KEY": "secret"}
                }
              }
            }"#,
        )
        .expect("parse json");

        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].alias, "docs");
        assert_eq!(parsed[0].definition.args[1], "<redacted>");
        assert_eq!(
            parsed[0].definition.args[2],
            "--url=https://x.test/path?api_key=<redacted>&ok=1"
        );
        assert_eq!(parsed[0].secret_refs[0].reference, "API_KEY");
        assert_eq!(parsed[0].definition.args[4], "<redacted>");
        assert_eq!(
            redact_url("https://user:password@example.test/mcp?token=value&ok=1"),
            "https://<redacted>@example.test/mcp?token=<redacted>&ok=1"
        );
        assert_ne!(
            sanitize_schema_hash("not-a-hash-secret"),
            "not-a-hash-secret"
        );

        let snapshot = snapshot_config(
            "cursor",
            Path::new("/tmp/mcp.json"),
            Some(Path::new("/tmp")),
            parsed,
            "2026-07-19T00:00:00Z",
        );
        assert_eq!(snapshot.bindings[0].workspace.as_deref(), Some("/tmp"));
        assert_eq!(snapshot.bindings[0].profile.as_deref(), Some("workspace"));
        let response = serde_json::to_string(&McpInventory {
            servers: snapshot.servers,
            bindings: snapshot.bindings,
            observations: snapshot.observations,
            diagnostics: snapshot.diagnostics,
            observed_at: "2026-07-19T00:00:00Z".to_owned(),
        })
        .expect("serialize inventory");
        assert!(!response.contains("super-secret"));
        assert!(!response.contains("api_key=secret"));
        assert!(!response.contains("hyphen-secret"));
        assert!(response.contains("mcp_plaintext_secret"));
    }

    fn test_action(kind: McpPlanActionKind, path: &Path) -> McpPlanAction {
        McpPlanAction {
            kind,
            runtime: "cursor".to_owned(),
            scope: "machine".to_owned(),
            target: "machine:test".to_owned(),
            server_id: "mcp:test".to_owned(),
            server_fingerprint: "test".to_owned(),
            before: McpStateSummary {
                configured: Some(false),
                enabled: Some(false),
                endpoint_fingerprint: None,
                allow_tools: Vec::new(),
                deny_tools: Vec::new(),
                approval_mode: None,
                secret_ref_count: 0,
            },
            after: McpStateSummary {
                configured: Some(true),
                enabled: Some(true),
                endpoint_fingerprint: None,
                allow_tools: Vec::new(),
                deny_tools: Vec::new(),
                approval_mode: Some(McpApprovalMode::Manual),
                secret_ref_count: 0,
            },
            risk: McpRiskLevel::Medium,
            reason: "test".to_owned(),
            evidence: vec![evidence("config", "test config", Some(path), false, false)],
            expected_source_hash: Some("test".to_owned()),
            expected_schema_hash: None,
            blocked: false,
        }
    }

    fn test_desired() -> McpEffectiveServer {
        McpEffectiveServer {
            desired: McpDesiredServer {
                server_id: "mcp:test".to_owned(),
                runtime: "cursor".to_owned(),
                alias: "docs".to_owned(),
                definition: Some(McpCanonicalDefinition {
                    transport: McpTransport::Stdio,
                    command: Some("/bin/docs".to_owned()),
                    args: vec!["--safe".to_owned()],
                    endpoint: None,
                }),
                desired_enabled: true,
                allow_tools: Vec::new(),
                deny_tools: Vec::new(),
                approval_mode: McpApprovalMode::Manual,
                risk_override: None,
                secret_refs: Vec::new(),
            },
            source_profile_ids: vec!["profile".to_owned()],
            source_profile_names: vec!["development".to_owned()],
            inherited_from: McpBindingTargetType::Machine,
            high_risk_context: false,
        }
    }

    #[tokio::test]
    async fn json_writer_preserves_unrelated_config_and_rollback_restores_original() {
        let temp = tempfile::tempdir().expect("temp dir");
        let path = temp.path().join(".cursor/mcp.json");
        tokio::fs::create_dir_all(path.parent().expect("parent"))
            .await
            .expect("create parent");
        let original = br#"{"editor":{"theme":"dark"},"mcpServers":{}}"#;
        tokio::fs::write(&path, original)
            .await
            .expect("seed config");

        let action = test_action(McpPlanActionKind::AddConfigure, &path);
        let desired = test_desired();
        let (backup, _, _) =
            mutate_json_config(&path, &temp.path().join("backups"), &action, Some(&desired))
                .await
                .expect("apply config");
        let applied: Value =
            serde_json::from_slice(&tokio::fs::read(&path).await.expect("read applied config"))
                .expect("parse applied config");
        assert_eq!(applied["editor"]["theme"], "dark");
        assert_eq!(applied["mcpServers"]["docs"]["command"], "/bin/docs");

        let rolled_back = rollback_backup(&backup).await;
        assert_eq!(rolled_back.0, McpApplyActionStatus::RolledBack);
        assert_eq!(
            tokio::fs::read(&path).await.expect("read restored config"),
            original
        );
    }

    #[tokio::test]
    async fn json_writer_rejects_compare_and_swap_drift_without_mutation() {
        let temp = tempfile::tempdir().expect("temp dir");
        let path = temp.path().join(".cursor/mcp.json");
        tokio::fs::create_dir_all(path.parent().expect("parent"))
            .await
            .expect("create parent");
        let original = br#"{"mcpServers":{"docs":{"command":"/bin/changed"}}}"#;
        tokio::fs::write(&path, original)
            .await
            .expect("seed config");
        let mut action = test_action(McpPlanActionKind::Update, &path);
        action.before.endpoint_fingerprint = Some("planned-fingerprint".to_owned());

        let error = mutate_json_config(
            &path,
            &temp.path().join("backups"),
            &action,
            Some(&test_desired()),
        )
        .await
        .expect_err("CAS drift must block");
        assert!(error.contains("CAS rejected"));
        assert_eq!(
            tokio::fs::read(&path).await.expect("read unchanged config"),
            original
        );
        assert!(!temp.path().join("backups").exists());
    }

    #[tokio::test]
    async fn rollback_cas_preserves_configuration_changed_after_apply() {
        let temp = tempfile::tempdir().expect("temp dir");
        let path = temp.path().join(".cursor/mcp.json");
        tokio::fs::create_dir_all(path.parent().expect("parent"))
            .await
            .expect("create parent");
        tokio::fs::write(&path, br#"{"mcpServers":{}}"#)
            .await
            .expect("seed config");
        let (backup, _, _) = mutate_json_config(
            &path,
            &temp.path().join("backups"),
            &test_action(McpPlanActionKind::AddConfigure, &path),
            Some(&test_desired()),
        )
        .await
        .expect("apply config");
        let external = br#"{"mcpServers":{},"externalEdit":true}"#;
        tokio::fs::write(&path, external)
            .await
            .expect("external edit");

        let result = rollback_backup(&backup).await;
        assert_eq!(result.0, McpApplyActionStatus::Blocked);
        assert!(result.1.contains("rollback CAS rejected"));
        assert_eq!(
            tokio::fs::read(&path).await.expect("read preserved edit"),
            external
        );
    }

    #[tokio::test]
    async fn add_configure_cas_never_overwrites_alias_created_after_planning() {
        let temp = tempfile::tempdir().expect("temp dir");
        let path = temp.path().join(".cursor/mcp.json");
        tokio::fs::create_dir_all(path.parent().expect("parent"))
            .await
            .expect("create parent");
        let external = br#"{"mcpServers":{"docs":{"command":"/bin/external"}}}"#;
        tokio::fs::write(&path, external)
            .await
            .expect("external alias");

        let error = mutate_json_config(
            &path,
            &temp.path().join("backups"),
            &test_action(McpPlanActionKind::AddConfigure, &path),
            Some(&test_desired()),
        )
        .await
        .expect_err("late alias must reject add/configure");
        assert!(error.contains("alias appeared after planning"));
        assert_eq!(
            tokio::fs::read(&path).await.expect("read preserved alias"),
            external
        );
        assert!(!temp.path().join("backups").exists());
    }

    #[tokio::test]
    async fn json_writer_refuses_to_back_up_inline_secret_material() {
        let temp = tempfile::tempdir().expect("temp dir");
        let path = temp.path().join(".cursor/mcp.json");
        tokio::fs::create_dir_all(path.parent().expect("parent"))
            .await
            .expect("create parent");
        let original = br#"{"mcpServers":{"docs":{"command":"/bin/docs","env":{"API_TOKEN":"must-not-persist"}}}}"#;
        tokio::fs::write(&path, original)
            .await
            .expect("seed config");

        let error = mutate_json_config(
            &path,
            &temp.path().join("backups"),
            &test_action(McpPlanActionKind::AddConfigure, &path),
            Some(&test_desired()),
        )
        .await
        .expect_err("plaintext secrets must block backup and apply");
        assert_eq!(
            error,
            "source configuration contains inline credential material; backup and apply are blocked"
        );
        assert_eq!(
            tokio::fs::read(&path).await.expect("read unchanged config"),
            original
        );
        assert!(!temp.path().join("backups").exists());
    }

    #[tokio::test]
    async fn apply_lock_recovers_stale_crash_marker_but_preserves_live_lock() {
        let temp = tempfile::tempdir().expect("temp dir");
        let lock_path = temp.path().join("config.cocli-mcp.lock");
        tokio::fs::write(
            &lock_path,
            (Utc::now() - chrono::Duration::minutes(20)).to_rfc3339(),
        )
        .await
        .expect("write stale lock");
        acquire_apply_lock(&lock_path)
            .await
            .expect("stale lock should recover");
        assert!(acquire_apply_lock(&lock_path).await.is_err());
    }

    #[test]
    fn native_probe_can_report_current_session_visibility_explicitly() {
        let observations = observations_from_json_probe(
            "codex",
            &serde_json::json!({
                "servers": {
                    "docs": {
                        "configured": true,
                        "loaded": true,
                        "currentSessionVisible": false,
                        "invoked": true
                    }
                }
            }),
            "2026-07-19T00:00:00Z",
            true,
        );

        assert_eq!(observations[0].current_session_visible, Some(false));
        assert_eq!(observations[0].invoked, Some(true));
        assert!(observations[0].evidence[0].proves_current_session_visibility);
    }

    #[test]
    fn duplicate_endpoint_is_reported_after_canonical_merge() {
        let observed_at = "2026-07-19T00:00:00Z";
        let definition = ServerDefinition {
            alias: "docs".to_owned(),
            definition: McpCanonicalDefinition {
                transport: McpTransport::Http,
                command: None,
                args: Vec::new(),
                endpoint: Some("https://example.test/mcp".to_owned()),
            },
            desired_enabled: Some(true),
            policy: None,
            secret_refs: Vec::new(),
            plaintext_secret: false,
        };
        let mut alternate = definition.clone();
        "documentation".clone_into(&mut alternate.alias);
        let mut aggregate = Aggregate::new(observed_at.to_owned());
        aggregate.extend(snapshot_config(
            "codex",
            Path::new("/tmp/codex.toml"),
            None,
            vec![definition],
            observed_at,
        ));
        aggregate.extend(snapshot_config(
            "cursor",
            Path::new("/tmp/cursor.json"),
            Some(Path::new("/tmp")),
            vec![alternate],
            observed_at,
        ));

        let inventory = aggregate.finalize();
        assert_eq!(inventory.servers.len(), 1);
        assert_eq!(inventory.servers[0].aliases.len(), 2);
        assert!(inventory
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "mcp_duplicate_endpoint"));
    }

    #[test]
    fn toml_config_discovers_codex_style_servers() {
        let parsed = parse_toml_servers(
            r#"
            [mcp_servers.docs]
            command = "/bin/docs"
            args = ["--token", "abc", "--safe"]
            enabled = true
            "#,
        )
        .expect("parse toml");

        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].alias, "docs");
        assert_eq!(parsed[0].definition.command.as_deref(), Some("/bin/docs"));
        assert_eq!(
            parsed[0].definition.args,
            vec!["--token", "<redacted>", "--safe"]
        );
        assert_eq!(parsed[0].desired_enabled, Some(true));
    }

    #[test]
    fn duplicate_alias_and_state_diagnostics_are_structured() {
        let observed_at = "2026-07-19T00:00:00Z".to_owned();
        let mut aggregate = Aggregate::new(observed_at.clone());
        aggregate.observations.push(ObservedMcpInstance {
            runtime: "codex".to_owned(),
            server_id: "one".to_owned(),
            alias: "docs".to_owned(),
            source_path: None,
            discoverable: true,
            configured: true,
            loaded: Some(false),
            enabled: Some(true),
            approved: Some(false),
            authenticated: None,
            healthy: Some(true),
            startup: Some(McpStartupState::Ready),
            current_session_visible: None,
            invoked: None,
            tool_count: None,
            schema_hash: None,
            evidence: Vec::new(),
            observed_at: observed_at.clone(),
        });
        aggregate.observations.push(ObservedMcpInstance {
            runtime: "codex".to_owned(),
            server_id: "two".to_owned(),
            alias: "docs".to_owned(),
            source_path: None,
            discoverable: true,
            configured: true,
            loaded: Some(true),
            enabled: Some(true),
            approved: Some(true),
            authenticated: Some(true),
            healthy: Some(true),
            startup: Some(McpStartupState::Ready),
            current_session_visible: None,
            invoked: None,
            tool_count: None,
            schema_hash: None,
            evidence: Vec::new(),
            observed_at,
        });

        let inventory = aggregate.finalize();
        assert!(inventory
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "mcp_duplicate_alias"));
        assert!(inventory
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "mcp_not_approved"));
    }

    #[derive(Default)]
    struct FakeRunner {
        outcomes: Mutex<HashMap<String, VecDeque<CommandOutcome>>>,
        calls: Mutex<Vec<String>>,
    }

    impl FakeRunner {
        fn with(mut self, args: &[&str], outcome: CommandOutcome) -> Self {
            self.outcomes
                .get_mut()
                .expect("outcomes mutex")
                .entry(args.join(" "))
                .or_default()
                .push_back(outcome);
            self
        }

        fn calls(&self) -> Vec<String> {
            self.calls.lock().expect("calls mutex").clone()
        }
    }

    #[async_trait::async_trait]
    impl CommandRunner for FakeRunner {
        async fn run(
            &self,
            _binary: &Path,
            args: &[&str],
            _workspace: &Path,
            _timeout: Duration,
        ) -> CommandOutcome {
            let key = args.join(" ");
            self.calls.lock().expect("calls mutex").push(key.clone());
            self.outcomes
                .lock()
                .expect("outcomes mutex")
                .get_mut(&key)
                .and_then(VecDeque::pop_front)
                .unwrap_or(CommandOutcome::Missing)
        }
    }

    fn runtime_info(name: &str) -> RuntimeInfo {
        RuntimeInfo {
            name: name.to_owned(),
            installed: true,
            binary: Some("/bin/runtime".to_owned()),
            version: None,
            models: Vec::new(),
            capabilities: Vec::new(),
            unavailable_reason: None,
        }
    }

    #[tokio::test]
    async fn runner_covers_missing_timeout_bad_json_and_partial_success() {
        let runner = FakeRunner::default()
            .with(&["mcp", "list"], CommandOutcome::Timeout)
            .with(
                &["mcp", "list", "--json"],
                CommandOutcome::Output(CommandOutput {
                    success: true,
                    stdout: "not-json".to_owned(),
                    stderr: String::new(),
                }),
            )
            .with(
                &["mcp", "doctor", "--json"],
                CommandOutcome::Output(CommandOutput {
                    success: true,
                    stdout: r#"{"servers":{"docs":{"loaded":true,"healthy":true}}}"#.to_owned(),
                    stderr: String::new(),
                }),
            );
        let observed_at = "2026-07-19T00:00:00Z";
        let snapshots = run_probes(
            &[runtime_info("cursor"), runtime_info("grok")],
            &["cursor".to_owned(), "claude".to_owned(), "grok".to_owned()],
            Path::new("/tmp"),
            observed_at,
            &runner,
        )
        .await;
        let mut aggregate = Aggregate::new(observed_at.to_owned());
        for snapshot in snapshots {
            aggregate.extend(snapshot);
        }
        let inventory = aggregate.finalize();

        assert!(inventory
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "mcp_probe_command_missing"
                && diagnostic.runtime == "claude"));
        assert!(inventory
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "mcp_probe_timeout"
                && diagnostic.runtime == "cursor"));
        assert!(inventory
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "mcp_probe_bad_json"
                && diagnostic.runtime == "grok"));
        assert!(inventory
            .observations
            .iter()
            .any(|observation| observation.runtime == "grok" && observation.alias == "docs"));
    }

    #[tokio::test]
    async fn text_runtime_detail_and_doctor_routes_are_called() {
        let runner = FakeRunner::default()
            .with(
                &["mcp", "list"],
                CommandOutcome::Output(CommandOutput {
                    success: true,
                    stdout: "docs\n".to_owned(),
                    stderr: String::new(),
                }),
            )
            .with(
                &["mcp", "list-tools", "docs"],
                CommandOutcome::Output(CommandOutput {
                    success: true,
                    stdout: "tool_a\n".to_owned(),
                    stderr: String::new(),
                }),
            );

        let _ = probe_runtime(
            "cursor",
            Some(Path::new("/bin/cursor-agent")),
            Path::new("/tmp"),
            "2026-07-19T00:00:00Z",
            &runner,
        )
        .await;
        assert_eq!(runner.calls(), vec!["mcp list", "mcp list-tools docs"]);

        assert_eq!(
            detail_probe_args("claude", "docs").expect("claude args"),
            vec!["mcp", "get", "docs"]
        );
        assert_eq!(
            detail_probe_args("cursor", "docs").expect("cursor args"),
            vec!["mcp", "list-tools", "docs"]
        );
        assert_eq!(probe_args("grok"), ["mcp", "list", "--json"]);
    }
}
