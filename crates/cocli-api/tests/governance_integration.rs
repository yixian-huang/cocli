use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use cocli_api::{
    governance_artifact_digests, router, GovernanceScopeCapability, GovernanceSkillTarget,
    McpInventory, RuntimeError, RuntimeInfo, RuntimeService, RuntimeSkill,
    RuntimeSkillCompatibility, RuntimeSkillEvidence, RuntimeSkillFinding, RuntimeSkillInspection,
    RuntimeSkillSearchPath,
};
use cocli_driver_core::{
    hash_mcp_capabilities, McpApplyActionResult, McpApplyActionStatus, McpApplyExecutionRequest,
    McpApplyExecutionResult, McpBackupDescriptor, McpCapabilityDetail, McpCapabilityOperation,
    McpCapabilitySnapshot, McpCapabilitySupport, McpPlan, McpPreflightAction, McpPreflightReport,
    McpReloadResult, McpReloadStatus, McpReloadStrategy, McpRollbackExecutionRequest,
    McpRollbackExecutionResult, McpRuntimeCapability, McpVerificationResult, McpVerificationStatus,
};
use cocli_store::{Agent, AgentStatus, Message, Store, WorkspaceProviderKey};
use serde_json::{json, Value};
use tempfile::tempdir;
use tower::ServiceExt;

const SECRET_CANARY: &str = "GOVERNANCE_INTEGRATION_SECRET_CANARY";

#[derive(Debug)]
struct UnifiedGovernanceRuntime {
    skill_workspace_root: PathBuf,
    mcp_config_root: PathBuf,
    mcp_applied: AtomicBool,
    mcp_apply_calls: AtomicUsize,
    mcp_rollback_calls: AtomicUsize,
}

impl UnifiedGovernanceRuntime {
    fn new(skill_workspace_root: PathBuf, mcp_config_root: PathBuf) -> Self {
        Self {
            skill_workspace_root,
            mcp_config_root,
            mcp_applied: AtomicBool::new(false),
            mcp_apply_calls: AtomicUsize::new(0),
            mcp_rollback_calls: AtomicUsize::new(0),
        }
    }

    fn target(&self, agent: &Agent, skill_name: &str) -> GovernanceSkillTarget {
        let scope_root = self.skill_workspace_root.join(agent.id.to_string());
        let search_root = scope_root.join(".fake/skills");
        GovernanceSkillTarget {
            scope_root,
            search_root: search_root.clone(),
            entry_path: search_root.join(skill_name),
        }
    }
}

#[async_trait]
impl RuntimeService for UnifiedGovernanceRuntime {
    async fn list(&self) -> Vec<RuntimeInfo> {
        vec![RuntimeInfo {
            name: "fake".to_owned(),
            installed: true,
            binary: None,
            version: Some("governance-integration-test".to_owned()),
            models: vec!["test-model".to_owned()],
            capabilities: vec!["reply".to_owned(), "skills:supported".to_owned()],
            unavailable_reason: None,
        }]
    }

    async fn reply(&self, _agent: &Agent, message: &Message) -> Result<String, RuntimeError> {
        Ok(format!("echo: {}", message.content))
    }

    fn skill_compatibility(&self, runtime: &str) -> RuntimeSkillCompatibility {
        if runtime == "fake" {
            RuntimeSkillCompatibility::Supported
        } else {
            RuntimeSkillCompatibility::Unknown
        }
    }

    async fn inspect_skills(&self, agent: &Agent) -> Result<RuntimeSkillInspection, RuntimeError> {
        let target = self.target(agent, "shared-governance");
        let evidence = RuntimeSkillEvidence {
            source: "filesystem".to_owned(),
            detail: "isolated integration test roots".to_owned(),
            proves_session_visibility: false,
        };
        let skills = target
            .entry_path
            .join("SKILL.md")
            .is_file()
            .then(|| RuntimeSkillFinding {
                skill: RuntimeSkill {
                    name: "shared-governance".to_owned(),
                    display_name: "Shared Governance".to_owned(),
                    description: "integration fixture".to_owned(),
                    user_invocable: true,
                    skill_type: "agent".to_owned(),
                    path: target
                        .entry_path
                        .join("SKILL.md")
                        .to_string_lossy()
                        .into_owned(),
                    install_path: Some(".fake/skills/shared-governance".to_owned()),
                },
                runtime: "fake".to_owned(),
                fingerprint: "sha256:shared-prefix-skill-domain".to_owned(),
                scope: "agent".to_owned(),
                source_path: target.entry_path.to_string_lossy().into_owned(),
                resolved_path: target
                    .entry_path
                    .canonicalize()
                    .ok()
                    .map(|path| path.to_string_lossy().into_owned()),
                presence: "installed".to_owned(),
                evidence: evidence.clone(),
                enabled: Some(true),
                valid: Some(true),
                duplicate: false,
                shadowed: false,
                issues: Vec::new(),
            })
            .into_iter()
            .collect();
        Ok(RuntimeSkillInspection {
            observed_at: chrono::Utc::now(),
            runtime: "fake".to_owned(),
            compatibility: RuntimeSkillCompatibility::Supported,
            evidence,
            search_paths: vec![RuntimeSkillSearchPath {
                path: target.search_root.to_string_lossy().into_owned(),
                scope: "agent".to_owned(),
                exists: target.search_root.exists(),
                readable: target.search_root.exists(),
                symlink: false,
                resolved_path: target
                    .search_root
                    .canonicalize()
                    .ok()
                    .map(|path| path.to_string_lossy().into_owned()),
                issue: None,
            }],
            skills,
            issues: Vec::new(),
        })
    }

    async fn inspect_machine_skills(
        &self,
        runtime: &str,
    ) -> Result<RuntimeSkillInspection, RuntimeError> {
        Ok(RuntimeSkillInspection {
            observed_at: chrono::Utc::now(),
            runtime: runtime.to_owned(),
            compatibility: self.skill_compatibility(runtime),
            evidence: RuntimeSkillEvidence::default(),
            search_paths: Vec::new(),
            skills: Vec::new(),
            issues: Vec::new(),
        })
    }

    async fn governance_skill_target(
        &self,
        agent: &Agent,
        skill_name: &str,
    ) -> Result<GovernanceSkillTarget, RuntimeError> {
        Ok(self.target(agent, skill_name))
    }

    async fn governance_scope_capabilities(
        &self,
        runtime: &str,
        scope: &str,
        scope_root: Option<&Path>,
    ) -> Result<Vec<GovernanceScopeCapability>, RuntimeError> {
        let root = match scope {
            "machine" => self.skill_workspace_root.join("machine/.fake/skills"),
            "workspace" => scope_root
                .ok_or_else(|| RuntimeError::Unsupported("workspace root required".to_owned()))?
                .join(".fake/skills"),
            "agent" => self.skill_workspace_root.join("agent-probe/.fake/skills"),
            _ => return Ok(Vec::new()),
        };
        Ok(vec![GovernanceScopeCapability {
            runtime: runtime.to_owned(),
            scope: scope.to_owned(),
            root_kind: scope.to_owned(),
            path: root.to_string_lossy().into_owned(),
            status: "supported".to_owned(),
            exists: root.exists(),
            writable: true,
            atomic_rename: true,
            supported: true,
            evidence: "isolated fake runtime root".to_owned(),
            blocked_reason: None,
        }])
    }

    async fn governance_managed_artifact_root(&self) -> Result<PathBuf, RuntimeError> {
        Ok(self.skill_workspace_root.join("managed-artifacts"))
    }

    async fn inspect_mcp(&self) -> Result<McpInventory, RuntimeError> {
        Ok(McpInventory {
            observed_at: "2026-07-19T00:00:00Z".to_owned(),
            ..McpInventory::default()
        })
    }

    async fn inspect_mcp_capabilities(&self) -> Result<McpCapabilitySnapshot, RuntimeError> {
        let mut operations = BTreeMap::new();
        for operation in [
            McpCapabilityOperation::ReadDiscover,
            McpCapabilityOperation::AddConfigure,
            McpCapabilityOperation::EnableDisable,
            McpCapabilityOperation::Verify,
            McpCapabilityOperation::Rollback,
        ] {
            operations.insert(
                operation,
                McpCapabilityDetail {
                    support: McpCapabilitySupport::Supported,
                    reason: "isolated integration test adapter".to_owned(),
                    evidence: Vec::new(),
                },
            );
        }
        let mut snapshot = McpCapabilitySnapshot {
            hash: String::new(),
            observed_at: "2026-07-19T00:00:00Z".to_owned(),
            runtimes: vec![McpRuntimeCapability {
                runtime: "fake".to_owned(),
                adapter: "fake_governance_adapter".to_owned(),
                binary_path: Some(
                    self.mcp_config_root
                        .join("fake")
                        .to_string_lossy()
                        .into_owned(),
                ),
                binary_version: Some("1.0.0".to_owned()),
                config_schema_version: "fake.mcp_servers.v1".to_owned(),
                destination: self
                    .mcp_config_root
                    .join("mcp.json")
                    .to_string_lossy()
                    .into_owned(),
                allowed_subtree: "mcp_servers".to_owned(),
                reload_strategy: McpReloadStrategy::Deferred,
                operations,
            }],
        };
        snapshot.hash = hash_mcp_capabilities(&snapshot);
        Ok(snapshot)
    }

    async fn preflight_mcp(&self, plan: &McpPlan) -> Result<McpPreflightReport, RuntimeError> {
        Ok(McpPreflightReport {
            plan_id: plan.id.clone(),
            plan_hash: plan.plan_hash.clone(),
            capability_hash: plan.capability_hash.clone(),
            observation_hash: plan.observation_hash.clone(),
            config_hash: plan.config_hash.clone(),
            actions: plan
                .actions
                .iter()
                .enumerate()
                .map(|(action_index, action)| McpPreflightAction {
                    action_index,
                    runtime: action.runtime.clone(),
                    server_id: action.server_id.clone(),
                    operation: McpCapabilityOperation::AddConfigure,
                    support: McpCapabilitySupport::Supported,
                    executable: true,
                    reason: "fake adapter preflight passed".to_owned(),
                    adapter: "fake_governance_adapter".to_owned(),
                    destination: self
                        .mcp_config_root
                        .join("mcp.json")
                        .to_string_lossy()
                        .into_owned(),
                    allowed_subtree: "mcp_servers".to_owned(),
                    reload_strategy: McpReloadStrategy::Deferred,
                    idempotency_key: format!("mcp:{}:{action_index}", plan.id),
                    expected_source_hash: action.expected_source_hash.clone(),
                    expected_schema_hash: action.expected_schema_hash.clone(),
                })
                .collect(),
            stale_reasons: Vec::new(),
            executable: true,
        })
    }

    async fn apply_mcp(
        &self,
        request: McpApplyExecutionRequest,
        _journal: Arc<dyn cocli_api::McpApplyJournalSink>,
    ) -> Result<McpApplyExecutionResult, RuntimeError> {
        self.mcp_apply_calls.fetch_add(1, Ordering::SeqCst);
        self.mcp_applied.store(true, Ordering::SeqCst);
        Ok(McpApplyExecutionResult {
            actions: request
                .plan
                .actions
                .iter()
                .enumerate()
                .map(|(action_index, action)| McpApplyActionResult {
                    action_index,
                    runtime: action.runtime.clone(),
                    server_id: action.server_id.clone(),
                    status: McpApplyActionStatus::Verified,
                    reason: "fake MCP config write verified".to_owned(),
                    backup: Some(McpBackupDescriptor {
                        id: format!("mcp-backup-{action_index}"),
                        runtime: action.runtime.clone(),
                        source_path: self
                            .mcp_config_root
                            .join("mcp.json")
                            .to_string_lossy()
                            .into_owned(),
                        backup_path: self
                            .mcp_config_root
                            .join("mcp.backup.json")
                            .to_string_lossy()
                            .into_owned(),
                        source_hash: "sha256:shared-prefix-mcp-before".to_owned(),
                        backup_hash: "sha256:shared-prefix-mcp-before".to_owned(),
                        applied_hash: "sha256:shared-prefix-mcp-after".to_owned(),
                        source_existed: true,
                    }),
                    before_source_hash: Some("sha256:shared-prefix-mcp-before".to_owned()),
                    after_source_hash: Some("sha256:shared-prefix-mcp-after".to_owned()),
                })
                .collect(),
            reloads: vec![McpReloadResult {
                runtime: "fake".to_owned(),
                status: McpReloadStatus::Deferred,
                reason: "runtime restart intentionally deferred".to_owned(),
            }],
            verification: McpVerificationResult {
                status: McpVerificationStatus::Matched,
                observation_hash: "sha256:shared-prefix-mcp-verified".to_owned(),
                mismatches: Vec::new(),
                written_config_hashes: BTreeMap::new(),
                session_effective: Default::default(),
            },
            journal: Vec::new(),
        })
    }

    async fn rollback_mcp(
        &self,
        request: McpRollbackExecutionRequest,
    ) -> Result<McpRollbackExecutionResult, RuntimeError> {
        self.mcp_rollback_calls.fetch_add(1, Ordering::SeqCst);
        Ok(McpRollbackExecutionResult {
            actions: request
                .backups
                .into_iter()
                .enumerate()
                .map(|(action_index, backup)| McpApplyActionResult {
                    action_index,
                    runtime: backup.runtime.clone(),
                    server_id: "shared-governance".to_owned(),
                    status: McpApplyActionStatus::RolledBack,
                    reason: "fake MCP rollback restored backup".to_owned(),
                    backup: Some(backup),
                    before_source_hash: None,
                    after_source_hash: None,
                })
                .collect(),
            verification: McpVerificationResult {
                status: McpVerificationStatus::Matched,
                observation_hash: "sha256:shared-prefix-mcp-rollback".to_owned(),
                mismatches: Vec::new(),
                written_config_hashes: BTreeMap::new(),
                session_effective: Default::default(),
            },
        })
    }
}

// Skill materialization uses Unix filesystem semantics (permissions/sync);
// Windows apply currently rolls back on governance directory sync.
#[cfg(unix)]
#[tokio::test]
async fn skill_and_mcp_governance_share_store_without_cross_domain_collisions() {
    let temp = tempdir().expect("temp directory");
    let home = temp.path().join("home");
    let workspace_root = temp.path().join("workspace");
    let db_path = temp.path().join("cocli.sqlite3");
    let skill_source = temp.path().join("skill-source");
    let runtime_root = temp.path().join("runtime-skill-roots");
    let mcp_config_root = temp.path().join("mcp-config");
    std::fs::create_dir_all(&home).expect("temp home");
    std::fs::create_dir_all(workspace_root.join(".cocli")).expect("workspace");
    std::fs::create_dir_all(&skill_source).expect("skill source");
    std::fs::create_dir_all(&mcp_config_root).expect("mcp config root");
    std::fs::write(
        skill_source.join("SKILL.md"),
        "---\nname: shared-governance\ndisplay-name: Shared Governance\ndescription: integration fixture\n---\n# Shared Governance\n",
    )
    .expect("skill manifest");
    std::fs::write(workspace_root.join(".cocli/skills.lock.json"), "{}\n").expect("lockfile");

    let digests = governance_artifact_digests(&skill_source).expect("artifact digests");
    let runtime = Arc::new(UnifiedGovernanceRuntime::new(
        runtime_root.clone(),
        mcp_config_root.clone(),
    ));
    let store = Store::open(&db_path).await.expect("file-backed store");
    let installation_id = store.current_installation_id().to_owned();
    let channel = store.create_channel("governance").await.expect("channel");
    let agent = store
        .create_agent(channel.id, "governed", "fake", None, AgentStatus::Stopped)
        .await
        .expect("agent");
    let workspace = store
        .create_workspace(
            WorkspaceProviderKey::new("directory").expect("provider"),
            "governance workspace",
            None,
            json!({ "home": home }),
        )
        .await
        .expect("workspace");
    store
        .bind_workspace(
            workspace.id,
            workspace_root.to_str().expect("workspace utf8"),
            None,
        )
        .await
        .expect("workspace binding");
    let app = router(store.clone(), runtime.clone());
    tokio::time::sleep(Duration::from_millis(50)).await;

    let (skill_profile_status, skill_profile) = json_request(
        app.clone(),
        "POST",
        "/api/skills/governance/profiles",
        json!({
            "schemaVersion": 1,
            "name": "shared-governance",
            "skills": [{
                "logicalIdentity": "shared-governance",
                "source": {"kind": "local", "location": skill_source.to_string_lossy()},
                "contentDigest": digests.content_digest,
                "manifestDigest": digests.manifest_digest,
                "targetRuntime": "fake",
                "installScope": "agent",
                "installationMode": "copy",
                "enabled": true,
                "updatePolicy": "pinned",
                "allowedSources": ["local"],
                "riskPolicy": "trusted"
            }]
        }),
    )
    .await;
    assert_eq!(skill_profile_status, StatusCode::CREATED, "{skill_profile}");
    let skill_profile_id = skill_profile["id"].as_str().expect("skill profile id");
    let (skill_binding_status, skill_binding) = json_request(
        app.clone(),
        "POST",
        "/api/skills/governance/bindings",
        json!({"profileId": skill_profile_id, "scope": "agent", "scopeId": agent.id}),
    )
    .await;
    assert_eq!(skill_binding_status, StatusCode::CREATED, "{skill_binding}");

    let (canary_status, canary_response) = json_request(
        app.clone(),
        "POST",
        "/api/runtimes/mcp/profiles",
        json!({
            "name": "rejected-canary",
            "servers": [{
                "serverId": "rejected-canary",
                "runtime": "fake",
                "alias": "rejected-canary",
                "definition": {
                    "transport": "stdio",
                    "command": "fake-mcp",
                    "args": ["--api-key", SECRET_CANARY]
                },
                "desiredEnabled": true,
                "allowTools": [],
                "denyTools": [],
                "approvalMode": "manual",
                "secretRefs": []
            }]
        }),
    )
    .await;
    assert_eq!(canary_status, StatusCode::BAD_REQUEST, "{canary_response}");
    assert!(!canary_response.to_string().contains(SECRET_CANARY));

    let (mcp_profile_status, mcp_profile) = json_request(
        app.clone(),
        "POST",
        "/api/runtimes/mcp/profiles",
        json!({
            "name": "shared-governance",
            "description": "MCP side of unified governance integration",
            "servers": [{
                "serverId": "shared-governance",
                "runtime": "fake",
                "alias": "shared-governance",
                "definition": {
                    "transport": "http",
                    "endpoint": "https://example.test/mcp"
                },
                "desiredEnabled": true,
                "allowTools": [],
                "denyTools": [],
                "approvalMode": "manual",
                "secretRefs": [{
                    "location": "headers.authorization",
                    "kind": "bearer",
                    "reference": "keychain://cocli/integration-token"
                }]
            }]
        }),
    )
    .await;
    assert_eq!(mcp_profile_status, StatusCode::CREATED, "{mcp_profile}");
    let mcp_profile_id = mcp_profile["id"].as_str().expect("mcp profile id");
    let (mcp_binding_status, _) = json_request(
        app.clone(),
        "POST",
        "/api/runtimes/mcp/bindings",
        json!({"profileId": mcp_profile_id, "targetType": "machine"}),
    )
    .await;
    assert_eq!(mcp_binding_status, StatusCode::CREATED);

    let skill_plan_request = json!({
        "scope": "agent",
        "scopeId": agent.id,
        "agentId": agent.id,
        "force": true
    });
    let (skill_plan_status, skill_plan) = json_request(
        app.clone(),
        "POST",
        "/api/skills/governance/plans",
        skill_plan_request,
    )
    .await;
    assert_eq!(skill_plan_status, StatusCode::CREATED, "{skill_plan}");
    let skill_plan_id = skill_plan["plan"]["id"].as_str().expect("skill plan id");
    let (skill_approve_status, skill_approved) = json_request(
        app.clone(),
        "POST",
        &format!("/api/skills/governance/plans/{skill_plan_id}/approve"),
        json!({"expectedVersion": 1}),
    )
    .await;
    assert_eq!(skill_approve_status, StatusCode::OK, "{skill_approved}");
    let skill_approved_version = skill_approved["plan"]["version"]
        .as_i64()
        .expect("skill approved version");
    let (skill_apply_preview_status, skill_apply_preview) = json_request(
        app.clone(),
        "POST",
        &format!("/api/skills/governance/plans/{skill_plan_id}/apply/preview"),
        json!({}),
    )
    .await;
    assert_eq!(
        skill_apply_preview_status,
        StatusCode::OK,
        "{skill_apply_preview}"
    );
    let skill_nonce = skill_apply_preview["confirmationNonce"]
        .as_str()
        .expect("skill nonce")
        .to_owned();
    let skill_idempotency = skill_apply_preview["idempotencyKey"]
        .as_str()
        .expect("skill idempotency")
        .to_owned();
    let (skill_apply_status, skill_applied) = json_request(
        app.clone(),
        "POST",
        &format!("/api/skills/governance/plans/{skill_plan_id}/apply"),
        json!({
            "expectedVersion": skill_approved_version,
            "idempotencyKey": skill_idempotency,
            "confirmationNonce": skill_nonce,
            "confirmHighRisk": false
        }),
    )
    .await;
    assert_eq!(skill_apply_status, StatusCode::OK, "{skill_applied}");
    assert_eq!(skill_applied["run"]["status"], "succeeded");
    let skill_run_id = skill_applied["run"]["id"].as_str().expect("skill run id");
    assert!(runtime_root
        .join(agent.id.to_string())
        .join(".fake/skills/shared-governance/SKILL.md")
        .is_file());

    let (mcp_plan_status, mcp_plan) =
        json_request(app.clone(), "POST", "/api/runtimes/mcp/plans", json!({})).await;
    assert_eq!(mcp_plan_status, StatusCode::CREATED, "{mcp_plan}");
    let mcp_plan = &mcp_plan["plan"];
    let mcp_plan_id = mcp_plan["id"].as_str().expect("mcp plan id");
    let mcp_plan_hash = mcp_plan["planHash"].as_str().expect("mcp plan hash");
    let mcp_observation_hash = mcp_plan["observationHash"]
        .as_str()
        .expect("mcp observation hash");
    let mcp_config_hash = mcp_plan["configHash"].as_str().expect("mcp config hash");
    let (mcp_approve_status, mcp_approved) = json_request(
        app.clone(),
        "POST",
        &format!("/api/runtimes/mcp/plans/{mcp_plan_id}/approve"),
        json!({
            "planHash": mcp_plan_hash,
            "actor": "integration-test",
            "expiresAt": "2099-07-19T10:00:00Z"
        }),
    )
    .await;
    assert_eq!(mcp_approve_status, StatusCode::OK, "{mcp_approved}");
    let (mcp_apply_status, mcp_applied) = json_request(
        app.clone(),
        "POST",
        &format!("/api/runtimes/mcp/plans/{mcp_plan_id}/apply"),
        json!({
            "planHash": mcp_plan_hash,
            "observationHash": mcp_observation_hash,
            "configHash": mcp_config_hash,
            "actor": "integration-test",
            "confirmHighRisk": true
        }),
    )
    .await;
    assert_eq!(mcp_apply_status, StatusCode::OK, "{mcp_applied}");
    assert_eq!(mcp_applied["run"]["status"], "verified");
    let mcp_run_id = mcp_applied["run"]["id"].as_str().expect("mcp run id");
    assert_eq!(runtime.mcp_apply_calls.load(Ordering::SeqCst), 1);

    let (bundle_status, bundle_preview) = json_request(
        app.clone(),
        "POST",
        "/api/runtimes/mcp/bundles/export-preview",
        json!({"actor": "integration-test", "includeCapabilityExpectations": true}),
    )
    .await;
    assert_eq!(bundle_status, StatusCode::OK, "{bundle_preview}");
    let bundle_text = bundle_preview["bundle"].to_string();
    assert!(!bundle_text.contains("approvalId"));
    assert!(!bundle_text.contains(SECRET_CANARY));

    let (lock_status, lock_preview) = json_request(
        app.clone(),
        "POST",
        "/api/skills/governance/lock/preview",
        json!({
            "scope": "workspace",
            "scopeId": workspace.id,
            "workspaceId": workspace.id,
            "force": true
        }),
    )
    .await;
    assert_eq!(lock_status, StatusCode::OK, "{lock_preview}");
    assert_eq!(lock_preview["writesRealDirectories"], false);
    assert!(lock_preview["preview"]["lockfileHash"].is_string());
    let (lockfile_read_status, lockfile_read) = json_request(
        app.clone(),
        "GET",
        &format!(
            "/api/skills/governance/workspace-lockfile?workspaceId={}",
            workspace.id
        ),
        json!({}),
    )
    .await;
    assert_eq!(lockfile_read_status, StatusCode::OK, "{lockfile_read}");
    let stored_lockfile = store
        .upsert_skill_governance_workspace_lockfile(
            &workspace.id.to_string(),
            ".cocli/skills.lock.json",
            "sha256:workspace-lock-v2",
            lockfile_read["diskFingerprint"]
                .as_str()
                .expect("disk fingerprint"),
            lockfile_read["diskHash"].as_str().expect("disk hash"),
            json!({"generation": 2}),
            None,
            None,
            json!({"createdVia": "governance_integration"}),
            json!({
                "restoreDocument": {"generation": 1},
                "restoreLockHash": "sha256:workspace-lock-v1"
            }),
            None,
        )
        .await
        .expect("workspace lockfile record");

    store.close().await;

    let reopened_store = Store::open(&db_path).await.expect("reopened store");
    let reopened_app = router(reopened_store.clone(), runtime.clone());
    tokio::time::sleep(Duration::from_millis(50)).await;
    let (skill_inventory_status, skill_inventory) = json_request(
        reopened_app.clone(),
        "GET",
        &format!("/api/agents/{}/skills/inventory?force=true", agent.id),
        json!({}),
    )
    .await;
    assert_eq!(skill_inventory_status, StatusCode::OK, "{skill_inventory}");
    assert_eq!(skill_inventory["skills"][0]["name"], "shared-governance");
    assert_eq!(
        skill_inventory["skills"][0]["sessionEffective"]["status"],
        "unknown"
    );
    let (mcp_inventory_status, mcp_inventory) = json_request(
        reopened_app.clone(),
        "GET",
        "/api/runtimes/mcp/inventory",
        json!({}),
    )
    .await;
    assert_eq!(mcp_inventory_status, StatusCode::OK, "{mcp_inventory}");

    let (skill_plans_status, skill_plans) = json_request(
        reopened_app.clone(),
        "GET",
        &format!(
            "/api/skills/governance/plans?scope=agent&scopeId={}",
            agent.id
        ),
        json!({}),
    )
    .await;
    assert_eq!(skill_plans_status, StatusCode::OK, "{skill_plans}");
    assert_eq!(skill_plans.as_array().map(Vec::len), Some(1));
    let (skill_runs_status, skill_runs) = json_request(
        reopened_app.clone(),
        "GET",
        &format!(
            "/api/skills/governance/runs?scope=agent&scopeId={}",
            agent.id
        ),
        json!({}),
    )
    .await;
    assert_eq!(skill_runs_status, StatusCode::OK, "{skill_runs}");
    assert_eq!(skill_runs.as_array().map(Vec::len), Some(1));
    assert_eq!(skill_runs[0]["id"], skill_run_id);
    let (materializations_status, materializations) = json_request(
        reopened_app.clone(),
        "GET",
        &format!(
            "/api/skills/governance/materializations?scope=agent&scopeId={}",
            agent.id
        ),
        json!({}),
    )
    .await;
    assert_eq!(
        materializations_status,
        StatusCode::OK,
        "{materializations}"
    );
    assert!(materializations
        .as_array()
        .is_some_and(|items| !items.is_empty()));

    let (mcp_run_status, mcp_run) = json_request(
        reopened_app.clone(),
        "GET",
        &format!("/api/runtimes/mcp/apply-runs/{mcp_run_id}"),
        json!({}),
    )
    .await;
    assert_eq!(mcp_run_status, StatusCode::OK, "{mcp_run}");
    assert_eq!(mcp_run["run"]["id"], mcp_run_id);
    assert_ne!(mcp_run_id, skill_run_id);
    assert_ne!(mcp_plan_id, skill_plan_id);
    assert_ne!(skill_nonce, "mcp:{mcp_plan_id}:0");

    let skill_run_uuid = skill_run_id.parse().expect("skill run uuid");
    let skill_audit = reopened_store
        .list_skill_governance_apply_audit("run", skill_run_uuid)
        .await
        .expect("skill run audit");
    assert!(skill_audit
        .iter()
        .any(|audit| audit.to_status.as_deref() == Some("succeeded")));
    assert!(reopened_store
        .list_skill_governance_apply_audit("run", mcp_run_id.parse().expect("mcp run uuid"))
        .await
        .expect("no skill audit for mcp run id")
        .is_empty());

    let (rollback_status, rolled_back) = json_request(
        reopened_app.clone(),
        "POST",
        &format!("/api/runtimes/mcp/apply-runs/{mcp_run_id}/rollback"),
        json!({"actor": "integration-test"}),
    )
    .await;
    assert_eq!(rollback_status, StatusCode::OK, "{rolled_back}");
    assert_eq!(rolled_back["run"]["rollbackStatus"], "rolled_back");
    assert_eq!(runtime.mcp_rollback_calls.load(Ordering::SeqCst), 1);
    let (skill_after_mcp_rollback_status, skill_after_mcp_rollback) = json_request(
        reopened_app.clone(),
        "GET",
        &format!("/api/skills/governance/runs/{skill_run_id}"),
        json!({}),
    )
    .await;
    assert_eq!(
        skill_after_mcp_rollback_status,
        StatusCode::OK,
        "{skill_after_mcp_rollback}"
    );
    assert_eq!(skill_after_mcp_rollback["status"], "succeeded");

    let (skill_rollback_preview_status, skill_rollback_preview) = json_request(
        reopened_app.clone(),
        "POST",
        &format!("/api/skills/governance/runs/{skill_run_id}/rollback/preview"),
        json!({}),
    )
    .await;
    assert_eq!(
        skill_rollback_preview_status,
        StatusCode::OK,
        "{skill_rollback_preview}"
    );
    let (skill_rollback_status, skill_rolled_back) = json_request(
        reopened_app.clone(),
        "POST",
        &format!("/api/skills/governance/runs/{skill_run_id}/rollback"),
        json!({
            "idempotencyKey": skill_rollback_preview["idempotencyKey"],
            "confirmationNonce": skill_rollback_preview["confirmationNonce"],
            "confirmRollback": true
        }),
    )
    .await;
    assert_eq!(skill_rollback_status, StatusCode::OK, "{skill_rolled_back}");
    assert_eq!(skill_rolled_back["rolledBack"], true);
    let (mcp_after_skill_rollback_status, mcp_after_skill_rollback) = json_request(
        reopened_app.clone(),
        "GET",
        &format!("/api/runtimes/mcp/apply-runs/{mcp_run_id}"),
        json!({}),
    )
    .await;
    assert_eq!(
        mcp_after_skill_rollback_status,
        StatusCode::OK,
        "{mcp_after_skill_rollback}"
    );
    assert_eq!(
        mcp_after_skill_rollback["run"]["rollbackStatus"],
        "rolled_back"
    );

    let (import_status, import_preview) = json_request(
        reopened_app.clone(),
        "POST",
        "/api/runtimes/mcp/bundles/import-preview",
        json!({
            "actor": "integration-test",
            "bundle": bundle_preview["bundle"],
            "rebindings": {
                "targets": {"machine:1": installation_id},
                "runtimes": {"runtime:fake": "fake"},
                "secretRefs": {"keychain://cocli/integration-token": "env://INTEGRATION_TOKEN"},
                "machineLocalValues": {},
                "profiles": {}
            }
        }),
    )
    .await;
    assert_eq!(import_status, StatusCode::CREATED, "{import_preview}");
    assert_eq!(import_preview["canCommit"], true);
    let import_audit_id = import_preview["audit"]["id"]
        .as_str()
        .expect("import audit id");
    let (rebind_status, rebound_import) = json_request(
        reopened_app.clone(),
        "POST",
        &format!("/api/runtimes/mcp/bundles/imports/{import_audit_id}/rebind"),
        json!({
            "expectedVersion": import_preview["audit"]["version"],
            "rebindings": {
                "targets": {"machine:1": installation_id},
                "runtimes": {"runtime:fake": "fake"},
                "secretRefs": {"keychain://cocli/integration-token": "env://INTEGRATION_TOKEN"},
                "machineLocalValues": {},
                "profiles": {}
            }
        }),
    )
    .await;
    assert_eq!(rebind_status, StatusCode::OK, "{rebound_import}");
    assert_eq!(rebound_import["canCommit"], true);
    let (commit_status, committed_import) = json_request(
        reopened_app.clone(),
        "POST",
        &format!("/api/runtimes/mcp/bundles/imports/{import_audit_id}/commit"),
        json!({
            "expectedVersion": rebound_import["audit"]["version"],
            "actor": "integration-test"
        }),
    )
    .await;
    assert_eq!(commit_status, StatusCode::OK, "{committed_import}");
    assert_eq!(committed_import["audit"]["status"], "committed");
    assert_eq!(
        committed_import["audit"]["result"]["approvalImported"],
        false
    );
    assert_eq!(committed_import["audit"]["result"]["applyImported"], false);

    let (lockfile_restore_preview_status, lockfile_restore_preview) = json_request(
        reopened_app.clone(),
        "POST",
        "/api/skills/governance/workspace-lockfile/restore/preview",
        json!({
            "workspaceId": workspace.id,
            "expectedVersion": stored_lockfile.version,
            "expectedDiskHash": lockfile_read["diskHash"]
        }),
    )
    .await;
    assert_eq!(
        lockfile_restore_preview_status,
        StatusCode::OK,
        "{lockfile_restore_preview}"
    );
    let (lockfile_restore_status, lockfile_restored) = json_request(
        reopened_app.clone(),
        "POST",
        "/api/skills/governance/workspace-lockfile/restore",
        json!({
            "workspaceId": workspace.id,
            "expectedVersion": stored_lockfile.version,
            "expectedDiskHash": lockfile_read["diskHash"],
            "expectedPreviewHash": lockfile_restore_preview["previewHash"],
            "idempotencyKey": lockfile_restore_preview["idempotencyKey"],
            "confirmationNonce": lockfile_restore_preview["confirmationNonce"]
        }),
    )
    .await;
    assert_eq!(
        lockfile_restore_status,
        StatusCode::OK,
        "{lockfile_restored}"
    );
    assert_eq!(
        std::fs::read_to_string(workspace_root.join(".cocli/skills.lock.json"))
            .expect("restored workspace lockfile"),
        "{\n  \"generation\": 1\n}\n"
    );
    let (lockfile_after_status, lockfile_after) = json_request(
        reopened_app.clone(),
        "GET",
        &format!(
            "/api/skills/governance/workspace-lockfile?workspaceId={}",
            workspace.id
        ),
        json!({}),
    )
    .await;
    assert_eq!(lockfile_after_status, StatusCode::OK, "{lockfile_after}");
    let restored_lockfile = reopened_store
        .get_skill_governance_workspace_lockfile(
            &workspace.id.to_string(),
            ".cocli/skills.lock.json",
        )
        .await
        .expect("restored lockfile lookup")
        .expect("restored lockfile record");
    assert_eq!(restored_lockfile.document, json!({"generation": 1}));
    assert_eq!(
        restored_lockfile.expected_disk_hash,
        lockfile_after["diskHash"]
    );

    let all_state = json!({
        "skillInventory": skill_inventory,
        "mcpInventory": mcp_inventory,
        "skillRuns": skill_runs,
        "mcpRun": mcp_after_skill_rollback,
        "bundleImport": committed_import,
        "materializations": materializations,
        "workspaceLockfile": lockfile_after,
        "skillAudit": skill_audit,
    })
    .to_string();
    assert!(!all_state.contains(SECRET_CANARY));
    assert!(!all_state.contains("raw-secret"));
    assert!(all_state.contains("shared-governance"));
}

async fn json_request(
    app: axum::Router,
    method: &str,
    uri: &str,
    body: Value,
) -> (StatusCode, Value) {
    let response = app
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .expect("request should build"),
        )
        .await
        .expect("request should complete");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should load");
    let body = serde_json::from_slice(&bytes).expect("response should be JSON");
    (status, body)
}
