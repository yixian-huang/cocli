use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::Utc;
use cocli_api::{McpApplyJournalSink, RuntimeInfo};
use cocli_driver_core::{
    hash_mcp_capabilities, hash_mcp_observation, McpApplyActionResult, McpApplyActionStatus,
    McpApplyExecutionRequest, McpApplyExecutionResult, McpApplyJournalEntry, McpApplyJournalPhase,
    McpApprovalMode, McpBackupDescriptor, McpBinding, McpCanonicalDefinition, McpCapabilityDetail,
    McpCapabilityOperation, McpCapabilitySnapshot, McpCapabilitySupport, McpDiagnostic,
    McpDiagnosticSeverity, McpEvidence, McpInventory, McpPlan, McpPlanAction, McpPlanActionKind,
    McpPreflightAction, McpPreflightReport, McpReloadResult, McpReloadStatus, McpReloadStrategy,
    McpRiskLevel, McpRollbackExecutionRequest, McpRollbackExecutionResult, McpRuntimeCapability,
    McpSecretRef, McpServer, McpSessionEffectiveStatus, McpStartupState, McpTransport,
    McpVerificationResult, McpVerificationStatus, ObservedMcpInstance,
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

/// Execution-boundary secret resolver. Implementations receive only approved
/// opaque references; resolved bytes are neither serializable nor printable
/// and are zeroed when dropped.
#[async_trait::async_trait]
pub(crate) trait SecretResolver: Send + Sync {
    async fn resolve(&self, reference: &McpSecretRef) -> Result<ResolvedSecret, String>;
}

pub(crate) struct EnvironmentSecretResolver;

#[async_trait::async_trait]
impl SecretResolver for EnvironmentSecretResolver {
    async fn resolve(&self, reference: &McpSecretRef) -> Result<ResolvedSecret, String> {
        let Some(name) = reference.reference.strip_prefix("env://") else {
            return Err(
                "secret reference requires an unavailable secure keychain resolver".to_owned(),
            );
        };
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
        {
            return Err("environment secret reference is invalid".to_owned());
        }
        let value = std::env::var_os(name)
            .ok_or_else(|| "environment secret reference is unavailable".to_owned())?;
        Ok(ResolvedSecret(value.to_string_lossy().as_bytes().to_vec()))
    }
}

pub(crate) struct ResolvedSecret(Vec<u8>);

impl ResolvedSecret {
    pub(crate) fn expose(&self) -> &[u8] {
        &self.0
    }
}

impl Drop for ResolvedSecret {
    fn drop(&mut self) {
        self.0.fill(0);
    }
}

impl std::fmt::Debug for ResolvedSecret {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ResolvedSecret([REDACTED])")
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

pub async fn capabilities(
    catalog: &[RuntimeInfo],
    config: &LocalRuntimeConfig,
) -> McpCapabilitySnapshot {
    capabilities_with_runner(catalog, config, &SystemCommandRunner).await
}

async fn capabilities_with_runner(
    catalog: &[RuntimeInfo],
    config: &LocalRuntimeConfig,
    runner: &dyn CommandRunner,
) -> McpCapabilitySnapshot {
    let mut runtimes = Vec::new();
    for runtime in target_runtimes(catalog) {
        runtimes.push(runtime_capability(catalog, config, &runtime, runner).await);
    }
    runtimes.sort_by(|left, right| left.runtime.cmp(&right.runtime));
    let mut snapshot = McpCapabilitySnapshot {
        hash: String::new(),
        observed_at: timestamp(),
        runtimes,
    };
    snapshot.hash = hash_mcp_capabilities(&snapshot);
    snapshot
}

pub async fn preflight(
    catalog: &[RuntimeInfo],
    config: &LocalRuntimeConfig,
    plan: &McpPlan,
) -> McpPreflightReport {
    let snapshot = capabilities(catalog, config).await;
    let inventory = inspect(catalog, config).await;
    let mut stale_reasons = Vec::new();
    if snapshot.hash != plan.capability_hash {
        stale_reasons.push("adapter_capability_or_version_drift".to_owned());
    }
    if hash_mcp_observation(&inventory) != plan.observation_hash {
        stale_reasons.push("observation_drift".to_owned());
    }
    let mut actions = plan
        .actions
        .iter()
        .enumerate()
        .map(|(action_index, action)| preflight_action(plan, &snapshot, action_index, action))
        .collect::<Vec<_>>();
    actions.sort_by_key(|action| action.action_index);
    let executable = stale_reasons.is_empty()
        && actions
            .iter()
            .filter(|action| action.executable)
            .all(|action| action.support == McpCapabilitySupport::Supported);
    McpPreflightReport {
        plan_id: plan.id.clone(),
        plan_hash: plan.plan_hash.clone(),
        capability_hash: snapshot.hash,
        observation_hash: hash_mcp_observation(&inventory),
        config_hash: plan.config_hash.clone(),
        actions,
        stale_reasons,
        executable,
    }
}

async fn runtime_capability(
    catalog: &[RuntimeInfo],
    config: &LocalRuntimeConfig,
    runtime: &str,
    runner: &dyn CommandRunner,
) -> McpRuntimeCapability {
    let info = catalog.iter().find(|entry| entry.name == runtime);
    let binary_path = info.and_then(|entry| entry.binary.clone());
    let binary_version = info.and_then(|entry| entry.version.clone());
    let installed = binary_path.is_some();
    let evidence = |detail: &str| {
        vec![McpEvidence {
            source: "adapter_capability_probe".to_owned(),
            detail: detail.to_owned(),
            source_path: binary_path.clone(),
            proves_runtime_loaded: false,
            proves_current_session_visibility: false,
        }]
    };
    let detail = |support, reason: &str| McpCapabilityDetail {
        support,
        reason: reason.to_owned(),
        evidence: evidence(reason),
    };
    let codex_writer_contract = if runtime == "codex" {
        match binary_path.as_deref() {
            Some(binary) => {
                let add = runner
                    .run(
                        Path::new(binary),
                        &["mcp", "add", "--help"],
                        &config.workspace_root,
                        PROBE_TIMEOUT,
                    )
                    .await;
                let remove = runner
                    .run(
                        Path::new(binary),
                        &["mcp", "remove", "--help"],
                        &config.workspace_root,
                        PROBE_TIMEOUT,
                    )
                    .await;
                if command_help_proves(&add, "add") && command_help_proves(&remove, "remove") {
                    (
                        McpCapabilitySupport::Supported,
                        "native CLI independently proves bounded mcp add and remove contracts",
                    )
                } else if matches!(add, CommandOutcome::Missing)
                    || matches!(remove, CommandOutcome::Missing)
                {
                    (
                        McpCapabilitySupport::Unsupported,
                        "native CLI disappeared during capability negotiation",
                    )
                } else {
                    (
                        McpCapabilitySupport::Unknown,
                        "native CLI add/remove help probes were incomplete, malformed, timed out, or unsuccessful",
                    )
                }
            }
            None => (
                McpCapabilitySupport::Unsupported,
                "Codex native CLI is unavailable",
            ),
        }
    } else {
        (
            McpCapabilitySupport::Unsupported,
            "native writer contract is not applicable",
        )
    };
    let mut operations = BTreeMap::new();
    let (adapter, schema, destination, subtree, reload_strategy) = match runtime {
        "codex" => (
            if installed {
                "codex_native_cli"
            } else {
                "codex_read_only"
            },
            "codex.mcp_servers.v1",
            "$CODEX_HOME/config.toml",
            "mcp_servers",
            McpReloadStrategy::NewSessionOnly,
        ),
        "cursor" => (
            "cursor_structured_json_fallback",
            "cursor.mcpServers.v1",
            ".cursor/mcp.json",
            "mcpServers",
            McpReloadStrategy::NewSessionOnly,
        ),
        "claude" => (
            "claude_structured_json_fallback",
            "claude.mcpServers.v1",
            ".mcp.json",
            "mcpServers",
            McpReloadStrategy::NewSessionOnly,
        ),
        _ => (
            "grok_read_only",
            "grok.mcp_servers.v1",
            "$GROK_HOME/config.toml",
            "mcp_servers",
            McpReloadStrategy::Deferred,
        ),
    };
    operations.insert(
        McpCapabilityOperation::ReadDiscover,
        detail(
            if installed {
                McpCapabilitySupport::Supported
            } else {
                McpCapabilitySupport::ReadOnly
            },
            if installed {
                "native readback probe and structured discovery are available"
            } else {
                "binary is missing; only structured configuration discovery is available"
            },
        ),
    );
    let structured_writer = matches!(runtime, "cursor" | "claude");
    let write_support = if structured_writer {
        McpCapabilitySupport::Supported
    } else if runtime == "codex" {
        codex_writer_contract.0
    } else if runtime == "grok" {
        McpCapabilitySupport::ReadOnly
    } else {
        McpCapabilitySupport::Unsupported
    };
    for operation in [
        McpCapabilityOperation::AddConfigure,
        McpCapabilityOperation::Remove,
        McpCapabilityOperation::Rollback,
    ] {
        operations.insert(
            operation,
            detail(
                write_support,
                if runtime == "codex" {
                    codex_writer_contract.1
                } else if structured_writer {
                    "controlled structured fallback preserves fields outside the MCP subtree"
                } else {
                    "no transactionally safe writer is enabled for this Runtime"
                },
            ),
        );
    }
    operations.insert(
        McpCapabilityOperation::EnableDisable,
        detail(
            if structured_writer {
                McpCapabilitySupport::Supported
            } else {
                McpCapabilitySupport::Unsupported
            },
            if structured_writer {
                "structured fallback has an explicit disabled field contract"
            } else {
                "native enable/disable contract is not proven"
            },
        ),
    );
    operations.insert(
        McpCapabilityOperation::SecretReference,
        detail(
            McpCapabilitySupport::Unsupported,
            "opaque references are accepted, but this adapter has no proven non-persistent secret injection contract",
        ),
    );
    operations.insert(
        McpCapabilityOperation::Reload,
        detail(
            McpCapabilitySupport::ReadOnly,
            "configuration becomes visible to new sessions; active sessions are never restarted implicitly",
        ),
    );
    operations.insert(
        McpCapabilityOperation::Verify,
        detail(
            if installed {
                McpCapabilitySupport::Supported
            } else {
                McpCapabilitySupport::ReadOnly
            },
            if installed {
                "fresh native readback plus inventory/doctor is available"
            } else {
                "verification is limited to structured configuration readback"
            },
        ),
    );
    let destination = if destination.starts_with('.') {
        config
            .workspace_root
            .join(destination)
            .display()
            .to_string()
    } else {
        destination.to_owned()
    };
    McpRuntimeCapability {
        runtime: runtime.to_owned(),
        adapter: adapter.to_owned(),
        binary_path,
        binary_version,
        config_schema_version: schema.to_owned(),
        destination,
        allowed_subtree: subtree.to_owned(),
        reload_strategy,
        operations,
    }
}

fn command_help_proves(outcome: &CommandOutcome, subcommand: &str) -> bool {
    let CommandOutcome::Output(output) = outcome else {
        return false;
    };
    if !output.success {
        return false;
    }
    let text = format!("{}\n{}", output.stdout, output.stderr).to_ascii_lowercase();
    let tokens = text
        .split(|character: char| !character.is_ascii_alphanumeric() && character != '-')
        .filter(|token| !token.is_empty())
        .collect::<HashSet<_>>();
    tokens.contains("usage") && tokens.contains(subcommand)
}

fn preflight_action(
    plan: &McpPlan,
    snapshot: &McpCapabilitySnapshot,
    action_index: usize,
    action: &McpPlanAction,
) -> McpPreflightAction {
    let operation = match action.kind {
        McpPlanActionKind::AddConfigure | McpPlanActionKind::Update => {
            McpCapabilityOperation::AddConfigure
        }
        McpPlanActionKind::Enable | McpPlanActionKind::Disable => {
            McpCapabilityOperation::EnableDisable
        }
        McpPlanActionKind::Remove => McpCapabilityOperation::Remove,
        McpPlanActionKind::AuthenticationRequired => McpCapabilityOperation::SecretReference,
        McpPlanActionKind::ApprovalRequired | McpPlanActionKind::ManualUnsupported => {
            McpCapabilityOperation::Verify
        }
    };
    let capability = snapshot
        .runtimes
        .iter()
        .find(|item| item.runtime == action.runtime);
    let detail = capability.and_then(|item| item.operations.get(&operation));
    let mut support = detail.map_or(McpCapabilitySupport::Unknown, |item| item.support);
    if action.runtime == "codex"
        && matches!(
            action.kind,
            McpPlanActionKind::Update | McpPlanActionKind::Enable | McpPlanActionKind::Disable
        )
    {
        support = McpCapabilitySupport::Unsupported;
    }
    let executable_kind = !matches!(
        action.kind,
        McpPlanActionKind::ApprovalRequired
            | McpPlanActionKind::AuthenticationRequired
            | McpPlanActionKind::ManualUnsupported
    );
    let source_proven = authoritative_source_hash(action)
        .as_deref()
        .is_some_and(|hash| action.expected_source_hash.as_deref() == Some(hash));
    let write_requires_source = matches!(
        action.kind,
        McpPlanActionKind::AddConfigure
            | McpPlanActionKind::Enable
            | McpPlanActionKind::Disable
            | McpPlanActionKind::Update
            | McpPlanActionKind::Remove
    );
    let executable = executable_kind
        && !action.blocked
        && support == McpCapabilitySupport::Supported
        && snapshot.hash == plan.capability_hash
        && (!write_requires_source || source_proven);
    let idempotency_key = sha256_bytes(
        format!(
            "{}:{action_index}:{}:{}",
            plan.plan_hash, action.runtime, action.server_id
        )
        .as_bytes(),
    );
    McpPreflightAction {
        action_index,
        runtime: action.runtime.clone(),
        server_id: action.server_id.clone(),
        operation,
        support,
        executable,
        reason: if write_requires_source && !source_proven {
            "no authoritative configuration source hash proves a safe write destination".to_owned()
        } else if action.runtime == "codex"
            && matches!(
                action.kind,
                McpPlanActionKind::Update | McpPlanActionKind::Enable | McpPlanActionKind::Disable
            )
        {
            "Codex native adapter does not expose a transactionally safe update or enable/disable contract"
                .to_owned()
        } else if !executable_kind || action.blocked {
            "plan action requires manual handling".to_owned()
        } else {
            detail.map_or_else(
                || "adapter capability is unknown".to_owned(),
                |item| item.reason.clone(),
            )
        },
        adapter: capability.map_or_else(|| "unknown".to_owned(), |item| item.adapter.clone()),
        destination: capability.map_or_else(String::new, |item| item.destination.clone()),
        allowed_subtree: capability.map_or_else(String::new, |item| item.allowed_subtree.clone()),
        reload_strategy: capability
            .map_or(McpReloadStrategy::Unsupported, |item| item.reload_strategy),
        idempotency_key,
        expected_source_hash: action.expected_source_hash.clone(),
        expected_schema_hash: action.expected_schema_hash.clone(),
    }
}

fn authoritative_source_hash(action: &McpPlanAction) -> Option<String> {
    action
        .evidence
        .iter()
        .filter(|item| item.source == "config")
        .find_map(|item| {
            let value = item.detail.split("source_sha256=").nth(1)?;
            let hash = value.split_whitespace().next()?;
            (hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
                .then(|| hash.to_ascii_lowercase())
        })
}

pub async fn apply(
    catalog: &[RuntimeInfo],
    config: &LocalRuntimeConfig,
    request: McpApplyExecutionRequest,
    sink: &dyn McpApplyJournalSink,
) -> McpApplyExecutionResult {
    let recovering = request.resume_journal.iter().any(|entry| {
        matches!(
            entry.phase,
            McpApplyJournalPhase::BackedUp
                | McpApplyJournalPhase::Written
                | McpApplyJournalPhase::ReloadPending
                | McpApplyJournalPhase::Reloaded
                | McpApplyJournalPhase::Verified
        )
    });
    let capability_snapshot = capabilities(catalog, config).await;
    if capability_snapshot.hash != request.capability_hash
        || request.plan.capability_hash != request.capability_hash
    {
        return blocked_execution(
            &request,
            "adapter capability or binary version changed after plan approval",
        );
    }
    let before = inspect(catalog, config).await;
    let observation_drift = hash_mcp_observation(&before) != request.plan.observation_hash;
    if observation_drift && !recovering {
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
    let preflight = preflight(catalog, config, &request.plan).await;
    if preflight
        .stale_reasons
        .iter()
        .any(|reason| reason != "observation_drift" || !recovering)
    {
        return blocked_execution(&request, "adapter preflight detected stale plan inputs");
    }
    let mut actions = Vec::with_capacity(request.plan.actions.len());
    let mut journal = request.resume_journal.clone();
    let mut sequence = journal
        .iter()
        .map(|entry| entry.sequence)
        .max()
        .unwrap_or(0);
    let mut changed_runtimes = HashSet::new();
    let mut source_hashes = HashMap::<PathBuf, String>::new();
    let mut execution_actions = Vec::with_capacity(request.plan.actions.len());
    for (action_index, planned_action) in request.plan.actions.iter().enumerate() {
        let source_path = action_config_path(config, planned_action);
        let execution_action = action_with_chained_source_hash(
            planned_action,
            source_hashes.get(&source_path).map(String::as_str),
        );
        execution_actions.push(execution_action);
        let action = &execution_actions[action_index];
        let preflight_action = preflight
            .actions
            .iter()
            .find(|item| item.action_index == action_index);
        let idempotency_key = preflight_action.map_or_else(
            || sha256_bytes(format!("{}:{action_index}", request.plan.plan_hash).as_bytes()),
            |item| item.idempotency_key.clone(),
        );
        if durable_checkpoint(
            sink,
            &request.run_id,
            &mut journal,
            &mut sequence,
            action_index,
            action,
            &idempotency_key,
            McpApplyJournalPhase::Preflight,
            None,
            "adapter capability and approved hashes were revalidated",
        )
        .await
        .is_err()
        {
            actions.push(McpApplyActionResult {
                action_index,
                runtime: action.runtime.clone(),
                server_id: action.server_id.clone(),
                status: McpApplyActionStatus::Blocked,
                reason: "durable preflight checkpoint failed; no mutation was attempted".to_owned(),
                backup: None,
                before_source_hash: None,
                after_source_hash: None,
            });
            continue;
        }
        let resume_state = recover_resume_state(&request.resume_journal, &idempotency_key).await;
        let result = if let Ok(Some(entry)) = resume_state {
            if let Some(backup) = entry.backup.as_ref() {
                release_owned_apply_lock(backup, &request.run_id).await;
            }
            if entry.phase == McpApplyJournalPhase::BackedUp
                && durable_checkpoint(
                    sink,
                    &request.run_id,
                    &mut journal,
                    &mut sequence,
                    action_index,
                    action,
                    &idempotency_key,
                    McpApplyJournalPhase::Written,
                    entry.backup.clone(),
                    "source hash proves the staged mutation completed before interruption",
                )
                .await
                .is_err()
            {
                actions.push(McpApplyActionResult {
                    action_index,
                    runtime: action.runtime.clone(),
                    server_id: action.server_id.clone(),
                    status: McpApplyActionStatus::Failed,
                    reason: "applied source was recovered, but the written checkpoint still failed"
                        .to_owned(),
                    backup: entry.backup.clone(),
                    before_source_hash: entry.backup.as_ref().map(|item| item.source_hash.clone()),
                    after_source_hash: entry.backup.as_ref().map(|item| item.applied_hash.clone()),
                });
                continue;
            }
            McpApplyActionResult {
                action_index,
                runtime: action.runtime.clone(),
                server_id: action.server_id.clone(),
                status: McpApplyActionStatus::Applied,
                reason: "durable journal proves write completed; non-idempotent mutation was not repeated"
                    .to_owned(),
                backup: entry.backup.clone(),
                before_source_hash: entry.backup.as_ref().map(|item| item.source_hash.clone()),
                after_source_hash: entry.backup.as_ref().map(|item| item.applied_hash.clone()),
            }
        } else if let Err(reason) = resume_state {
            McpApplyActionResult {
                action_index,
                runtime: action.runtime.clone(),
                server_id: action.server_id.clone(),
                status: McpApplyActionStatus::Blocked,
                reason,
                backup: None,
                before_source_hash: None,
                after_source_hash: None,
            }
        } else if observation_drift {
            McpApplyActionResult {
                action_index,
                runtime: action.runtime.clone(),
                server_id: action.server_id.clone(),
                status: McpApplyActionStatus::Blocked,
                reason:
                    "observation drift blocks actions not proven written by the durable journal"
                        .to_owned(),
                backup: None,
                before_source_hash: None,
                after_source_hash: None,
            }
        } else if preflight_action.is_some_and(|item| !item.executable) {
            McpApplyActionResult {
                action_index,
                runtime: action.runtime.clone(),
                server_id: action.server_id.clone(),
                status: McpApplyActionStatus::Blocked,
                reason: preflight_action.map_or_else(
                    || "adapter preflight did not authorize execution".to_owned(),
                    |item| item.reason.clone(),
                ),
                backup: None,
                before_source_hash: None,
                after_source_hash: None,
            }
        } else {
            apply_action(
                catalog,
                config,
                &request,
                action_index,
                action,
                &backup_root,
                &idempotency_key,
                sink,
                &mut journal,
                &mut sequence,
            )
            .await
        };
        if result.status == McpApplyActionStatus::Applied {
            changed_runtimes.insert(result.runtime.clone());
            if let Some(hash) = result.after_source_hash.as_ref() {
                source_hashes.insert(source_path, hash.clone());
            }
        } else if matches!(
            result.status,
            McpApplyActionStatus::Failed | McpApplyActionStatus::Blocked
        ) {
            let _ = durable_checkpoint(
                sink,
                &request.run_id,
                &mut journal,
                &mut sequence,
                action_index,
                action,
                &idempotency_key,
                McpApplyJournalPhase::Failed,
                result.backup.clone(),
                &result.reason,
            )
            .await;
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
    for (action_index, action) in execution_actions.iter().enumerate() {
        if actions
            .get(action_index)
            .is_some_and(|result| result.status == McpApplyActionStatus::Applied)
        {
            let idempotency_key = preflight.actions[action_index].idempotency_key.clone();
            let _ = durable_checkpoint(
                sink,
                &request.run_id,
                &mut journal,
                &mut sequence,
                action_index,
                action,
                &idempotency_key,
                McpApplyJournalPhase::ReloadPending,
                actions[action_index].backup.clone(),
                "active sessions were not restarted; new session activation is required",
            )
            .await;
        }
    }
    let after = inspect(catalog, config).await;
    let verification = verify_plan(&request, &actions, &after);
    if verification.status == McpVerificationStatus::Matched {
        for action in &mut actions {
            if action.status == McpApplyActionStatus::Applied {
                action.status = McpApplyActionStatus::Verified;
                let _ = durable_checkpoint(
                    sink,
                    &request.run_id,
                    &mut journal,
                    &mut sequence,
                    action.action_index,
                    &execution_actions[action.action_index],
                    &preflight.actions[action.action_index].idempotency_key,
                    McpApplyJournalPhase::Verified,
                    action.backup.clone(),
                    "fresh inventory and structured readback match desired configuration",
                )
                .await;
            }
        }
    }
    McpApplyExecutionResult {
        actions,
        reloads,
        verification,
        journal,
    }
}

fn action_with_chained_source_hash(
    action: &McpPlanAction,
    current_source_hash: Option<&str>,
) -> McpPlanAction {
    let mut action = action.clone();
    if let Some(hash) = current_source_hash {
        action.expected_source_hash = Some(hash.to_owned());
    }
    action
}

async fn recover_resume_state(
    journal: &[McpApplyJournalEntry],
    idempotency_key: &str,
) -> Result<Option<McpApplyJournalEntry>, String> {
    if let Some(entry) = journal.iter().rev().find(|entry| {
        entry.idempotency_key == idempotency_key
            && matches!(
                entry.phase,
                McpApplyJournalPhase::Written
                    | McpApplyJournalPhase::ReloadPending
                    | McpApplyJournalPhase::Reloaded
                    | McpApplyJournalPhase::Verified
            )
    }) {
        return Ok(Some(entry.clone()));
    }
    let Some(entry) = journal.iter().rev().find(|entry| {
        entry.idempotency_key == idempotency_key && entry.phase == McpApplyJournalPhase::BackedUp
    }) else {
        return Ok(None);
    };
    let Some(backup) = entry.backup.as_ref() else {
        return Err("interrupted backup checkpoint has no recovery descriptor".to_owned());
    };
    if backup.applied_hash.is_empty() {
        return Err("interrupted backup checkpoint has no expected applied hash".to_owned());
    }
    let current = match tokio::fs::read(&backup.source_path).await {
        Ok(current) => current,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(_) => return Err("interrupted destination could not be read for recovery".to_owned()),
    };
    let current_hash = sha256_bytes(&current);
    if current_hash == backup.applied_hash {
        Ok(Some(entry.clone()))
    } else if current_hash == backup.source_hash {
        Ok(None)
    } else {
        Err("interrupted destination differs from both backup and expected applied hashes; manual recovery is required".to_owned())
    }
}

async fn release_owned_apply_lock(backup: &McpBackupDescriptor, owner: &str) {
    let source_path = Path::new(&backup.source_path);
    let lock_path = if source_path.extension().and_then(|value| value.to_str()) == Some("toml") {
        source_path.with_extension("toml.cocli-mcp.lock")
    } else {
        source_path.with_extension("json.cocli-mcp.lock")
    };
    let owned = tokio::fs::read_to_string(&lock_path)
        .await
        .ok()
        .is_some_and(|marker| marker.lines().nth(1).is_some_and(|value| value == owner));
    if owned {
        let _ = tokio::fs::remove_file(lock_path).await;
    }
}

#[allow(clippy::too_many_arguments)]
async fn durable_checkpoint(
    sink: &dyn McpApplyJournalSink,
    run_id: &str,
    journal: &mut Vec<McpApplyJournalEntry>,
    sequence: &mut u64,
    action_index: usize,
    action: &McpPlanAction,
    idempotency_key: &str,
    phase: McpApplyJournalPhase,
    backup: Option<McpBackupDescriptor>,
    reason: &str,
) -> Result<(), String> {
    if journal
        .iter()
        .any(|entry| entry.idempotency_key == idempotency_key && entry.phase == phase)
    {
        return Ok(());
    }
    let next_sequence = *sequence + 1;
    let entry = journal_entry(
        next_sequence,
        action_index,
        action,
        idempotency_key,
        phase,
        backup,
        reason,
    );
    sink.checkpoint(run_id, &entry)
        .await
        .map_err(|_| "durable apply journal checkpoint failed".to_owned())?;
    *sequence = next_sequence;
    journal.push(entry);
    Ok(())
}

fn journal_entry(
    sequence: u64,
    action_index: usize,
    action: &McpPlanAction,
    idempotency_key: &str,
    phase: McpApplyJournalPhase,
    backup: Option<McpBackupDescriptor>,
    reason: &str,
) -> McpApplyJournalEntry {
    McpApplyJournalEntry {
        sequence,
        action_index,
        runtime: action.runtime.clone(),
        server_id: action.server_id.clone(),
        idempotency_key: idempotency_key.to_owned(),
        phase,
        attempt: 1,
        expected_source_hash: action.expected_source_hash.clone(),
        expected_schema_hash: action.expected_schema_hash.clone(),
        backup,
        reason: reason.to_owned(),
        evidence: action.evidence.clone(),
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
            written_config_hashes: BTreeMap::new(),
            session_effective: McpSessionEffectiveStatus::Unknown,
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
            written_config_hashes: BTreeMap::new(),
            session_effective: McpSessionEffectiveStatus::Unknown,
        },
        journal: request.resume_journal.clone(),
    }
}

#[allow(clippy::too_many_arguments)]
async fn apply_action(
    catalog: &[RuntimeInfo],
    config: &LocalRuntimeConfig,
    request: &McpApplyExecutionRequest,
    action_index: usize,
    action: &McpPlanAction,
    backup_root: &Path,
    idempotency_key: &str,
    sink: &dyn McpApplyJournalSink,
    journal: &mut Vec<McpApplyJournalEntry>,
    sequence: &mut u64,
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
    if !matches!(action.runtime.as_str(), "codex" | "cursor" | "claude") {
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
        let resolver = EnvironmentSecretResolver;
        for reference in &desired
            .expect("desired was checked above")
            .desired
            .secret_refs
        {
            match resolver.resolve(reference).await {
                Ok(secret) if !secret.expose().is_empty() => {}
                Ok(_) => {
                    "secret reference resolved to an empty value; action is blocked"
                        .clone_into(&mut result.reason);
                    return result;
                }
                Err(reason) => {
                    result.reason = reason;
                    return result;
                }
            }
        }
        "secret reference resolved only at execution boundary, but this Runtime adapter has no proven non-persistent injection contract; action is blocked"
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

    if action.runtime == "codex" {
        return apply_codex_native(
            catalog,
            config,
            &request.run_id,
            action_index,
            action,
            desired,
            backup_root,
            idempotency_key,
            sink,
            journal,
            sequence,
        )
        .await;
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
    if acquire_apply_lock(&lock_path, &request.run_id)
        .await
        .is_err()
    {
        "configuration is locked by another apply operation".clone_into(&mut result.reason);
        return result;
    }
    if durable_checkpoint(
        sink,
        &request.run_id,
        journal,
        sequence,
        action_index,
        action,
        idempotency_key,
        McpApplyJournalPhase::Locked,
        None,
        "exclusive destination lock was acquired",
    )
    .await
    .is_err()
    {
        let _ = tokio::fs::remove_file(&lock_path).await;
        "durable lock checkpoint failed; no configuration write was attempted"
            .clone_into(&mut result.reason);
        return result;
    }

    let outcome = mutate_json_config(
        &path,
        backup_root,
        &request.run_id,
        action_index,
        action,
        desired,
        idempotency_key,
        sink,
        journal,
        sequence,
    )
    .await;
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
        Err(reason) => {
            if let Some(backup) = journal
                .iter()
                .rev()
                .find(|entry| {
                    entry.idempotency_key == idempotency_key
                        && entry.phase == McpApplyJournalPhase::BackedUp
                })
                .and_then(|entry| entry.backup.clone())
            {
                result.before_source_hash = Some(backup.source_hash.clone());
                result.after_source_hash = Some(backup.applied_hash.clone());
                result.backup = Some(backup);
            }
            if reason.contains("recovery is required") {
                result.status = McpApplyActionStatus::Failed;
            }
            result.reason = reason;
        }
    }
    result
}

#[allow(clippy::too_many_arguments)]
async fn apply_codex_native(
    catalog: &[RuntimeInfo],
    config: &LocalRuntimeConfig,
    run_id: &str,
    action_index: usize,
    action: &McpPlanAction,
    desired: Option<&cocli_driver_core::McpEffectiveServer>,
    backup_root: &Path,
    idempotency_key: &str,
    sink: &dyn McpApplyJournalSink,
    journal: &mut Vec<McpApplyJournalEntry>,
    sequence: &mut u64,
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
    if !matches!(
        action.kind,
        McpPlanActionKind::AddConfigure | McpPlanActionKind::Remove
    ) {
        "Codex native adapter only enables transactionally bounded add and remove"
            .clone_into(&mut result.reason);
        return result;
    }
    let Some(binary) = catalog
        .iter()
        .find(|entry| entry.name == "codex")
        .and_then(|entry| entry.binary.as_deref())
    else {
        "Codex native CLI is unavailable".clone_into(&mut result.reason);
        return result;
    };
    let path = action_config_path(config, action);
    let Some(codex_home) = path.parent() else {
        "Codex configuration destination is invalid".clone_into(&mut result.reason);
        return result;
    };
    if tokio::fs::create_dir_all(codex_home).await.is_err() {
        "Codex configuration destination could not be prepared".clone_into(&mut result.reason);
        return result;
    }
    let source = match tokio::fs::read(&path).await {
        Ok(source) => source,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(_) => {
            "Codex configuration could not be read".clone_into(&mut result.reason);
            return result;
        }
    };
    if source.len() as u64 > MAX_CONFIG_BYTES || toml_contains_secret_material(&source) {
        "Codex source is too large or contains inline credential material"
            .clone_into(&mut result.reason);
        return result;
    }
    let before_hash = sha256_bytes(&source);
    let Some(expected_source_hash) = action.expected_source_hash.as_deref() else {
        "Codex plan has no authoritative source evidence; native apply is blocked"
            .clone_into(&mut result.reason);
        return result;
    };
    if expected_source_hash != before_hash {
        "Codex source hash changed after planning; native apply CAS rejected the write"
            .clone_into(&mut result.reason);
        return result;
    }
    let lock_path = path.with_extension("toml.cocli-mcp.lock");
    if acquire_apply_lock(&lock_path, run_id).await.is_err() {
        "Codex configuration is locked by another apply operation".clone_into(&mut result.reason);
        return result;
    }
    if durable_checkpoint(
        sink,
        run_id,
        journal,
        sequence,
        action_index,
        action,
        idempotency_key,
        McpApplyJournalPhase::Locked,
        None,
        "exclusive destination lock was acquired",
    )
    .await
    .is_err()
    {
        let _ = tokio::fs::remove_file(&lock_path).await;
        "durable lock checkpoint failed; no native command was attempted"
            .clone_into(&mut result.reason);
        return result;
    }
    let source_existed = tokio::fs::metadata(&path).await.is_ok();
    let backup = match write_backup(backup_root, &path, &source, source_existed, &before_hash).await
    {
        Ok(backup) => backup,
        Err(reason) => {
            let _ = tokio::fs::remove_file(&lock_path).await;
            result.reason = reason;
            return result;
        }
    };
    let alias = desired
        .map(|server| server.desired.alias.clone())
        .or_else(|| {
            std::str::from_utf8(&source)
                .ok()
                .and_then(|text| parse_toml_servers(text).ok())
                .and_then(|servers| {
                    servers.into_iter().find_map(|server| {
                        let fingerprint = endpoint_fingerprint(&server.definition);
                        (fingerprint == action.server_fingerprint
                            || action.before.endpoint_fingerprint.as_deref()
                                == Some(fingerprint.as_str()))
                        .then_some(server.alias)
                    })
                })
        });
    let Some(alias) = alias else {
        let _ = tokio::fs::remove_file(&lock_path).await;
        "Codex server alias could not be proven from the approved source"
            .clone_into(&mut result.reason);
        return result;
    };
    if alias.is_empty()
        || !alias
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        let _ = tokio::fs::remove_file(&lock_path).await;
        "Codex server alias cannot be isolated safely by the native adapter"
            .clone_into(&mut result.reason);
        return result;
    }
    let args = match action.kind {
        McpPlanActionKind::Remove => vec!["mcp".to_owned(), "remove".to_owned(), alias.clone()],
        McpPlanActionKind::AddConfigure => {
            let Some(definition) = desired.and_then(|server| server.desired.definition.as_ref())
            else {
                let _ = tokio::fs::remove_file(&lock_path).await;
                "Codex desired definition is unavailable".clone_into(&mut result.reason);
                return result;
            };
            if let Some(endpoint) = definition.endpoint.as_ref() {
                vec![
                    "mcp".to_owned(),
                    "add".to_owned(),
                    alias.clone(),
                    "--url".to_owned(),
                    endpoint.clone(),
                ]
            } else if let Some(command) = definition.command.as_ref() {
                let mut args = vec![
                    "mcp".to_owned(),
                    "add".to_owned(),
                    alias.clone(),
                    "--".to_owned(),
                    command.clone(),
                ];
                args.extend(definition.args.clone());
                args
            } else {
                let _ = tokio::fs::remove_file(&lock_path).await;
                "Codex desired definition has no supported endpoint".clone_into(&mut result.reason);
                return result;
            }
        }
        _ => unreachable!(),
    };
    let staging_home = backup_root.join(format!("codex-native-stage-{}", Uuid::new_v4()));
    if tokio::fs::create_dir_all(&staging_home).await.is_err() {
        let _ = tokio::fs::remove_file(&lock_path).await;
        "Codex native staging home could not be prepared".clone_into(&mut result.reason);
        return result;
    }
    let staging_path = staging_home.join("config.toml");
    if source_existed && atomic_write(&staging_path, &source).await.is_err() {
        let _ = tokio::fs::remove_dir_all(&staging_home).await;
        let _ = tokio::fs::remove_file(&lock_path).await;
        "Codex source could not be copied into isolated staging".clone_into(&mut result.reason);
        return result;
    }
    let output = tokio::time::timeout(
        PROBE_TIMEOUT,
        Command::new(binary)
            .args(&args)
            .current_dir(&config.workspace_root)
            .env("CODEX_HOME", &staging_home)
            .stdin(Stdio::null())
            .output(),
    )
    .await;
    let succeeded = matches!(output, Ok(Ok(ref output)) if output.status.success());
    if !succeeded {
        let _ = tokio::fs::remove_dir_all(&staging_home).await;
        let _ = tokio::fs::remove_file(&lock_path).await;
        "Codex native MCP command failed or timed out; real configuration was not touched"
            .clone_into(&mut result.reason);
        return result;
    }
    let after = tokio::fs::read(&staging_path).await.unwrap_or_default();
    let parsed = std::str::from_utf8(&after)
        .ok()
        .and_then(|text| parse_toml_servers(text).ok());
    let native_change_proven = parsed.as_ref().is_some_and(|servers| match action.kind {
        McpPlanActionKind::AddConfigure => servers.iter().any(|server| server.alias == alias),
        McpPlanActionKind::Remove => servers.iter().all(|server| server.alias != alias),
        _ => false,
    });
    if !native_change_proven
        || toml_contains_secret_material(&after)
        || !toml_changes_only_server(&source, &after, &alias)
    {
        let _ = tokio::fs::remove_dir_all(&staging_home).await;
        let _ = tokio::fs::remove_file(&lock_path).await;
        "Codex native output could not be safely validated; real configuration was not touched"
            .clone_into(&mut result.reason);
        return result;
    }
    let after_hash = sha256_bytes(&after);
    if before_hash == after_hash {
        let _ = tokio::fs::remove_dir_all(&staging_home).await;
        let _ = tokio::fs::remove_file(&lock_path).await;
        "Codex native command did not produce a provable source change"
            .clone_into(&mut result.reason);
        return result;
    }
    let current = tokio::fs::read(&path).await.unwrap_or_default();
    if sha256_bytes(&current) != before_hash {
        let _ = tokio::fs::remove_dir_all(&staging_home).await;
        let _ = tokio::fs::remove_file(&lock_path).await;
        "Codex source changed during native staging; CAS rejected atomic replace"
            .clone_into(&mut result.reason);
        return result;
    }
    let mut backup = backup;
    backup.applied_hash.clone_from(&after_hash);
    if durable_checkpoint(
        sink,
        run_id,
        journal,
        sequence,
        action_index,
        action,
        idempotency_key,
        McpApplyJournalPhase::BackedUp,
        Some(backup.clone()),
        "source backup and expected applied hash were durably recorded before mutation",
    )
    .await
    .is_err()
    {
        let _ = tokio::fs::remove_dir_all(&staging_home).await;
        let _ = tokio::fs::remove_file(&lock_path).await;
        result.backup = Some(backup);
        "durable backup checkpoint failed; real configuration was not touched"
            .clone_into(&mut result.reason);
        return result;
    }
    if let Err(reason) = atomic_write(&path, &after).await {
        let _ = tokio::fs::remove_dir_all(&staging_home).await;
        let _ = tokio::fs::remove_file(&lock_path).await;
        result.reason = reason;
        return result;
    }
    let _ = tokio::fs::remove_dir_all(&staging_home).await;
    if durable_checkpoint(
        sink,
        run_id,
        journal,
        sequence,
        action_index,
        action,
        idempotency_key,
        McpApplyJournalPhase::Written,
        Some(backup.clone()),
        "configuration mutation completed with source CAS and atomic replace",
    )
    .await
    .is_err()
    {
        let _ = tokio::fs::remove_file(&lock_path).await;
        result.status = McpApplyActionStatus::Failed;
        result.backup = Some(backup);
        result.before_source_hash = Some(before_hash);
        result.after_source_hash = Some(after_hash);
        "configuration changed, but the written checkpoint failed; recovery is required"
            .clone_into(&mut result.reason);
        return result;
    }
    let _ = tokio::fs::remove_file(&lock_path).await;
    result.status = McpApplyActionStatus::Applied;
    "Codex native MCP command completed in staging and was atomically installed under source CAS"
        .clone_into(&mut result.reason);
    result.before_source_hash = Some(before_hash);
    result.after_source_hash = Some(after_hash);
    result.backup = Some(backup);
    result
}

fn contains_secret_bytes(source: &[u8]) -> bool {
    let text = String::from_utf8_lossy(source).to_ascii_lowercase();
    [
        "bearer ",
        "api_key",
        "api-key",
        "access_token",
        "client_secret",
        "password",
        "authorization",
    ]
    .iter()
    .any(|marker| text.contains(marker))
}

fn toml_contains_secret_material(source: &[u8]) -> bool {
    if contains_secret_bytes(source) {
        return true;
    }
    let Ok(text) = std::str::from_utf8(source) else {
        return true;
    };
    let mut sensitive_table = false;
    for raw_line in text.lines() {
        let line = toml_without_comment(raw_line).trim();
        if line.starts_with('[') && line.ends_with(']') {
            sensitive_table = toml_key_path_is_sensitive(line.trim_matches(['[', ']']));
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let sensitive_key = toml_key_path_is_sensitive(key);
        if ((sensitive_table || sensitive_key) && !value.trim().is_empty()) || secret_like(key) {
            return true;
        }
        if let Some(value) = toml_string(value.trim()) {
            if redact_url(&value) != value || redact_text(&value) != value {
                return true;
            }
        }
        let array = parse_toml_string_array(value.trim());
        if !array.is_empty() && redact_args(&array) != array {
            return true;
        }
    }
    false
}

fn toml_without_comment(line: &str) -> &str {
    let mut quote = None;
    let mut escaped = false;
    for (index, character) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if quote == Some('"') && character == '\\' {
            escaped = true;
            continue;
        }
        if matches!(character, '\'' | '"') {
            if quote == Some(character) {
                quote = None;
            } else if quote.is_none() {
                quote = Some(character);
            }
            continue;
        }
        if character == '#' && quote.is_none() {
            return &line[..index];
        }
    }
    line
}

fn toml_key_path_is_sensitive(path: &str) -> bool {
    let mut segment = String::new();
    let mut segments = Vec::new();
    let mut quote = None;
    let mut escaped = false;
    let mut had_escape = false;
    for character in path.chars() {
        if escaped {
            // Escaped key names are intentionally treated as ambiguous below rather than decoded.
            segment.push(character);
            escaped = false;
            continue;
        }
        if quote == Some('"') && character == '\\' {
            escaped = true;
            had_escape = true;
            continue;
        }
        if matches!(character, '\'' | '"') {
            if quote == Some(character) {
                quote = None;
            } else if quote.is_none() {
                quote = Some(character);
            } else {
                segment.push(character);
            }
            continue;
        }
        if character == '.' && quote.is_none() {
            segments.push(std::mem::take(&mut segment));
        } else if !character.is_whitespace() || quote.is_some() {
            segment.push(character);
        }
    }
    segments.push(segment);
    had_escape
        || escaped
        || quote.is_some()
        || segments.into_iter().any(|segment| {
            matches!(
                segment.to_ascii_lowercase().as_str(),
                "env" | "headers" | "authentication"
            )
        })
}

fn toml_changes_only_server(before: &[u8], after: &[u8], alias: &str) -> bool {
    let (Ok(before), Ok(after)) = (std::str::from_utf8(before), std::str::from_utf8(after)) else {
        return false;
    };
    toml_without_server(before, alias) == toml_without_server(after, alias)
}

fn toml_without_server(text: &str, alias: &str) -> String {
    let mut retained = Vec::new();
    let mut skip = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(section) = trimmed
            .strip_prefix('[')
            .and_then(|value| value.strip_suffix(']'))
        {
            skip = ["mcp_servers.", "mcpServers.", "mcp."]
                .into_iter()
                .any(|prefix| {
                    section
                        .strip_prefix(prefix)
                        .is_some_and(|rest| rest == alias || rest.starts_with(&format!("{alias}.")))
                });
        }
        if !skip {
            retained.push(line.trim_end());
        }
    }
    retained.join("\n").trim().to_owned()
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
        "codex" => config.workspace_root.join(".codex").join("config.toml"),
        "claude" => config.workspace_root.join(".mcp.json"),
        _ => config.workspace_root.join(".cursor").join("mcp.json"),
    }
}

#[allow(clippy::too_many_arguments)]
async fn mutate_json_config(
    path: &Path,
    backup_root: &Path,
    run_id: &str,
    action_index: usize,
    action: &McpPlanAction,
    desired: Option<&cocli_driver_core::McpEffectiveServer>,
    idempotency_key: &str,
    sink: &dyn McpApplyJournalSink,
    journal: &mut Vec<McpApplyJournalEntry>,
    sequence: &mut u64,
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
    let expected_source_hash = action.expected_source_hash.as_deref().ok_or_else(|| {
        "plan has no authoritative source hash; structured write is blocked".to_owned()
    })?;
    if expected_source_hash != before_hash {
        return Err(
            "configuration source hash changed after planning; CAS rejected the write".to_owned(),
        );
    }
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
            let next = definition_json(definition, desired.desired.desired_enabled);
            if let Some(existing) = servers.get_mut(&alias) {
                merge_owned_server_fields(existing, next)?;
            } else {
                servers.insert(alias, next);
            }
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
    backup.applied_hash.clone_from(&after_hash);
    durable_checkpoint(
        sink,
        run_id,
        journal,
        sequence,
        action_index,
        action,
        idempotency_key,
        McpApplyJournalPhase::BackedUp,
        Some(backup.clone()),
        "source backup and expected applied hash were durably recorded before mutation",
    )
    .await?;
    let current = match tokio::fs::read(path).await {
        Ok(current) => current,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(_) => return Err("configuration could not be re-read before replace".to_owned()),
    };
    if sha256_bytes(&current) != before_hash {
        return Err("configuration changed after backup; CAS rejected atomic replace".to_owned());
    }
    atomic_write(path, &next).await?;
    durable_checkpoint(
        sink,
        run_id,
        journal,
        sequence,
        action_index,
        action,
        idempotency_key,
        McpApplyJournalPhase::Written,
        Some(backup.clone()),
        "configuration mutation completed with source CAS and atomic replace",
    )
    .await
    .map_err(|_| {
        "configuration changed, but the written checkpoint failed; recovery is required".to_owned()
    })?;
    Ok((backup, before_hash, after_hash))
}

fn merge_owned_server_fields(existing: &mut Value, desired: Value) -> Result<(), String> {
    let existing = existing
        .as_object_mut()
        .ok_or_else(|| "configured server entry cannot be round-tripped safely".to_owned())?;
    let desired = desired
        .as_object()
        .ok_or_else(|| "desired server entry is invalid".to_owned())?;
    for key in [
        "type",
        "transport",
        "command",
        "args",
        "url",
        "endpoint",
        "enabled",
        "disabled",
    ] {
        existing.remove(key);
    }
    for (key, value) in desired {
        existing.insert(key.clone(), value.clone());
    }
    Ok(())
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

async fn acquire_apply_lock(path: &Path, owner: &str) -> Result<(), ()> {
    for attempt in 0..2 {
        match tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .await
        {
            Ok(mut file) => {
                let marker = format!("{}\n{owner}", Utc::now().to_rfc3339());
                file.write_all(marker.as_bytes()).await.map_err(|_| ())?;
                file.sync_all().await.map_err(|_| ())?;
                return Ok(());
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists && attempt == 0 => {
                let marker = tokio::fs::read_to_string(path).await.ok();
                let owned_by_resume = marker.as_deref().is_some_and(|value| {
                    value
                        .lines()
                        .nth(1)
                        .is_some_and(|existing| existing == owner)
                });
                let stale = marker
                    .as_deref()
                    .and_then(|value| value.lines().next())
                    .and_then(|value| chrono::DateTime::parse_from_rfc3339(value.trim()).ok())
                    .is_some_and(|created_at| {
                        Utc::now().signed_duration_since(created_at.with_timezone(&Utc))
                            > STALE_APPLY_LOCK
                    });
                if owned_by_resume || stale {
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
    let lock_path = if source_path.extension().and_then(|value| value.to_str()) == Some("toml") {
        source_path.with_extension("toml.cocli-mcp.lock")
    } else {
        source_path.with_extension("json.cocli-mcp.lock")
    };
    if acquire_apply_lock(&lock_path, &format!("rollback:{}", backup.id))
        .await
        .is_err()
    {
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
    let written_config_hashes = results
        .iter()
        .filter_map(|result| {
            result.after_source_hash.as_ref().map(|hash| {
                (
                    format!("{}/{}", result.runtime, result.server_id),
                    hash.clone(),
                )
            })
        })
        .collect();
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
        written_config_hashes,
        session_effective: if applied {
            McpSessionEffectiveStatus::NewSessionRequired
        } else {
            McpSessionEffectiveStatus::Unknown
        },
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
            paths.push(workspace.join(".mcp.json"));
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
            &sha256_bytes(&bytes),
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
    source_hash: &str,
    observed_at: &str,
) -> Snapshot {
    let mut snapshot = Snapshot::default();
    for definition in definitions {
        let fingerprint = endpoint_fingerprint(&definition.definition);
        let server_id = server_id(&fingerprint);
        let evidence = evidence(
            "config",
            &format!("configured MCP server; source_sha256={source_hash}"),
            Some(path),
            false,
            false,
        );
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
            "0000000000000000000000000000000000000000000000000000000000000000",
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
            expected_source_hash: Some(sha256_bytes(&std::fs::read(path).unwrap_or_default())),
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

    async fn test_mutate_json_config(
        path: &Path,
        backup_root: &Path,
        action: &McpPlanAction,
        desired: Option<&McpEffectiveServer>,
    ) -> Result<(McpBackupDescriptor, String, String), String> {
        let mut journal = Vec::new();
        let mut sequence = 0;
        mutate_json_config(
            path,
            backup_root,
            "test-run",
            0,
            action,
            desired,
            "test-idempotency-key",
            &cocli_api::NoMcpApplyJournalSink,
            &mut journal,
            &mut sequence,
        )
        .await
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
            test_mutate_json_config(&path, &temp.path().join("backups"), &action, Some(&desired))
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
        let repeated = rollback_backup(&backup).await;
        assert_eq!(repeated.0, McpApplyActionStatus::RolledBack);
        assert_eq!(repeated.1, "backup was already restored");
    }

    #[tokio::test]
    async fn json_writer_preserves_unknown_fields_inside_managed_server() {
        let temp = tempfile::tempdir().expect("temp dir");
        let path = temp.path().join(".cursor/mcp.json");
        tokio::fs::create_dir_all(path.parent().expect("parent"))
            .await
            .expect("create parent");
        let original = br#"{"mcpServers":{"docs":{"command":"/bin/old","vendorExtension":{"keep":true},"customFlag":7}}}"#;
        tokio::fs::write(&path, original)
            .await
            .expect("seed config");
        let action = test_action(McpPlanActionKind::Update, &path);

        test_mutate_json_config(
            &path,
            &temp.path().join("backups"),
            &action,
            Some(&test_desired()),
        )
        .await
        .expect("update config");

        let applied: Value =
            serde_json::from_slice(&tokio::fs::read(&path).await.expect("read applied config"))
                .expect("parse applied config");
        assert_eq!(applied["mcpServers"]["docs"]["command"], "/bin/docs");
        assert_eq!(
            applied["mcpServers"]["docs"]["vendorExtension"]["keep"],
            true
        );
        assert_eq!(applied["mcpServers"]["docs"]["customFlag"], 7);
    }

    #[tokio::test]
    async fn same_source_actions_follow_a_durable_hash_chain() {
        let temp = tempfile::tempdir().expect("temp dir");
        let path = temp.path().join(".cursor/mcp.json");
        tokio::fs::create_dir_all(path.parent().expect("parent"))
            .await
            .expect("create parent");
        tokio::fs::write(&path, br#"{"mcpServers":{}}"#)
            .await
            .expect("seed config");
        let first = test_action(McpPlanActionKind::AddConfigure, &path);
        let planned_second = test_action(McpPlanActionKind::Disable, &path);
        let (_, _, first_hash) = test_mutate_json_config(
            &path,
            &temp.path().join("backups-first"),
            &first,
            Some(&test_desired()),
        )
        .await
        .expect("first action");
        assert_ne!(
            planned_second.expected_source_hash.as_deref(),
            Some(first_hash.as_str())
        );
        let chained = action_with_chained_source_hash(&planned_second, Some(&first_hash));
        test_mutate_json_config(
            &path,
            &temp.path().join("backups-second"),
            &chained,
            Some(&test_desired()),
        )
        .await
        .expect("second action uses first action hash");
        let applied: Value =
            serde_json::from_slice(&tokio::fs::read(&path).await.expect("read chained config"))
                .expect("parse chained config");
        assert_eq!(applied["mcpServers"]["docs"]["disabled"], true);
    }

    #[tokio::test]
    async fn file_level_source_hash_cas_rejects_unrelated_drift() {
        let temp = tempfile::tempdir().expect("temp dir");
        let path = temp.path().join(".cursor/mcp.json");
        tokio::fs::create_dir_all(path.parent().expect("parent"))
            .await
            .expect("create parent");
        tokio::fs::write(&path, br#"{"mcpServers":{},"editor":{"theme":"dark"}}"#)
            .await
            .expect("seed config");
        let action = test_action(McpPlanActionKind::AddConfigure, &path);
        let drifted = br#"{"mcpServers":{},"editor":{"theme":"light"}}"#;
        tokio::fs::write(&path, drifted)
            .await
            .expect("external edit");

        let error = test_mutate_json_config(
            &path,
            &temp.path().join("backups"),
            &action,
            Some(&test_desired()),
        )
        .await
        .expect_err("file-level drift must reject write");
        assert!(error.contains("source hash changed"));
        assert_eq!(tokio::fs::read(&path).await.expect("unchanged"), drifted);
        assert!(!temp.path().join("backups").exists());
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

        let error = test_mutate_json_config(
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
        let (backup, _, _) = test_mutate_json_config(
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

        let error = test_mutate_json_config(
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

        let error = test_mutate_json_config(
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
        acquire_apply_lock(&lock_path, "first-owner")
            .await
            .expect("stale lock should recover");
        assert!(acquire_apply_lock(&lock_path, "second-owner")
            .await
            .is_err());
        acquire_apply_lock(&lock_path, "first-owner")
            .await
            .expect("same durable run may reclaim its interrupted lock");
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
            "1111111111111111111111111111111111111111111111111111111111111111",
            observed_at,
        ));
        aggregate.extend(snapshot_config(
            "cursor",
            Path::new("/tmp/cursor.json"),
            Some(Path::new("/tmp")),
            vec![alternate],
            "2222222222222222222222222222222222222222222222222222222222222222",
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
    fn codex_toml_backup_guard_rejects_env_userinfo_and_unrelated_rewrites() {
        assert!(toml_contains_secret_material(
            br#"[mcp_servers.docs.env]
CUSTOM = "canary-value""#
        ));
        assert!(toml_contains_secret_material(
            br#"[mcp_servers.docs]
url = "https://user:pass@example.test/mcp""#
        ));
        assert!(toml_contains_secret_material(
            br#"[mcp_servers.docs]
"env" = { CUSTOM = "canary-value" }"#
        ));
        assert!(toml_contains_secret_material(
            br#"[mcp_servers.docs]
env.CUSTOM = "canary-value""#
        ));
        assert!(toml_contains_secret_material(
            br#"[mcp_servers.docs."env"]
CUSTOM = "canary-value""#
        ));
        assert!(toml_contains_secret_material(
            br#"[mcp_servers.docs.env] # runtime credential map
CUSTOM = "canary-value""#
        ));
        assert!(!toml_contains_secret_material(
            br#"[mcp_servers.docs] # https://example.test/#fragment
command = "/bin/docs#stable""#
        ));
        assert!(!toml_contains_secret_material(
            br#"model = "gpt-test"
[mcp_servers.docs]
command = "/bin/docs""#
        ));
        let before = br#"model = "gpt-test"
[mcp_servers.docs]
command = "/bin/old""#;
        let safe_after = br#"model = "gpt-test"
[mcp_servers.docs]
command = "/bin/new""#;
        let unsafe_after = br#"model = "changed"
[mcp_servers.docs]
command = "/bin/new""#;
        assert!(toml_changes_only_server(before, safe_after, "docs"));
        assert!(!toml_changes_only_server(before, unsafe_after, "docs"));
    }

    #[tokio::test]
    async fn codex_native_blocks_plaintext_before_backup_or_command() {
        let temp = tempfile::tempdir().expect("temp dir");
        let config = LocalRuntimeConfig::new(temp.path().join("workspace"), String::new());
        let path = config.workspace_root.join(".codex/config.toml");
        tokio::fs::create_dir_all(path.parent().expect("parent"))
            .await
            .expect("create config parent");
        tokio::fs::write(
            &path,
            br#"[mcp_servers.docs.env]
CUSTOM = "phase2c-secret-canary""#,
        )
        .await
        .expect("seed unsafe config");
        let mut action = test_action(McpPlanActionKind::AddConfigure, &path);
        "codex".clone_into(&mut action.runtime);
        let mut desired = test_desired();
        "codex".clone_into(&mut desired.desired.runtime);
        let mut runtime = runtime_info("codex");
        runtime.binary = Some("/bin/false".to_owned());
        let backup_root = temp.path().join("backups");
        let mut journal = Vec::new();
        let mut sequence = 0;

        let result = apply_codex_native(
            &[runtime],
            &config,
            "test-run",
            0,
            &action,
            Some(&desired),
            &backup_root,
            "test-idempotency-key",
            &cocli_api::NoMcpApplyJournalSink,
            &mut journal,
            &mut sequence,
        )
        .await;

        assert_eq!(result.status, McpApplyActionStatus::Blocked);
        assert!(result.reason.contains("inline credential material"));
        assert!(!backup_root.exists());
        assert!(journal.is_empty());
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

    struct PhaseFailJournalSink {
        fail_phase: McpApplyJournalPhase,
        persisted: Mutex<Vec<McpApplyJournalEntry>>,
    }

    #[async_trait::async_trait]
    impl McpApplyJournalSink for PhaseFailJournalSink {
        async fn checkpoint(
            &self,
            _run_id: &str,
            entry: &McpApplyJournalEntry,
        ) -> Result<(), cocli_api::RuntimeError> {
            if entry.phase == self.fail_phase {
                return Err(cocli_api::RuntimeError::Delivery(
                    "simulated journal interruption".to_owned(),
                ));
            }
            self.persisted
                .lock()
                .expect("persisted journal mutex")
                .push(entry.clone());
            Ok(())
        }
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
    async fn capability_matrix_is_version_bound_and_grok_writer_stays_read_only() {
        let temp = tempfile::tempdir().expect("temp dir");
        let config = LocalRuntimeConfig::new(temp.path().to_path_buf(), String::new());
        let mut codex = runtime_info("codex");
        codex.version = Some("1.2.3".to_owned());
        let first = capabilities(
            &[
                codex.clone(),
                runtime_info("cursor"),
                runtime_info("claude"),
                runtime_info("grok"),
            ],
            &config,
        )
        .await;
        let grok = first
            .runtimes
            .iter()
            .find(|runtime| runtime.runtime == "grok")
            .expect("grok capability");
        assert_eq!(
            grok.operations[&McpCapabilityOperation::AddConfigure].support,
            McpCapabilitySupport::ReadOnly
        );
        assert_eq!(grok.adapter, "grok_read_only");
        codex.version = Some("1.2.4".to_owned());
        let second = capabilities(&[codex], &config).await;
        assert_ne!(first.hash, second.hash);
    }

    #[tokio::test]
    async fn codex_writer_requires_proven_native_help_contract() {
        let temp = tempfile::tempdir().expect("temp dir");
        let config = LocalRuntimeConfig::new(temp.path().to_path_buf(), String::new());
        let codex = runtime_info("codex");
        let supported_runner = FakeRunner::default()
            .with(
                &["mcp", "add", "--help"],
                CommandOutcome::Output(CommandOutput {
                    success: true,
                    stdout: "Usage: codex mcp add NAME".to_owned(),
                    stderr: String::new(),
                }),
            )
            .with(
                &["mcp", "remove", "--help"],
                CommandOutcome::Output(CommandOutput {
                    success: true,
                    stdout: "Usage: codex mcp remove NAME".to_owned(),
                    stderr: String::new(),
                }),
            );
        let supported =
            capabilities_with_runner(&[codex.clone()], &config, &supported_runner).await;
        let supported_codex = supported
            .runtimes
            .iter()
            .find(|runtime| runtime.runtime == "codex")
            .expect("Codex capability");
        assert_eq!(
            supported_codex.operations[&McpCapabilityOperation::AddConfigure].support,
            McpCapabilitySupport::Supported
        );

        for outcome in [
            CommandOutcome::Timeout,
            CommandOutcome::Output(CommandOutput {
                success: false,
                stdout: String::new(),
                stderr: "failed".to_owned(),
            }),
            CommandOutcome::Output(CommandOutput {
                success: true,
                stdout: "Commands: list get".to_owned(),
                stderr: String::new(),
            }),
        ] {
            let runner = FakeRunner::default()
                .with(&["mcp", "add", "--help"], outcome.clone())
                .with(&["mcp", "remove", "--help"], outcome);
            let snapshot = capabilities_with_runner(&[codex.clone()], &config, &runner).await;
            let codex_capability = snapshot
                .runtimes
                .iter()
                .find(|runtime| runtime.runtime == "codex")
                .expect("Codex capability");
            assert_eq!(
                codex_capability.operations[&McpCapabilityOperation::AddConfigure].support,
                McpCapabilitySupport::Unknown
            );
        }

        let mut missing = codex;
        missing.binary = None;
        missing.installed = false;
        let snapshot = capabilities_with_runner(&[missing], &config, &FakeRunner::default()).await;
        let codex_capability = snapshot
            .runtimes
            .iter()
            .find(|runtime| runtime.runtime == "codex")
            .expect("Codex capability");
        assert_eq!(
            codex_capability.operations[&McpCapabilityOperation::AddConfigure].support,
            McpCapabilitySupport::Unsupported
        );
    }

    #[tokio::test]
    async fn missing_binary_degrades_discovery_without_claiming_native_verification() {
        let temp = tempfile::tempdir().expect("temp dir");
        let config = LocalRuntimeConfig::new(temp.path().to_path_buf(), String::new());
        let mut cursor = runtime_info("cursor");
        cursor.binary = None;
        cursor.installed = false;
        let snapshot = capabilities(&[cursor], &config).await;
        let cursor = snapshot
            .runtimes
            .iter()
            .find(|runtime| runtime.runtime == "cursor")
            .expect("cursor capability");
        assert_eq!(
            cursor.operations[&McpCapabilityOperation::Verify].support,
            McpCapabilitySupport::ReadOnly
        );
        assert!(cursor.binary_version.is_none());
    }

    #[tokio::test]
    async fn resume_skips_only_journaled_completed_non_idempotent_writes() {
        let entry = |sequence, phase, key: &str| McpApplyJournalEntry {
            sequence,
            action_index: 0,
            runtime: "cursor".to_owned(),
            server_id: "docs".to_owned(),
            idempotency_key: key.to_owned(),
            phase,
            attempt: 1,
            expected_source_hash: Some("before".to_owned()),
            expected_schema_hash: Some("schema".to_owned()),
            backup: None,
            reason: "test checkpoint".to_owned(),
            evidence: Vec::new(),
        };
        let journal = vec![
            entry(1, McpApplyJournalPhase::BackedUp, "idem"),
            entry(2, McpApplyJournalPhase::Written, "idem"),
            entry(3, McpApplyJournalPhase::Written, "other"),
        ];
        assert_eq!(
            recover_resume_state(&journal, "idem")
                .await
                .expect("written checkpoint")
                .map(|entry| entry.sequence),
            Some(2)
        );
        assert!(recover_resume_state(&journal[..1], "idem").await.is_err());
        assert!(recover_resume_state(&journal, "missing")
            .await
            .expect("missing key")
            .is_none());
    }

    #[tokio::test]
    async fn backup_checkpoint_failure_prevents_configuration_write() {
        let temp = tempfile::tempdir().expect("temp dir");
        let path = temp.path().join(".cursor/mcp.json");
        tokio::fs::create_dir_all(path.parent().expect("parent"))
            .await
            .expect("create parent");
        let original = br#"{"mcpServers":{}}"#;
        tokio::fs::write(&path, original)
            .await
            .expect("seed config");
        let action = test_action(McpPlanActionKind::AddConfigure, &path);
        let sink = PhaseFailJournalSink {
            fail_phase: McpApplyJournalPhase::BackedUp,
            persisted: Mutex::new(Vec::new()),
        };
        let mut journal = Vec::new();
        let mut sequence = 0;

        let error = mutate_json_config(
            &path,
            &temp.path().join("backups"),
            "test-run",
            0,
            &action,
            Some(&test_desired()),
            "idem",
            &sink,
            &mut journal,
            &mut sequence,
        )
        .await
        .expect_err("backup checkpoint interruption must abort");
        assert!(error.contains("journal checkpoint failed"));
        assert_eq!(tokio::fs::read(&path).await.expect("unchanged"), original);
        assert!(journal.is_empty());
    }

    #[tokio::test]
    async fn written_checkpoint_crash_is_recovered_without_repeating_mutation() {
        let temp = tempfile::tempdir().expect("temp dir");
        let path = temp.path().join(".cursor/mcp.json");
        tokio::fs::create_dir_all(path.parent().expect("parent"))
            .await
            .expect("create parent");
        tokio::fs::write(&path, br#"{"mcpServers":{}}"#)
            .await
            .expect("seed config");
        let action = test_action(McpPlanActionKind::AddConfigure, &path);
        let sink = PhaseFailJournalSink {
            fail_phase: McpApplyJournalPhase::Written,
            persisted: Mutex::new(Vec::new()),
        };
        let mut journal = Vec::new();
        let mut sequence = 0;

        let error = mutate_json_config(
            &path,
            &temp.path().join("backups"),
            "test-run",
            0,
            &action,
            Some(&test_desired()),
            "idem",
            &sink,
            &mut journal,
            &mut sequence,
        )
        .await
        .expect_err("written checkpoint interruption must surface recovery");
        assert!(error.contains("recovery is required"));
        assert_eq!(journal.len(), 1);
        assert_eq!(journal[0].phase, McpApplyJournalPhase::BackedUp);
        let recovered = recover_resume_state(&journal, "idem")
            .await
            .expect("hash-based recovery")
            .expect("completed write");
        assert_eq!(recovered.phase, McpApplyJournalPhase::BackedUp);
        let applied = tokio::fs::read(&path).await.expect("read applied source");
        assert_eq!(
            sha256_bytes(&applied),
            recovered.backup.expect("backup descriptor").applied_hash
        );
    }

    #[tokio::test]
    async fn environment_secret_resolver_never_exposes_canary_in_debug_or_errors() {
        const NAME: &str = "COCLI_PHASE2C_SECRET_CANARY";
        const CANARY: &str = "phase2c-canary-value";
        std::env::set_var(NAME, CANARY);
        let resolver = EnvironmentSecretResolver;
        let secret = resolver
            .resolve(&McpSecretRef {
                location: "env.API_TOKEN".to_owned(),
                kind: "env".to_owned(),
                reference: format!("env://{NAME}"),
            })
            .await
            .expect("resolve env reference");
        assert_eq!(secret.expose(), CANARY.as_bytes());
        assert_eq!(format!("{secret:?}"), "ResolvedSecret([REDACTED])");
        assert!(!format!("{secret:?}").contains(CANARY));
        drop(secret);
        std::env::remove_var(NAME);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn codex_native_adapter_uses_isolated_home_and_argument_array() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().expect("temp dir");
        let binary = temp.path().join("fake-codex");
        tokio::fs::write(
            &binary,
            "#!/bin/sh\nmkdir -p \"$CODEX_HOME\"\nprintf '[mcp_servers.docs]\\ncommand = \"/bin/docs\"\\nargs = [\"--safe\"]\\n' > \"$CODEX_HOME/config.toml\"\n",
        )
        .await
        .expect("write fake binary");
        tokio::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700))
            .await
            .expect("chmod fake binary");
        let config = LocalRuntimeConfig::new(temp.path().join("workspace"), String::new());
        tokio::fs::create_dir_all(&config.workspace_root)
            .await
            .expect("workspace");
        let path = config.workspace_root.join(".codex/config.toml");
        let mut action = test_action(McpPlanActionKind::AddConfigure, &path);
        "codex".clone_into(&mut action.runtime);
        let mut desired = test_desired();
        "codex".clone_into(&mut desired.desired.runtime);
        let mut runtime = runtime_info("codex");
        runtime.binary = Some(binary.display().to_string());
        runtime.version = Some("test".to_owned());
        let mut journal = Vec::new();
        let mut sequence = 0;
        let result = apply_codex_native(
            &[runtime],
            &config,
            "test-run",
            0,
            &action,
            Some(&desired),
            &temp.path().join("backups"),
            "test-idempotency-key",
            &cocli_api::NoMcpApplyJournalSink,
            &mut journal,
            &mut sequence,
        )
        .await;
        assert_eq!(result.status, McpApplyActionStatus::Applied);
        let written = tokio::fs::read_to_string(&path)
            .await
            .expect("isolated config written");
        assert!(written.contains("mcp_servers.docs"));
        assert!(!temp.path().join(".codex/config.toml").exists());
    }

    #[tokio::test]
    async fn runner_covers_timeout_nonzero_bad_json_and_partial_success() {
        let runner = FakeRunner::default()
            .with(&["mcp", "list"], CommandOutcome::Timeout)
            .with(
                &["mcp", "list"],
                CommandOutcome::Output(CommandOutput {
                    success: false,
                    stdout: String::new(),
                    stderr: "simulated failure".to_owned(),
                }),
            )
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
            &[
                runtime_info("cursor"),
                runtime_info("claude"),
                runtime_info("grok"),
            ],
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
            .any(|diagnostic| diagnostic.code == "mcp_probe_failed"
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
