//! Local HTTP API and runtime-neutral application service.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path as FsPath, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::extract::{Path, Query, Request, State};
use axum::http::{header, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use cocli_store::{
    Agent, AgentActivity, AgentLifecycleStatus, AgentOperation, AgentSession, AgentSessionFinish,
    AgentStatus, AgentTurn, Channel, ChannelAgent, Delivery, DeliveryStats, MemoryDocument,
    MemoryDocumentEntry, MemoryMoveResult, MemoryNamespace, MemoryScope, MemoryTopic, Message,
    MessageRole, NewAgentTurn, SkillLibraryFile, Store, StoreError, SubjectType, SubjectWorkspace,
    Task, TaskStatus, Workspace, WorkspaceBinding,
};
use serde::{Deserialize, Serialize};
use tokio::sync::{watch, Mutex, Notify};
use uuid::Uuid;

mod skill_apply;
mod skill_apply_http;
mod skill_governance;
mod skill_governance_http;
mod skill_http;
mod skill_import;
mod skill_managed_http;

/// Deterministic content and manifest digests used by trusted local Skill
/// profiles. This helper is read-only and never executes artifact content.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GovernanceArtifactDigests {
    pub content_digest: String,
    pub manifest_digest: String,
}

/// Computes Phase 3B digests for a local Skill directory without mutating it.
pub fn governance_artifact_digests(root: &FsPath) -> Result<GovernanceArtifactDigests, String> {
    let artifact = skill_apply::load_local_artifact(root)?;
    Ok(GovernanceArtifactDigests {
        content_digest: artifact.content_digest,
        manifest_digest: artifact.manifest_digest,
    })
}

/// Reconciles SQLite-managed skill installs with runtime workspace files.
///
/// This runs before the local listener starts so interrupted install,
/// refresh, uninstall, and delete operations cannot leave a split-brain
/// catalog visible to the next process.
pub async fn reconcile_skill_state(
    store: &Store,
    runtime: &Arc<dyn RuntimeService>,
) -> Result<(), RuntimeError> {
    skill_http::reconcile_skill_state(store, runtime).await
}

/// Runtime discovery information consumed by the local product surface.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RuntimeInfo {
    /// Stable runtime registry key.
    pub name: String,
    /// Whether the runtime can be started on this machine.
    pub installed: bool,
    /// Discovered executable path.
    pub binary: Option<String>,
    /// Discovered runtime version.
    pub version: Option<String>,
    /// Models offered by the runtime.
    pub models: Vec<String>,
    /// Runtime-neutral capability names.
    pub capabilities: Vec<String>,
    /// Structured human-readable reason for an unavailable runtime.
    pub unavailable_reason: Option<String>,
}

/// A best-effort event emitted while a local agent is executing.
///
/// Live events are an observability surface only. Durable messages and runtime
/// history remain the source of truth when a client reconnects or misses an
/// event.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveEvent {
    pub kind: String,
    pub channel_id: Option<Uuid>,
    pub agent_id: Option<Uuid>,
    pub message_id: Option<Uuid>,
    pub payload: serde_json::Value,
    pub occurred_at: DateTime<Utc>,
}

impl LiveEvent {
    /// Creates a timestamped event with the supplied routing identifiers.
    pub fn new(
        kind: impl Into<String>,
        channel_id: Option<Uuid>,
        agent_id: Option<Uuid>,
        message_id: Option<Uuid>,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            kind: kind.into(),
            channel_id,
            agent_id,
            message_id,
            payload,
            occurred_at: Utc::now(),
        }
    }
}

/// Best-effort sink used to expose live execution without coupling runtime
/// correctness to a connected UI.
#[async_trait]
pub trait LiveEventSink: Send + Sync {
    async fn emit(&self, event: LiveEvent);
}

#[derive(Debug, Default)]
struct NoLiveEventSink;

#[async_trait]
impl LiveEventSink for NoLiveEventSink {
    async fn emit(&self, _event: LiveEvent) {}
}

/// Live local runtime session state for one agent.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RuntimeSessionStatus {
    pub agent_id: Uuid,
    pub session_id: String,
    pub runtime: String,
    pub model: Option<String>,
    pub running: bool,
    pub recovering: bool,
    pub active_turn: bool,
    pub supports_turn_cancel: bool,
    pub supports_turn_steer: bool,
    pub supports_thread_fork: bool,
    pub input_tokens: u64,
    pub context_window_tokens: u64,
    pub context_util_pct: f64,
    pub tier: String,
    pub fork_suggested: bool,
    pub session_age_seconds: u64,
    pub recovery: Option<RuntimeRecoveryStatus>,
}

impl RuntimeSessionStatus {
    fn stateless(agent: &Agent, running: bool) -> Self {
        Self {
            agent_id: agent.id,
            session_id: String::new(),
            runtime: agent.runtime.clone(),
            model: agent.model.clone(),
            running,
            recovering: false,
            active_turn: false,
            supports_turn_cancel: false,
            supports_turn_steer: false,
            supports_thread_fork: false,
            input_tokens: 0,
            context_window_tokens: 0,
            context_util_pct: 0.0,
            tier: "healthy".to_owned(),
            fork_suggested: false,
            session_age_seconds: 0,
            recovery: None,
        }
    }
}

/// Quota/rate-limit recovery state for one local runtime session.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RuntimeRecoveryStatus {
    pub provider: String,
    pub reason: String,
    pub expected_recovery_at_ms: i64,
}

/// Result returned by an explicit local recovery probe.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RuntimeRecoveryProbeResult {
    pub result: String,
    pub detail: String,
}

/// Result of a native or restart-backed local thread fork.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RuntimeForkResult {
    pub fork_id: String,
    pub native: bool,
}

/// Process-local runtime metrics exposed by the local server.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct RuntimeMetricsSnapshot {
    pub counters: BTreeMap<String, i64>,
    pub gauges: BTreeMap<String, f64>,
}

/// Runtime skill-loading confidence shown by the local management UI.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeSkillCompatibility {
    Supported,
    Uncertain,
    Unsupported,
    Unknown,
}

/// Skill discovered in one runtime's global or workspace search roots.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSkill {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub user_invocable: bool,
    #[serde(rename = "type")]
    pub skill_type: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install_path: Option<String>,
}

/// Evidence behind one inventory or doctor result.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSkillEvidence {
    pub source: String,
    pub detail: String,
    pub proves_session_visibility: bool,
}

impl Default for RuntimeSkillEvidence {
    fn default() -> Self {
        Self {
            source: "filesystem".to_owned(),
            detail: "runtime-reported skill candidates".to_owned(),
            proves_session_visibility: false,
        }
    }
}

/// One runtime search root and its filesystem health.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSkillSearchPath {
    pub path: String,
    pub scope: String,
    pub exists: bool,
    pub readable: bool,
    pub symlink: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issue: Option<String>,
}

/// Actionable inventory or doctor finding.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSkillIssue {
    /// Stable identifier for the logical root cause, independent of display order.
    pub fingerprint: String,
    pub code: String,
    pub severity: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_name: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_paths: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_codes: Vec<String>,
}

/// One discovered candidate plus the evidence needed to interpret it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSkillFinding {
    #[serde(flatten)]
    pub skill: RuntimeSkill,
    pub runtime: String,
    /// Stable identity used to deduplicate filesystem aliases and machine/Agent overlays.
    pub fingerprint: String,
    pub scope: String,
    pub source_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_path: Option<String>,
    pub presence: String,
    pub evidence: RuntimeSkillEvidence,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid: Option<bool>,
    pub duplicate: bool,
    pub shadowed: bool,
    pub issues: Vec<RuntimeSkillIssue>,
}

/// Runtime-owned canonical destination for one governed Agent Skill.
///
/// The governance applier never accepts an arbitrary target path from an API
/// caller. It asks the Runtime adapter to derive the search root and entry from
/// the Agent workspace and logical Skill name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GovernanceSkillTarget {
    pub scope_root: PathBuf,
    pub search_root: PathBuf,
    pub entry_path: PathBuf,
}

/// Read-only capability evidence for one Runtime-derived Skill root.
///
/// API callers never provide `path` back as an automatic mutation target. The
/// Runtime service resolves the target again from the canonical scope before
/// every governed write.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GovernanceScopeCapability {
    pub runtime: String,
    pub scope: String,
    pub root_kind: String,
    pub path: String,
    pub status: String,
    pub exists: bool,
    pub writable: bool,
    pub atomic_rename: bool,
    pub supported: bool,
    pub evidence: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_reason: Option<String>,
}

/// Filesystem/native-probe report returned by the runtime boundary.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSkillInspection {
    pub observed_at: DateTime<Utc>,
    pub runtime: String,
    pub compatibility: RuntimeSkillCompatibility,
    pub evidence: RuntimeSkillEvidence,
    pub search_paths: Vec<RuntimeSkillSearchPath>,
    pub skills: Vec<RuntimeSkillFinding>,
    pub issues: Vec<RuntimeSkillIssue>,
}

/// One file entry exposed by the installed-skill browser.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSkillFileEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: i64,
}

/// File content returned by the installed-skill browser.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RuntimeSkillFileContent {
    pub content: String,
    pub binary: bool,
}

/// Durable history events emitted by a local runtime service.
#[derive(Clone, Debug, PartialEq)]
pub enum RuntimeHistoryEvent {
    SessionStarted {
        agent_id: Uuid,
        channel_id: Option<Uuid>,
        session_id: String,
        launch_id: String,
        started_at: DateTime<Utc>,
    },
    SessionEnded {
        agent_id: Uuid,
        launch_id: String,
        end_reason: String,
        turn_count: i64,
        input_tokens: i64,
        output_tokens: i64,
        cost_usd: f64,
        context_window: i64,
        ended_at: DateTime<Utc>,
    },
    TurnCompleted {
        agent_id: Uuid,
        channel_id: Uuid,
        source_message_id: Option<Uuid>,
        session_id: String,
        launch_id: String,
        turn_number: i64,
        started_at: DateTime<Utc>,
        ended_at: DateTime<Utc>,
        input_tokens: i64,
        output_tokens: i64,
        cost_usd: f64,
        context_window: i64,
        entries: serde_json::Value,
    },
    Activity {
        agent_id: Uuid,
        session_id: String,
        launch_id: String,
        activity: String,
        detail: Option<String>,
        trajectory: Vec<String>,
        created_at: DateTime<Utc>,
    },
}

/// Runtime-neutral sink for durable session, turn, and activity history.
#[async_trait]
pub trait RuntimeHistorySink: Send + Sync {
    async fn record(&self, event: RuntimeHistoryEvent) -> Result<(), StoreError>;
}

/// One SQLite-backed memory document materialized into a runtime workspace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeKnowledgeFile {
    /// Safe path relative to the per-agent workspace.
    pub relative_path: String,
    /// Complete Markdown document body.
    pub content: String,
}

/// Durable knowledge loaded before a runtime starts or handles another turn.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RuntimeKnowledgeSnapshot {
    /// Files managed by cocli inside the runtime workspace.
    pub files: Vec<RuntimeKnowledgeFile>,
    /// Agent-private L1 memory index.
    pub agent_index: String,
    /// Combined L2 memory indexes for all member Channels.
    pub channel_index: String,
}

/// Runtime-neutral source for durable local knowledge.
#[async_trait]
pub trait RuntimeKnowledgeProvider: Send + Sync {
    async fn snapshot(&self, agent: &Agent) -> Result<RuntimeKnowledgeSnapshot, RuntimeError>;
}

/// Runtime-neutral source for per-agent local bridge capability tokens.
#[async_trait]
pub trait RuntimeBridgeTokenProvider: Send + Sync {
    async fn token(&self, agent_id: Uuid) -> Result<String, RuntimeError>;
}

/// SQLite bridge token provider installed by the local server.
#[derive(Clone, Debug)]
pub struct SqliteRuntimeBridgeTokenProvider {
    store: Store,
}

impl SqliteRuntimeBridgeTokenProvider {
    pub fn new(store: Store) -> Self {
        Self { store }
    }
}

#[async_trait]
impl RuntimeBridgeTokenProvider for SqliteRuntimeBridgeTokenProvider {
    async fn token(&self, agent_id: Uuid) -> Result<String, RuntimeError> {
        self.store
            .ensure_agent_bridge_token(agent_id)
            .await
            .map_err(|error| {
                RuntimeError::Delivery(format!(
                    "failed to load local bridge capability token: {error}"
                ))
            })
    }
}

/// SQLite knowledge provider installed by the local server.
#[derive(Clone, Debug)]
pub struct SqliteRuntimeKnowledgeProvider {
    store: Store,
}

impl SqliteRuntimeKnowledgeProvider {
    pub fn new(store: Store) -> Self {
        Self { store }
    }
}

#[async_trait]
impl RuntimeKnowledgeProvider for SqliteRuntimeKnowledgeProvider {
    async fn snapshot(&self, agent: &Agent) -> Result<RuntimeKnowledgeSnapshot, RuntimeError> {
        let agent_namespace = MemoryNamespace::Agent(agent.id);
        let agent_entries = self
            .store
            .list_memory_namespace(agent_namespace)
            .await
            .map_err(|error| {
                RuntimeError::Delivery(format!("failed to load durable runtime memory: {error}"))
            })?;
        let channels = self
            .store
            .list_agent_channels(agent.id)
            .await
            .map_err(|error| {
                RuntimeError::Delivery(format!("failed to load agent channel memberships: {error}"))
            })?;

        let agent_index = memory_index_from_entries(&agent_entries, &agent_namespace.index_path());
        let mut channel_indexes = Vec::new();
        let mut files = Vec::with_capacity(agent_entries.len());
        append_runtime_knowledge_files(
            &mut files,
            agent_entries,
            &agent_namespace.prefix(),
            "memory",
        )?;
        for channel in channels {
            let namespace = MemoryNamespace::Channel(channel.id);
            let entries = self
                .store
                .list_memory_namespace(namespace)
                .await
                .map_err(|error| {
                    RuntimeError::Delivery(format!(
                        "failed to load durable channel memory for {}: {error}",
                        channel.id
                    ))
                })?;
            let index = memory_index_from_entries(&entries, &namespace.index_path());
            if !index.is_empty() {
                channel_indexes.push(format!("## #{} ({})\n\n{index}", channel.name, channel.id));
            }
            append_runtime_knowledge_files(
                &mut files,
                entries,
                &namespace.prefix(),
                &format!("memory/channels/{}", channel.id),
            )?;
        }
        files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));

        Ok(RuntimeKnowledgeSnapshot {
            files,
            agent_index,
            channel_index: channel_indexes.join("\n\n"),
        })
    }
}

fn memory_index_from_entries(entries: &[MemoryDocumentEntry], index_path: &str) -> String {
    entries
        .iter()
        .find(|entry| entry.path == index_path)
        .map(|entry| entry.body.clone())
        .unwrap_or_default()
}

fn append_runtime_knowledge_files(
    output: &mut Vec<RuntimeKnowledgeFile>,
    entries: Vec<MemoryDocumentEntry>,
    source_prefix: &str,
    target_prefix: &str,
) -> Result<(), RuntimeError> {
    for entry in entries {
        let relative = entry.path.strip_prefix(source_prefix).ok_or_else(|| {
            RuntimeError::Delivery(format!(
                "invalid durable memory path outside namespace: {}",
                entry.path
            ))
        })?;
        if relative.is_empty()
            || relative.starts_with('/')
            || relative
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..")
        {
            return Err(RuntimeError::Delivery(format!(
                "invalid durable memory path: {}",
                entry.path
            )));
        }
        output.push(RuntimeKnowledgeFile {
            relative_path: format!("{target_prefix}/{relative}"),
            content: entry.body,
        });
    }
    Ok(())
}

/// SQLite implementation installed by the local server after it opens the store.
#[derive(Clone, Debug)]
pub struct SqliteRuntimeHistorySink {
    store: Store,
}

impl SqliteRuntimeHistorySink {
    pub fn new(store: Store) -> Self {
        Self { store }
    }
}

#[async_trait]
impl RuntimeHistorySink for SqliteRuntimeHistorySink {
    async fn record(&self, event: RuntimeHistoryEvent) -> Result<(), StoreError> {
        match event {
            RuntimeHistoryEvent::SessionStarted {
                agent_id,
                channel_id,
                session_id,
                launch_id,
                started_at,
            } => {
                self.store
                    .create_agent_session(
                        agent_id,
                        channel_id,
                        &session_id,
                        Some(&launch_id),
                        None,
                        "chat",
                        started_at,
                    )
                    .await?;
            }
            RuntimeHistoryEvent::SessionEnded {
                agent_id,
                launch_id,
                end_reason,
                turn_count,
                input_tokens,
                output_tokens,
                cost_usd,
                context_window,
                ended_at,
            } => {
                self.store
                    .finish_agent_session(
                        agent_id,
                        &launch_id,
                        &AgentSessionFinish {
                            end_reason,
                            turn_count,
                            input_tokens,
                            output_tokens,
                            cost_usd,
                            context_window,
                            task_summary: None,
                            files_changed: None,
                            task_success: None,
                            ended_at,
                        },
                    )
                    .await?;
            }
            RuntimeHistoryEvent::TurnCompleted {
                agent_id,
                channel_id,
                source_message_id,
                session_id,
                launch_id,
                turn_number,
                started_at,
                ended_at,
                input_tokens,
                output_tokens,
                cost_usd,
                context_window,
                entries,
            } => {
                self.store
                    .upsert_agent_turn(&NewAgentTurn {
                        agent_id,
                        session_id,
                        launch_id: Some(launch_id),
                        turn_number,
                        started_at,
                        ended_at: Some(ended_at),
                        input_tokens,
                        output_tokens,
                        cost_usd,
                        context_window,
                        entries,
                        session_type: "chat".to_owned(),
                        channel_id: Some(channel_id),
                        source_message_id,
                    })
                    .await?;
            }
            RuntimeHistoryEvent::Activity {
                agent_id,
                session_id,
                launch_id,
                activity,
                detail,
                trajectory,
                created_at,
            } => {
                let session_row_id = self
                    .store
                    .current_agent_session(agent_id)
                    .await?
                    .filter(|session| session.launch_id.as_deref() == Some(launch_id.as_str()))
                    .map(|session| session.id);
                self.store
                    .insert_agent_activity(
                        agent_id,
                        session_row_id,
                        Some(&session_id),
                        &activity,
                        detail.as_deref(),
                        &trajectory,
                        Some(&launch_id),
                        created_at,
                    )
                    .await?;
            }
        }
        Ok(())
    }
}

/// Runtime failures surfaced to the HTTP application layer.
#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    /// The runtime rejected or failed a message delivery.
    #[error("{0}")]
    Delivery(String),
    /// The requested control is not supported by the selected runtime.
    #[error("{0}")]
    Unsupported(String),
    /// The runtime is already processing another turn.
    #[error("{0}")]
    Busy(String),
    /// No live runtime session exists for the requested agent.
    #[error("{0}")]
    NotFound(String),
}

/// Runtime-neutral boundary used by the local product loop.
#[async_trait]
pub trait RuntimeService: Send + Sync {
    /// Installs the durable history sink after the local store is available.
    fn set_history_sink(&self, _sink: Arc<dyn RuntimeHistorySink>) {}

    /// Installs the durable knowledge provider after the local store is available.
    fn set_knowledge_provider(&self, _provider: Arc<dyn RuntimeKnowledgeProvider>) {}

    /// Installs the scoped bridge-token provider after local storage opens.
    fn set_bridge_token_provider(&self, _provider: Arc<dyn RuntimeBridgeTokenProvider>) {}

    /// Installs the best-effort live execution event sink.
    fn set_live_event_sink(&self, _sink: Arc<dyn LiveEventSink>) {}

    /// Returns the current runtime catalog.
    async fn list(&self) -> Vec<RuntimeInfo>;

    /// Delivers a user message and returns one completed reply.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError`] when the runtime cannot complete delivery.
    async fn reply(&self, agent: &Agent, message: &Message) -> Result<String, RuntimeError>;

    /// Ensures a live local session exists for an agent.
    async fn start(&self, agent: &Agent) -> Result<RuntimeSessionStatus, RuntimeError> {
        Ok(RuntimeSessionStatus::stateless(agent, true))
    }

    /// Stops and forgets a live local session.
    async fn stop(&self, _agent_id: Uuid) -> Result<(), RuntimeError> {
        Ok(())
    }

    /// Cancels the active turn.
    async fn cancel(&self, _agent_id: Uuid) -> Result<(), RuntimeError> {
        Err(RuntimeError::Unsupported(
            "turn cancellation is not supported".to_owned(),
        ))
    }

    /// Steers the active turn.
    async fn steer(&self, _agent_id: Uuid, _input: &str) -> Result<(), RuntimeError> {
        Err(RuntimeError::Unsupported(
            "turn steering is not supported".to_owned(),
        ))
    }

    /// Forks the current thread, natively or by restarting with fresh context.
    async fn fork(&self, _agent: &Agent, _reason: &str) -> Result<RuntimeForkResult, RuntimeError> {
        Err(RuntimeError::Unsupported(
            "thread fork is not supported".to_owned(),
        ))
    }

    /// Returns the current live-session status.
    async fn status(&self, agent: &Agent) -> Result<RuntimeSessionStatus, RuntimeError> {
        Ok(RuntimeSessionStatus::stateless(agent, false))
    }

    /// Returns process-local runtime metrics.
    async fn metrics(&self) -> RuntimeMetricsSnapshot {
        RuntimeMetricsSnapshot::default()
    }

    /// Returns skill compatibility for one runtime name.
    fn skill_compatibility(&self, _runtime: &str) -> RuntimeSkillCompatibility {
        RuntimeSkillCompatibility::Unknown
    }

    /// Scans runtime-specific global and workspace skill roots.
    async fn list_skills(&self, _agent: &Agent) -> Result<Vec<RuntimeSkill>, RuntimeError> {
        Ok(Vec::new())
    }

    /// Inspects skill candidates and explains the evidence behind discovery.
    /// The default preserves compatibility for runtime implementations that
    /// have not yet added a native or filesystem doctor.
    async fn inspect_skills(&self, agent: &Agent) -> Result<RuntimeSkillInspection, RuntimeError> {
        let evidence = RuntimeSkillEvidence::default();
        let runtime = agent.runtime.clone();
        let skills = self
            .list_skills(agent)
            .await?
            .into_iter()
            .map(|skill| RuntimeSkillFinding {
                fingerprint: format!("{}:{}", runtime, skill.path),
                scope: skill.skill_type.clone(),
                source_path: skill.path.clone(),
                resolved_path: None,
                presence: "discovered".to_owned(),
                runtime: runtime.clone(),
                evidence: evidence.clone(),
                enabled: None,
                valid: None,
                duplicate: false,
                shadowed: false,
                issues: Vec::new(),
                skill,
            })
            .collect();
        Ok(RuntimeSkillInspection {
            observed_at: Utc::now(),
            runtime,
            compatibility: self.skill_compatibility(&agent.runtime),
            evidence,
            search_paths: Vec::new(),
            skills,
            issues: Vec::new(),
        })
    }

    /// Inspects user/global skill roots for a Runtime without creating an Agent.
    async fn inspect_machine_skills(
        &self,
        runtime: &str,
    ) -> Result<RuntimeSkillInspection, RuntimeError> {
        Ok(RuntimeSkillInspection {
            observed_at: Utc::now(),
            runtime: runtime.to_owned(),
            compatibility: self.skill_compatibility(runtime),
            evidence: RuntimeSkillEvidence::default(),
            search_paths: Vec::new(),
            skills: Vec::new(),
            issues: Vec::new(),
        })
    }

    /// Resolves a canonical Agent-workspace target without creating it or
    /// stopping/restarting a Runtime Session.
    async fn governance_skill_target(
        &self,
        _agent: &Agent,
        _skill_name: &str,
    ) -> Result<GovernanceSkillTarget, RuntimeError> {
        Err(RuntimeError::Unsupported(
            "runtime has no governed Agent Skill target".to_owned(),
        ))
    }

    /// Reports canonical Runtime Skill roots for a machine/user or resolved
    /// Workspace scope without creating any directory.
    async fn governance_scope_capabilities(
        &self,
        _runtime: &str,
        _scope: &str,
        _scope_root: Option<&FsPath>,
    ) -> Result<Vec<GovernanceScopeCapability>, RuntimeError> {
        Err(RuntimeError::Unsupported(
            "runtime has no governed machine/workspace Skill root contract".to_owned(),
        ))
    }

    /// Resolves one canonical machine/user or Workspace Skill target. The
    /// optional root is supplied only after cocli resolves a durable Workspace
    /// binding; arbitrary HTTP paths are never forwarded here.
    async fn governance_skill_target_in_scope(
        &self,
        _runtime: &str,
        _scope: &str,
        _scope_root: Option<&FsPath>,
        _skill_name: &str,
    ) -> Result<GovernanceSkillTarget, RuntimeError> {
        Err(RuntimeError::Unsupported(
            "runtime has no governed machine/workspace Skill target".to_owned(),
        ))
    }

    /// Returns the cocli-owned immutable artifact root. This is not a Runtime
    /// Skill search path and never confers ownership over a whole Runtime root.
    async fn governance_managed_artifact_root(&self) -> Result<PathBuf, RuntimeError> {
        Err(RuntimeError::Unsupported(
            "runtime service has no managed Skill artifact store".to_owned(),
        ))
    }

    /// Atomically installs or refreshes one library skill in the agent workspace.
    async fn install_skill(
        &self,
        _agent: &Agent,
        _skill_name: &str,
        _files: &[SkillLibraryFile],
    ) -> Result<String, RuntimeError> {
        Err(RuntimeError::Unsupported(
            "runtime skill installation is not supported".to_owned(),
        ))
    }

    /// Removes one managed skill from the agent workspace.
    async fn uninstall_skill(
        &self,
        _agent: &Agent,
        _install_path: &str,
    ) -> Result<(), RuntimeError> {
        Err(RuntimeError::Unsupported(
            "runtime skill installation is not supported".to_owned(),
        ))
    }

    /// Lists files below one managed skill install.
    async fn list_skill_files(
        &self,
        _agent: &Agent,
        _install_path: &str,
    ) -> Result<Vec<RuntimeSkillFileEntry>, RuntimeError> {
        Err(RuntimeError::Unsupported(
            "runtime skill browsing is not supported".to_owned(),
        ))
    }

    /// Reads one file below a managed skill install.
    async fn read_skill_file(
        &self,
        _agent: &Agent,
        _install_path: &str,
        _relative_path: &str,
    ) -> Result<RuntimeSkillFileContent, RuntimeError> {
        Err(RuntimeError::Unsupported(
            "runtime skill browsing is not supported".to_owned(),
        ))
    }

    /// Returns or advances quota-recovery state for one agent.
    async fn probe_recovery(
        &self,
        _agent: &Agent,
    ) -> Result<RuntimeRecoveryProbeResult, RuntimeError> {
        Err(RuntimeError::Unsupported(
            "runtime recovery probes are not supported".to_owned(),
        ))
    }
}

/// Runtime service used when no local runtime registry is available.
#[derive(Debug, Default)]
pub struct NoRuntimeService;

#[async_trait]
impl RuntimeService for NoRuntimeService {
    async fn list(&self) -> Vec<RuntimeInfo> {
        Vec::new()
    }

    async fn reply(&self, _agent: &Agent, _message: &Message) -> Result<String, RuntimeError> {
        Err(RuntimeError::Delivery(
            "no runtime service is configured".to_owned(),
        ))
    }
}

/// Deterministic fake runtime used to validate the local product loop.
#[derive(Debug, Default)]
pub struct EchoRuntimeService;

#[async_trait]
impl RuntimeService for EchoRuntimeService {
    async fn list(&self) -> Vec<RuntimeInfo> {
        vec![RuntimeInfo {
            name: "fake".to_owned(),
            installed: true,
            binary: None,
            version: Some("local-loop".to_owned()),
            models: vec!["test-model".to_owned()],
            capabilities: vec!["reply".to_owned()],
            unavailable_reason: None,
        }]
    }

    async fn reply(&self, _agent: &Agent, message: &Message) -> Result<String, RuntimeError> {
        Ok(format!("echo: {}", message.content))
    }
}

#[derive(Clone)]
pub(crate) struct AppState {
    store: Store,
    runtime: Arc<dyn RuntimeService>,
    deliveries: Arc<DeliveryCoordinator>,
    _delivery_worker: Arc<DeliveryWorker>,
    skill_mutation_locks: SkillMutationLocks,
    skill_snapshots: Arc<skill_http::SkillSnapshotCoordinator>,
    bridge_mutation_lock: Arc<Mutex<()>>,
}

type SkillMutationLocks = Arc<Mutex<HashMap<Uuid, Arc<Mutex<()>>>>>;

/// Tuning for the local SQLite-backed delivery worker.
#[derive(Clone, Copy, Debug)]
pub struct DeliveryConfig {
    pub batch_size: i64,
    pub max_attempts: i64,
    pub poll_interval: Duration,
    pub attempt_timeout: Duration,
    pub base_backoff: Duration,
    pub max_backoff: Duration,
}

impl Default for DeliveryConfig {
    fn default() -> Self {
        Self {
            batch_size: 32,
            max_attempts: 10,
            poll_interval: Duration::from_secs(3),
            attempt_timeout: Duration::from_secs(3 * 60),
            base_backoff: Duration::from_secs(2),
            max_backoff: Duration::from_secs(5 * 60),
        }
    }
}

/// Coordinates durable SQLite deliveries with the runtime service.
pub struct DeliveryCoordinator {
    store: Store,
    runtime: Arc<dyn RuntimeService>,
    live_events: Arc<dyn LiveEventSink>,
    config: DeliveryConfig,
    wake: Notify,
    ready: AtomicBool,
    ready_notify: Notify,
}

#[derive(Default)]
struct DeliveryBatchResult {
    replies: Vec<Message>,
}

enum DeliveryAttemptError {
    Retryable(RuntimeError),
    Indeterminate(String),
}

struct DeliveryWorker {
    _shutdown: watch::Sender<()>,
}

impl DeliveryCoordinator {
    fn new(
        store: Store,
        runtime: Arc<dyn RuntimeService>,
        live_events: Arc<dyn LiveEventSink>,
        config: DeliveryConfig,
    ) -> Arc<Self> {
        Arc::new(Self {
            store,
            runtime,
            live_events,
            config,
            wake: Notify::new(),
            ready: AtomicBool::new(false),
            ready_notify: Notify::new(),
        })
    }

    fn spawn(self: &Arc<Self>) -> Arc<DeliveryWorker> {
        let coordinator = Arc::clone(self);
        let (shutdown, mut shutdown_rx) = watch::channel(());
        tokio::spawn(async move {
            if let Err(error) = coordinator.store.release_in_flight_deliveries().await {
                tracing::error!(%error, "failed to release in-flight deliveries at startup");
            }
            coordinator.ready.store(true, Ordering::Release);
            coordinator.ready_notify.notify_waiters();
            coordinator.run(&mut shutdown_rx).await;
        });
        Arc::new(DeliveryWorker {
            _shutdown: shutdown,
        })
    }

    async fn wait_until_ready(&self) {
        while !self.ready.load(Ordering::Acquire) {
            let notified = self.ready_notify.notified();
            if self.ready.load(Ordering::Acquire) {
                return;
            }
            notified.await;
        }
    }

    async fn run(self: Arc<Self>, shutdown: &mut watch::Receiver<()>) {
        let mut interval = tokio::time::interval(self.config.poll_interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        interval.tick().await;
        loop {
            tokio::select! {
                _ = interval.tick() => {}
                _ = self.wake.notified() => {}
                result = shutdown.changed() => {
                    if result.is_err() {
                        break;
                    }
                }
            }
            if let Err(error) = self.dispatch_due().await {
                tracing::error!(%error, "durable delivery dispatch failed");
            }
        }
    }

    async fn dispatch_message(&self, message_id: Uuid) -> Result<DeliveryBatchResult, StoreError> {
        self.wait_until_ready().await;
        let queued = self.store.list_message_deliveries(message_id).await?;
        let mut reserved = Vec::with_capacity(queued.len());
        let now = Utc::now();
        for delivery in queued {
            if let Some(delivery) = self
                .store
                .reserve_delivery(delivery.id, self.config.max_attempts, now)
                .await?
            {
                reserved.push(delivery);
            }
        }
        let result = self.process(reserved).await?;
        self.wake.notify_one();
        Ok(result)
    }

    async fn dispatch_due(&self) -> Result<(), StoreError> {
        let reserved = self
            .store
            .reserve_due_deliveries(self.config.batch_size, self.config.max_attempts, Utc::now())
            .await?;
        let _ = self.process(reserved).await?;
        Ok(())
    }

    async fn process(&self, deliveries: Vec<Delivery>) -> Result<DeliveryBatchResult, StoreError> {
        let mut tasks = tokio::task::JoinSet::new();
        let mut task_deliveries = HashMap::new();
        for (index, delivery) in deliveries.into_iter().enumerate() {
            let Some(agent) = self.store.get_agent(delivery.agent_id).await? else {
                continue;
            };
            let Some(message) = self.store.get_message(delivery.message_id).await? else {
                continue;
            };
            let runtime = Arc::clone(&self.runtime);
            let attempt_timeout = self.config.attempt_timeout;
            let task_delivery = delivery.clone();
            let handle = tasks.spawn(async move {
                match tokio::time::timeout(attempt_timeout, runtime.reply(&agent, &message)).await {
                    Ok(result) => result.map_err(DeliveryAttemptError::Retryable),
                    Err(_) => match runtime.stop(agent.id).await {
                        Ok(()) => Err(DeliveryAttemptError::Retryable(RuntimeError::Delivery(
                            "durable delivery attempt timed out; runtime was stopped before retry"
                                .to_owned(),
                        ))),
                        Err(stop_error) => Err(DeliveryAttemptError::Indeterminate(format!(
                            "durable delivery attempt timed out and runtime stop failed; refusing automatic retry: {stop_error}"
                        ))),
                    },
                }
            });
            task_deliveries.insert(handle.id(), (index, task_delivery));
        }

        let mut completed = Vec::new();
        while let Some(result) = tasks.join_next_with_id().await {
            match result {
                Ok((task_id, result)) => {
                    if let Some((index, delivery)) = task_deliveries.remove(&task_id) {
                        completed.push((index, delivery, result));
                    }
                }
                Err(error) => {
                    if let Some((index, delivery)) = task_deliveries.remove(&error.id()) {
                        completed.push((
                            index,
                            delivery,
                            Err(DeliveryAttemptError::Retryable(RuntimeError::Delivery(
                                format!("durable delivery runtime task failed: {error}"),
                            ))),
                        ));
                    }
                    tracing::error!(%error, "durable delivery runtime task failed");
                }
            }
        }
        completed.sort_by_key(|(index, _, _)| *index);

        let mut batch = DeliveryBatchResult::default();
        for (_, delivery, result) in completed {
            match result {
                Ok(content) => {
                    let reply = self.store.complete_delivery(&delivery, &content).await?;
                    self.live_events
                        .emit(LiveEvent::new(
                            "delivery_completed",
                            Some(reply.channel_id),
                            reply.agent_id,
                            Some(reply.id),
                            serde_json::json!({ "deliveryId": delivery.id }),
                        ))
                        .await;
                    batch.replies.push(reply);
                }
                Err(DeliveryAttemptError::Retryable(error)) => {
                    let next_attempt_at = Utc::now()
                        + chrono::Duration::from_std(self.backoff(delivery.attempts))
                            .unwrap_or_else(|_| chrono::Duration::minutes(5));
                    let _ = self
                        .store
                        .defer_delivery(
                            delivery.id,
                            &error.to_string(),
                            next_attempt_at,
                            self.config.max_attempts,
                        )
                        .await?;
                }
                Err(DeliveryAttemptError::Indeterminate(error)) => {
                    let _ = self
                        .store
                        .defer_delivery(delivery.id, &error, Utc::now(), delivery.attempts)
                        .await?;
                }
            }
        }
        Ok(batch)
    }

    fn backoff(&self, attempts: i64) -> Duration {
        let exponent = attempts.saturating_sub(1).min(8) as u32;
        self.config
            .base_backoff
            .saturating_mul(1_u32 << exponent)
            .min(self.config.max_backoff)
    }
}

/// Builds the local HTTP router.
pub fn router(store: Store, runtime: Arc<dyn RuntimeService>) -> Router {
    router_with_delivery_config(store, runtime, DeliveryConfig::default())
}

/// Builds the local HTTP router with a best-effort live execution sink.
pub fn router_with_live_events(
    store: Store,
    runtime: Arc<dyn RuntimeService>,
    live_events: Arc<dyn LiveEventSink>,
) -> Router {
    router_with_delivery_config_and_live_events(
        store,
        runtime,
        DeliveryConfig::default(),
        live_events,
    )
}

/// Builds the local HTTP router with explicit durable-delivery tuning.
pub fn router_with_delivery_config(
    store: Store,
    runtime: Arc<dyn RuntimeService>,
    delivery_config: DeliveryConfig,
) -> Router {
    router_with_delivery_config_and_live_events(
        store,
        runtime,
        delivery_config,
        Arc::new(NoLiveEventSink),
    )
}

fn router_with_delivery_config_and_live_events(
    store: Store,
    runtime: Arc<dyn RuntimeService>,
    delivery_config: DeliveryConfig,
    live_events: Arc<dyn LiveEventSink>,
) -> Router {
    let deliveries = DeliveryCoordinator::new(
        store.clone(),
        Arc::clone(&runtime),
        live_events,
        delivery_config,
    );
    let delivery_worker = deliveries.spawn();
    let bridge_router = Router::new()
        .route(
            "/api/bridge/agents/:agent_id/messages",
            post(bridge_send_message),
        )
        .route(
            "/api/bridge/agents/:agent_id/inbox",
            get(bridge_check_messages),
        )
        .route(
            "/api/bridge/agents/:agent_id/history",
            get(bridge_read_history),
        )
        .route(
            "/api/bridge/agents/:agent_id/channels",
            get(bridge_list_channels).post(bridge_create_channel),
        )
        .route(
            "/api/bridge/agents/:agent_id/agents",
            get(bridge_list_agents).post(bridge_create_agent),
        )
        .route(
            "/api/bridge/agents/:agent_id/channels/join-agent",
            post(bridge_join_agent_to_channel),
        )
        .route(
            "/api/bridge/agents/:agent_id/workspaces",
            get(bridge_list_workspaces).post(bridge_attach_workspace),
        )
        .route(
            "/api/bridge/agents/:agent_id/tasks",
            get(bridge_list_tasks).post(bridge_create_tasks),
        )
        .route(
            "/api/bridge/agents/:agent_id/tasks/claim",
            post(bridge_claim_tasks),
        )
        .route(
            "/api/bridge/agents/:agent_id/tasks/unclaim",
            post(bridge_unclaim_task),
        )
        .route(
            "/api/bridge/agents/:agent_id/tasks/update-status",
            post(bridge_update_task_status),
        )
        .route(
            "/api/bridge/agents/:agent_id/tasks/dependencies",
            get(bridge_get_task_dependencies).post(bridge_add_task_dependency),
        )
        .route(
            "/api/bridge/agents/:agent_id/working",
            get(bridge_get_working_state).post(bridge_set_working_state),
        )
        .route(
            "/api/bridge/agents/:agent_id/working/clear",
            post(bridge_clear_working_state),
        )
        .route(
            "/api/bridge/agents/:agent_id/memory/index",
            get(bridge_get_memory_index),
        )
        .route(
            "/api/bridge/agents/:agent_id/memory/list",
            get(bridge_list_memory_namespace),
        )
        .route(
            "/api/bridge/agents/:agent_id/memory/topic",
            get(bridge_get_memory_topic).post(bridge_write_memory_topic),
        )
        .route(
            "/api/bridge/agents/:agent_id/memory/move",
            post(bridge_move_memory_topic),
        )
        .route_layer(middleware::from_fn_with_state(
            store.clone(),
            authorize_bridge_request,
        ));
    Router::new()
        .merge(skill_apply_http::router())
        .merge(skill_governance_http::router())
        .merge(skill_managed_http::router())
        .merge(skill_http::router())
        .merge(bridge_router)
        .route("/healthz", get(health))
        .route("/api/metrics", get(runtime_metrics))
        .route("/api/search", get(global_search))
        .route("/api/deliveries/stats", get(delivery_stats))
        .route("/api/runtimes", get(list_runtimes))
        .route("/api/channels", get(list_channels).post(create_channel))
        .route("/api/channels/:channel_id", delete(delete_channel))
        .route(
            "/api/channels/:channel_id/agents",
            get(list_channel_members).post(add_channel_member),
        )
        .route(
            "/api/channels/:channel_id/agents/:agent_id",
            delete(remove_channel_member),
        )
        .route(
            "/api/channels/:channel_id/workspaces",
            get(list_channel_workspaces).post(attach_channel_workspace),
        )
        .route(
            "/api/channels/:channel_id/workspaces/:workspace_id",
            post(attach_existing_channel_workspace).delete(detach_channel_workspace),
        )
        .route(
            "/api/channels/:channel_id/messages",
            get(list_messages).post(post_message),
        )
        .route(
            "/api/channels/:channel_id/tasks",
            get(list_tasks).post(create_task),
        )
        .route(
            "/api/channels/:channel_id/tasks/:task_number/claim",
            post(claim_task),
        )
        .route(
            "/api/channels/:channel_id/tasks/:task_number/unclaim",
            post(unclaim_task),
        )
        .route(
            "/api/channels/:channel_id/tasks/:task_number/status",
            post(update_task_status),
        )
        .route(
            "/api/channels/:channel_id/tasks/:task_number/dependencies",
            get(get_task_dependencies)
                .post(add_task_dependency)
                .delete(remove_task_dependency),
        )
        .route("/api/agents", get(list_agents).post(create_agent))
        .route("/api/agents/:agent_id/channels", get(list_agent_channels))
        .route(
            "/api/agents/:agent_id/messages",
            get(list_agent_messages).post(post_agent_message),
        )
        .route(
            "/api/agents/:agent_id/workspaces",
            get(list_agent_workspaces).post(attach_agent_workspace),
        )
        .route(
            "/api/agents/:agent_id/workspaces/:workspace_id",
            post(attach_existing_agent_workspace).delete(detach_agent_workspace),
        )
        .route(
            "/api/workspaces/:workspace_id",
            get(get_workspace)
                .put(update_workspace)
                .delete(delete_workspace),
        )
        .route(
            "/api/workspaces/:workspace_id/attachments",
            get(list_workspace_attachments),
        )
        .route(
            "/api/workspaces/:workspace_id/bindings",
            get(list_workspace_bindings),
        )
        .route(
            "/api/workspaces/:workspace_id/binding",
            get(get_current_workspace_binding).put(rebind_workspace),
        )
        .route(
            "/api/workspaces/:workspace_id/verify",
            post(verify_workspace_binding),
        )
        .route("/api/agents/:agent_id/start", post(start_agent))
        .route("/api/agents/:agent_id/stop", post(stop_agent))
        .route("/api/agents/:agent_id/runtime", get(runtime_status))
        .route("/api/agents/:agent_id/sessions", get(list_agent_sessions))
        .route(
            "/api/agents/:agent_id/sessions/current",
            get(current_agent_session),
        )
        .route(
            "/api/agents/:agent_id/sessions/:session_id/turns",
            get(list_session_turns),
        )
        .route("/api/agents/:agent_id/turns", get(list_agent_turns))
        .route("/api/agents/:agent_id/turns/:turn_id", get(get_agent_turn))
        .route("/api/agents/:agent_id/activity", get(list_agent_activity))
        .route(
            "/api/agents/:agent_id/operations",
            get(list_agent_operations),
        )
        .route(
            "/api/agents/:agent_id/working",
            get(get_agent_working_state),
        )
        .route(
            "/api/agents/:agent_id/memory/index",
            get(get_agent_memory_index),
        )
        .route(
            "/api/agents/:agent_id/memory/topic",
            get(get_agent_memory_topic),
        )
        .route(
            "/api/channels/:channel_id/memory/index",
            get(get_channel_memory_index),
        )
        .route(
            "/api/channels/:channel_id/memory/topic",
            get(get_channel_memory_topic),
        )
        .route("/api/agents/:agent_id/turn/cancel", post(cancel_turn))
        .route("/api/agents/:agent_id/turn/steer", post(steer_turn))
        .route("/api/agents/:agent_id/thread/fork", post(fork_thread))
        .route("/api/agents/:agent_id/recovery/probe", post(probe_recovery))
        .with_state(AppState {
            store,
            runtime,
            deliveries,
            _delivery_worker: delivery_worker,
            skill_mutation_locks: Arc::new(Mutex::new(HashMap::new())),
            skill_snapshots: skill_http::SkillSnapshotCoordinator::new(),
            bridge_mutation_lock: Arc::new(Mutex::new(())),
        })
}

#[derive(Deserialize)]
struct GlobalSearchQuery {
    q: String,
    #[serde(default = "default_search_limit")]
    limit: usize,
}

fn default_search_limit() -> usize {
    40
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GlobalSearchResult {
    kind: &'static str,
    id: String,
    title: String,
    snippet: String,
    channel_id: Option<Uuid>,
    agent_id: Option<Uuid>,
    message_id: Option<Uuid>,
    task_number: Option<i64>,
    path: Option<String>,
}

#[derive(Serialize)]
struct GlobalSearchResponse {
    results: Vec<GlobalSearchResult>,
}

async fn global_search(
    State(state): State<AppState>,
    Query(query): Query<GlobalSearchQuery>,
) -> Result<Json<GlobalSearchResponse>, ApiError> {
    let search_query = non_empty("search query", &query.q)?;
    let search = search_query.to_lowercase();
    let limit = query.limit.clamp(1, 100);
    let channels = state.store.list_channels().await?;
    let agents = state.store.list_agents().await?;
    let agent_names = agents
        .iter()
        .map(|agent| (agent.id, agent.name.as_str()))
        .collect::<HashMap<_, _>>();
    let mut results = Vec::new();

    for channel in &channels {
        if results.len() >= limit {
            break;
        }
        if !channel.is_system && channel.name.to_lowercase().contains(&search) {
            results.push(GlobalSearchResult {
                kind: "channel",
                id: channel.id.to_string(),
                title: format!("#{}", channel.name),
                snippet: "Channel".to_owned(),
                channel_id: Some(channel.id),
                agent_id: None,
                message_id: None,
                task_number: None,
                path: None,
            });
        }
        for message in state.store.list_messages(channel.id).await? {
            if message.content.to_lowercase().contains(&search) {
                let direct_agent_id = channel
                    .is_system
                    .then_some(channel.direct_agent_id)
                    .flatten();
                let title = direct_agent_id
                    .and_then(|agent_id| agent_names.get(&agent_id))
                    .map(|name| format!("@{name} · direct message #{}", message.seq))
                    .unwrap_or_else(|| format!("#{} · message #{}", channel.name, message.seq));
                results.push(GlobalSearchResult {
                    kind: "message",
                    id: message.id.to_string(),
                    title,
                    snippet: compact_snippet(&message.content, 180),
                    channel_id: (!channel.is_system).then_some(channel.id),
                    agent_id: direct_agent_id,
                    message_id: Some(message.id),
                    task_number: None,
                    path: None,
                });
                if results.len() >= limit {
                    break;
                }
            }
        }

        if results.len() >= limit {
            break;
        }
        if channel.is_system {
            continue;
        }
        for task in state.store.list_tasks(channel.id, None).await? {
            let searchable = format!(
                "{} {}",
                task.title,
                task.progress.as_deref().unwrap_or_default()
            );
            if searchable.to_lowercase().contains(&search) {
                results.push(GlobalSearchResult {
                    kind: "task",
                    id: task.id.to_string(),
                    title: format!(
                        "#{} · task #{} {}",
                        channel.name, task.task_number, task.title
                    ),
                    snippet: task
                        .progress
                        .as_deref()
                        .map(|progress| compact_snippet(progress, 180))
                        .unwrap_or_else(|| task_status_label(task.status).to_owned()),
                    channel_id: Some(channel.id),
                    agent_id: None,
                    message_id: task.message_id,
                    task_number: Some(task.task_number),
                    path: None,
                });
                if results.len() >= limit {
                    break;
                }
            }
        }
    }

    for agent in agents {
        if results.len() >= limit {
            break;
        }
        let searchable = format!(
            "{} {} {}",
            agent.name,
            agent.runtime,
            agent.model.as_deref().unwrap_or_default()
        );
        if searchable.to_lowercase().contains(&search) {
            results.push(GlobalSearchResult {
                kind: "agent",
                id: agent.id.to_string(),
                title: format!("@{}", agent.name),
                snippet: format!(
                    "{} · {}",
                    agent.runtime,
                    agent.model.as_deref().unwrap_or("runtime default")
                ),
                channel_id: None,
                agent_id: Some(agent.id),
                message_id: None,
                task_number: None,
                path: None,
            });
        }
    }

    Ok(Json(GlobalSearchResponse { results }))
}

fn compact_snippet(value: &str, max_chars: usize) -> String {
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = compact.chars();
    let snippet = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() {
        format!("{snippet}…")
    } else {
        snippet
    }
}

fn task_status_label(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Todo => "todo",
        TaskStatus::InProgress => "in_progress",
        TaskStatus::InReview => "in_review",
        TaskStatus::Done => "done",
    }
}

async fn authorize_bridge_request(
    State(store): State<Store>,
    request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let agent_id = request
        .uri()
        .path()
        .strip_prefix("/api/bridge/agents/")
        .and_then(|path| path.split('/').next())
        .and_then(|value| Uuid::parse_str(value).ok())
        .ok_or_else(|| ApiError::unauthorized("invalid bridge agent scope"))?;
    let supplied = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::unauthorized("bridge capability token is required"))?;
    let expected = store
        .agent_bridge_token(agent_id)
        .await?
        .ok_or_else(|| ApiError::unauthorized("bridge capability token is not provisioned"))?;
    if !constant_time_eq(supplied.as_bytes(), expected.as_bytes()) {
        return Err(ApiError::unauthorized("bridge capability token is invalid"));
    }
    Ok(next.run(request).await)
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

#[derive(Serialize)]
struct HealthResponse {
    ok: bool,
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { ok: true })
}

async fn list_runtimes(State(state): State<AppState>) -> Json<Vec<RuntimeInfo>> {
    Json(state.runtime.list().await)
}

async fn runtime_metrics(State(state): State<AppState>) -> Json<RuntimeMetricsSnapshot> {
    Json(state.runtime.metrics().await)
}

async fn delivery_stats(State(state): State<AppState>) -> Result<Json<DeliveryStats>, ApiError> {
    Ok(Json(state.store.delivery_stats(Utc::now()).await?))
}

async fn list_channels(State(state): State<AppState>) -> Result<Json<Vec<Channel>>, ApiError> {
    Ok(Json(
        state
            .store
            .list_channels()
            .await?
            .into_iter()
            .filter(|channel| !channel.is_system)
            .collect(),
    ))
}

#[derive(Deserialize)]
struct CreateChannelRequest {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    goal: Option<String>,
}

async fn create_channel(
    State(state): State<AppState>,
    Json(request): Json<CreateChannelRequest>,
) -> Result<(StatusCode, Json<Channel>), ApiError> {
    let name = non_empty("channel name", &request.name)?;
    let channel = state.store.create_channel(name).await?;
    let channel = state
        .store
        .update_channel_profile(
            channel.id,
            request.description.as_deref(),
            request.goal.as_deref(),
        )
        .await?
        .ok_or_else(|| ApiError::not_found("channel not found after creation"))?;
    Ok((StatusCode::CREATED, Json(channel)))
}

async fn list_agents(State(state): State<AppState>) -> Result<Json<Vec<Agent>>, ApiError> {
    Ok(Json(state.store.list_agents().await?))
}

async fn delete_channel(
    State(state): State<AppState>,
    Path(channel_id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    require_channel(&state.store, channel_id).await?;
    state.store.delete_channel(channel_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_channel_members(
    State(state): State<AppState>,
    Path(channel_id): Path<Uuid>,
) -> Result<Json<Vec<Agent>>, ApiError> {
    require_channel(&state.store, channel_id).await?;
    Ok(Json(state.store.list_channel_agents(channel_id).await?))
}

#[derive(Deserialize)]
struct AddChannelMemberRequest {
    agent_id: Uuid,
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    delivery_policy: Option<String>,
    #[serde(default)]
    created_by_agent_id: Option<Uuid>,
}

async fn add_channel_member(
    State(state): State<AppState>,
    Path(channel_id): Path<Uuid>,
    Json(request): Json<AddChannelMemberRequest>,
) -> Result<(StatusCode, Json<ChannelAgent>), ApiError> {
    require_channel(&state.store, channel_id).await?;
    require_agent(&state.store, request.agent_id).await?;
    let membership = state
        .store
        .add_agent_to_channel(
            channel_id,
            request.agent_id,
            request.role.as_deref(),
            request.delivery_policy.as_deref(),
            request.created_by_agent_id,
            Some(channel_id),
        )
        .await?;
    Ok((StatusCode::CREATED, Json(membership)))
}

async fn remove_channel_member(
    State(state): State<AppState>,
    Path((channel_id, agent_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    state
        .store
        .remove_agent_from_channel(channel_id, agent_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_agent_channels(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
) -> Result<Json<Vec<Channel>>, ApiError> {
    require_agent(&state.store, agent_id).await?;
    Ok(Json(
        state
            .store
            .list_agent_channels(agent_id)
            .await?
            .into_iter()
            .filter(|channel| !channel.is_system)
            .collect(),
    ))
}

#[derive(Serialize)]
struct AgentMessage {
    id: Uuid,
    seq: i64,
    agent_id: Option<Uuid>,
    role: MessageRole,
    content: String,
    created_at: DateTime<Utc>,
}

impl From<Message> for AgentMessage {
    fn from(message: Message) -> Self {
        Self {
            id: message.id,
            seq: message.seq,
            agent_id: message.agent_id,
            role: message.role,
            content: message.content,
            created_at: message.created_at,
        }
    }
}

#[derive(Serialize)]
struct AgentPendingDelivery {
    id: Uuid,
    state: cocli_store::DeliveryState,
    attempts: i64,
}

impl From<Delivery> for AgentPendingDelivery {
    fn from(delivery: Delivery) -> Self {
        Self {
            id: delivery.id,
            state: delivery.state,
            attempts: delivery.attempts,
        }
    }
}

#[derive(Serialize)]
struct AgentPostMessageResponse {
    message: AgentMessage,
    replies: Vec<AgentMessage>,
    pending_deliveries: Vec<AgentPendingDelivery>,
}

async fn list_agent_messages(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
) -> Result<Json<Vec<AgentMessage>>, ApiError> {
    require_agent(&state.store, agent_id).await?;
    let channel = state
        .store
        .ensure_direct_channel_for_agent(agent_id)
        .await?;
    Ok(Json(
        state
            .store
            .list_messages(channel.id)
            .await?
            .into_iter()
            .map(AgentMessage::from)
            .collect(),
    ))
}

async fn post_agent_message(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Json(request): Json<PostMessageRequest>,
) -> Result<(StatusCode, Json<AgentPostMessageResponse>), ApiError> {
    let agent = require_agent(&state.store, agent_id).await?;
    if agent.lifecycle_status != AgentLifecycleStatus::Active
        || agent.status != AgentStatus::Running
    {
        return Err(ApiError::conflict(
            "agent must be active before it can receive a direct message",
        ));
    }
    let channel = state
        .store
        .ensure_direct_channel_for_agent(agent_id)
        .await?;
    let (status, Json(response)) =
        post_message_to_channel(&state, channel.id, &request.content).await?;
    Ok((
        status,
        Json(AgentPostMessageResponse {
            message: AgentMessage::from(response.message),
            replies: response
                .replies
                .into_iter()
                .map(AgentMessage::from)
                .collect(),
            pending_deliveries: response
                .pending_deliveries
                .into_iter()
                .map(AgentPendingDelivery::from)
                .collect(),
        }),
    ))
}

#[derive(Deserialize)]
struct AttachWorkspaceRequest {
    kind: String,
    #[serde(default)]
    locator: Option<String>,
    #[serde(default)]
    metadata: serde_json::Value,
}

async fn attach_channel_workspace(
    State(state): State<AppState>,
    Path(channel_id): Path<Uuid>,
    Json(request): Json<AttachWorkspaceRequest>,
) -> Result<(StatusCode, Json<Workspace>), ApiError> {
    require_channel(&state.store, channel_id).await?;
    let kind = non_empty("workspace kind", &request.kind)?;
    let workspace = state
        .store
        .attach_workspace(
            "channel",
            channel_id,
            kind,
            request.locator.as_deref(),
            request.metadata,
        )
        .await?;
    Ok((StatusCode::CREATED, Json(workspace)))
}

async fn list_channel_workspaces(
    State(state): State<AppState>,
    Path(channel_id): Path<Uuid>,
) -> Result<Json<Vec<Workspace>>, ApiError> {
    require_channel(&state.store, channel_id).await?;
    Ok(Json(
        state.store.list_workspaces("channel", channel_id).await?,
    ))
}

async fn attach_agent_workspace(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Json(request): Json<AttachWorkspaceRequest>,
) -> Result<(StatusCode, Json<Workspace>), ApiError> {
    require_agent(&state.store, agent_id).await?;
    let kind = non_empty("workspace kind", &request.kind)?;
    let workspace = state
        .store
        .attach_workspace(
            "agent",
            agent_id,
            kind,
            request.locator.as_deref(),
            request.metadata,
        )
        .await?;
    Ok((StatusCode::CREATED, Json(workspace)))
}

async fn list_agent_workspaces(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
) -> Result<Json<Vec<Workspace>>, ApiError> {
    require_agent(&state.store, agent_id).await?;
    Ok(Json(state.store.list_workspaces("agent", agent_id).await?))
}

#[derive(Deserialize)]
struct AttachExistingWorkspaceRequest {
    #[serde(default)]
    role: Option<String>,
}

async fn attach_existing_channel_workspace(
    State(state): State<AppState>,
    Path((channel_id, workspace_id)): Path<(Uuid, Uuid)>,
    Json(request): Json<AttachExistingWorkspaceRequest>,
) -> Result<(StatusCode, Json<SubjectWorkspace>), ApiError> {
    require_channel(&state.store, channel_id).await?;
    let attachment = state
        .store
        .attach_existing_workspace(
            workspace_id,
            SubjectType::Channel,
            channel_id,
            request.role.as_deref(),
        )
        .await?;
    Ok((StatusCode::CREATED, Json(attachment)))
}

async fn attach_existing_agent_workspace(
    State(state): State<AppState>,
    Path((agent_id, workspace_id)): Path<(Uuid, Uuid)>,
    Json(request): Json<AttachExistingWorkspaceRequest>,
) -> Result<(StatusCode, Json<SubjectWorkspace>), ApiError> {
    require_agent(&state.store, agent_id).await?;
    let attachment = state
        .store
        .attach_existing_workspace(
            workspace_id,
            SubjectType::Agent,
            agent_id,
            request.role.as_deref(),
        )
        .await?;
    Ok((StatusCode::CREATED, Json(attachment)))
}

async fn detach_channel_workspace(
    State(state): State<AppState>,
    Path((channel_id, workspace_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    require_channel(&state.store, channel_id).await?;
    if !state
        .store
        .detach_workspace(workspace_id, SubjectType::Channel, channel_id)
        .await?
    {
        return Err(ApiError::not_found("workspace attachment not found"));
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn detach_agent_workspace(
    State(state): State<AppState>,
    Path((agent_id, workspace_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    require_agent(&state.store, agent_id).await?;
    if !state
        .store
        .detach_workspace(workspace_id, SubjectType::Agent, agent_id)
        .await?
    {
        return Err(ApiError::not_found("workspace attachment not found"));
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn get_workspace(
    State(state): State<AppState>,
    Path(workspace_id): Path<Uuid>,
) -> Result<Json<Workspace>, ApiError> {
    Ok(Json(
        state
            .store
            .get_workspace(workspace_id)
            .await?
            .ok_or_else(|| ApiError::not_found("workspace not found"))?,
    ))
}

#[derive(Deserialize)]
struct UpdateWorkspaceRequest {
    display_name: String,
    #[serde(default)]
    portable_locator: Option<String>,
    #[serde(default)]
    metadata: serde_json::Value,
}

async fn update_workspace(
    State(state): State<AppState>,
    Path(workspace_id): Path<Uuid>,
    Json(request): Json<UpdateWorkspaceRequest>,
) -> Result<Json<Workspace>, ApiError> {
    let display_name = non_empty("workspace display name", &request.display_name)?;
    Ok(Json(
        state
            .store
            .update_workspace(
                workspace_id,
                display_name,
                request.portable_locator.as_deref(),
                request.metadata,
            )
            .await?
            .ok_or_else(|| ApiError::not_found("workspace not found"))?,
    ))
}

async fn delete_workspace(
    State(state): State<AppState>,
    Path(workspace_id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    if !state.store.delete_workspace(workspace_id).await? {
        return Err(ApiError::not_found("workspace not found"));
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn list_workspace_attachments(
    State(state): State<AppState>,
    Path(workspace_id): Path<Uuid>,
) -> Result<Json<Vec<SubjectWorkspace>>, ApiError> {
    require_workspace(&state.store, workspace_id).await?;
    Ok(Json(
        state.store.list_workspace_attachments(workspace_id).await?,
    ))
}

async fn list_workspace_bindings(
    State(state): State<AppState>,
    Path(workspace_id): Path<Uuid>,
) -> Result<Json<Vec<WorkspaceBinding>>, ApiError> {
    require_workspace(&state.store, workspace_id).await?;
    Ok(Json(
        state.store.list_workspace_bindings(workspace_id).await?,
    ))
}

async fn get_current_workspace_binding(
    State(state): State<AppState>,
    Path(workspace_id): Path<Uuid>,
) -> Result<Json<WorkspaceBinding>, ApiError> {
    require_workspace(&state.store, workspace_id).await?;
    Ok(Json(
        state
            .store
            .current_workspace_binding(workspace_id)
            .await?
            .ok_or_else(|| ApiError::not_found("current workspace binding not found"))?,
    ))
}

#[derive(Deserialize)]
struct RebindWorkspaceRequest {
    local_locator: String,
    #[serde(default)]
    secret_ref: Option<String>,
}

async fn rebind_workspace(
    State(state): State<AppState>,
    Path(workspace_id): Path<Uuid>,
    Json(request): Json<RebindWorkspaceRequest>,
) -> Result<Json<WorkspaceBinding>, ApiError> {
    let local_locator = non_empty("workspace local locator", &request.local_locator)?;
    Ok(Json(
        state
            .store
            .bind_workspace(workspace_id, local_locator, request.secret_ref.as_deref())
            .await?,
    ))
}

async fn verify_workspace_binding(
    State(state): State<AppState>,
    Path(workspace_id): Path<Uuid>,
) -> Result<Json<WorkspaceBinding>, ApiError> {
    Ok(Json(
        state.store.verify_workspace_binding(workspace_id).await?,
    ))
}

#[derive(Deserialize)]
struct CreateAgentRequest {
    #[serde(default)]
    channel_id: Option<Uuid>,
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    instructions: Option<String>,
    runtime: String,
    model: Option<String>,
}

async fn create_agent(
    State(state): State<AppState>,
    Json(request): Json<CreateAgentRequest>,
) -> Result<(StatusCode, Json<Agent>), ApiError> {
    let name = non_empty("agent name", &request.name)?;
    let runtime_name = non_empty("runtime", &request.runtime)?;
    let runtimes = state.runtime.list().await;
    let runtime = runtimes
        .iter()
        .find(|runtime| runtime.name == runtime_name)
        .ok_or_else(|| ApiError::bad_request(format!("unknown runtime: {runtime_name}")))?;
    if !runtime.installed {
        return Err(ApiError::bad_request(
            runtime
                .unavailable_reason
                .clone()
                .unwrap_or_else(|| format!("runtime is unavailable: {runtime_name}")),
        ));
    }
    let agent = if let Some(channel_id) = request.channel_id {
        require_channel(&state.store, channel_id).await?;
        state
            .store
            .create_agent(
                channel_id,
                name,
                runtime_name,
                request.model.as_deref(),
                AgentStatus::Running,
            )
            .await?
    } else {
        state
            .store
            .create_standalone_agent(
                name,
                runtime_name,
                request.model.as_deref(),
                AgentStatus::Running,
                None,
                None,
            )
            .await?
    };
    let agent = state
        .store
        .update_agent_profile(
            agent.id,
            request.description.as_deref(),
            request.instructions.as_deref(),
        )
        .await?
        .ok_or_else(|| ApiError::not_found("agent not found after creation"))?;
    if let Err(error) = state.store.ensure_agent_bridge_token(agent.id).await {
        if let Err(rollback_error) = state.store.delete_agent(agent.id).await {
            return Err(ApiError::conflict(format!(
                "bridge token provisioning failed ({error}); agent rollback also failed: {rollback_error}"
            )));
        }
        return Err(error.into());
    }
    Ok((StatusCode::CREATED, Json(agent)))
}

async fn start_agent(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
) -> Result<Json<Agent>, ApiError> {
    let agent = require_agent(&state.store, agent_id).await?;
    state.store.ensure_agent_bridge_token(agent.id).await?;
    state.runtime.start(&agent).await?;
    let _ = match set_agent_status(&state.store, agent_id, AgentStatus::Running).await {
        Ok(result) => result,
        Err(status_error) => match state.runtime.stop(agent.id).await {
            Ok(()) => return Err(status_error),
            Err(stop_error) => {
                match set_agent_status(&state.store, agent_id, AgentStatus::Running).await {
                    Ok(result) => result,
                    Err(recovery_error) => {
                        return Err(ApiError::conflict(format!(
                            "agent status update failed ({}); runtime stop also failed ({stop_error}); status recovery failed: {}",
                            status_error.message, recovery_error.message
                        )));
                    }
                }
            }
        },
    };
    let result =
        set_agent_lifecycle_status(&state.store, agent_id, AgentLifecycleStatus::Active).await?;
    state.store.nudge_agent_deliveries(agent_id).await?;
    state.deliveries.wake.notify_one();
    Ok(result)
}

async fn stop_agent(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
) -> Result<Json<Agent>, ApiError> {
    state.runtime.stop(agent_id).await?;
    let _ = set_agent_status(&state.store, agent_id, AgentStatus::Stopped).await?;
    set_agent_lifecycle_status(&state.store, agent_id, AgentLifecycleStatus::Paused).await
}

async fn runtime_status(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
) -> Result<Json<RuntimeSessionStatus>, ApiError> {
    let agent = require_agent(&state.store, agent_id).await?;
    Ok(Json(state.runtime.status(&agent).await?))
}

#[derive(Deserialize)]
struct AgentSessionsQuery {
    #[serde(default = "default_session_limit")]
    limit: i64,
    #[serde(rename = "type")]
    session_type: Option<String>,
}

async fn list_agent_sessions(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Query(query): Query<AgentSessionsQuery>,
) -> Result<Json<Vec<AgentSession>>, ApiError> {
    require_agent(&state.store, agent_id).await?;
    Ok(Json(
        state
            .store
            .list_agent_sessions(agent_id, query.limit, query.session_type.as_deref())
            .await?,
    ))
}

async fn current_agent_session(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
) -> Result<Json<Option<AgentSession>>, ApiError> {
    require_agent(&state.store, agent_id).await?;
    Ok(Json(state.store.current_agent_session(agent_id).await?))
}

#[derive(Deserialize)]
struct AgentTurnsQuery {
    #[serde(default, rename = "sessionId")]
    session_id: Option<String>,
    #[serde(default = "default_turn_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
}

async fn list_agent_turns(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Query(query): Query<AgentTurnsQuery>,
) -> Result<Json<Vec<AgentTurn>>, ApiError> {
    require_agent(&state.store, agent_id).await?;
    Ok(Json(
        state
            .store
            .list_agent_turns(
                agent_id,
                query.session_id.as_deref(),
                query.limit,
                query.offset,
            )
            .await?,
    ))
}

#[derive(Deserialize)]
struct TurnPageQuery {
    #[serde(default = "default_session_turn_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
}

async fn list_session_turns(
    State(state): State<AppState>,
    Path((agent_id, session_id)): Path<(Uuid, String)>,
    Query(query): Query<TurnPageQuery>,
) -> Result<Json<Vec<AgentTurn>>, ApiError> {
    require_agent(&state.store, agent_id).await?;
    Ok(Json(
        state
            .store
            .list_agent_turns(agent_id, Some(&session_id), query.limit, query.offset)
            .await?,
    ))
}

async fn get_agent_turn(
    State(state): State<AppState>,
    Path((agent_id, turn_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<AgentTurn>, ApiError> {
    require_agent(&state.store, agent_id).await?;
    state
        .store
        .get_agent_turn(agent_id, turn_id)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::not_found("turn not found"))
}

#[derive(Deserialize)]
struct AgentActivityQuery {
    #[serde(default = "default_activity_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
}

async fn list_agent_activity(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Query(query): Query<AgentActivityQuery>,
) -> Result<Json<Vec<AgentActivity>>, ApiError> {
    require_agent(&state.store, agent_id).await?;
    Ok(Json(
        state
            .store
            .list_agent_activity(agent_id, query.limit, query.offset)
            .await?,
    ))
}

#[derive(Deserialize)]
struct AgentOperationsQuery {
    #[serde(default = "default_operation_limit")]
    limit: i64,
}

async fn list_agent_operations(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Query(query): Query<AgentOperationsQuery>,
) -> Result<Json<Vec<AgentOperation>>, ApiError> {
    require_agent(&state.store, agent_id).await?;
    Ok(Json(
        state
            .store
            .list_agent_operations(agent_id, query.limit)
            .await?,
    ))
}

async fn get_agent_working_state(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
) -> Result<Json<Option<cocli_store::WorkingState>>, ApiError> {
    require_agent(&state.store, agent_id).await?;
    Ok(Json(state.store.get_working_state(agent_id).await?))
}

fn default_session_limit() -> i64 {
    20
}

fn default_turn_limit() -> i64 {
    50
}

fn default_session_turn_limit() -> i64 {
    100
}

fn default_activity_limit() -> i64 {
    50
}

fn default_operation_limit() -> i64 {
    100
}

#[derive(Deserialize)]
struct MemoryTopicQuery {
    #[serde(rename = "type")]
    memory_type: String,
    topic: String,
}

async fn get_agent_memory_index(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
) -> Result<Json<MemoryDocument>, ApiError> {
    require_agent(&state.store, agent_id).await?;
    Ok(Json(
        state
            .store
            .get_memory_index(MemoryNamespace::Agent(agent_id))
            .await?,
    ))
}

async fn get_agent_memory_topic(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Query(query): Query<MemoryTopicQuery>,
) -> Result<Json<MemoryDocument>, ApiError> {
    require_agent(&state.store, agent_id).await?;
    let topic = state
        .store
        .get_memory_topic(
            MemoryNamespace::Agent(agent_id),
            &query.memory_type,
            &query.topic,
        )
        .await?
        .ok_or_else(|| ApiError::not_found("memory topic not found"))?;
    Ok(Json(MemoryDocument {
        body: topic.body,
        version: topic.version,
    }))
}

async fn get_channel_memory_index(
    State(state): State<AppState>,
    Path(channel_id): Path<Uuid>,
) -> Result<Json<MemoryDocument>, ApiError> {
    require_channel(&state.store, channel_id).await?;
    Ok(Json(
        state
            .store
            .get_memory_index(MemoryNamespace::Channel(channel_id))
            .await?,
    ))
}

async fn get_channel_memory_topic(
    State(state): State<AppState>,
    Path(channel_id): Path<Uuid>,
    Query(query): Query<MemoryTopicQuery>,
) -> Result<Json<MemoryDocument>, ApiError> {
    require_channel(&state.store, channel_id).await?;
    let topic = state
        .store
        .get_memory_topic(
            MemoryNamespace::Channel(channel_id),
            &query.memory_type,
            &query.topic,
        )
        .await?
        .ok_or_else(|| ApiError::not_found("memory topic not found"))?;
    Ok(Json(MemoryDocument {
        body: topic.body,
        version: topic.version,
    }))
}

#[derive(Serialize)]
struct ControlResponse {
    ok: bool,
}

async fn cancel_turn(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
) -> Result<Json<ControlResponse>, ApiError> {
    require_agent(&state.store, agent_id).await?;
    state.runtime.cancel(agent_id).await?;
    Ok(Json(ControlResponse { ok: true }))
}

#[derive(Deserialize)]
struct SteerTurnRequest {
    input: String,
}

async fn steer_turn(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Json(request): Json<SteerTurnRequest>,
) -> Result<Json<ControlResponse>, ApiError> {
    require_agent(&state.store, agent_id).await?;
    let input = non_empty("steer input", &request.input)?;
    state.runtime.steer(agent_id, input).await?;
    Ok(Json(ControlResponse { ok: true }))
}

#[derive(Deserialize)]
struct ForkThreadRequest {
    #[serde(default)]
    reason: String,
}

async fn fork_thread(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    request: Option<Json<ForkThreadRequest>>,
) -> Result<Json<RuntimeForkResult>, ApiError> {
    let agent = require_agent(&state.store, agent_id).await?;
    let reason = request
        .as_ref()
        .map(|Json(request)| request.reason.trim())
        .unwrap_or_default();
    Ok(Json(state.runtime.fork(&agent, reason).await?))
}

async fn probe_recovery(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
) -> Result<Json<RuntimeRecoveryProbeResult>, ApiError> {
    let agent = require_agent(&state.store, agent_id).await?;
    Ok(Json(state.runtime.probe_recovery(&agent).await?))
}

#[derive(Deserialize)]
struct BridgeSendMessageRequest {
    #[serde(default)]
    target: String,
    content: String,
}

async fn bridge_send_message(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Json(request): Json<BridgeSendMessageRequest>,
) -> Result<(StatusCode, Json<Message>), ApiError> {
    let agent = require_agent(&state.store, agent_id).await?;
    let content = non_empty("message content", &request.content)?;
    let channel_id = resolve_bridge_channel(&state.store, &agent, &request.target).await?;
    let message = state
        .store
        .append_message(channel_id, Some(agent.id), MessageRole::Assistant, content)
        .await?;
    Ok((StatusCode::CREATED, Json(message)))
}

#[derive(Deserialize)]
struct BridgeInboxQuery {
    #[serde(default = "default_bridge_message_limit")]
    limit: i64,
}

#[derive(Serialize)]
struct BridgeInboxResponse {
    messages: Vec<Message>,
}

async fn bridge_check_messages(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Query(query): Query<BridgeInboxQuery>,
) -> Result<Json<BridgeInboxResponse>, ApiError> {
    require_agent(&state.store, agent_id).await?;
    let messages = state
        .store
        .consume_agent_inbox(agent_id, query.limit)
        .await?;
    Ok(Json(BridgeInboxResponse { messages }))
}

#[derive(Deserialize)]
struct BridgeHistoryQuery {
    #[serde(default)]
    channel: String,
    #[serde(default = "default_bridge_message_limit")]
    limit: i64,
    before: Option<i64>,
    after: Option<i64>,
}

#[derive(Serialize)]
struct BridgeHistoryResponse {
    channel: Channel,
    messages: Vec<Message>,
}

async fn bridge_read_history(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Query(query): Query<BridgeHistoryQuery>,
) -> Result<Json<BridgeHistoryResponse>, ApiError> {
    let agent = require_agent(&state.store, agent_id).await?;
    let channel_id = resolve_bridge_channel(&state.store, &agent, &query.channel).await?;
    let channel = state
        .store
        .get_channel(channel_id)
        .await?
        .ok_or_else(|| ApiError::not_found("channel not found"))?;
    let messages = state
        .store
        .list_message_page(channel_id, query.limit, query.before, query.after)
        .await?;
    Ok(Json(BridgeHistoryResponse { channel, messages }))
}

#[derive(Deserialize)]
struct BridgeChannelListQuery {
    #[serde(default)]
    scope: String,
    #[serde(default)]
    q: String,
}

async fn bridge_list_channels(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Query(query): Query<BridgeChannelListQuery>,
) -> Result<Json<Vec<Channel>>, ApiError> {
    require_agent(&state.store, agent_id).await?;
    let mut channels = match query.scope.trim() {
        "" | "member" => state.store.list_agent_channels(agent_id).await?,
        "all" => state.store.list_channels().await?,
        other => {
            return Err(ApiError::bad_request(format!(
                "unsupported channel list scope: {other}"
            )))
        }
    };
    channels.retain(|channel| !channel.is_system);
    let search = query.q.trim().to_lowercase();
    if !search.is_empty() {
        channels.retain(|channel| channel.name.to_lowercase().contains(&search));
    }
    Ok(Json(channels))
}

#[derive(Deserialize)]
struct BridgeCreateChannelRequest {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    goal: Option<String>,
    #[serde(default)]
    idempotency_key: Option<String>,
    #[serde(default)]
    source_channel_id: Option<Uuid>,
    #[serde(default)]
    source_session_id: Option<String>,
}

async fn bridge_create_channel(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Json(request): Json<BridgeCreateChannelRequest>,
) -> Result<(StatusCode, Json<Channel>), ApiError> {
    let agent = require_agent(&state.store, agent_id).await?;
    let name = non_empty("channel name", &request.name)?;
    let source_channel_id =
        validate_operation_source_channel(&state.store, agent.id, request.source_channel_id)
            .await?;
    let fingerprint = serde_json::to_string(&serde_json::json!({
        "name": name,
        "description": request.description.as_deref(),
        "goal": request.goal.as_deref(),
    }))
    .expect("bridge channel request should serialize");
    let _guard = state.bridge_mutation_lock.lock().await;
    if let Some(operation) = prior_agent_operation(
        &state.store,
        agent.id,
        "channel.create",
        request.idempotency_key.as_deref(),
        &fingerprint,
    )
    .await?
    {
        let channel_id = Uuid::parse_str(&operation.result_id)
            .map_err(|_| ApiError::conflict("idempotent channel result is invalid"))?;
        let channel = require_channel(&state.store, channel_id).await?;
        return Ok((StatusCode::OK, Json(channel)));
    }
    let channel = state.store.create_channel(name).await?;
    let channel = state
        .store
        .update_channel_profile(
            channel.id,
            request.description.as_deref(),
            request.goal.as_deref(),
        )
        .await?
        .ok_or_else(|| ApiError::not_found("channel not found after creation"))?;
    state
        .store
        .add_agent_to_channel(
            channel.id,
            agent.id,
            Some("creator"),
            Some("subscribed"),
            Some(agent.id),
            Some(channel.id),
        )
        .await?;
    record_agent_operation(
        &state.store,
        agent.id,
        "channel.create",
        request.idempotency_key.as_deref(),
        &fingerprint,
        "channel",
        &channel.id.to_string(),
        source_channel_id,
        request.source_session_id.as_deref(),
    )
    .await?;
    Ok((StatusCode::CREATED, Json(channel)))
}

#[derive(Deserialize)]
struct BridgeAgentListQuery {
    #[serde(default)]
    channel: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    q: String,
}

async fn bridge_list_agents(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Query(query): Query<BridgeAgentListQuery>,
) -> Result<Json<Vec<Agent>>, ApiError> {
    let caller = require_agent(&state.store, agent_id).await?;
    let mut agents = if query.channel.trim().is_empty() {
        state.store.list_agents().await?
    } else {
        let channel_id = resolve_bridge_channel(&state.store, &caller, &query.channel).await?;
        state.store.list_channel_agents(channel_id).await?
    };
    let status = query.status.trim();
    if !status.is_empty() {
        let lifecycle = match status {
            "active" => AgentLifecycleStatus::Active,
            "paused" => AgentLifecycleStatus::Paused,
            "archived" => AgentLifecycleStatus::Archived,
            other => {
                return Err(ApiError::bad_request(format!(
                    "unsupported agent lifecycle status: {other}"
                )))
            }
        };
        agents.retain(|agent| agent.lifecycle_status == lifecycle);
    }
    let search = query.q.trim().to_lowercase();
    if !search.is_empty() {
        agents.retain(|agent| agent.name.to_lowercase().contains(&search));
    }
    Ok(Json(agents))
}

#[derive(Deserialize)]
struct BridgeCreateAgentRequest {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    instructions: Option<String>,
    #[serde(default)]
    runtime: Option<String>,
    model: Option<String>,
    #[serde(default)]
    channel: String,
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    idempotency_key: Option<String>,
    #[serde(default)]
    source_channel_id: Option<Uuid>,
    #[serde(default)]
    source_session_id: Option<String>,
}

async fn bridge_create_agent(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Json(request): Json<BridgeCreateAgentRequest>,
) -> Result<(StatusCode, Json<Agent>), ApiError> {
    let creator = require_agent(&state.store, agent_id).await?;
    let name = non_empty("agent name", &request.name)?;
    let runtime = request.runtime.as_deref().unwrap_or(&creator.runtime);
    let runtime = non_empty("runtime", runtime)?;
    let runtimes = state.runtime.list().await;
    let runtime_info = runtimes
        .iter()
        .find(|candidate| candidate.name == runtime)
        .ok_or_else(|| ApiError::bad_request(format!("unknown runtime: {runtime}")))?;
    if !runtime_info.installed {
        return Err(ApiError::bad_request(
            runtime_info
                .unavailable_reason
                .clone()
                .unwrap_or_else(|| format!("runtime is unavailable: {runtime}")),
        ));
    }
    let target_channel_id = if request.channel.trim().is_empty() {
        None
    } else {
        Some(resolve_bridge_channel(&state.store, &creator, &request.channel).await?)
    };
    let source_channel_id = request.source_channel_id.or(target_channel_id);
    let source_channel_id =
        validate_operation_source_channel(&state.store, creator.id, source_channel_id).await?;
    let fingerprint = serde_json::to_string(&serde_json::json!({
        "name": name,
        "description": request.description.as_deref(),
        "instructions": request.instructions.as_deref(),
        "runtime": runtime,
        "model": request.model.as_deref(),
        "channelId": target_channel_id,
        "role": request.role.as_deref(),
    }))
    .expect("bridge agent request should serialize");
    let _guard = state.bridge_mutation_lock.lock().await;
    if let Some(operation) = prior_agent_operation(
        &state.store,
        creator.id,
        "agent.create",
        request.idempotency_key.as_deref(),
        &fingerprint,
    )
    .await?
    {
        let existing_agent_id = Uuid::parse_str(&operation.result_id)
            .map_err(|_| ApiError::conflict("idempotent agent result is invalid"))?;
        let agent = require_agent(&state.store, existing_agent_id).await?;
        return Ok((StatusCode::OK, Json(agent)));
    }
    let agent = state
        .store
        .create_standalone_agent(
            name,
            runtime,
            request.model.as_deref(),
            AgentStatus::Running,
            Some(creator.id),
            source_channel_id,
        )
        .await?;
    let agent = state
        .store
        .update_agent_profile(
            agent.id,
            request.description.as_deref(),
            request.instructions.as_deref(),
        )
        .await?
        .ok_or_else(|| ApiError::not_found("agent not found after creation"))?;
    state.store.ensure_agent_bridge_token(agent.id).await?;
    if let Some(channel_id) = target_channel_id {
        state
            .store
            .add_agent_to_channel(
                channel_id,
                agent.id,
                request.role.as_deref(),
                Some("subscribed"),
                Some(creator.id),
                Some(channel_id),
            )
            .await?;
    }
    record_agent_operation(
        &state.store,
        creator.id,
        "agent.create",
        request.idempotency_key.as_deref(),
        &fingerprint,
        "agent",
        &agent.id.to_string(),
        source_channel_id,
        request.source_session_id.as_deref(),
    )
    .await?;
    Ok((StatusCode::CREATED, Json(agent)))
}

#[derive(Deserialize)]
struct BridgeInviteAgentRequest {
    channel: String,
    agent_id: Uuid,
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    idempotency_key: Option<String>,
    #[serde(default)]
    source_channel_id: Option<Uuid>,
    #[serde(default)]
    source_session_id: Option<String>,
}

async fn bridge_join_agent_to_channel(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Json(request): Json<BridgeInviteAgentRequest>,
) -> Result<(StatusCode, Json<ChannelAgent>), ApiError> {
    let creator = require_agent(&state.store, agent_id).await?;
    let channel_id = resolve_bridge_channel(&state.store, &creator, &request.channel).await?;
    require_agent(&state.store, request.agent_id).await?;
    let source_channel_id = validate_operation_source_channel(
        &state.store,
        creator.id,
        request.source_channel_id.or(Some(channel_id)),
    )
    .await?;
    let fingerprint = serde_json::to_string(&serde_json::json!({
        "channelId": channel_id,
        "agentId": request.agent_id,
        "role": request.role.as_deref(),
    }))
    .expect("bridge membership request should serialize");
    let _guard = state.bridge_mutation_lock.lock().await;
    if prior_agent_operation(
        &state.store,
        creator.id,
        "channel.join_agent",
        request.idempotency_key.as_deref(),
        &fingerprint,
    )
    .await?
    .is_some()
    {
        let membership = state
            .store
            .get_channel_agent(channel_id, request.agent_id)
            .await?
            .ok_or_else(|| ApiError::conflict("idempotent membership result is missing"))?;
        return Ok((StatusCode::OK, Json(membership)));
    }
    let membership = state
        .store
        .add_agent_to_channel(
            channel_id,
            request.agent_id,
            request.role.as_deref(),
            Some("subscribed"),
            Some(creator.id),
            Some(channel_id),
        )
        .await?;
    record_agent_operation(
        &state.store,
        creator.id,
        "channel.join_agent",
        request.idempotency_key.as_deref(),
        &fingerprint,
        "membership",
        &format!("{channel_id}/{}", request.agent_id),
        source_channel_id,
        request.source_session_id.as_deref(),
    )
    .await?;
    Ok((StatusCode::CREATED, Json(membership)))
}

#[derive(Deserialize)]
struct BridgeWorkspaceQuery {
    #[serde(default)]
    scope: String,
    #[serde(default)]
    channel: String,
}

async fn bridge_list_workspaces(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Query(query): Query<BridgeWorkspaceQuery>,
) -> Result<Json<Vec<Workspace>>, ApiError> {
    let agent = require_agent(&state.store, agent_id).await?;
    let (owner_type, owner_id) = bridge_workspace_owner(&state.store, &agent, &query).await?;
    Ok(Json(
        state.store.list_workspaces(owner_type, owner_id).await?,
    ))
}

#[derive(Deserialize)]
struct BridgeAttachWorkspaceRequest {
    kind: String,
    #[serde(default)]
    locator: Option<String>,
    #[serde(default)]
    metadata: serde_json::Value,
    #[serde(default)]
    scope: String,
    #[serde(default)]
    channel: String,
}

async fn bridge_attach_workspace(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Json(request): Json<BridgeAttachWorkspaceRequest>,
) -> Result<(StatusCode, Json<Workspace>), ApiError> {
    let agent = require_agent(&state.store, agent_id).await?;
    let kind = non_empty("workspace kind", &request.kind)?;
    let query = BridgeWorkspaceQuery {
        scope: request.scope,
        channel: request.channel,
    };
    let (owner_type, owner_id) = bridge_workspace_owner(&state.store, &agent, &query).await?;
    let workspace = state
        .store
        .attach_workspace(
            owner_type,
            owner_id,
            kind,
            request.locator.as_deref(),
            request.metadata,
        )
        .await?;
    Ok((StatusCode::CREATED, Json(workspace)))
}

#[derive(Deserialize)]
struct BridgeTaskListQuery {
    #[serde(default)]
    channel: String,
    status: Option<String>,
}

#[derive(Serialize)]
struct BridgeTasksResponse {
    tasks: Vec<Task>,
}

async fn bridge_list_tasks(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Query(query): Query<BridgeTaskListQuery>,
) -> Result<Json<BridgeTasksResponse>, ApiError> {
    let agent = require_agent(&state.store, agent_id).await?;
    let channel_id = resolve_bridge_channel(&state.store, &agent, &query.channel).await?;
    let status = parse_task_status_filter(query.status.as_deref())?;
    Ok(Json(BridgeTasksResponse {
        tasks: state.store.list_tasks(channel_id, status).await?,
    }))
}

#[derive(Deserialize)]
struct BridgeTaskInput {
    title: String,
}

#[derive(Deserialize)]
struct BridgeCreateTasksRequest {
    #[serde(default)]
    channel: String,
    tasks: Vec<BridgeTaskInput>,
}

async fn bridge_create_tasks(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Json(request): Json<BridgeCreateTasksRequest>,
) -> Result<(StatusCode, Json<BridgeTasksResponse>), ApiError> {
    let agent = require_agent(&state.store, agent_id).await?;
    let channel_id = resolve_bridge_channel(&state.store, &agent, &request.channel).await?;
    if request.tasks.is_empty() {
        return Err(ApiError::bad_request("at least one task is required"));
    }
    let mut tasks = Vec::with_capacity(request.tasks.len());
    for input in request.tasks {
        let title = non_empty("task title", &input.title)?;
        tasks.push(
            state
                .store
                .create_task(channel_id, title, None, Some(agent.id))
                .await?,
        );
    }
    Ok((StatusCode::CREATED, Json(BridgeTasksResponse { tasks })))
}

#[derive(Deserialize)]
struct BridgeClaimTasksRequest {
    #[serde(default)]
    channel: String,
    #[serde(default, alias = "taskNumbers")]
    task_numbers: Vec<i64>,
    #[serde(default, alias = "messageIds")]
    message_ids: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BridgeTaskMutationResult {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    task_number: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    created: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    task: Option<Task>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

#[derive(Serialize)]
struct BridgeClaimTasksResponse {
    results: Vec<BridgeTaskMutationResult>,
}

async fn bridge_claim_tasks(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Json(request): Json<BridgeClaimTasksRequest>,
) -> Result<Json<BridgeClaimTasksResponse>, ApiError> {
    let agent = require_agent(&state.store, agent_id).await?;
    let channel_id = resolve_bridge_channel(&state.store, &agent, &request.channel).await?;
    if request.task_numbers.is_empty() && request.message_ids.is_empty() {
        return Err(ApiError::bad_request(
            "task_numbers or message_ids is required",
        ));
    }
    let mut results = Vec::with_capacity(request.task_numbers.len() + request.message_ids.len());
    for task_number in request.task_numbers {
        match state
            .store
            .claim_task(channel_id, task_number, agent.id)
            .await
        {
            Ok(task) => results.push(BridgeTaskMutationResult {
                success: true,
                task_number: Some(task.task_number),
                message_id: None,
                created: None,
                task: Some(task),
                reason: None,
            }),
            Err(error) => results.push(BridgeTaskMutationResult {
                success: false,
                task_number: Some(task_number),
                message_id: None,
                created: None,
                task: None,
                reason: Some(error.to_string()),
            }),
        }
    }
    for message_id in request.message_ids {
        let result = claim_message_as_task(&state.store, &agent, channel_id, &message_id).await;
        match result {
            Ok((task, created)) => results.push(BridgeTaskMutationResult {
                success: true,
                task_number: Some(task.task_number),
                message_id: Some(message_id),
                created: created.then_some(true),
                task: Some(task),
                reason: None,
            }),
            Err(error) => results.push(BridgeTaskMutationResult {
                success: false,
                task_number: None,
                message_id: Some(message_id),
                created: None,
                task: None,
                reason: Some(error.message),
            }),
        }
    }
    Ok(Json(BridgeClaimTasksResponse { results }))
}

async fn claim_message_as_task(
    store: &Store,
    agent: &Agent,
    channel_id: Uuid,
    message_id: &str,
) -> Result<(Task, bool), ApiError> {
    let message_id = Uuid::parse_str(message_id)
        .map_err(|_| ApiError::bad_request("message_ids must contain full UUIDs"))?;
    let message = store
        .get_message(message_id)
        .await?
        .filter(|message| message.channel_id == channel_id)
        .ok_or_else(|| ApiError::not_found("message not found in task channel"))?;
    if let Some(task) = store.get_task_by_message(channel_id, message.id).await? {
        return Ok((
            store
                .claim_task(channel_id, task.task_number, agent.id)
                .await?,
            false,
        ));
    }
    let task = store
        .create_task(
            channel_id,
            &message.content,
            Some(message.id),
            Some(agent.id),
        )
        .await?;
    Ok((
        store
            .claim_task(channel_id, task.task_number, agent.id)
            .await?,
        true,
    ))
}

#[derive(Deserialize)]
struct BridgeTaskNumberRequest {
    #[serde(default)]
    channel: String,
    #[serde(alias = "taskNumber")]
    task_number: i64,
}

async fn bridge_unclaim_task(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Json(request): Json<BridgeTaskNumberRequest>,
) -> Result<Json<Task>, ApiError> {
    let agent = require_agent(&state.store, agent_id).await?;
    let channel_id = resolve_bridge_channel(&state.store, &agent, &request.channel).await?;
    let task = state
        .store
        .get_task(channel_id, request.task_number)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("task #{} not found", request.task_number)))?;
    if task.assignee_id != Some(agent.id) {
        return Err(ApiError::conflict("task is not claimed by this agent"));
    }
    Ok(Json(
        state
            .store
            .unclaim_task(channel_id, request.task_number)
            .await?,
    ))
}

#[derive(Deserialize)]
struct BridgeUpdateTaskStatusRequest {
    #[serde(default)]
    channel: String,
    #[serde(alias = "taskNumber")]
    task_number: i64,
    status: TaskStatus,
    progress: Option<String>,
}

async fn bridge_update_task_status(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Json(request): Json<BridgeUpdateTaskStatusRequest>,
) -> Result<Json<Task>, ApiError> {
    let agent = require_agent(&state.store, agent_id).await?;
    let channel_id = resolve_bridge_channel(&state.store, &agent, &request.channel).await?;
    let task = state
        .store
        .get_task(channel_id, request.task_number)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("task #{} not found", request.task_number)))?;
    if task.assignee_id.is_some() && task.assignee_id != Some(agent.id) {
        return Err(ApiError::conflict("task is claimed by another agent"));
    }
    Ok(Json(
        state
            .store
            .update_task_status(
                channel_id,
                request.task_number,
                request.status,
                request.progress.as_deref(),
            )
            .await?,
    ))
}

#[derive(Deserialize)]
struct BridgeTaskDependenciesQuery {
    #[serde(default)]
    channel: String,
    task_number: i64,
}

async fn bridge_get_task_dependencies(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Query(query): Query<BridgeTaskDependenciesQuery>,
) -> Result<Json<TaskDependenciesResponse>, ApiError> {
    let agent = require_agent(&state.store, agent_id).await?;
    let channel_id = resolve_bridge_channel(&state.store, &agent, &query.channel).await?;
    if state
        .store
        .get_task(channel_id, query.task_number)
        .await?
        .is_none()
    {
        return Err(ApiError::not_found(format!(
            "task #{} not found",
            query.task_number
        )));
    }
    Ok(Json(TaskDependenciesResponse {
        task_number: query.task_number,
        depends_on: state
            .store
            .get_task_dependencies(channel_id, query.task_number)
            .await?,
    }))
}

#[derive(Deserialize)]
struct BridgeAddTaskDependencyRequest {
    #[serde(default)]
    channel: String,
    #[serde(alias = "taskNumber")]
    task_number: i64,
    #[serde(alias = "dependsOn")]
    depends_on: i64,
}

async fn bridge_add_task_dependency(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Json(request): Json<BridgeAddTaskDependencyRequest>,
) -> Result<Json<TaskDependenciesResponse>, ApiError> {
    let agent = require_agent(&state.store, agent_id).await?;
    let channel_id = resolve_bridge_channel(&state.store, &agent, &request.channel).await?;
    state
        .store
        .add_task_dependency(channel_id, request.task_number, request.depends_on)
        .await?;
    Ok(Json(TaskDependenciesResponse {
        task_number: request.task_number,
        depends_on: state
            .store
            .get_task_dependencies(channel_id, request.task_number)
            .await?,
    }))
}

#[derive(Deserialize)]
struct BridgeSetWorkingStateRequest {
    summary: String,
    #[serde(default, alias = "channelName")]
    channel_name: Option<String>,
    #[serde(default, alias = "taskNumber")]
    task_number: Option<i64>,
    #[serde(default, alias = "nextStepHint")]
    next_step_hint: Option<String>,
}

#[derive(Serialize)]
struct BridgeWorkingStateResponse {
    state: Option<cocli_store::WorkingState>,
}

async fn bridge_get_working_state(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
) -> Result<Json<BridgeWorkingStateResponse>, ApiError> {
    require_agent(&state.store, agent_id).await?;
    Ok(Json(BridgeWorkingStateResponse {
        state: state.store.get_working_state(agent_id).await?,
    }))
}

async fn bridge_set_working_state(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Json(request): Json<BridgeSetWorkingStateRequest>,
) -> Result<Json<BridgeWorkingStateResponse>, ApiError> {
    require_agent(&state.store, agent_id).await?;
    let summary = non_empty("working summary", &request.summary)?;
    let working = state
        .store
        .set_working_state(
            agent_id,
            summary,
            request.channel_name.as_deref(),
            request.task_number,
            request.next_step_hint.as_deref(),
        )
        .await?;
    Ok(Json(BridgeWorkingStateResponse {
        state: Some(working),
    }))
}

#[derive(Serialize)]
struct BridgeClearWorkingStateResponse {
    cleared: bool,
}

async fn bridge_clear_working_state(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
) -> Result<Json<BridgeClearWorkingStateResponse>, ApiError> {
    require_agent(&state.store, agent_id).await?;
    Ok(Json(BridgeClearWorkingStateResponse {
        cleared: state.store.clear_working_state(agent_id).await?,
    }))
}

#[derive(Deserialize)]
struct BridgeMemoryScopeQuery {
    scope: MemoryScope,
    #[serde(default)]
    channel_id: Option<Uuid>,
}

#[derive(Deserialize)]
struct BridgeMemoryTopicQuery {
    scope: MemoryScope,
    #[serde(default)]
    channel_id: Option<Uuid>,
    #[serde(rename = "type")]
    memory_type: String,
    topic: String,
}

#[derive(Serialize)]
struct BridgeMemoryNamespaceResponse {
    entries: Vec<MemoryDocumentEntry>,
}

async fn bridge_get_memory_index(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Query(query): Query<BridgeMemoryScopeQuery>,
) -> Result<Json<MemoryDocument>, ApiError> {
    let agent = require_agent(&state.store, agent_id).await?;
    let namespace =
        resolve_bridge_memory_namespace(&state.store, &agent, query.scope, query.channel_id)
            .await?;
    Ok(Json(state.store.get_memory_index(namespace).await?))
}

async fn bridge_list_memory_namespace(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Query(query): Query<BridgeMemoryScopeQuery>,
) -> Result<Json<BridgeMemoryNamespaceResponse>, ApiError> {
    let agent = require_agent(&state.store, agent_id).await?;
    let namespace =
        resolve_bridge_memory_namespace(&state.store, &agent, query.scope, query.channel_id)
            .await?;
    Ok(Json(BridgeMemoryNamespaceResponse {
        entries: state.store.list_memory_namespace(namespace).await?,
    }))
}

async fn bridge_get_memory_topic(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Query(query): Query<BridgeMemoryTopicQuery>,
) -> Result<Json<MemoryTopic>, ApiError> {
    let agent = require_agent(&state.store, agent_id).await?;
    let namespace =
        resolve_bridge_memory_namespace(&state.store, &agent, query.scope, query.channel_id)
            .await?;
    state
        .store
        .get_memory_topic(namespace, &query.memory_type, &query.topic)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::not_found("memory topic not found"))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BridgeWriteMemoryTopicRequest {
    scope: MemoryScope,
    #[serde(default, alias = "channel_id")]
    channel_id: Option<Uuid>,
    #[serde(rename = "type")]
    memory_type: String,
    topic: String,
    #[serde(default)]
    description: String,
    body: String,
    #[serde(default, alias = "if_version")]
    if_version: Option<i64>,
}

async fn bridge_write_memory_topic(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Json(request): Json<BridgeWriteMemoryTopicRequest>,
) -> Result<Json<MemoryTopic>, ApiError> {
    let agent = require_agent(&state.store, agent_id).await?;
    let namespace =
        resolve_bridge_memory_namespace(&state.store, &agent, request.scope, request.channel_id)
            .await?;
    Ok(Json(
        state
            .store
            .write_memory_topic(
                namespace,
                &request.memory_type,
                &request.topic,
                &request.description,
                &request.body,
                Some(&agent.name),
                request.if_version,
            )
            .await?,
    ))
}

#[derive(Deserialize)]
struct BridgeMoveMemoryTopicRequest {
    from_scope: MemoryScope,
    #[serde(default)]
    from_channel_id: Option<Uuid>,
    to_scope: MemoryScope,
    #[serde(default)]
    to_channel_id: Option<Uuid>,
    #[serde(rename = "type")]
    memory_type: String,
    topic: String,
}

async fn bridge_move_memory_topic(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Json(request): Json<BridgeMoveMemoryTopicRequest>,
) -> Result<Json<MemoryMoveResult>, ApiError> {
    let agent = require_agent(&state.store, agent_id).await?;
    let from = resolve_bridge_memory_namespace(
        &state.store,
        &agent,
        request.from_scope,
        request.from_channel_id,
    )
    .await?;
    let to = resolve_bridge_memory_namespace(
        &state.store,
        &agent,
        request.to_scope,
        request.to_channel_id,
    )
    .await?;
    Ok(Json(
        state
            .store
            .move_memory_topic(
                from,
                to,
                &request.memory_type,
                &request.topic,
                Some(&agent.name),
            )
            .await?,
    ))
}

async fn resolve_bridge_memory_namespace(
    store: &Store,
    agent: &Agent,
    scope: MemoryScope,
    channel_id: Option<Uuid>,
) -> Result<MemoryNamespace, ApiError> {
    match scope {
        MemoryScope::Agent => {
            if channel_id.is_some() {
                return Err(ApiError::bad_request(
                    "channel_id is forbidden for agent memory",
                ));
            }
            Ok(MemoryNamespace::Agent(agent.id))
        }
        MemoryScope::Channel => {
            let channel_id = channel_id.ok_or_else(|| {
                ApiError::bad_request("channel_id is required for channel memory")
            })?;
            require_agent_channel_membership(store, agent.id, channel_id).await?;
            Ok(MemoryNamespace::Channel(channel_id))
        }
    }
}

fn default_bridge_message_limit() -> i64 {
    50
}

async fn resolve_bridge_channel(
    store: &Store,
    agent: &Agent,
    target: &str,
) -> Result<Uuid, ApiError> {
    let target = target.trim();
    if target.is_empty() {
        return Err(ApiError::bad_request("channel target is required"));
    }
    let channel_id = if let Ok(channel_id) = Uuid::parse_str(target) {
        store
            .get_channel(channel_id)
            .await?
            .map(|channel| channel.id)
            .ok_or_else(|| ApiError::not_found("channel not found"))?
    } else {
        let name = target.strip_prefix('#').unwrap_or(target);
        store
            .get_channel_by_name(name)
            .await?
            .map(|channel| channel.id)
            .ok_or_else(|| ApiError::not_found("channel not found"))?
    };
    require_agent_channel_membership(store, agent.id, channel_id).await?;
    Ok(channel_id)
}

async fn prior_agent_operation(
    store: &Store,
    caller_agent_id: Uuid,
    action: &str,
    idempotency_key: Option<&str>,
    request_fingerprint: &str,
) -> Result<Option<AgentOperation>, ApiError> {
    let Some(key) = idempotency_key.map(str::trim).filter(|key| !key.is_empty()) else {
        return Ok(None);
    };
    if key.chars().count() > 200 {
        return Err(ApiError::bad_request(
            "idempotency_key must be at most 200 characters",
        ));
    }
    let Some(operation) = store
        .get_agent_operation(caller_agent_id, action, key)
        .await?
    else {
        return Ok(None);
    };
    if operation.request_fingerprint != request_fingerprint {
        return Err(ApiError::conflict(
            "idempotency_key was already used with different inputs",
        ));
    }
    Ok(Some(operation))
}

#[allow(clippy::too_many_arguments)]
async fn record_agent_operation(
    store: &Store,
    caller_agent_id: Uuid,
    action: &str,
    idempotency_key: Option<&str>,
    request_fingerprint: &str,
    result_type: &str,
    result_id: &str,
    source_channel_id: Option<Uuid>,
    source_session_id: Option<&str>,
) -> Result<AgentOperation, ApiError> {
    Ok(store
        .record_agent_operation(
            caller_agent_id,
            action,
            idempotency_key.map(str::trim).filter(|key| !key.is_empty()),
            request_fingerprint,
            result_type,
            result_id,
            source_channel_id,
            source_session_id
                .map(str::trim)
                .filter(|session| !session.is_empty()),
        )
        .await?)
}

async fn validate_operation_source_channel(
    store: &Store,
    caller_agent_id: Uuid,
    source_channel_id: Option<Uuid>,
) -> Result<Option<Uuid>, ApiError> {
    if let Some(channel_id) = source_channel_id {
        require_agent_channel_membership(store, caller_agent_id, channel_id).await?;
    }
    Ok(source_channel_id)
}

async fn bridge_workspace_owner(
    store: &Store,
    agent: &Agent,
    query: &BridgeWorkspaceQuery,
) -> Result<(&'static str, Uuid), ApiError> {
    match query.scope.trim() {
        "" | "agent" => Ok(("agent", agent.id)),
        "channel" => Ok((
            "channel",
            resolve_bridge_channel(store, agent, &query.channel).await?,
        )),
        other => Err(ApiError::bad_request(format!(
            "unsupported workspace scope: {other}"
        ))),
    }
}

async fn require_agent(store: &Store, agent_id: Uuid) -> Result<Agent, ApiError> {
    store
        .get_agent(agent_id)
        .await?
        .ok_or_else(|| ApiError::not_found("agent not found"))
}

async fn require_agent_channel_membership(
    store: &Store,
    agent_id: Uuid,
    channel_id: Uuid,
) -> Result<ChannelAgent, ApiError> {
    require_channel(store, channel_id).await?;
    store
        .get_channel_agent(channel_id, agent_id)
        .await?
        .ok_or_else(|| ApiError::forbidden("agent does not belong to the requested channel"))
}

async fn set_agent_status(
    store: &Store,
    agent_id: Uuid,
    status: AgentStatus,
) -> Result<Json<Agent>, ApiError> {
    let agent = store
        .set_agent_status(agent_id, status)
        .await?
        .ok_or_else(|| ApiError::not_found("agent not found"))?;
    Ok(Json(agent))
}

async fn set_agent_lifecycle_status(
    store: &Store,
    agent_id: Uuid,
    status: AgentLifecycleStatus,
) -> Result<Json<Agent>, ApiError> {
    let agent = store
        .set_agent_lifecycle_status(agent_id, status)
        .await?
        .ok_or_else(|| ApiError::not_found("agent not found"))?;
    Ok(Json(agent))
}

async fn list_messages(
    State(state): State<AppState>,
    Path(channel_id): Path<Uuid>,
) -> Result<Json<Vec<Message>>, ApiError> {
    Ok(Json(state.store.list_messages(channel_id).await?))
}

#[derive(Deserialize)]
struct TaskListQuery {
    status: Option<String>,
}

async fn list_tasks(
    State(state): State<AppState>,
    Path(channel_id): Path<Uuid>,
    Query(query): Query<TaskListQuery>,
) -> Result<Json<Vec<Task>>, ApiError> {
    require_channel(&state.store, channel_id).await?;
    let status = parse_task_status_filter(query.status.as_deref())?;
    Ok(Json(state.store.list_tasks(channel_id, status).await?))
}

#[derive(Deserialize)]
struct CreateTaskRequest {
    title: String,
    #[serde(default, alias = "messageId")]
    message_id: Option<Uuid>,
    #[serde(default, alias = "createdByAgentId")]
    created_by_agent_id: Option<Uuid>,
}

async fn create_task(
    State(state): State<AppState>,
    Path(channel_id): Path<Uuid>,
    Json(request): Json<CreateTaskRequest>,
) -> Result<(StatusCode, Json<Task>), ApiError> {
    require_channel(&state.store, channel_id).await?;
    let title = non_empty("task title", &request.title)?;
    Ok((
        StatusCode::CREATED,
        Json(
            state
                .store
                .create_task(
                    channel_id,
                    title,
                    request.message_id,
                    request.created_by_agent_id,
                )
                .await?,
        ),
    ))
}

#[derive(Deserialize)]
struct ClaimTaskRequest {
    #[serde(alias = "agentId")]
    agent_id: Uuid,
}

async fn claim_task(
    State(state): State<AppState>,
    Path((channel_id, task_number)): Path<(Uuid, i64)>,
    Json(request): Json<ClaimTaskRequest>,
) -> Result<Json<Task>, ApiError> {
    let agent = require_agent(&state.store, request.agent_id).await?;
    require_agent_channel_membership(&state.store, agent.id, channel_id).await?;
    Ok(Json(
        state
            .store
            .claim_task(channel_id, task_number, agent.id)
            .await?,
    ))
}

async fn unclaim_task(
    State(state): State<AppState>,
    Path((channel_id, task_number)): Path<(Uuid, i64)>,
) -> Result<Json<Task>, ApiError> {
    Ok(Json(
        state.store.unclaim_task(channel_id, task_number).await?,
    ))
}

#[derive(Deserialize)]
struct UpdateTaskStatusRequest {
    status: TaskStatus,
    progress: Option<String>,
}

async fn update_task_status(
    State(state): State<AppState>,
    Path((channel_id, task_number)): Path<(Uuid, i64)>,
    Json(request): Json<UpdateTaskStatusRequest>,
) -> Result<Json<Task>, ApiError> {
    Ok(Json(
        state
            .store
            .update_task_status(
                channel_id,
                task_number,
                request.status,
                request.progress.as_deref(),
            )
            .await?,
    ))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TaskDependenciesResponse {
    task_number: i64,
    depends_on: Vec<i64>,
}

async fn get_task_dependencies(
    State(state): State<AppState>,
    Path((channel_id, task_number)): Path<(Uuid, i64)>,
) -> Result<Json<TaskDependenciesResponse>, ApiError> {
    if state
        .store
        .get_task(channel_id, task_number)
        .await?
        .is_none()
    {
        return Err(ApiError::not_found(format!(
            "task #{task_number} not found"
        )));
    }
    Ok(Json(TaskDependenciesResponse {
        task_number,
        depends_on: state
            .store
            .get_task_dependencies(channel_id, task_number)
            .await?,
    }))
}

#[derive(Deserialize)]
struct TaskDependencyRequest {
    #[serde(alias = "dependsOn")]
    depends_on: i64,
}

async fn add_task_dependency(
    State(state): State<AppState>,
    Path((channel_id, task_number)): Path<(Uuid, i64)>,
    Json(request): Json<TaskDependencyRequest>,
) -> Result<(StatusCode, Json<TaskDependenciesResponse>), ApiError> {
    state
        .store
        .add_task_dependency(channel_id, task_number, request.depends_on)
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(TaskDependenciesResponse {
            task_number,
            depends_on: state
                .store
                .get_task_dependencies(channel_id, task_number)
                .await?,
        }),
    ))
}

async fn remove_task_dependency(
    State(state): State<AppState>,
    Path((channel_id, task_number)): Path<(Uuid, i64)>,
    Json(request): Json<TaskDependencyRequest>,
) -> Result<Json<TaskDependenciesResponse>, ApiError> {
    state
        .store
        .remove_task_dependency(channel_id, task_number, request.depends_on)
        .await?;
    Ok(Json(TaskDependenciesResponse {
        task_number,
        depends_on: state
            .store
            .get_task_dependencies(channel_id, task_number)
            .await?,
    }))
}

#[derive(Deserialize)]
struct PostMessageRequest {
    content: String,
}

#[derive(Serialize)]
struct PostMessageResponse {
    message: Message,
    replies: Vec<Message>,
    pending_deliveries: Vec<Delivery>,
}

async fn post_message(
    State(state): State<AppState>,
    Path(channel_id): Path<Uuid>,
    Json(request): Json<PostMessageRequest>,
) -> Result<(StatusCode, Json<PostMessageResponse>), ApiError> {
    require_channel(&state.store, channel_id).await?;
    post_message_to_channel(&state, channel_id, &request.content).await
}

async fn post_message_to_channel(
    state: &AppState,
    channel_id: Uuid,
    content: &str,
) -> Result<(StatusCode, Json<PostMessageResponse>), ApiError> {
    let content = non_empty("message content", content)?;
    let message = state
        .store
        .append_user_message_with_deliveries(channel_id, content)
        .await?;
    let replies = state.deliveries.dispatch_message(message.id).await?.replies;
    let pending_deliveries = state.store.list_message_deliveries(message.id).await?;
    let status = if pending_deliveries.is_empty() {
        StatusCode::CREATED
    } else {
        StatusCode::ACCEPTED
    };
    Ok((
        status,
        Json(PostMessageResponse {
            message,
            replies,
            pending_deliveries,
        }),
    ))
}

fn non_empty<'a>(field: &str, value: &'a str) -> Result<&'a str, ApiError> {
    let value = value.trim();
    if value.is_empty() {
        Err(ApiError::bad_request(format!("{field} must not be empty")))
    } else {
        Ok(value)
    }
}

fn parse_task_status_filter(status: Option<&str>) -> Result<Option<TaskStatus>, ApiError> {
    match status.map(str::trim).filter(|value| !value.is_empty()) {
        None | Some("all") => Ok(None),
        Some("todo") => Ok(Some(TaskStatus::Todo)),
        Some("in_progress") => Ok(Some(TaskStatus::InProgress)),
        Some("in_review") => Ok(Some(TaskStatus::InReview)),
        Some("done") => Ok(Some(TaskStatus::Done)),
        Some(value) => Err(ApiError::bad_request(format!(
            "unsupported task status: {value}"
        ))),
    }
}

async fn require_channel(store: &Store, channel_id: Uuid) -> Result<Channel, ApiError> {
    store
        .get_channel(channel_id)
        .await?
        .ok_or_else(|| ApiError::not_found("channel not found"))
}

async fn require_workspace(store: &Store, workspace_id: Uuid) -> Result<Workspace, ApiError> {
    store
        .get_workspace(workspace_id)
        .await?
        .ok_or_else(|| ApiError::not_found("workspace not found"))
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
    body: Option<serde_json::Value>,
}

impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
            body: None,
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
            body: None,
        }
    }

    fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: message.into(),
            body: None,
        }
    }

    fn conflict(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            message: message.into(),
            body: None,
        }
    }

    fn forbidden(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            message: message.into(),
            body: None,
        }
    }

    fn json(status: StatusCode, body: serde_json::Value) -> Self {
        let message = body
            .get("error")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("request failed")
            .to_owned();
        Self {
            status,
            message,
            body: Some(body),
        }
    }
}

impl From<StoreError> for ApiError {
    fn from(error: StoreError) -> Self {
        let status = match &error {
            StoreError::TaskNotFound { .. } => Some(StatusCode::NOT_FOUND),
            StoreError::TaskAlreadyClaimed { .. }
            | StoreError::TaskUnmetDependencies { .. }
            | StoreError::InvalidTaskTransition { .. }
            | StoreError::TaskDependencyCycle => Some(StatusCode::CONFLICT),
            StoreError::TaskDependencySelf => Some(StatusCode::BAD_REQUEST),
            StoreError::InvalidMemoryType(_)
            | StoreError::InvalidMemoryTopic(_)
            | StoreError::InvalidMemoryDescription(_)
            | StoreError::MemoryMoveSameNamespace => Some(StatusCode::BAD_REQUEST),
            StoreError::MemoryTopicTooLarge { .. }
            | StoreError::MemoryNamespaceFull { .. }
            | StoreError::MemoryIndexFull { .. } => Some(StatusCode::UNPROCESSABLE_ENTITY),
            StoreError::SkillLibraryNotFound(_) | StoreError::SkillInstallNotFound(_) => {
                Some(StatusCode::NOT_FOUND)
            }
            StoreError::WorkspaceNotFound(_) | StoreError::WorkspaceBindingNotFound(_) => {
                Some(StatusCode::NOT_FOUND)
            }
            StoreError::SkillNameConflict(_) | StoreError::SkillAlreadyInstalled { .. } => {
                Some(StatusCode::CONFLICT)
            }
            StoreError::SkillGovernanceNotFound { .. } => Some(StatusCode::NOT_FOUND),
            StoreError::SkillGovernanceVersionConflict { .. }
            | StoreError::SkillGovernanceTransitionConflict { .. }
            | StoreError::SkillGovernanceLockHeld { .. }
            | StoreError::SkillGovernanceIdempotencyConflict { .. } => Some(StatusCode::CONFLICT),
            StoreError::InvalidSkillName(_)
            | StoreError::InvalidSkillFilePath(_)
            | StoreError::InvalidSkillFileSize { .. } => Some(StatusCode::BAD_REQUEST),
            StoreError::SkillLibraryTooLarge { .. } => Some(StatusCode::UNPROCESSABLE_ENTITY),
            _ => None,
        };
        if let Some(status) = status {
            return Self {
                status,
                message: error.to_string(),
                body: None,
            };
        }
        tracing::error!(%error, "local store request failed");
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: "local store request failed".to_owned(),
            body: None,
        }
    }
}

impl From<RuntimeError> for ApiError {
    fn from(error: RuntimeError) -> Self {
        tracing::error!(%error, "runtime delivery failed");
        let status = match error {
            RuntimeError::Delivery(_) => StatusCode::BAD_GATEWAY,
            RuntimeError::Unsupported(_) => StatusCode::UNPROCESSABLE_ENTITY,
            RuntimeError::Busy(_) => StatusCode::CONFLICT,
            RuntimeError::NotFound(_) => StatusCode::NOT_FOUND,
        };
        Self {
            status,
            message: error.to_string(),
            body: None,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        if let Some(body) = self.body {
            return (self.status, Json(body)).into_response();
        }
        (
            self.status,
            Json(ErrorResponse {
                error: self.message,
            }),
        )
            .into_response()
    }
}
