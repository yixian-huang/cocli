//! Runtime-neutral MCP desired-state, resolution, and dry-run planning contract.
//!
//! This module deliberately has no writer or applier interface. Profiles and
//! approvals describe future intent; they cannot mutate Runtime configuration.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::{
    McpCanonicalDefinition, McpEvidence, McpInventory, McpSecretRef, McpServer, ObservedMcpInstance,
};

type ProfileCandidate<'a> = (&'a McpProfile, &'a McpProfileBinding, McpDesiredServer);
type CandidateMap<'a> = BTreeMap<(String, String, McpBindingTargetType), Vec<ProfileCandidate<'a>>>;
type ServerCandidates<'a> =
    BTreeMap<(String, String), Vec<(McpBindingTargetType, Vec<ProfileCandidate<'a>>)>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpApprovalMode {
    Manual,
    PerTool,
    PreApproved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum McpRiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpBindingTargetType {
    Machine,
    Workspace,
    Agent,
}

impl McpBindingTargetType {
    #[must_use]
    pub const fn precedence(self) -> u8 {
        match self {
            Self::Machine => 0,
            Self::Workspace => 1,
            Self::Agent => 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpBindingTarget {
    pub target_type: McpBindingTargetType,
    pub target_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpDesiredTarget {
    pub machine_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
}

impl McpDesiredTarget {
    fn contains(&self, target: &McpBindingTarget) -> bool {
        match target.target_type {
            McpBindingTargetType::Machine => target.target_id == self.machine_id,
            McpBindingTargetType::Workspace => {
                self.workspace_id.as_deref() == Some(target.target_id.as_str())
            }
            McpBindingTargetType::Agent => {
                self.agent_id.as_deref() == Some(target.target_id.as_str())
            }
        }
    }

    #[must_use]
    pub fn label(&self) -> String {
        let mut parts = vec![format!("machine:{}", self.machine_id)];
        if let Some(workspace_id) = &self.workspace_id {
            parts.push(format!("workspace:{workspace_id}"));
        }
        if let Some(agent_id) = &self.agent_id {
            parts.push(format!("agent:{agent_id}"));
        }
        parts.join("/")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpDesiredServer {
    pub server_id: String,
    pub runtime: String,
    pub alias: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub definition: Option<McpCanonicalDefinition>,
    pub desired_enabled: bool,
    #[serde(default)]
    pub allow_tools: Vec<String>,
    #[serde(default)]
    pub deny_tools: Vec<String>,
    pub approval_mode: McpApprovalMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk_override: Option<McpRiskLevel>,
    #[serde(default)]
    pub secret_refs: Vec<McpSecretRef>,
}

impl McpDesiredServer {
    fn normalize(&mut self) {
        self.allow_tools.sort();
        self.allow_tools.dedup();
        self.deny_tools.sort();
        self.deny_tools.dedup();
        self.secret_refs.sort_by(|left, right| {
            (&left.location, &left.kind, &left.reference).cmp(&(
                &right.location,
                &right.kind,
                &right.reference,
            ))
        });
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpProfile {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub version: i64,
    pub servers: Vec<McpDesiredServer>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpProfileBinding {
    pub id: String,
    pub profile_id: String,
    pub target: McpBindingTarget,
    pub version: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpEffectiveServer {
    #[serde(flatten)]
    pub desired: McpDesiredServer,
    pub source_profile_ids: Vec<String>,
    pub source_profile_names: Vec<String>,
    pub inherited_from: McpBindingTargetType,
    pub high_risk_context: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpProfileConflict {
    pub runtime: String,
    pub server_id: String,
    pub precedence: McpBindingTargetType,
    pub profile_ids: Vec<String>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpProfileResolution {
    pub profile_id: String,
    pub profile_name: String,
    pub binding_id: String,
    pub target: McpBindingTarget,
    pub applied: bool,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpEffectiveDesiredState {
    pub target: McpDesiredTarget,
    pub servers: Vec<McpEffectiveServer>,
    pub conflicts: Vec<McpProfileConflict>,
    pub resolution: Vec<McpProfileResolution>,
}

/// Rejects obvious inline credentials while allowing opaque secret references.
/// Error text is intentionally generic so rejected values never reach logs or APIs.
pub fn validate_mcp_profile(profile: &McpProfile) -> Result<(), &'static str> {
    if profile.name.trim().is_empty() {
        return Err("profile name is required");
    }
    if contains_plaintext_secret(&profile.name)
        || profile
            .description
            .as_deref()
            .is_some_and(contains_plaintext_secret)
    {
        return Err("profile contains suspected plaintext secret");
    }
    for server in &profile.servers {
        if server.server_id.trim().is_empty()
            || server.runtime.trim().is_empty()
            || server.alias.trim().is_empty()
        {
            return Err("profile server identity is required");
        }
        if [
            server.server_id.as_str(),
            server.runtime.as_str(),
            server.alias.as_str(),
        ]
        .into_iter()
        .chain(server.allow_tools.iter().map(String::as_str))
        .chain(server.deny_tools.iter().map(String::as_str))
        .any(contains_plaintext_secret)
        {
            return Err("profile contains suspected plaintext secret");
        }
        if server
            .allow_tools
            .iter()
            .any(|tool| server.deny_tools.contains(tool))
        {
            return Err("a tool cannot be both allowed and denied");
        }
        if let Some(definition) = &server.definition {
            if definition
                .command
                .as_deref()
                .is_some_and(contains_plaintext_secret)
                || args_contain_plaintext_secret(&definition.args)
                || definition
                    .endpoint
                    .as_deref()
                    .is_some_and(endpoint_contains_secret)
            {
                return Err("profile contains suspected plaintext secret");
            }
        }
        for secret_ref in &server.secret_refs {
            let reference = secret_ref.reference.as_str();
            let reference_value = reference
                .split_once("://")
                .map(|(_, value)| value)
                .unwrap_or_default();
            if secret_ref.location.trim().is_empty()
                || secret_ref.kind.trim().is_empty()
                || contains_plaintext_secret(&secret_ref.location)
                || contains_plaintext_secret(&secret_ref.kind)
                || !matches!(
                    reference.split_once("://").map(|(scheme, _)| scheme),
                    Some("env" | "keychain" | "secret" | "vault")
                )
                || reference
                    .split_once("://")
                    .map_or(true, |(_, value)| value.trim().is_empty())
                || looks_like_secret_value(reference_value)
            {
                return Err("secretRefs must use an approved opaque reference scheme");
            }
        }
    }
    Ok(())
}

fn contains_plaintext_secret(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    [
        "token=",
        "access_token=",
        "password=",
        "secret=",
        "client_secret=",
        "api_key=",
        "api-key=",
        "authorization=",
        "authorization:",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
        || lower.contains("bearer ")
        || looks_like_secret_value(&lower)
}

fn args_contain_plaintext_secret(args: &[String]) -> bool {
    args.iter().any(|value| contains_plaintext_secret(value))
        || args
            .windows(2)
            .any(|pair| secret_identifier(&pair[0]) && !pair[1].contains("://"))
}

fn secret_identifier(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    [
        "token",
        "secret",
        "api_key",
        "api-key",
        "apikey",
        "access_key",
        "access-key",
        "client-secret",
        "password",
        "authorization",
        "bearer",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

fn looks_like_secret_value(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.starts_with("sk-")
        || lower.starts_with("sk_")
        || lower.starts_with("ghp_")
        || lower.starts_with("github_pat_")
        || lower.starts_with("xox")
        || lower.starts_with("eyj")
        || lower.starts_with("bearer ")
}

fn endpoint_contains_secret(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    let authority = lower
        .split_once("://")
        .map_or(lower.as_str(), |(_, rest)| rest)
        .split('/')
        .next()
        .unwrap_or_default();
    authority.contains('@')
        || [
            "token=",
            "access_token=",
            "password=",
            "secret=",
            "client_secret=",
            "api_key=",
            "api-key=",
        ]
        .iter()
        .any(|marker| lower.contains(marker))
        || lower
            .split_once('?')
            .map(|(_, query)| {
                query.split('&').any(|part| {
                    part.split_once('=')
                        .is_some_and(|(key, _)| secret_identifier(key))
                })
            })
            .unwrap_or(false)
}

/// Resolves applicable bindings with fixed machine < workspace < agent precedence.
/// Same-precedence differences become explicit conflicts instead of last-write-wins.
#[must_use]
pub fn resolve_mcp_desired_state(
    profiles: &[McpProfile],
    bindings: &[McpProfileBinding],
    target: McpDesiredTarget,
) -> McpEffectiveDesiredState {
    let profile_by_id = profiles
        .iter()
        .map(|profile| (profile.id.as_str(), profile))
        .collect::<BTreeMap<_, _>>();
    let mut applicable = bindings
        .iter()
        .filter(|binding| target.contains(&binding.target))
        .filter_map(|binding| {
            profile_by_id
                .get(binding.profile_id.as_str())
                .map(|profile| (binding, *profile))
        })
        .collect::<Vec<_>>();
    applicable.sort_by(
        |(left_binding, left_profile), (right_binding, right_profile)| {
            (
                left_binding.target.target_type.precedence(),
                left_profile.id.as_str(),
                left_binding.id.as_str(),
            )
                .cmp(&(
                    right_binding.target.target_type.precedence(),
                    right_profile.id.as_str(),
                    right_binding.id.as_str(),
                ))
        },
    );

    let mut candidates = CandidateMap::new();
    let mut resolution = Vec::new();
    for (binding, profile) in &applicable {
        for server in &profile.servers {
            let mut server = server.clone();
            server.normalize();
            candidates
                .entry((
                    server.runtime.clone(),
                    server.server_id.clone(),
                    binding.target.target_type,
                ))
                .or_default()
                .push((profile, binding, server));
        }
        resolution.push(McpProfileResolution {
            profile_id: profile.id.clone(),
            profile_name: profile.name.clone(),
            binding_id: binding.id.clone(),
            target: binding.target.clone(),
            applied: false,
            reason: "applicable profile; server-level precedence is resolved below".to_owned(),
        });
    }

    let mut by_server = ServerCandidates::new();
    for ((runtime, server_id, precedence), entries) in candidates {
        by_server
            .entry((runtime, server_id))
            .or_default()
            .push((precedence, entries));
    }

    let mut servers = Vec::new();
    let mut conflicts = Vec::new();
    for ((runtime, server_id), mut levels) in by_server {
        levels.sort_by_key(|(precedence, _)| precedence.precedence());
        let (precedence, entries) = levels
            .last()
            .expect("server candidates cannot contain an empty precedence level");
        let first = &entries[0].2;
        if entries
            .iter()
            .skip(1)
            .any(|(_, _, desired)| desired != first)
        {
            let mut profile_ids = entries
                .iter()
                .map(|(profile, _, _)| profile.id.clone())
                .collect::<Vec<_>>();
            profile_ids.sort();
            profile_ids.dedup();
            conflicts.push(McpProfileConflict {
                runtime,
                server_id,
                precedence: *precedence,
                profile_ids,
                reason: "same-precedence profiles define different desired state".to_owned(),
            });
            continue;
        }
        let mut source_profile_ids = entries
            .iter()
            .map(|(profile, _, _)| profile.id.clone())
            .collect::<Vec<_>>();
        let mut source_profile_names = entries
            .iter()
            .map(|(profile, _, _)| profile.name.clone())
            .collect::<Vec<_>>();
        source_profile_ids.sort();
        source_profile_ids.dedup();
        source_profile_names.sort();
        source_profile_names.dedup();
        let high_risk_context = source_profile_names.iter().any(|name| {
            let lower = name.to_ascii_lowercase();
            lower.contains("production") || lower.contains("prod") || lower.contains("ops")
        });
        for (_, binding, _) in entries {
            if let Some(item) = resolution
                .iter_mut()
                .find(|item| item.binding_id == binding.id)
            {
                item.applied = true;
                item.reason = format!(
                    "won {} precedence for {runtime}/{server_id}",
                    precedence_label(*precedence)
                );
            }
        }
        servers.push(McpEffectiveServer {
            desired: first.clone(),
            source_profile_ids,
            source_profile_names,
            inherited_from: *precedence,
            high_risk_context,
        });
    }
    servers.sort_by(|left, right| {
        (&left.desired.runtime, &left.desired.server_id)
            .cmp(&(&right.desired.runtime, &right.desired.server_id))
    });
    conflicts.sort_by(|left, right| {
        (&left.runtime, &left.server_id, left.precedence).cmp(&(
            &right.runtime,
            &right.server_id,
            right.precedence,
        ))
    });
    resolution.sort_by(|left, right| {
        (
            left.target.target_type.precedence(),
            &left.profile_id,
            &left.binding_id,
        )
            .cmp(&(
                right.target.target_type.precedence(),
                &right.profile_id,
                &right.binding_id,
            ))
    });
    McpEffectiveDesiredState {
        target,
        servers,
        conflicts,
        resolution,
    }
}

fn precedence_label(value: McpBindingTargetType) -> &'static str {
    match value {
        McpBindingTargetType::Machine => "machine",
        McpBindingTargetType::Workspace => "workspace",
        McpBindingTargetType::Agent => "agent",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpPlanActionKind {
    AddConfigure,
    Enable,
    Disable,
    Update,
    Remove,
    ApprovalRequired,
    AuthenticationRequired,
    ManualUnsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpStateSummary {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub configured: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint_fingerprint: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allow_tools: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deny_tools: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_mode: Option<McpApprovalMode>,
    pub secret_ref_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpPlanAction {
    pub kind: McpPlanActionKind,
    pub runtime: String,
    pub scope: String,
    pub target: String,
    pub server_id: String,
    pub server_fingerprint: String,
    pub before: McpStateSummary,
    pub after: McpStateSummary,
    pub risk: McpRiskLevel,
    pub reason: String,
    #[serde(default)]
    pub evidence: Vec<McpEvidence>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_source_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_schema_hash: Option<String>,
    pub blocked: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpCapabilitySupport {
    Supported,
    ReadOnly,
    Unsupported,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpCapabilityOperation {
    ReadDiscover,
    AddConfigure,
    EnableDisable,
    Remove,
    SecretReference,
    Reload,
    Verify,
    Rollback,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpCapabilityDetail {
    pub support: McpCapabilitySupport,
    pub reason: String,
    #[serde(default)]
    pub evidence: Vec<McpEvidence>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpReloadStrategy {
    NativeReload,
    NewSessionOnly,
    #[default]
    Deferred,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpRuntimeCapability {
    pub runtime: String,
    pub adapter: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binary_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binary_version: Option<String>,
    pub config_schema_version: String,
    pub destination: String,
    pub allowed_subtree: String,
    pub reload_strategy: McpReloadStrategy,
    pub operations: BTreeMap<McpCapabilityOperation, McpCapabilityDetail>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpCapabilitySnapshot {
    pub hash: String,
    pub observed_at: String,
    pub runtimes: Vec<McpRuntimeCapability>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpPreflightAction {
    pub action_index: usize,
    pub runtime: String,
    pub server_id: String,
    pub operation: McpCapabilityOperation,
    pub support: McpCapabilitySupport,
    pub executable: bool,
    pub reason: String,
    pub adapter: String,
    pub destination: String,
    pub allowed_subtree: String,
    pub reload_strategy: McpReloadStrategy,
    pub idempotency_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_source_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_schema_hash: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpPreflightReport {
    pub plan_id: String,
    pub plan_hash: String,
    pub capability_hash: String,
    pub observation_hash: String,
    pub config_hash: String,
    pub actions: Vec<McpPreflightAction>,
    pub stale_reasons: Vec<String>,
    pub executable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpPlan {
    pub id: String,
    pub target: McpDesiredTarget,
    pub effective_desired_state: McpEffectiveDesiredState,
    pub actions: Vec<McpPlanAction>,
    pub observation_hash: String,
    pub config_hash: String,
    #[serde(default)]
    pub capability_hash: String,
    pub plan_hash: String,
    pub generated_at: String,
    pub dry_run: bool,
    pub applied: bool,
}

/// Runtime-neutral request passed to an MCP configuration applier only after
/// the API has revalidated the durable approval and all plan base hashes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpApplyExecutionRequest {
    pub run_id: String,
    pub plan: McpPlan,
    pub actor: String,
    pub confirm_high_risk: bool,
    #[serde(default)]
    pub capability_hash: String,
    #[serde(default)]
    pub resume_journal: Vec<McpApplyJournalEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpApplyJournalPhase {
    Preflight,
    Locked,
    BackedUp,
    Written,
    ReloadPending,
    Reloaded,
    Verified,
    Failed,
    RollingBack,
    RolledBack,
    RecoveryRequired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpApplyJournalEntry {
    pub sequence: u64,
    pub action_index: usize,
    pub runtime: String,
    pub server_id: String,
    pub idempotency_key: String,
    pub phase: McpApplyJournalPhase,
    pub attempt: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_source_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_schema_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup: Option<McpBackupDescriptor>,
    pub reason: String,
    #[serde(default)]
    pub evidence: Vec<McpEvidence>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpApplyActionStatus {
    Applied,
    Skipped,
    Blocked,
    Failed,
    Verified,
    RolledBack,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpReloadStatus {
    NotRequired,
    Reloaded,
    Deferred,
    Blocked,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpVerificationStatus {
    Matched,
    Mismatched,
    Blocked,
    Failed,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpSessionEffectiveStatus {
    Effective,
    NewSessionRequired,
    #[default]
    Unknown,
}

/// Opaque backup metadata. Backup contents never cross the adapter boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpBackupDescriptor {
    pub id: String,
    pub runtime: String,
    pub source_path: String,
    pub backup_path: String,
    pub source_hash: String,
    pub backup_hash: String,
    pub applied_hash: String,
    pub source_existed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpApplyActionResult {
    pub action_index: usize,
    pub runtime: String,
    pub server_id: String,
    pub status: McpApplyActionStatus,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup: Option<McpBackupDescriptor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before_source_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_source_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpReloadResult {
    pub runtime: String,
    pub status: McpReloadStatus,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpVerificationResult {
    pub status: McpVerificationStatus,
    pub observation_hash: String,
    #[serde(default)]
    pub mismatches: Vec<String>,
    #[serde(default)]
    pub written_config_hashes: BTreeMap<String, String>,
    #[serde(default)]
    pub session_effective: McpSessionEffectiveStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpApplyExecutionResult {
    pub actions: Vec<McpApplyActionResult>,
    pub reloads: Vec<McpReloadResult>,
    pub verification: McpVerificationResult,
    #[serde(default)]
    pub journal: Vec<McpApplyJournalEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpRollbackExecutionRequest {
    pub run_id: String,
    pub actor: String,
    pub backups: Vec<McpBackupDescriptor>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpRollbackExecutionResult {
    pub actions: Vec<McpApplyActionResult>,
    pub verification: McpVerificationResult,
}

#[must_use]
pub fn hash_mcp_observation(inventory: &McpInventory) -> String {
    let mut servers = inventory
        .servers
        .iter()
        .map(stable_server)
        .collect::<Vec<_>>();
    servers.sort_by_key(stable_json);
    let mut observations = inventory
        .observations
        .iter()
        .map(stable_observation)
        .collect::<Vec<_>>();
    observations.sort_by_key(stable_json);
    let mut diagnostics = inventory
        .diagnostics
        .iter()
        .map(|item| {
            json!({
                "code": item.code,
                "severity": item.severity,
                "runtime": item.runtime,
                "serverId": item.server_id,
                "message": item.message,
                "evidence": stable_evidence(&item.evidence),
            })
        })
        .collect::<Vec<_>>();
    diagnostics.sort_by_key(stable_json);
    hash_value(&json!({
        "servers": servers,
        "observations": observations,
        "diagnostics": diagnostics,
    }))
}

#[must_use]
pub fn hash_mcp_config(state: &McpEffectiveDesiredState) -> String {
    let mut stable = state.clone();
    stable.servers.sort_by(|left, right| {
        (&left.desired.runtime, &left.desired.server_id)
            .cmp(&(&right.desired.runtime, &right.desired.server_id))
    });
    stable.conflicts.sort_by(|left, right| {
        (&left.runtime, &left.server_id, left.precedence).cmp(&(
            &right.runtime,
            &right.server_id,
            right.precedence,
        ))
    });
    stable.resolution.sort_by(|left, right| {
        (&left.profile_id, &left.binding_id).cmp(&(&right.profile_id, &right.binding_id))
    });
    hash_value(&serde_json::to_value(stable).expect("effective state is serializable"))
}

#[must_use]
pub fn hash_mcp_capabilities(snapshot: &McpCapabilitySnapshot) -> String {
    let mut runtimes = snapshot.runtimes.clone();
    runtimes.sort_by(|left, right| left.runtime.cmp(&right.runtime));
    hash_value(&serde_json::to_value(runtimes).expect("capabilities are serializable"))
}

/// Binds an adapter capability snapshot into an existing dry-run plan. The
/// generated plan hash changes whenever the adapter contract or binary version
/// changes, invalidating approvals without relying on wall-clock timestamps.
pub fn bind_mcp_plan_capabilities(plan: &mut McpPlan, snapshot: &McpCapabilitySnapshot) {
    plan.capability_hash = hash_mcp_capabilities(snapshot);
    plan.plan_hash = hash_value(&json!({
        "observationHash": plan.observation_hash,
        "configHash": plan.config_hash,
        "capabilityHash": plan.capability_hash,
        "actions": plan.actions,
    }));
}

#[must_use]
pub fn generate_mcp_plan(
    id: String,
    generated_at: String,
    state: McpEffectiveDesiredState,
    inventory: &McpInventory,
) -> McpPlan {
    let observation_hash = hash_mcp_observation(inventory);
    let config_hash = hash_mcp_config(&state);
    let mut actions = Vec::new();
    let observed_servers = inventory
        .servers
        .iter()
        .map(|server| (server.id.as_str(), server))
        .collect::<BTreeMap<_, _>>();
    let desired_keys = state
        .servers
        .iter()
        .map(|server| {
            (
                server.desired.runtime.as_str(),
                server.desired.server_id.as_str(),
            )
        })
        .collect::<BTreeSet<_>>();
    let has_authoritative_profile = !state.resolution.is_empty();
    let target_label = state.target.label();

    for conflict in &state.conflicts {
        actions.push(manual_action(
            &target_label,
            &conflict.runtime,
            &conflict.server_id,
            "profile conflict blocks deterministic planning",
            Vec::new(),
            None,
            None,
        ));
    }

    for desired in &state.servers {
        let item = &desired.desired;
        let observation = inventory.observations.iter().find(|observation| {
            observation.runtime == item.runtime && observation.server_id == item.server_id
        });
        let observed_server = observed_servers.get(item.server_id.as_str()).copied();
        if !matches!(
            item.runtime.as_str(),
            "codex" | "cursor" | "claude" | "grok"
        ) {
            actions.push(manual_desired_action(
                &target_label,
                desired,
                "Runtime has no Phase 2A planning contract",
                observation,
                observed_server,
            ));
            continue;
        }
        let Some(observation) = observation else {
            if item.desired_enabled {
                actions.push(manual_desired_action(
                    &target_label,
                    desired,
                    "no observation exists; add/configure cannot be represented as safely automatable",
                    None,
                    None,
                ));
            }
            continue;
        };
        if observation.evidence.is_empty() {
            actions.push(manual_desired_action(
                &target_label,
                desired,
                "observation has no evidence",
                Some(observation),
                observed_server,
            ));
            continue;
        }
        if !observation.configured && item.desired_enabled {
            actions.push(action(
                McpPlanActionKind::AddConfigure,
                &target_label,
                desired,
                "desired server is discovered but not configured",
                observation,
                observed_server,
                McpRiskLevel::Medium,
            ));
        } else if observation.configured {
            let desired_fingerprint = item.definition.as_ref().map(mcp_definition_fingerprint);
            if desired_fingerprint.is_some()
                && observed_server.map(|server| server.endpoint_fingerprint.as_str())
                    != desired_fingerprint.as_deref()
            {
                actions.push(action(
                    McpPlanActionKind::Update,
                    &target_label,
                    desired,
                    "desired definition fingerprint differs from observation",
                    observation,
                    observed_server,
                    McpRiskLevel::Medium,
                ));
            }
            match observation.enabled {
                Some(enabled) if enabled != item.desired_enabled => actions.push(action(
                    if item.desired_enabled {
                        McpPlanActionKind::Enable
                    } else {
                        McpPlanActionKind::Disable
                    },
                    &target_label,
                    desired,
                    "desired enabled state differs from observation",
                    observation,
                    observed_server,
                    if item.desired_enabled {
                        McpRiskLevel::Low
                    } else {
                        McpRiskLevel::Medium
                    },
                )),
                None => actions.push(manual_desired_action(
                    &target_label,
                    desired,
                    "enabled state is unknown",
                    Some(observation),
                    observed_server,
                )),
                _ => {}
            }
        }
        if item.desired_enabled && observation.approved == Some(false) {
            actions.push(action(
                McpPlanActionKind::ApprovalRequired,
                &target_label,
                desired,
                "Runtime reports approval is required",
                observation,
                observed_server,
                McpRiskLevel::High,
            ));
        }
        if item.desired_enabled
            && (observation.authenticated == Some(false)
                || (!item.secret_refs.is_empty() && observation.authenticated != Some(true)))
        {
            actions.push(action(
                McpPlanActionKind::AuthenticationRequired,
                &target_label,
                desired,
                "credentials must be supplied through secret references by a future apply flow",
                observation,
                observed_server,
                McpRiskLevel::High,
            ));
        }
        if item.desired_enabled && observation.approved.is_none() {
            actions.push(manual_desired_action(
                &target_label,
                desired,
                "approval state is unknown",
                Some(observation),
                observed_server,
            ));
        }
        if item.desired_enabled && observation.authenticated.is_none() {
            actions.push(manual_desired_action(
                &target_label,
                desired,
                "authentication state is unknown",
                Some(observation),
                observed_server,
            ));
        }
        if !item.allow_tools.is_empty()
            || !item.deny_tools.is_empty()
            || item.approval_mode != McpApprovalMode::Manual
        {
            actions.push(manual_desired_action(
                &target_label,
                desired,
                "observed tool policy is unavailable; policy change requires manual verification",
                Some(observation),
                observed_server,
            ));
        }
    }

    for observation in inventory.observations.iter().filter(|observation| {
        has_authoritative_profile
            && observation.configured
            && !desired_keys
                .contains(&(observation.runtime.as_str(), observation.server_id.as_str()))
    }) {
        if observation.evidence.is_empty() {
            actions.push(manual_action(
                &target_label,
                &observation.runtime,
                &observation.server_id,
                "configured server is absent from desired state, but removal evidence is missing",
                Vec::new(),
                None,
                observation.schema_hash.clone(),
            ));
            continue;
        }
        let observed_server = observed_servers
            .get(observation.server_id.as_str())
            .copied();
        let fingerprint = observed_server.map_or_else(
            || observation.server_id.clone(),
            |server| server.endpoint_fingerprint.clone(),
        );
        actions.push(McpPlanAction {
            kind: McpPlanActionKind::Remove,
            runtime: observation.runtime.clone(),
            scope: "effective_target".to_owned(),
            target: target_label.clone(),
            server_id: observation.server_id.clone(),
            server_fingerprint: fingerprint,
            before: before_summary(observation, observed_server),
            after: McpStateSummary {
                configured: Some(false),
                enabled: Some(false),
                endpoint_fingerprint: None,
                allow_tools: Vec::new(),
                deny_tools: Vec::new(),
                approval_mode: None,
                secret_ref_count: 0,
            },
            risk: McpRiskLevel::Critical,
            reason: "observed configured server is absent from effective desired state".to_owned(),
            evidence: stable_evidence_items(&observation.evidence),
            expected_source_hash: source_hash(observation),
            expected_schema_hash: observation.schema_hash.clone(),
            blocked: false,
        });
    }

    actions.sort_by(action_order);
    let plan_hash = hash_value(&json!({
        "observationHash": observation_hash,
        "configHash": config_hash,
        "actions": actions,
    }));
    McpPlan {
        id,
        target: state.target.clone(),
        effective_desired_state: state,
        actions,
        observation_hash,
        config_hash,
        capability_hash: String::new(),
        plan_hash,
        generated_at,
        dry_run: true,
        applied: false,
    }
}

fn action(
    kind: McpPlanActionKind,
    target: &str,
    desired: &McpEffectiveServer,
    reason: &str,
    observation: &ObservedMcpInstance,
    observed_server: Option<&McpServer>,
    base_risk: McpRiskLevel,
) -> McpPlanAction {
    let requested_risk = desired.desired.risk_override.unwrap_or(McpRiskLevel::Low);
    let context_risk = if desired.high_risk_context || !desired.desired.allow_tools.is_empty() {
        McpRiskLevel::High
    } else {
        McpRiskLevel::Low
    };
    let risk = base_risk.max(requested_risk).max(context_risk);
    McpPlanAction {
        kind,
        runtime: desired.desired.runtime.clone(),
        scope: precedence_label(desired.inherited_from).to_owned(),
        target: target.to_owned(),
        server_id: desired.desired.server_id.clone(),
        server_fingerprint: desired.desired.definition.as_ref().map_or_else(
            || desired.desired.server_id.clone(),
            mcp_definition_fingerprint,
        ),
        before: before_summary(observation, observed_server),
        after: after_summary(&desired.desired),
        risk,
        reason: reason.to_owned(),
        evidence: stable_evidence_items(&observation.evidence),
        expected_source_hash: source_hash(observation),
        expected_schema_hash: observation.schema_hash.clone(),
        blocked: matches!(
            kind,
            McpPlanActionKind::ApprovalRequired | McpPlanActionKind::AuthenticationRequired
        ),
    }
}

fn manual_action(
    target: &str,
    runtime: &str,
    server_id: &str,
    reason: &str,
    evidence: Vec<McpEvidence>,
    expected_source_hash: Option<String>,
    expected_schema_hash: Option<String>,
) -> McpPlanAction {
    McpPlanAction {
        kind: McpPlanActionKind::ManualUnsupported,
        runtime: runtime.to_owned(),
        scope: "effective_target".to_owned(),
        target: target.to_owned(),
        server_id: server_id.to_owned(),
        server_fingerprint: server_id.to_owned(),
        before: empty_summary(),
        after: empty_summary(),
        risk: McpRiskLevel::High,
        reason: reason.to_owned(),
        evidence: stable_evidence_items(&evidence),
        expected_source_hash,
        expected_schema_hash,
        blocked: true,
    }
}

fn manual_desired_action(
    target: &str,
    desired: &McpEffectiveServer,
    reason: &str,
    observation: Option<&ObservedMcpInstance>,
    observed_server: Option<&McpServer>,
) -> McpPlanAction {
    McpPlanAction {
        kind: McpPlanActionKind::ManualUnsupported,
        runtime: desired.desired.runtime.clone(),
        scope: precedence_label(desired.inherited_from).to_owned(),
        target: target.to_owned(),
        server_id: desired.desired.server_id.clone(),
        server_fingerprint: desired.desired.definition.as_ref().map_or_else(
            || desired.desired.server_id.clone(),
            mcp_definition_fingerprint,
        ),
        before: observation.map_or_else(empty_summary, |value| {
            before_summary(value, observed_server)
        }),
        after: after_summary(&desired.desired),
        risk: McpRiskLevel::High,
        reason: reason.to_owned(),
        evidence: observation.map_or_else(Vec::new, |value| stable_evidence_items(&value.evidence)),
        expected_source_hash: observation.and_then(source_hash),
        expected_schema_hash: observation.and_then(|value| value.schema_hash.clone()),
        blocked: true,
    }
}

fn before_summary(
    observation: &ObservedMcpInstance,
    server: Option<&McpServer>,
) -> McpStateSummary {
    McpStateSummary {
        configured: Some(observation.configured),
        enabled: observation.enabled,
        endpoint_fingerprint: server.map(|value| value.endpoint_fingerprint.clone()),
        allow_tools: Vec::new(),
        deny_tools: Vec::new(),
        approval_mode: None,
        secret_ref_count: server.map_or(0, |value| value.secret_refs.len()),
    }
}

fn after_summary(desired: &McpDesiredServer) -> McpStateSummary {
    McpStateSummary {
        configured: Some(desired.desired_enabled),
        enabled: Some(desired.desired_enabled),
        endpoint_fingerprint: desired.definition.as_ref().map(mcp_definition_fingerprint),
        allow_tools: desired.allow_tools.clone(),
        deny_tools: desired.deny_tools.clone(),
        approval_mode: Some(desired.approval_mode),
        secret_ref_count: desired.secret_refs.len(),
    }
}

fn empty_summary() -> McpStateSummary {
    McpStateSummary {
        configured: None,
        enabled: None,
        endpoint_fingerprint: None,
        allow_tools: Vec::new(),
        deny_tools: Vec::new(),
        approval_mode: None,
        secret_ref_count: 0,
    }
}

#[must_use]
pub fn mcp_definition_fingerprint(definition: &McpCanonicalDefinition) -> String {
    hash_value(&serde_json::to_value(definition).expect("definition is serializable"))
}

fn source_hash(observation: &ObservedMcpInstance) -> Option<String> {
    if let Some(hash) = observation.evidence.iter().find_map(|item| {
        item.detail
            .split("source_sha256=")
            .nth(1)
            .and_then(|value| value.split_whitespace().next())
            .filter(|value| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .map(ToOwned::to_owned)
    }) {
        Some(hash)
    } else if observation.evidence.is_empty() {
        None
    } else {
        Some(hash_value(&Value::Array(stable_evidence(
            &observation.evidence,
        ))))
    }
}

fn action_order(left: &McpPlanAction, right: &McpPlanAction) -> Ordering {
    (
        &left.runtime,
        &left.scope,
        &left.target,
        &left.server_fingerprint,
        left.kind,
        &left.server_id,
        &left.reason,
    )
        .cmp(&(
            &right.runtime,
            &right.scope,
            &right.target,
            &right.server_fingerprint,
            right.kind,
            &right.server_id,
            &right.reason,
        ))
}

fn stable_server(server: &McpServer) -> Value {
    json!({
        "id": server.id,
        "canonicalName": server.canonical_name,
        "definition": server.definition,
        "endpointFingerprint": server.endpoint_fingerprint,
        "aliases": sorted(&server.aliases),
        "provenance": stable_evidence(&server.provenance),
        "secretRefs": server.secret_refs,
    })
}

fn stable_observation(observation: &ObservedMcpInstance) -> Value {
    json!({
        "runtime": observation.runtime,
        "serverId": observation.server_id,
        "alias": observation.alias,
        "sourcePath": observation.source_path,
        "discoverable": observation.discoverable,
        "configured": observation.configured,
        "loaded": observation.loaded,
        "enabled": observation.enabled,
        "approved": observation.approved,
        "authenticated": observation.authenticated,
        "healthy": observation.healthy,
        "startup": observation.startup,
        "currentSessionVisible": observation.current_session_visible,
        "invoked": observation.invoked,
        "toolCount": observation.tool_count,
        "schemaHash": observation.schema_hash,
        "evidence": stable_evidence(&observation.evidence),
    })
}

fn stable_evidence(evidence: &[McpEvidence]) -> Vec<Value> {
    let mut values = evidence
        .iter()
        .map(|item| {
            json!({
                "source": item.source,
                "detail": item.detail,
                "sourcePath": item.source_path,
                "provesRuntimeLoaded": item.proves_runtime_loaded,
                "provesCurrentSessionVisibility": item.proves_current_session_visibility,
            })
        })
        .collect::<Vec<_>>();
    values.sort_by_key(stable_json);
    values
}

fn stable_evidence_items(evidence: &[McpEvidence]) -> Vec<McpEvidence> {
    let mut values = evidence.to_vec();
    values.sort_by(|left, right| {
        (
            &left.source,
            &left.detail,
            &left.source_path,
            left.proves_runtime_loaded,
            left.proves_current_session_visibility,
        )
            .cmp(&(
                &right.source,
                &right.detail,
                &right.source_path,
                right.proves_runtime_loaded,
                right.proves_current_session_visibility,
            ))
    });
    values
}

fn sorted(values: &[String]) -> Vec<String> {
    let mut values = values.to_vec();
    values.sort();
    values
}

fn stable_json(value: &Value) -> String {
    serde_json::to_string(value).expect("stable value is serializable")
}

fn hash_value(value: &Value) -> String {
    let bytes = serde_json::to_vec(value).expect("stable value is serializable");
    hex_digest(Sha256::digest(bytes).as_slice())
}

fn hex_digest(digest: &[u8]) -> String {
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{McpDiagnosticSeverity, McpStartupState, McpTransport};

    fn desired(enabled: bool) -> McpDesiredServer {
        McpDesiredServer {
            server_id: "srv-docs".to_owned(),
            runtime: "codex".to_owned(),
            alias: "docs".to_owned(),
            definition: Some(McpCanonicalDefinition {
                transport: McpTransport::Http,
                command: None,
                args: Vec::new(),
                endpoint: Some("https://example.test/mcp".to_owned()),
            }),
            desired_enabled: enabled,
            allow_tools: vec!["search".to_owned()],
            deny_tools: Vec::new(),
            approval_mode: McpApprovalMode::Manual,
            risk_override: None,
            secret_refs: Vec::new(),
        }
    }

    fn profile(id: &str, name: &str, enabled: bool) -> McpProfile {
        McpProfile {
            id: id.to_owned(),
            name: name.to_owned(),
            description: None,
            version: 1,
            servers: vec![desired(enabled)],
            created_at: "2026-07-19T00:00:00Z".to_owned(),
            updated_at: "2026-07-19T00:00:00Z".to_owned(),
        }
    }

    fn binding(id: &str, profile_id: &str, target_type: McpBindingTargetType) -> McpProfileBinding {
        McpProfileBinding {
            id: id.to_owned(),
            profile_id: profile_id.to_owned(),
            target: McpBindingTarget {
                target_type,
                target_id: match target_type {
                    McpBindingTargetType::Machine => "machine-1",
                    McpBindingTargetType::Workspace => "workspace-1",
                    McpBindingTargetType::Agent => "agent-1",
                }
                .to_owned(),
            },
            version: 1,
            created_at: "2026-07-19T00:00:00Z".to_owned(),
            updated_at: "2026-07-19T00:00:00Z".to_owned(),
        }
    }

    fn target() -> McpDesiredTarget {
        McpDesiredTarget {
            machine_id: "machine-1".to_owned(),
            workspace_id: Some("workspace-1".to_owned()),
            agent_id: Some("agent-1".to_owned()),
        }
    }

    fn evidence() -> Vec<McpEvidence> {
        vec![McpEvidence {
            source: "codex_app_server".to_owned(),
            detail: "sanitized native status".to_owned(),
            source_path: Some("/tmp/config.toml".to_owned()),
            proves_runtime_loaded: true,
            proves_current_session_visibility: false,
        }]
    }

    fn observation(configured: bool, enabled: Option<bool>) -> ObservedMcpInstance {
        ObservedMcpInstance {
            runtime: "codex".to_owned(),
            server_id: "srv-docs".to_owned(),
            alias: "docs".to_owned(),
            source_path: Some("/tmp/config.toml".to_owned()),
            discoverable: true,
            configured,
            loaded: Some(configured),
            enabled,
            approved: Some(true),
            authenticated: Some(true),
            healthy: Some(true),
            startup: Some(McpStartupState::Ready),
            current_session_visible: None,
            invoked: Some(false),
            tool_count: Some(1),
            schema_hash: Some("schema-v1".to_owned()),
            evidence: evidence(),
            observed_at: "2026-07-19T00:00:00Z".to_owned(),
        }
    }

    #[test]
    fn higher_precedence_wins_and_same_precedence_conflicts() {
        let profiles = vec![
            profile("machine", "base", true),
            profile("agent-a", "agent-a", false),
            profile("agent-b", "agent-b", true),
        ];
        let resolved = resolve_mcp_desired_state(
            &profiles,
            &[
                binding("b0", "machine", McpBindingTargetType::Machine),
                binding("b1", "agent-a", McpBindingTargetType::Agent),
                binding("b2", "agent-b", McpBindingTargetType::Agent),
            ],
            target(),
        );
        assert!(resolved.servers.is_empty());
        assert_eq!(
            resolved.conflicts[0].precedence,
            McpBindingTargetType::Agent
        );
        assert_eq!(resolved.conflicts[0].profile_ids, ["agent-a", "agent-b"]);

        let resolved = resolve_mcp_desired_state(
            &profiles,
            &[
                binding("b0", "machine", McpBindingTargetType::Machine),
                binding("b1", "agent-a", McpBindingTargetType::Agent),
            ],
            target(),
        );
        assert!(!resolved.servers[0].desired.desired_enabled);
        assert_eq!(
            resolved.servers[0].inherited_from,
            McpBindingTargetType::Agent
        );
    }

    #[test]
    fn plaintext_secrets_are_rejected_without_echoing_them() {
        let mut profile = profile("p", "safe", true);
        profile.servers[0]
            .definition
            .as_mut()
            .expect("definition")
            .args = vec!["--token=super-secret-value".to_owned()];
        let error = validate_mcp_profile(&profile).expect_err("plaintext secret must fail");
        assert_eq!(error, "profile contains suspected plaintext secret");
        assert!(!error.contains("super-secret-value"));

        profile.servers[0]
            .definition
            .as_mut()
            .expect("definition")
            .args = vec!["--api-key".to_owned(), "sk-plaintext".to_owned()];
        let split_error =
            validate_mcp_profile(&profile).expect_err("split plaintext secret must fail");
        assert_eq!(split_error, "profile contains suspected plaintext secret");
        assert!(!split_error.contains("sk-plaintext"));

        profile.servers[0]
            .definition
            .as_mut()
            .expect("definition")
            .args = vec!["--auth-token".to_owned(), "opaque-but-plaintext".to_owned()];
        assert_eq!(
            validate_mcp_profile(&profile).expect_err("auth token must fail"),
            "profile contains suspected plaintext secret"
        );

        profile.servers[0]
            .definition
            .as_mut()
            .expect("definition")
            .args
            .clear();
        profile.servers[0].secret_refs = vec![McpSecretRef {
            location: "headers.authorization".to_owned(),
            kind: "bearer".to_owned(),
            reference: "keychain://cocli/docs-token".to_owned(),
        }];
        validate_mcp_profile(&profile).expect("opaque reference is valid");

        "secret://sk-plaintext".clone_into(&mut profile.servers[0].secret_refs[0].reference);
        assert_eq!(
            validate_mcp_profile(&profile).expect_err("secret value disguised as ref must fail"),
            "secretRefs must use an approved opaque reference scheme"
        );
    }

    #[test]
    fn plan_is_stably_sorted_hashed_and_marks_unknown_manual() {
        let state = resolve_mcp_desired_state(
            &[profile("p", "production ops", true)],
            &[binding("b", "p", McpBindingTargetType::Machine)],
            target(),
        );
        let mut unknown = observation(true, None);
        unknown.evidence.clear();
        let inventory = McpInventory {
            observations: vec![unknown],
            observed_at: "one".to_owned(),
            ..McpInventory::default()
        };
        let first = generate_mcp_plan("a".to_owned(), "one".to_owned(), state.clone(), &inventory);
        let mut changed_time = inventory.clone();
        "two".clone_into(&mut changed_time.observed_at);
        "two".clone_into(&mut changed_time.observations[0].observed_at);
        let second = generate_mcp_plan("b".to_owned(), "two".to_owned(), state, &changed_time);
        assert_eq!(first.plan_hash, second.plan_hash);
        assert_eq!(first.actions[0].kind, McpPlanActionKind::ManualUnsupported);
        assert!(first.actions[0].blocked);
        assert_eq!(first.actions[0].risk, McpRiskLevel::High);
    }

    #[test]
    fn plan_covers_configure_enable_disable_update_remove_and_gates() {
        let mut enabled_profile = profile("p", "production", true);
        enabled_profile.servers[0].secret_refs.push(McpSecretRef {
            location: "env.API_TOKEN".to_owned(),
            kind: "env".to_owned(),
            reference: "env://DOCS_TOKEN".to_owned(),
        });
        let state = resolve_mcp_desired_state(
            &[enabled_profile],
            &[binding("b", "p", McpBindingTargetType::Machine)],
            target(),
        );
        let mut observed = observation(true, Some(false));
        observed.approved = Some(false);
        observed.authenticated = Some(false);
        let stale_server = McpServer {
            id: "srv-docs".to_owned(),
            canonical_name: "docs".to_owned(),
            definition: McpCanonicalDefinition {
                transport: McpTransport::Http,
                command: None,
                args: Vec::new(),
                endpoint: Some("https://old.test/mcp".to_owned()),
            },
            endpoint_fingerprint: "old-fingerprint".to_owned(),
            aliases: vec!["docs".to_owned()],
            provenance: evidence(),
            secret_refs: Vec::new(),
        };
        let mut remove = observation(true, Some(true));
        "remove-me".clone_into(&mut remove.server_id);
        "remove".clone_into(&mut remove.alias);
        let inventory = McpInventory {
            servers: vec![stale_server],
            observations: vec![observed, remove],
            diagnostics: vec![crate::McpDiagnostic {
                code: "stable".to_owned(),
                severity: McpDiagnosticSeverity::Info,
                runtime: "codex".to_owned(),
                server_id: None,
                message: "stable".to_owned(),
                evidence: evidence(),
                observed_at: "ignored".to_owned(),
            }],
            observed_at: "ignored".to_owned(),
            ..McpInventory::default()
        };
        let plan = generate_mcp_plan("plan".to_owned(), "now".to_owned(), state, &inventory);
        let kinds = plan
            .actions
            .iter()
            .map(|action| action.kind)
            .collect::<BTreeSet<_>>();
        assert!(kinds.contains(&McpPlanActionKind::Enable));
        assert!(kinds.contains(&McpPlanActionKind::Update));
        assert!(kinds.contains(&McpPlanActionKind::Remove));
        assert!(kinds.contains(&McpPlanActionKind::ApprovalRequired));
        assert!(kinds.contains(&McpPlanActionKind::AuthenticationRequired));
        assert!(plan
            .actions
            .iter()
            .all(|action| action.risk >= McpRiskLevel::High));

        let configure_inventory = McpInventory {
            observations: vec![observation(false, Some(false))],
            ..McpInventory::default()
        };
        let configure_state = resolve_mcp_desired_state(
            &[profile("p", "base", true)],
            &[binding("b", "p", McpBindingTargetType::Machine)],
            target(),
        );
        let configure = generate_mcp_plan(
            "configure".to_owned(),
            "now".to_owned(),
            configure_state,
            &configure_inventory,
        );
        assert!(configure
            .actions
            .iter()
            .any(|action| action.kind == McpPlanActionKind::AddConfigure));

        let disable_state = resolve_mcp_desired_state(
            &[profile("p", "base", false)],
            &[binding("b", "p", McpBindingTargetType::Machine)],
            target(),
        );
        let disable_inventory = McpInventory {
            observations: vec![observation(true, Some(true))],
            ..McpInventory::default()
        };
        let disable = generate_mcp_plan(
            "disable".to_owned(),
            "now".to_owned(),
            disable_state,
            &disable_inventory,
        );
        assert!(disable
            .actions
            .iter()
            .any(|action| action.kind == McpPlanActionKind::Disable));

        let unmanaged = generate_mcp_plan(
            "unmanaged".to_owned(),
            "now".to_owned(),
            resolve_mcp_desired_state(&[], &[], target()),
            &disable_inventory,
        );
        assert!(unmanaged.actions.is_empty());
    }
}
