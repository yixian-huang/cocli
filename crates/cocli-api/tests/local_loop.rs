use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use chrono::{Duration as ChronoDuration, Utc};
use cocli_api::{
    reconcile_skill_state, router, router_with_delivery_config, DeliveryConfig, McpDiagnostic,
    McpDiagnosticSeverity, McpEvidence, McpInventory, RuntimeError, RuntimeInfo, RuntimeService,
    RuntimeSkill, RuntimeSkillCompatibility, RuntimeSkillFileContent, RuntimeSkillFileEntry,
};
use cocli_driver_core::{
    McpApplyActionResult, McpApplyActionStatus, McpApplyExecutionRequest, McpApplyExecutionResult,
    McpBackupDescriptor, McpReloadResult, McpReloadStatus, McpRollbackExecutionRequest,
    McpRollbackExecutionResult, McpVerificationResult, McpVerificationStatus,
};
use cocli_store::{
    Agent, AgentStatus, Message, MessageRole, NewAgentTurn, NewMcpApplyRun, NewSkillLibrary,
    SkillLibraryFile, Store,
};
use serde_json::{json, Value};
use tempfile::tempdir;
use tower::ServiceExt;

#[derive(Debug)]
struct FakeRuntime;

#[derive(Debug)]
struct FailingStartRuntime;

#[derive(Debug, Default)]
struct MutableMcpRuntime {
    inventory: Mutex<McpInventory>,
}

#[derive(Debug, Default)]
struct ApplyMcpRuntime {
    apply_calls: AtomicUsize,
    rollback_calls: AtomicUsize,
    applied: AtomicBool,
}

#[async_trait]
impl RuntimeService for FakeRuntime {
    async fn list(&self) -> Vec<RuntimeInfo> {
        vec![RuntimeInfo {
            name: "fake".to_owned(),
            installed: true,
            binary: None,
            version: Some("test".to_owned()),
            models: vec!["test-model".to_owned()],
            capabilities: vec!["reply".to_owned()],
            unavailable_reason: None,
        }]
    }

    async fn reply(&self, _agent: &Agent, message: &Message) -> Result<String, RuntimeError> {
        Ok(format!("echo: {}", message.content))
    }

    async fn inspect_mcp(&self) -> Result<McpInventory, RuntimeError> {
        Ok(McpInventory {
            diagnostics: vec![McpDiagnostic {
                code: "cli_missing".to_owned(),
                severity: McpDiagnosticSeverity::Warning,
                runtime: "cursor".to_owned(),
                server_id: None,
                message: "cursor MCP probe is unavailable".to_owned(),
                evidence: vec![McpEvidence {
                    source: "cursor_cli".to_owned(),
                    detail: "binary was not discovered".to_owned(),
                    source_path: None,
                    proves_runtime_loaded: false,
                    proves_current_session_visibility: false,
                }],
                observed_at: "2026-07-19T00:00:00Z".to_owned(),
            }],
            observed_at: "2026-07-19T00:00:00Z".to_owned(),
            ..McpInventory::default()
        })
    }
}

#[async_trait]
impl RuntimeService for FailingStartRuntime {
    async fn list(&self) -> Vec<RuntimeInfo> {
        FakeRuntime.list().await
    }

    async fn reply(&self, _agent: &Agent, _message: &Message) -> Result<String, RuntimeError> {
        Err(RuntimeError::Delivery(
            "runtime should not receive a reply".to_owned(),
        ))
    }

    async fn start(&self, _agent: &Agent) -> Result<cocli_api::RuntimeSessionStatus, RuntimeError> {
        Err(RuntimeError::Delivery(
            "simulated startup failure".to_owned(),
        ))
    }
}

#[async_trait]
impl RuntimeService for MutableMcpRuntime {
    async fn list(&self) -> Vec<RuntimeInfo> {
        FakeRuntime.list().await
    }

    async fn reply(&self, _agent: &Agent, message: &Message) -> Result<String, RuntimeError> {
        Ok(format!("echo: {}", message.content))
    }

    async fn inspect_mcp(&self) -> Result<McpInventory, RuntimeError> {
        Ok(self
            .inventory
            .lock()
            .expect("mutable MCP inventory should not be poisoned")
            .clone())
    }
}

#[async_trait]
impl RuntimeService for ApplyMcpRuntime {
    async fn list(&self) -> Vec<RuntimeInfo> {
        FakeRuntime.list().await
    }

    async fn reply(&self, _agent: &Agent, message: &Message) -> Result<String, RuntimeError> {
        Ok(format!("echo: {}", message.content))
    }

    async fn inspect_mcp(&self) -> Result<McpInventory, RuntimeError> {
        let mut inventory = McpInventory::default();
        if self.applied.load(Ordering::SeqCst) {
            inventory.diagnostics.push(McpDiagnostic {
                code: "post_apply_state".to_owned(),
                severity: McpDiagnosticSeverity::Info,
                runtime: "cursor".to_owned(),
                server_id: None,
                message: "post-apply observation".to_owned(),
                evidence: Vec::new(),
                observed_at: "2026-07-19T00:00:00Z".to_owned(),
            });
        }
        Ok(inventory)
    }

    async fn apply_mcp(
        &self,
        _request: McpApplyExecutionRequest,
    ) -> Result<McpApplyExecutionResult, RuntimeError> {
        self.apply_calls.fetch_add(1, Ordering::SeqCst);
        self.applied.store(true, Ordering::SeqCst);
        Ok(McpApplyExecutionResult {
            actions: vec![McpApplyActionResult {
                action_index: 0,
                runtime: "cursor".to_owned(),
                server_id: "docs".to_owned(),
                status: McpApplyActionStatus::Verified,
                reason: "verified safely".to_owned(),
                backup: Some(McpBackupDescriptor {
                    id: "backup-test".to_owned(),
                    runtime: "cursor".to_owned(),
                    source_path: "/tmp/config.json".to_owned(),
                    backup_path: "/tmp/backup.json".to_owned(),
                    source_hash: "before".to_owned(),
                    backup_hash: "before".to_owned(),
                    applied_hash: "after".to_owned(),
                    source_existed: true,
                }),
                before_source_hash: Some("before".to_owned()),
                after_source_hash: Some("after".to_owned()),
            }],
            reloads: vec![McpReloadResult {
                runtime: "cursor".to_owned(),
                status: McpReloadStatus::Deferred,
                reason: "active sessions were not restarted".to_owned(),
            }],
            verification: McpVerificationResult {
                status: McpVerificationStatus::Matched,
                observation_hash: "verified-observation".to_owned(),
                mismatches: Vec::new(),
            },
        })
    }

    async fn rollback_mcp(
        &self,
        request: McpRollbackExecutionRequest,
    ) -> Result<McpRollbackExecutionResult, RuntimeError> {
        self.rollback_calls.fetch_add(1, Ordering::SeqCst);
        Ok(McpRollbackExecutionResult {
            actions: request
                .backups
                .into_iter()
                .enumerate()
                .map(|(action_index, backup)| McpApplyActionResult {
                    action_index,
                    runtime: backup.runtime.clone(),
                    server_id: "docs".to_owned(),
                    status: McpApplyActionStatus::RolledBack,
                    reason: "backup restored atomically".to_owned(),
                    backup: Some(backup),
                    before_source_hash: None,
                    after_source_hash: None,
                })
                .collect(),
            verification: McpVerificationResult {
                status: McpVerificationStatus::Matched,
                observation_hash: "rollback-observation".to_owned(),
                mismatches: Vec::new(),
            },
        })
    }
}

#[derive(Debug, Default)]
struct FakeSkillRuntime {
    installs: Mutex<HashMap<(uuid::Uuid, String), Vec<SkillLibraryFile>>>,
    install_calls: AtomicUsize,
    install_delay: Duration,
    install_started: Mutex<Option<Arc<tokio::sync::Notify>>>,
}

#[async_trait]
impl RuntimeService for FakeSkillRuntime {
    async fn list(&self) -> Vec<RuntimeInfo> {
        vec![RuntimeInfo {
            name: "fake".to_owned(),
            installed: true,
            binary: None,
            version: Some("test".to_owned()),
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

    async fn list_skills(&self, agent: &Agent) -> Result<Vec<RuntimeSkill>, RuntimeError> {
        let installs = self
            .installs
            .lock()
            .expect("fake skill installs should not be poisoned");
        Ok(installs
            .keys()
            .filter(|(agent_id, _)| *agent_id == agent.id)
            .map(|(_, install_path)| {
                let name = install_path
                    .rsplit('/')
                    .next()
                    .expect("install path should have name")
                    .to_owned();
                RuntimeSkill {
                    name: name.clone(),
                    display_name: name,
                    description: "fake installed skill".to_owned(),
                    user_invocable: true,
                    skill_type: "workspace".to_owned(),
                    path: format!("{install_path}/SKILL.md"),
                    install_path: Some(install_path.clone()),
                }
            })
            .collect())
    }

    async fn install_skill(
        &self,
        agent: &Agent,
        skill_name: &str,
        files: &[SkillLibraryFile],
    ) -> Result<String, RuntimeError> {
        self.install_calls.fetch_add(1, Ordering::Relaxed);
        let install_started = self
            .install_started
            .lock()
            .expect("install notification should not be poisoned")
            .clone();
        if let Some(started) = install_started {
            started.notify_waiters();
        }
        if !self.install_delay.is_zero() {
            tokio::time::sleep(self.install_delay).await;
        }
        let install_path = format!(".fake/skills/{skill_name}");
        self.installs
            .lock()
            .expect("fake skill installs should not be poisoned")
            .insert((agent.id, install_path.clone()), files.to_vec());
        Ok(install_path)
    }

    async fn uninstall_skill(&self, agent: &Agent, install_path: &str) -> Result<(), RuntimeError> {
        self.installs
            .lock()
            .expect("fake skill installs should not be poisoned")
            .remove(&(agent.id, install_path.to_owned()));
        Ok(())
    }

    async fn list_skill_files(
        &self,
        agent: &Agent,
        install_path: &str,
    ) -> Result<Vec<RuntimeSkillFileEntry>, RuntimeError> {
        let installs = self
            .installs
            .lock()
            .expect("fake skill installs should not be poisoned");
        let files = installs
            .get(&(agent.id, install_path.to_owned()))
            .ok_or_else(|| RuntimeError::NotFound("fake skill install not found".to_owned()))?;
        Ok(files
            .iter()
            .map(|file| RuntimeSkillFileEntry {
                name: file.rel_path.clone(),
                is_dir: false,
                size: file.size,
            })
            .collect())
    }

    async fn read_skill_file(
        &self,
        agent: &Agent,
        install_path: &str,
        relative_path: &str,
    ) -> Result<RuntimeSkillFileContent, RuntimeError> {
        if relative_path == ".cocli-managed" {
            let name = install_path
                .rsplit('/')
                .next()
                .ok_or_else(|| RuntimeError::NotFound("fake skill name not found".to_owned()))?;
            return Ok(RuntimeSkillFileContent {
                content: name.to_owned(),
                binary: false,
            });
        }
        let installs = self
            .installs
            .lock()
            .expect("fake skill installs should not be poisoned");
        let file = installs
            .get(&(agent.id, install_path.to_owned()))
            .and_then(|files| files.iter().find(|file| file.rel_path == relative_path))
            .ok_or_else(|| RuntimeError::NotFound("fake skill file not found".to_owned()))?;
        match String::from_utf8(file.content.clone()) {
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
}

#[tokio::test]
async fn exposes_read_only_mcp_inventory_and_doctor() {
    let store = Store::in_memory().await.expect("store should open");
    let app = router(store, Arc::new(FakeRuntime));

    let (inventory_status, inventory) =
        json_request(app.clone(), "GET", "/api/runtimes/mcp/inventory", json!({})).await;
    assert_eq!(inventory_status, StatusCode::OK);
    assert_eq!(inventory["diagnostics"][0]["code"], "cli_missing");
    assert_eq!(inventory["diagnostics"][0]["runtime"], "cursor");
    assert_eq!(inventory["observedAt"], "2026-07-19T00:00:00Z");

    let (doctor_status, doctor) =
        json_request(app, "GET", "/api/runtimes/mcp/doctor", json!({})).await;
    assert_eq!(doctor_status, StatusCode::OK);
    assert_eq!(doctor["summary"]["status"], "warning");
    assert_eq!(doctor["summary"]["runtimeCount"], 1);
    assert_eq!(doctor["summary"]["warningCount"], 1);
    assert_eq!(doctor["inventory"]["diagnostics"][0]["code"], "cli_missing");
}

fn mcp_profile_body(name: &str, enabled: bool) -> Value {
    json!({
        "name": name,
        "description": "API governance profile",
        "servers": [{
            "serverId": "srv-docs",
            "runtime": "codex",
            "alias": "docs",
            "definition": {
                "transport": "http",
                "endpoint": "https://example.test/mcp"
            },
            "desiredEnabled": enabled,
            "allowTools": [],
            "denyTools": [],
            "approvalMode": "manual",
            "secretRefs": [{
                "location": "headers.authorization",
                "kind": "bearer",
                "reference": "keychain://cocli/docs-token"
            }]
        }]
    })
}

#[tokio::test]
async fn mcp_profile_plan_and_approval_api_is_dry_run_versioned_and_stale_aware() {
    let store = Store::in_memory().await.expect("store should open");
    let app = router(store, Arc::new(FakeRuntime));

    let (create_status, profile) = json_request(
        app.clone(),
        "POST",
        "/api/runtimes/mcp/profiles",
        mcp_profile_body("production docs", true),
    )
    .await;
    assert_eq!(create_status, StatusCode::CREATED);
    assert_eq!(profile["version"], 1);
    let profile_id = profile["id"].as_str().expect("profile id");
    let profile_path = format!("/api/runtimes/mcp/profiles/{profile_id}");

    let (get_status, fetched) = json_request(app.clone(), "GET", &profile_path, json!({})).await;
    assert_eq!(get_status, StatusCode::OK);
    assert_eq!(fetched["name"], "production docs");

    let (binding_status, binding) = json_request(
        app.clone(),
        "POST",
        "/api/runtimes/mcp/bindings",
        json!({ "profileId": profile_id, "targetType": "machine" }),
    )
    .await;
    assert_eq!(binding_status, StatusCode::CREATED);
    assert_eq!(binding["target"]["targetType"], "machine");

    let (effective_status, effective) =
        json_request(app.clone(), "GET", "/api/runtimes/mcp/effective", json!({})).await;
    assert_eq!(effective_status, StatusCode::OK);
    assert_eq!(effective["servers"][0]["serverId"], "srv-docs");
    assert_eq!(effective["servers"][0]["highRiskContext"], true);

    let (plan_status, plan_view) =
        json_request(app.clone(), "POST", "/api/runtimes/mcp/plans", json!({})).await;
    assert_eq!(plan_status, StatusCode::CREATED);
    assert_eq!(plan_view["plan"]["dryRun"], true);
    assert_eq!(plan_view["plan"]["applied"], false);
    assert_eq!(
        plan_view["plan"]["actions"][0]["kind"],
        "manual_unsupported"
    );
    assert_eq!(plan_view["plan"]["actions"][0]["blocked"], true);
    let plan_id = plan_view["plan"]["id"].as_str().expect("plan id");
    let plan_hash = plan_view["plan"]["planHash"].as_str().expect("plan hash");
    let observation_hash = plan_view["plan"]["observationHash"]
        .as_str()
        .expect("observation hash");
    let config_hash = plan_view["plan"]["configHash"]
        .as_str()
        .expect("config hash");

    let missing_expiry_status = status_request(
        app.clone(),
        "POST",
        &format!("/api/runtimes/mcp/plans/{plan_id}/approve"),
        json!({ "planHash": plan_hash, "actor": "api-test" }),
    )
    .await;
    assert_eq!(missing_expiry_status, StatusCode::UNPROCESSABLE_ENTITY);

    let (approve_status, approved) = json_request(
        app.clone(),
        "POST",
        &format!("/api/runtimes/mcp/plans/{plan_id}/approve"),
        json!({
            "planHash": plan_hash,
            "actor": "api-test",
            "expiresAt": "2099-07-19T10:00:00Z"
        }),
    )
    .await;
    assert_eq!(approve_status, StatusCode::OK);
    assert_eq!(approved["approvalStatus"], "approved");
    assert_eq!(approved["approvedButNotApplied"], true);
    assert_eq!(approved["plan"]["applied"], false);
    let high_risk_status = status_request(
        app.clone(),
        "POST",
        &format!("/api/runtimes/mcp/plans/{plan_id}/apply"),
        json!({
            "planHash": plan_hash,
            "observationHash": observation_hash,
            "configHash": config_hash,
            "actor": "api-test",
            "confirmHighRisk": false
        }),
    )
    .await;
    assert_eq!(high_risk_status, StatusCode::BAD_REQUEST);

    let mut update = mcp_profile_body("production docs", false);
    update["expectedVersion"] = json!(1);
    let (update_status, updated) =
        json_request(app.clone(), "PUT", &profile_path, update.clone()).await;
    assert_eq!(update_status, StatusCode::OK);
    assert_eq!(updated["version"], 2);

    let (conflict_status, _) = json_request(app.clone(), "PUT", &profile_path, update).await;
    assert_eq!(conflict_status, StatusCode::CONFLICT);

    let (stale_status, stale) = json_request(
        app.clone(),
        "GET",
        &format!("/api/runtimes/mcp/plans/{plan_id}"),
        json!({}),
    )
    .await;
    assert_eq!(stale_status, StatusCode::OK);
    assert_eq!(stale["approvalStatus"], "stale");
    assert!(stale["staleReasons"]
        .as_array()
        .expect("stale reasons")
        .iter()
        .any(|reason| reason == "desired_config_drift"));
    assert_eq!(stale["approvedButNotApplied"], false);

    let (_, replacement_plan) =
        json_request(app.clone(), "POST", "/api/runtimes/mcp/plans", json!({})).await;
    let replacement_id = replacement_plan["plan"]["id"].as_str().expect("plan id");
    let replacement_hash = replacement_plan["plan"]["planHash"]
        .as_str()
        .expect("plan hash");
    let (reject_status, rejected) = json_request(
        app.clone(),
        "POST",
        &format!("/api/runtimes/mcp/plans/{replacement_id}/reject"),
        json!({
            "planHash": replacement_hash,
            "actor": "api-test",
            "reason": "requires operator review"
        }),
    )
    .await;
    assert_eq!(reject_status, StatusCode::OK);
    assert_eq!(rejected["approvalStatus"], "rejected");
    assert_eq!(rejected["decision"]["reason"], "requires operator review");

    let binding_id = binding["id"].as_str().expect("binding id");
    let (unbind_status, _) = json_request(
        app.clone(),
        "DELETE",
        &format!("/api/runtimes/mcp/bindings/{binding_id}?expectedVersion=1"),
        json!({}),
    )
    .await;
    assert_eq!(unbind_status, StatusCode::OK);

    let (delete_status, _) = json_request(
        app,
        "DELETE",
        &format!("{profile_path}?expectedVersion=2"),
        json!({}),
    )
    .await;
    assert_eq!(delete_status, StatusCode::OK);
}

#[tokio::test]
async fn mcp_profile_api_rejects_plaintext_secret_without_echoing_it() {
    let store = Store::in_memory().await.expect("store should open");
    let app = router(store, Arc::new(FakeRuntime));
    let mut body = mcp_profile_body("unsafe", true);
    body["servers"][0]["definition"]["args"] = json!(["--api-key", "sk-do-not-echo-this"]);
    let (status, response) = json_request(app, "POST", "/api/runtimes/mcp/profiles", body).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let serialized = response.to_string();
    assert!(serialized.contains("suspected plaintext secret"));
    assert!(!serialized.contains("sk-do-not-echo-this"));
}

#[tokio::test]
async fn mcp_approval_becomes_stale_after_observation_drift() {
    let store = Store::in_memory().await.expect("store should open");
    let runtime = Arc::new(MutableMcpRuntime::default());
    let app = router(store.clone(), runtime.clone());
    let (_, plan_view) =
        json_request(app.clone(), "POST", "/api/runtimes/mcp/plans", json!({})).await;
    let plan_id = plan_view["plan"]["id"].as_str().expect("plan id");
    let plan_hash = plan_view["plan"]["planHash"].as_str().expect("plan hash");
    let observation_hash = plan_view["plan"]["observationHash"]
        .as_str()
        .expect("observation hash");
    let config_hash = plan_view["plan"]["configHash"]
        .as_str()
        .expect("config hash");
    let (approve_status, _) = json_request(
        app.clone(),
        "POST",
        &format!("/api/runtimes/mcp/plans/{plan_id}/approve"),
        json!({
            "planHash": plan_hash,
            "actor": "api-test",
            "expiresAt": "2099-07-19T10:00:00Z"
        }),
    )
    .await;
    assert_eq!(approve_status, StatusCode::OK);
    let decision = store
        .get_mcp_plan_decision(plan_id)
        .await
        .expect("read approval")
        .expect("approval exists");
    let interrupted = store
        .create_mcp_apply_run(NewMcpApplyRun {
            plan_id: plan_id.to_owned(),
            approval_id: decision.id,
            plan_hash: plan_hash.to_owned(),
            observation_hash: observation_hash.to_owned(),
            config_hash: config_hash.to_owned(),
            actor: "api-test".to_owned(),
            confirm_high_risk: false,
        })
        .await
        .expect("persist interrupted run");

    runtime
        .inventory
        .lock()
        .expect("mutable MCP inventory should not be poisoned")
        .diagnostics
        .push(McpDiagnostic {
            code: "runtime_drift".to_owned(),
            severity: McpDiagnosticSeverity::Warning,
            runtime: "codex".to_owned(),
            server_id: None,
            message: "Runtime observation changed".to_owned(),
            evidence: Vec::new(),
            observed_at: "2026-07-19T01:00:00Z".to_owned(),
        });

    let (status, stale) = json_request(
        app.clone(),
        "GET",
        &format!("/api/runtimes/mcp/plans/{plan_id}"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(stale["approvalStatus"], "stale");
    assert!(stale["staleReasons"]
        .as_array()
        .expect("stale reasons")
        .iter()
        .any(|reason| reason == "observation_drift"));
    let (apply_status, recovered) = json_request(
        app,
        "POST",
        &format!("/api/runtimes/mcp/plans/{plan_id}/apply"),
        json!({
            "planHash": plan_hash,
            "observationHash": observation_hash,
            "configHash": config_hash,
            "actor": "api-test",
            "confirmHighRisk": true
        }),
    )
    .await;
    assert_eq!(apply_status, StatusCode::OK);
    assert_eq!(recovered["run"]["id"], interrupted.id.to_string());
    assert_eq!(recovered["run"]["status"], "failed");
    assert_eq!(
        recovered["run"]["staleReasons"][0],
        "observation or desired configuration drifted during an interrupted apply"
    );
}

#[tokio::test]
async fn mcp_apply_api_revalidates_hashes_is_idempotent_and_rolls_back() {
    let store = Store::in_memory().await.expect("store should open");
    let runtime = Arc::new(ApplyMcpRuntime::default());
    let app = router(store.clone(), runtime.clone());
    let (_, plan_view) =
        json_request(app.clone(), "POST", "/api/runtimes/mcp/plans", json!({})).await;
    let plan = &plan_view["plan"];
    let plan_id = plan["id"].as_str().expect("plan id");
    let plan_hash = plan["planHash"].as_str().expect("plan hash");
    let observation_hash = plan["observationHash"].as_str().expect("observation hash");
    let config_hash = plan["configHash"].as_str().expect("config hash");
    let (approve_status, _) = json_request(
        app.clone(),
        "POST",
        &format!("/api/runtimes/mcp/plans/{plan_id}/approve"),
        json!({
            "planHash": plan_hash,
            "actor": "api-test",
            "expiresAt": "2099-07-19T10:00:00Z"
        }),
    )
    .await;
    assert_eq!(approve_status, StatusCode::OK);
    let decision = store
        .get_mcp_plan_decision(plan_id)
        .await
        .expect("read approval")
        .expect("approval exists");
    let interrupted = store
        .create_mcp_apply_run(NewMcpApplyRun {
            plan_id: plan_id.to_owned(),
            approval_id: decision.id,
            plan_hash: plan_hash.to_owned(),
            observation_hash: observation_hash.to_owned(),
            config_hash: config_hash.to_owned(),
            actor: "api-test".to_owned(),
            confirm_high_risk: false,
        })
        .await
        .expect("persist resumable run");

    let (stale_status, stale) = json_request(
        app.clone(),
        "POST",
        &format!("/api/runtimes/mcp/plans/{plan_id}/apply"),
        json!({
            "planHash": "wrong",
            "observationHash": observation_hash,
            "configHash": config_hash,
            "actor": "api-test",
            "confirmHighRisk": false
        }),
    )
    .await;
    assert_eq!(stale_status, StatusCode::CONFLICT);
    assert!(stale["error"].as_str().expect("error").contains("hashes"));

    let apply_body = json!({
        "planHash": plan_hash,
        "observationHash": observation_hash,
        "configHash": config_hash,
        "actor": "api-test",
        "confirmHighRisk": false
    });
    let (apply_status, applied) = json_request(
        app.clone(),
        "POST",
        &format!("/api/runtimes/mcp/plans/{plan_id}/apply"),
        apply_body.clone(),
    )
    .await;
    assert_eq!(apply_status, StatusCode::OK);
    assert_eq!(applied["run"]["status"], "verified");
    assert_eq!(applied["run"]["verification"]["status"], "matched");
    assert_eq!(applied["run"]["reloads"][0]["status"], "deferred");
    assert_eq!(applied["run"]["canRollback"], true);
    let run_id = applied["run"]["id"].as_str().expect("run id");
    assert_eq!(run_id, interrupted.id.to_string());

    let (_, repeated) = json_request(
        app.clone(),
        "POST",
        &format!("/api/runtimes/mcp/plans/{plan_id}/apply"),
        apply_body,
    )
    .await;
    assert_eq!(repeated["run"]["id"], run_id);
    assert_eq!(runtime.apply_calls.load(Ordering::SeqCst), 1);

    let (rollback_status, rolled_back) = json_request(
        app.clone(),
        "POST",
        &format!("/api/runtimes/mcp/apply-runs/{run_id}/rollback"),
        json!({ "actor": "api-test" }),
    )
    .await;
    assert_eq!(rollback_status, StatusCode::OK);
    assert_eq!(rolled_back["run"]["rollbackStatus"], "rolled_back");
    let (_, repeated_rollback) = json_request(
        app,
        "POST",
        &format!("/api/runtimes/mcp/apply-runs/{run_id}/rollback"),
        json!({ "actor": "api-test" }),
    )
    .await;
    assert_eq!(repeated_rollback["run"]["rollbackStatus"], "rolled_back");
    assert_eq!(runtime.rollback_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn mcp_apply_api_never_executes_an_expired_approval() {
    let store = Store::in_memory().await.expect("store should open");
    let runtime = Arc::new(ApplyMcpRuntime::default());
    let app = router(store, runtime.clone());
    let (_, plan_view) =
        json_request(app.clone(), "POST", "/api/runtimes/mcp/plans", json!({})).await;
    let plan = &plan_view["plan"];
    let plan_id = plan["id"].as_str().expect("plan id");
    let expires_at = (Utc::now() + ChronoDuration::milliseconds(500)).to_rfc3339();
    let approve_status = status_request(
        app.clone(),
        "POST",
        &format!("/api/runtimes/mcp/plans/{plan_id}/approve"),
        json!({
            "planHash": plan["planHash"],
            "actor": "api-test",
            "expiresAt": expires_at
        }),
    )
    .await;
    assert_eq!(approve_status, StatusCode::OK);
    tokio::time::sleep(Duration::from_millis(600)).await;

    let apply_status = status_request(
        app,
        "POST",
        &format!("/api/runtimes/mcp/plans/{plan_id}/apply"),
        json!({
            "planHash": plan["planHash"],
            "observationHash": plan["observationHash"],
            "configHash": plan["configHash"],
            "actor": "api-test",
            "confirmHighRisk": false
        }),
    )
    .await;
    assert_eq!(apply_status, StatusCode::CONFLICT);
    assert_eq!(runtime.apply_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn startup_skill_reconciliation_repairs_missing_files_and_removes_orphans() {
    let store = Store::in_memory().await.expect("store should open");
    let channel = store
        .create_channel("skill-reconcile")
        .await
        .expect("channel");
    let agent = store
        .create_agent(
            channel.id,
            "skill-agent",
            "fake",
            None,
            AgentStatus::Stopped,
        )
        .await
        .expect("agent");
    let library = store
        .create_skill_library(NewSkillLibrary {
            name: "managed-demo".to_owned(),
            display_name: "Managed Demo".to_owned(),
            description: "reconcile".to_owned(),
            user_invocable: true,
            source_kind: "local".to_owned(),
            source_url: "/tmp/managed-demo".to_owned(),
            source_subpath: None,
            source_ref: None,
            files: vec![SkillLibraryFile {
                rel_path: "SKILL.md".to_owned(),
                mode: 0o644,
                content: b"# managed".to_vec(),
                size: 9,
            }],
        })
        .await
        .expect("library");
    let install = store
        .create_agent_skill_install(agent.id, library.id, ".fake/skills/managed-demo")
        .await
        .expect("install record");
    let runtime = Arc::new(FakeSkillRuntime::default());
    let runtime_service: Arc<dyn RuntimeService> = runtime.clone();

    reconcile_skill_state(&store, &runtime_service)
        .await
        .expect("missing runtime files should be restored");
    assert_eq!(
        runtime
            .list_skills(&agent)
            .await
            .expect("skills after restore")
            .len(),
        1
    );

    store
        .delete_agent_skill_install(agent.id, install.id)
        .await
        .expect("catalog install should be removed");
    reconcile_skill_state(&store, &runtime_service)
        .await
        .expect("orphan runtime files should be removed");
    assert!(runtime
        .list_skills(&agent)
        .await
        .expect("skills after orphan cleanup")
        .is_empty());
}

#[derive(Debug, Default)]
struct FlakyRuntime {
    calls: AtomicUsize,
}

#[derive(Debug, Default)]
struct PanicOnceRuntime {
    calls: AtomicUsize,
}

#[derive(Debug, Default)]
struct TimeoutOnceRuntime {
    calls: AtomicUsize,
    stops: AtomicUsize,
}

#[async_trait]
impl RuntimeService for PanicOnceRuntime {
    async fn list(&self) -> Vec<RuntimeInfo> {
        FakeRuntime.list().await
    }

    async fn reply(&self, _agent: &Agent, message: &Message) -> Result<String, RuntimeError> {
        if self.calls.fetch_add(1, Ordering::Relaxed) == 0 {
            panic!("simulated runtime task panic");
        }
        Ok(format!("recovered after panic: {}", message.content))
    }
}

#[async_trait]
impl RuntimeService for FlakyRuntime {
    async fn list(&self) -> Vec<RuntimeInfo> {
        FakeRuntime.list().await
    }

    async fn reply(&self, _agent: &Agent, message: &Message) -> Result<String, RuntimeError> {
        if self.calls.fetch_add(1, Ordering::Relaxed) == 0 {
            Err(RuntimeError::Delivery("temporary failure".to_owned()))
        } else {
            Ok(format!("recovered: {}", message.content))
        }
    }
}

#[async_trait]
impl RuntimeService for TimeoutOnceRuntime {
    async fn list(&self) -> Vec<RuntimeInfo> {
        FakeRuntime.list().await
    }

    async fn reply(&self, _agent: &Agent, message: &Message) -> Result<String, RuntimeError> {
        if self.calls.fetch_add(1, Ordering::Relaxed) == 0 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            Ok(format!("late: {}", message.content))
        } else {
            Ok(format!("retried safely: {}", message.content))
        }
    }

    async fn stop(&self, _agent_id: uuid::Uuid) -> Result<(), RuntimeError> {
        self.stops.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
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

async fn status_request(app: axum::Router, method: &str, uri: &str, body: Value) -> StatusCode {
    app.oneshot(
        Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .expect("request should build"),
    )
    .await
    .expect("request should complete")
    .status()
}

async fn bridge_json_request(
    app: axum::Router,
    method: &str,
    uri: &str,
    body: Value,
    token: &str,
) -> (StatusCode, Value) {
    let response = app
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {token}"))
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

async fn bridge_status_request(
    app: axum::Router,
    method: &str,
    uri: &str,
    body: Value,
    token: &str,
) -> StatusCode {
    app.oneshot(
        Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::from(body.to_string()))
            .expect("request should build"),
    )
    .await
    .expect("request should complete")
    .status()
}

#[tokio::test]
async fn agent_channel_ontology_routes_support_standalone_membership_and_workspaces() {
    let store = Store::in_memory().await.expect("store should open");
    let app = router(store, Arc::new(FakeRuntime));

    let (agent_status, agent) = json_request(
        app.clone(),
        "POST",
        "/api/agents",
        json!({
            "name": "solo",
            "description": "Persistent generalist",
            "instructions": "Prefer explicit evidence.",
            "runtime": "fake",
            "model": "test-model"
        }),
    )
    .await;
    assert_eq!(agent_status, StatusCode::CREATED);
    assert_eq!(agent["description"], "Persistent generalist");
    assert_eq!(agent["instructions"], "Prefer explicit evidence.");
    assert!(agent.get("channel_id").is_none());
    let agent_id = agent["id"].as_str().expect("agent id");
    assert_eq!(
        status_request(
            app.clone(),
            "GET",
            &format!("/api/agents/{agent_id}/direct-channel"),
            json!({}),
        )
        .await,
        StatusCode::NOT_FOUND,
    );

    let (message_status, direct_reply) = json_request(
        app.clone(),
        "POST",
        &format!("/api/agents/{agent_id}/messages"),
        json!({"content": "work directly with me"}),
    )
    .await;
    assert_eq!(message_status, StatusCode::CREATED);
    assert_eq!(
        direct_reply["replies"][0]["content"],
        "echo: work directly with me"
    );
    assert!(direct_reply["message"].get("channel_id").is_none());
    assert!(direct_reply["replies"][0].get("channel_id").is_none());

    let (history_status, direct_history) = json_request(
        app.clone(),
        "GET",
        &format!("/api/agents/{agent_id}/messages"),
        json!({}),
    )
    .await;
    assert_eq!(history_status, StatusCode::OK);
    assert_eq!(direct_history.as_array().map(Vec::len), Some(2));
    assert!(direct_history
        .as_array()
        .expect("direct messages")
        .iter()
        .all(|message| message.get("channel_id").is_none()));

    let (_, channel) = json_request(
        app.clone(),
        "POST",
        "/api/channels",
        json!({"name": "planning"}),
    )
    .await;
    let channel_id = channel["id"].as_str().expect("channel id");

    let (join_status, membership) = json_request(
        app.clone(),
        "POST",
        &format!("/api/channels/{channel_id}/agents"),
        json!({
            "agent_id": agent_id,
            "role": "planner",
            "delivery_policy": "subscribed"
        }),
    )
    .await;
    assert_eq!(join_status, StatusCode::CREATED);
    assert_eq!(membership["agent_id"], agent_id);

    let (channels_status, channels) = json_request(
        app.clone(),
        "GET",
        &format!("/api/agents/{agent_id}/channels"),
        json!({}),
    )
    .await;
    assert_eq!(channels_status, StatusCode::OK);
    assert_eq!(channels.as_array().expect("channels array").len(), 1);
    assert!(channels
        .as_array()
        .expect("channels array")
        .iter()
        .all(|channel| channel["is_system"] == false));

    let (workspace_status, workspace) = json_request(
        app.clone(),
        "POST",
        &format!("/api/channels/{channel_id}/workspaces"),
        json!({
            "kind": "directory",
            "locator": "/tmp/planning",
            "metadata": {"label": "Planning files"}
        }),
    )
    .await;
    assert_eq!(workspace_status, StatusCode::CREATED);
    assert_eq!(workspace["metadata"]["label"], "Planning files");

    let delete_status = status_request(
        app.clone(),
        "DELETE",
        &format!("/api/channels/{channel_id}"),
        json!({}),
    )
    .await;
    assert_eq!(delete_status, StatusCode::NO_CONTENT);

    let (agents_status, agents) = json_request(app, "GET", "/api/agents", json!({})).await;
    assert_eq!(agents_status, StatusCode::OK);
    assert_eq!(agents.as_array().expect("agents array").len(), 1);
}

#[tokio::test]
async fn bridge_agents_can_create_and_organize_persistent_subjects() {
    let store = Store::in_memory().await.expect("store should open");
    let app = router(store.clone(), Arc::new(FakeRuntime));
    let (_, founder) = json_request(
        app.clone(),
        "POST",
        "/api/agents",
        json!({"name": "founder", "runtime": "fake"}),
    )
    .await;
    let founder_id = founder["id"].as_str().expect("founder id");
    let token = store
        .agent_bridge_token(founder_id.parse().expect("founder uuid"))
        .await
        .expect("bridge token query")
        .expect("bridge token");

    let (channel_status, channel) = bridge_json_request(
        app.clone(),
        "POST",
        &format!("/api/bridge/agents/{founder_id}/channels"),
        json!({
            "name": "research",
            "description": "Long-running investigation",
            "goal": "Produce an evidence-backed answer",
            "idempotency_key": "create-research",
            "source_session_id": "session-founder"
        }),
        &token,
    )
    .await;
    assert_eq!(channel_status, StatusCode::CREATED);
    assert_eq!(channel["description"], "Long-running investigation");
    assert_eq!(channel["goal"], "Produce an evidence-backed answer");
    let channel_id = channel["id"].as_str().expect("channel id");
    let (replay_status, replayed_channel) = bridge_json_request(
        app.clone(),
        "POST",
        &format!("/api/bridge/agents/{founder_id}/channels"),
        json!({
            "name": "research",
            "description": "Long-running investigation",
            "goal": "Produce an evidence-backed answer",
            "idempotency_key": "create-research",
            "source_session_id": "session-founder"
        }),
        &token,
    )
    .await;
    assert_eq!(replay_status, StatusCode::OK);
    assert_eq!(replayed_channel["id"], channel_id);

    let (agent_status, reviewer) = bridge_json_request(
        app.clone(),
        "POST",
        &format!("/api/bridge/agents/{founder_id}/agents"),
        json!({
            "name": "reviewer",
            "instructions": "Challenge unsupported conclusions.",
            "channel": channel_id,
            "role": "reviewer",
            "idempotency_key": "create-reviewer",
            "source_channel_id": channel_id
        }),
        &token,
    )
    .await;
    assert_eq!(agent_status, StatusCode::CREATED);
    assert_eq!(
        reviewer["runtime"], "fake",
        "creator Runtime is the default"
    );
    assert_eq!(
        reviewer["instructions"],
        "Challenge unsupported conclusions."
    );
    let reviewer_id = reviewer["id"].as_str().expect("reviewer id");

    let (_, observer) = json_request(
        app.clone(),
        "POST",
        "/api/agents",
        json!({"name": "observer", "runtime": "fake"}),
    )
    .await;
    let observer_id = observer["id"].as_str().expect("observer id");
    let (join_status, membership) = bridge_json_request(
        app.clone(),
        "POST",
        &format!("/api/bridge/agents/{founder_id}/channels/join-agent"),
        json!({"channel": channel_id, "agent_id": observer_id, "role": "observer"}),
        &token,
    )
    .await;
    assert_eq!(join_status, StatusCode::CREATED);
    assert_eq!(membership["agent_id"], observer_id);

    let (members_status, members) = json_request(
        app.clone(),
        "GET",
        &format!("/api/channels/{channel_id}/agents"),
        json!({}),
    )
    .await;
    assert_eq!(members_status, StatusCode::OK);
    let member_ids = members
        .as_array()
        .expect("channel members")
        .iter()
        .filter_map(|agent| agent["id"].as_str())
        .collect::<Vec<_>>();
    assert!(member_ids.contains(&founder_id));
    assert!(member_ids.contains(&reviewer_id));
    assert!(member_ids.contains(&observer_id));

    let (operations_status, operations) = json_request(
        app.clone(),
        "GET",
        &format!("/api/agents/{founder_id}/operations"),
        json!({}),
    )
    .await;
    assert_eq!(operations_status, StatusCode::OK);
    let actions = operations
        .as_array()
        .expect("operation audit")
        .iter()
        .filter_map(|operation| operation["action"].as_str())
        .collect::<Vec<_>>();
    assert!(actions.contains(&"channel.create"));
    assert!(actions.contains(&"agent.create"));
    assert!(actions.contains(&"channel.join_agent"));

    let (runtime_status, runtime_error) = bridge_json_request(
        app.clone(),
        "POST",
        &format!("/api/bridge/agents/{founder_id}/agents"),
        json!({"name": "invalid-runtime", "runtime": "missing"}),
        &token,
    )
    .await;
    assert_eq!(runtime_status, StatusCode::BAD_REQUEST);
    assert!(runtime_error["error"]
        .as_str()
        .is_some_and(|error| error.contains("unknown runtime")));

    let (forbidden_status, _) = bridge_json_request(
        app,
        "GET",
        &format!("/api/bridge/agents/{reviewer_id}/history?channel=missing"),
        json!({}),
        &store
            .agent_bridge_token(reviewer_id.parse().expect("reviewer uuid"))
            .await
            .expect("bridge token query")
            .expect("reviewer token"),
    )
    .await;
    assert_eq!(forbidden_status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn skills_routes_complete_the_local_import_install_and_refresh_loop() {
    let source = tempdir().expect("skill source should create");
    std::fs::create_dir_all(source.path().join("scripts")).expect("scripts should create");
    std::fs::write(
        source.path().join("SKILL.md"),
        "---\nname: Demo Skill\ndisplay-name: Demo Skill\ndescription: local test skill\nuser-invocable: true\n---\n# Demo\n",
    )
    .expect("skill manifest should write");
    std::fs::write(source.path().join("scripts/run.sh"), "echo first\n")
        .expect("skill script should write");

    let store = Store::in_memory().await.expect("store should open");
    let app = router(store, Arc::new(FakeSkillRuntime::default()));
    let (_, channel) = json_request(
        app.clone(),
        "POST",
        "/api/channels",
        json!({"name": "skills"}),
    )
    .await;
    let channel_id = channel["id"].as_str().expect("channel id");
    let (_, agent) = json_request(
        app.clone(),
        "POST",
        "/api/agents",
        json!({
            "channel_id": channel_id,
            "name": "skilled",
            "runtime": "fake",
            "model": "test-model"
        }),
    )
    .await;
    let agent_id = agent["id"].as_str().expect("agent id");

    let (compatibility_status, compatibility) =
        json_request(app.clone(), "GET", "/api/runtimes/compatibility", json!({})).await;
    assert_eq!(compatibility_status, StatusCode::OK);
    assert_eq!(compatibility["fake"], "supported");

    let (import_status, imported) = json_request(
        app.clone(),
        "POST",
        "/api/zones/local/skills/library",
        json!({
            "url": source.path().to_str().expect("source path"),
            "name": "demo-local"
        }),
    )
    .await;
    assert_eq!(import_status, StatusCode::OK);
    assert_eq!(imported["files"], 2);
    let library_id = imported["library_id"].as_str().expect("library id");

    let (conflict_status, conflict) = json_request(
        app.clone(),
        "POST",
        "/api/zones/local/skills/library",
        json!({
            "url": source.path().to_str().expect("source path"),
            "name": "demo-local"
        }),
    )
    .await;
    assert_eq!(conflict_status, StatusCode::CONFLICT);
    assert_eq!(conflict["existing_id"], library_id);
    assert_eq!(
        conflict["existing_source"],
        source.path().to_str().expect("source path")
    );

    let (list_status, library) = json_request(
        app.clone(),
        "GET",
        "/api/zones/local/skills/library",
        json!({}),
    )
    .await;
    assert_eq!(list_status, StatusCode::OK);
    assert_eq!(library["entries"][0]["name"], "demo-local");
    assert_eq!(library["entries"][0]["displayName"], "Demo Skill");
    assert_eq!(library["entries"][0]["sourceKind"], "local");
    assert_eq!(library["entries"][0]["zoneId"], "local");

    let (install_status, installed) = json_request(
        app.clone(),
        "POST",
        &format!("/api/agents/{agent_id}/skills"),
        json!({"libraryId": library_id}),
    )
    .await;
    assert_eq!(install_status, StatusCode::OK);
    assert_eq!(installed["installPath"], ".fake/skills/demo-local");
    let install_id = installed["installId"].as_str().expect("install id");

    let (duplicate_status, duplicate_error) = json_request(
        app.clone(),
        "POST",
        &format!("/api/agents/{agent_id}/skills"),
        json!({"libraryId": library_id}),
    )
    .await;
    assert_eq!(duplicate_status, StatusCode::CONFLICT);
    assert!(duplicate_error["error"]
        .as_str()
        .is_some_and(|error| error.contains("already installed")));

    let (skills_status, skills) = json_request(
        app.clone(),
        "GET",
        &format!("/api/agents/{agent_id}/skills"),
        json!({}),
    )
    .await;
    assert_eq!(skills_status, StatusCode::OK);
    assert_eq!(skills["skills"][0]["state"], "managed");
    assert_eq!(skills["skills"][0]["libraryId"], library_id);
    assert_eq!(skills["skills"][0]["presence"], "installed");
    assert_eq!(skills["skills"][0]["runtime"], "fake");
    assert_eq!(skills["skills"][0]["evidence"]["source"], "filesystem");
    assert_eq!(
        skills["skills"][0]["evidence"]["provesSessionVisibility"],
        false
    );

    let (inventory_status, inventory) = json_request(
        app.clone(),
        "GET",
        &format!("/api/agents/{agent_id}/skills/inventory"),
        json!({}),
    )
    .await;
    assert_eq!(inventory_status, StatusCode::OK);
    assert_eq!(inventory["agentId"], agent_id);
    assert_eq!(inventory["runtime"], "fake");
    assert_eq!(inventory["compatibility"], "supported");
    assert_eq!(inventory["skills"][0]["state"], "managed");

    let (machine_inventory_status, machine_inventory) = json_request(
        app.clone(),
        "GET",
        "/api/runtimes/skills/inventory",
        json!({}),
    )
    .await;
    assert_eq!(machine_inventory_status, StatusCode::OK);
    assert_eq!(machine_inventory["agents"][0]["agentId"], agent_id);

    let (agent_doctor_status, agent_doctor) = json_request(
        app.clone(),
        "GET",
        &format!("/api/agents/{agent_id}/skills/doctor"),
        json!({}),
    )
    .await;
    assert_eq!(agent_doctor_status, StatusCode::OK);
    assert_eq!(agent_doctor["summary"]["status"], "ok");
    assert_eq!(
        agent_doctor["inventory"]["skills"][0]["presence"],
        "installed"
    );

    let (doctor_status, doctor) =
        json_request(app.clone(), "GET", "/api/runtimes/skills/doctor", json!({})).await;
    assert_eq!(doctor_status, StatusCode::OK);
    assert_eq!(doctor["summary"]["status"], "ok");
    assert_eq!(doctor["summary"]["agentCount"], 1);
    assert_eq!(doctor["summary"]["skillCount"], 1);
    assert!(doctor["runtimes"]
        .as_array()
        .is_some_and(|runtimes| runtimes.iter().any(|runtime| {
            runtime["runtime"] == "fake"
                && runtime["agentCount"] == 1
                && runtime["evidenceSources"][0] == "filesystem"
        })));

    let (files_status, files) = json_request(
        app.clone(),
        "GET",
        &format!("/api/agents/{agent_id}/skills/{install_id}/files"),
        json!({}),
    )
    .await;
    assert_eq!(files_status, StatusCode::OK);
    assert!(files["files"]
        .as_array()
        .is_some_and(|files| files.iter().any(|file| file["name"] == "scripts/run.sh")));

    let (read_status, first_script) = json_request(
        app.clone(),
        "GET",
        &format!("/api/agents/{agent_id}/skills/{install_id}/files/scripts%2Frun.sh"),
        json!({}),
    )
    .await;
    assert_eq!(read_status, StatusCode::OK);
    assert_eq!(first_script["content"], "echo first\n");

    std::fs::write(source.path().join("scripts/run.sh"), "echo refreshed\n")
        .expect("refreshed script should write");
    let (refresh_status, refresh) = json_request(
        app.clone(),
        "POST",
        &format!("/api/zones/local/skills/library/{library_id}/reinstall"),
        json!({}),
    )
    .await;
    assert_eq!(refresh_status, StatusCode::OK);
    assert_eq!(refresh["updated"], true);

    let (_, refreshed_script) = json_request(
        app.clone(),
        "GET",
        &format!("/api/agents/{agent_id}/skills/{install_id}/files/scripts%2Frun.sh"),
        json!({}),
    )
    .await;
    assert_eq!(refreshed_script["content"], "echo refreshed\n");

    let (uninstall_status, uninstalled) = json_request(
        app.clone(),
        "DELETE",
        &format!("/api/agents/{agent_id}/skills/{install_id}"),
        json!({}),
    )
    .await;
    assert_eq!(uninstall_status, StatusCode::OK);
    assert_eq!(uninstalled["ok"], true);

    let (delete_status, deleted) = json_request(
        app,
        "DELETE",
        &format!("/api/zones/local/skills/library/{library_id}"),
        json!({}),
    )
    .await;
    assert_eq!(delete_status, StatusCode::OK);
    assert_eq!(deleted["deleted"], library_id);
}

#[tokio::test]
async fn concurrent_duplicate_skill_install_mutates_runtime_once() {
    let store = Store::in_memory().await.expect("store should open");
    let channel = store
        .create_channel("concurrent-skills")
        .await
        .expect("channel should create");
    let agent = store
        .create_agent(
            channel.id,
            "skilled",
            "fake",
            Some("test-model"),
            AgentStatus::Stopped,
        )
        .await
        .expect("agent should create");
    let library = store
        .create_skill_library(NewSkillLibrary {
            name: "serialized-install".to_owned(),
            display_name: "Serialized Install".to_owned(),
            description: "concurrent install test".to_owned(),
            user_invocable: true,
            source_kind: "local".to_owned(),
            source_url: "/tmp/serialized-install".to_owned(),
            source_subpath: None,
            source_ref: None,
            files: vec![SkillLibraryFile {
                rel_path: "SKILL.md".to_owned(),
                mode: 0o644,
                content: b"# Serialized Install\n".to_vec(),
                size: 21,
            }],
        })
        .await
        .expect("library should create");
    let runtime = Arc::new(FakeSkillRuntime {
        installs: Mutex::new(HashMap::new()),
        install_calls: AtomicUsize::new(0),
        install_delay: Duration::from_millis(50),
        install_started: Mutex::new(None),
    });
    let app = router(store.clone(), runtime.clone());
    let uri = format!("/api/agents/{}/skills", agent.id);
    let body = json!({"libraryId": library.id});

    let first = json_request(app.clone(), "POST", &uri, body.clone());
    let second = json_request(app, "POST", &uri, body);
    let ((first_status, _), (second_status, _)) = tokio::join!(first, second);

    let statuses = [first_status, second_status];
    assert_eq!(
        statuses
            .iter()
            .filter(|status| **status == StatusCode::OK)
            .count(),
        1
    );
    assert_eq!(
        statuses
            .iter()
            .filter(|status| **status == StatusCode::CONFLICT)
            .count(),
        1
    );
    assert_eq!(runtime.install_calls.load(Ordering::Relaxed), 1);
    assert_eq!(
        store
            .list_agent_skill_installs(agent.id)
            .await
            .expect("installs should list")
            .len(),
        1
    );
    assert_eq!(
        runtime
            .list_skills(&agent)
            .await
            .expect("runtime skills should list")
            .len(),
        1
    );
}

#[tokio::test]
async fn library_reinstall_and_uninstall_are_serialized() {
    let source = tempdir().expect("skill source should create");
    std::fs::write(
        source.path().join("SKILL.md"),
        "---\nname: serialized-library\n---\n# Initial\n",
    )
    .expect("skill source should write");

    let store = Store::in_memory().await.expect("store should open");
    let channel = store
        .create_channel("serialized-library")
        .await
        .expect("channel should create");
    let agent = store
        .create_agent(
            channel.id,
            "skilled",
            "fake",
            Some("test-model"),
            AgentStatus::Stopped,
        )
        .await
        .expect("agent should create");
    let library = store
        .create_skill_library(NewSkillLibrary {
            name: "serialized-library".to_owned(),
            display_name: "Serialized Library".to_owned(),
            description: "library mutation lock test".to_owned(),
            user_invocable: true,
            source_kind: "local".to_owned(),
            source_url: source
                .path()
                .to_str()
                .expect("source path should be UTF-8")
                .to_owned(),
            source_subpath: None,
            source_ref: None,
            files: vec![SkillLibraryFile {
                rel_path: "SKILL.md".to_owned(),
                mode: 0o644,
                content: b"# Initial\n".to_vec(),
                size: 10,
            }],
        })
        .await
        .expect("library should create");
    let install_started = Arc::new(tokio::sync::Notify::new());
    let runtime = Arc::new(FakeSkillRuntime {
        installs: Mutex::new(HashMap::new()),
        install_calls: AtomicUsize::new(0),
        install_delay: Duration::from_millis(50),
        install_started: Mutex::new(None),
    });
    let app = router(store.clone(), runtime.clone());
    let (install_status, installed) = json_request(
        app.clone(),
        "POST",
        &format!("/api/agents/{}/skills", agent.id),
        json!({"libraryId": library.id}),
    )
    .await;
    assert_eq!(install_status, StatusCode::OK);
    let install_id = installed["installId"]
        .as_str()
        .expect("install id")
        .to_owned();
    *runtime
        .install_started
        .lock()
        .expect("install notification should not be poisoned") = Some(Arc::clone(&install_started));

    std::fs::write(
        source.path().join("SKILL.md"),
        "---\nname: serialized-library\n---\n# Refreshed\n",
    )
    .expect("refreshed source should write");
    let reinstall_started = install_started.notified();
    tokio::pin!(reinstall_started);
    let reinstall_app = app.clone();
    let reinstall_uri = format!("/api/zones/local/skills/library/{}/reinstall", library.id);
    let reinstall = tokio::spawn(async move {
        json_request(reinstall_app, "POST", &reinstall_uri, json!({})).await
    });
    reinstall_started.await;

    let (uninstall_status, _) = json_request(
        app,
        "DELETE",
        &format!("/api/agents/{}/skills/{install_id}", agent.id),
        json!({}),
    )
    .await;
    let (reinstall_status, _) = reinstall.await.expect("reinstall task should complete");

    assert_eq!(reinstall_status, StatusCode::OK);
    assert_eq!(uninstall_status, StatusCode::OK);
    assert!(store
        .list_agent_skill_installs(agent.id)
        .await
        .expect("installs should list")
        .is_empty());
    assert!(runtime
        .list_skills(&agent)
        .await
        .expect("runtime skills should list")
        .is_empty());
}

#[tokio::test]
async fn wiki_is_not_exposed_as_a_core_api() {
    let store = Store::in_memory().await.expect("store should open");
    let app = router(store.clone(), Arc::new(FakeRuntime));
    assert_eq!(
        status_request(app.clone(), "GET", "/api/wiki/pages", json!({})).await,
        StatusCode::NOT_FOUND
    );

    let channel = store
        .create_channel("wiki-negative")
        .await
        .expect("channel");
    let agent = store
        .create_agent(
            channel.id,
            "wiki-negative-agent",
            "fake",
            None,
            AgentStatus::Running,
        )
        .await
        .expect("agent");
    let token = store
        .ensure_agent_bridge_token(agent.id)
        .await
        .expect("bridge token");
    let status = bridge_status_request(
        app,
        "GET",
        &format!("/api/bridge/agents/{}/wiki/pages", agent.id),
        json!({}),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn memory_routes_support_private_shared_write_read_and_move() {
    let store = Store::in_memory().await.expect("store should open");
    let app = router(store.clone(), Arc::new(FakeRuntime));
    let (_, channel) = json_request(
        app.clone(),
        "POST",
        "/api/channels",
        json!({"name": "memory-api"}),
    )
    .await;
    let channel_id = channel["id"].as_str().expect("channel id");
    let (_, agent) = json_request(
        app.clone(),
        "POST",
        "/api/agents",
        json!({
            "channel_id": channel_id,
            "name": "rememberer",
            "runtime": "fake",
            "model": "test-model"
        }),
    )
    .await;
    let agent_id = agent["id"].as_str().expect("agent id");
    let bridge_token = store
        .agent_bridge_token(agent_id.parse().expect("agent uuid"))
        .await
        .expect("bridge token query")
        .expect("bridge token");

    let (write_status, written) = bridge_json_request(
        app.clone(),
        "POST",
        &format!("/api/bridge/agents/{agent_id}/memory/topic"),
        json!({
            "scope": "agent",
            "type": "project",
            "topic": "apollo",
            "description": "Apollo plan",
            "body": "# Apollo\n\nShip locally."
        }),
        &bridge_token,
    )
    .await;
    assert_eq!(write_status, StatusCode::OK);
    assert_eq!(written["version"], 1);
    assert_eq!(written["type"], "project");

    let (public_index_status, public_index) = json_request(
        app.clone(),
        "GET",
        &format!("/api/agents/{agent_id}/memory/index"),
        json!({}),
    )
    .await;
    assert_eq!(public_index_status, StatusCode::OK);
    assert!(public_index["body"]
        .as_str()
        .is_some_and(|body| body.contains("project_apollo")));

    let (public_topic_status, public_topic) = json_request(
        app.clone(),
        "GET",
        &format!("/api/agents/{agent_id}/memory/topic?type=project&topic=apollo"),
        json!({}),
    )
    .await;
    assert_eq!(public_topic_status, StatusCode::OK);
    assert_eq!(public_topic["version"], 1);
    assert!(public_topic["body"]
        .as_str()
        .is_some_and(|body| body.contains("Ship locally.")));

    let (move_status, moved) = bridge_json_request(
        app.clone(),
        "POST",
        &format!("/api/bridge/agents/{agent_id}/memory/move"),
        json!({
            "from_scope": "agent",
            "to_scope": "channel",
            "to_channel_id": channel_id,
            "type": "project",
            "topic": "apollo"
        }),
        &bridge_token,
    )
    .await;
    assert_eq!(move_status, StatusCode::OK);
    assert!(moved["from"]
        .as_str()
        .is_some_and(|path| path.starts_with("agents/")));
    assert!(moved["to"]
        .as_str()
        .is_some_and(|path| path.starts_with("channels/")));

    let (missing_status, _) = json_request(
        app.clone(),
        "GET",
        &format!("/api/agents/{agent_id}/memory/topic?type=project&topic=apollo"),
        json!({}),
    )
    .await;
    assert_eq!(missing_status, StatusCode::NOT_FOUND);

    let (channel_topic_status, channel_topic) = json_request(
        app.clone(),
        "GET",
        &format!("/api/channels/{channel_id}/memory/topic?type=project&topic=apollo"),
        json!({}),
    )
    .await;
    assert_eq!(channel_topic_status, StatusCode::OK);
    assert!(channel_topic["body"]
        .as_str()
        .is_some_and(|body| body.contains("Ship locally.")));

    let (list_status, namespace) = bridge_json_request(
        app.clone(),
        "GET",
        &format!("/api/bridge/agents/{agent_id}/memory/list?scope=channel&channel_id={channel_id}"),
        json!({}),
        &bridge_token,
    )
    .await;
    assert_eq!(list_status, StatusCode::OK);
    assert_eq!(namespace["entries"].as_array().map(Vec::len), Some(2));

    let (_, other_channel) = json_request(
        app.clone(),
        "POST",
        "/api/channels",
        json!({"name": "not-a-member"}),
    )
    .await;
    let other_channel_id = other_channel["id"].as_str().expect("other channel id");
    let (forbidden_status, _) = bridge_json_request(
        app,
        "GET",
        &format!(
            "/api/bridge/agents/{agent_id}/memory/index?scope=channel&channel_id={other_channel_id}"
        ),
        json!({}),
        &bridge_token,
    )
    .await;
    assert_eq!(forbidden_status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn post_message_persists_user_message_and_fake_agent_reply() {
    let store = Store::in_memory().await.expect("store should open");
    let app = router(store, Arc::new(FakeRuntime));

    let (_, channel) = json_request(
        app.clone(),
        "POST",
        "/api/channels",
        json!({"name": "general"}),
    )
    .await;
    let channel_id = channel["id"]
        .as_str()
        .expect("channel id should be present");

    let (agent_status, _) = json_request(
        app.clone(),
        "POST",
        "/api/agents",
        json!({
            "channel_id": channel_id,
            "name": "echo",
            "runtime": "fake",
            "model": "test-model"
        }),
    )
    .await;
    assert_eq!(agent_status, StatusCode::CREATED);

    let (message_status, posted) = json_request(
        app.clone(),
        "POST",
        &format!("/api/channels/{channel_id}/messages"),
        json!({"content": "hello"}),
    )
    .await;
    assert_eq!(message_status, StatusCode::CREATED);
    assert_eq!(posted["replies"][0]["content"], "echo: hello");
    assert_eq!(posted["pending_deliveries"], json!([]));

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/channels/{channel_id}/messages"))
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("request should complete");
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should load");
    let messages: Value = serde_json::from_slice(&bytes).expect("messages response should be JSON");

    assert_eq!(messages.as_array().map(Vec::len), Some(2));
}

#[tokio::test]
async fn global_search_finds_channels_agents_messages_and_tasks() {
    let store = Store::in_memory().await.expect("store should open");
    let app = router(store, Arc::new(FakeRuntime));
    let (_, channel) = json_request(
        app.clone(),
        "POST",
        "/api/channels",
        json!({"name": "needle-channel"}),
    )
    .await;
    let channel_id = channel["id"].as_str().expect("channel id");
    let _ = json_request(
        app.clone(),
        "POST",
        "/api/agents",
        json!({
            "channel_id": channel_id,
            "name": "needle-agent",
            "runtime": "fake",
            "model": "test-model"
        }),
    )
    .await;
    let _ = json_request(
        app.clone(),
        "POST",
        &format!("/api/channels/{channel_id}/messages"),
        json!({"content": "needle message"}),
    )
    .await;
    let _ = json_request(
        app.clone(),
        "POST",
        &format!("/api/channels/{channel_id}/tasks"),
        json!({"title": "needle task"}),
    )
    .await;
    let (status, body) = json_request(app, "GET", "/api/search?q=needle", json!({})).await;

    assert_eq!(status, StatusCode::OK);
    let kinds = body["results"]
        .as_array()
        .expect("search results")
        .iter()
        .filter_map(|result| result["kind"].as_str())
        .collect::<Vec<_>>();
    assert!(kinds.contains(&"channel"));
    assert!(kinds.contains(&"agent"));
    assert!(kinds.contains(&"message"));
    assert!(kinds.contains(&"task"));
}

#[tokio::test]
async fn global_search_routes_direct_messages_to_the_agent_subject() {
    let store = Store::in_memory().await.expect("store should open");
    let app = router(store, Arc::new(FakeRuntime));
    let (_, agent) = json_request(
        app.clone(),
        "POST",
        "/api/agents",
        json!({
            "name": "researcher",
            "runtime": "fake",
            "model": "test-model"
        }),
    )
    .await;
    let agent_id = agent["id"].as_str().expect("agent id");
    let (message_status, _) = json_request(
        app.clone(),
        "POST",
        &format!("/api/agents/{agent_id}/messages"),
        json!({"content": "private-needle"}),
    )
    .await;
    assert_eq!(message_status, StatusCode::CREATED);

    let (status, body) = json_request(app, "GET", "/api/search?q=private-needle", json!({})).await;
    assert_eq!(status, StatusCode::OK);
    let results = body["results"].as_array().expect("search results");
    assert!(!results.is_empty());
    assert!(results.iter().all(|result| result["kind"] == "message"));
    assert!(results.iter().all(|result| result["agentId"] == agent_id));
    assert!(results.iter().all(|result| result["channelId"].is_null()));
    assert!(results.iter().all(|result| result["title"]
        .as_str()
        .is_some_and(|title| title.starts_with("@researcher"))));
}

#[tokio::test]
async fn agent_creation_is_persistent_and_runtime_start_is_lazy() {
    let store = Store::in_memory().await.expect("store should open");
    let app = router(store.clone(), Arc::new(FailingStartRuntime));
    let (_, channel) = json_request(
        app.clone(),
        "POST",
        "/api/channels",
        json!({"name": "startup-failure"}),
    )
    .await;

    let (status, body) = json_request(
        app.clone(),
        "POST",
        "/api/agents",
        json!({
            "channel_id": channel["id"],
            "name": "broken",
            "runtime": "fake",
            "model": "test-model"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    let agent_id = body["id"].as_str().expect("agent id");
    assert_eq!(body["lifecycle_status"], "active");

    let (start_status, start_error) = json_request(
        app,
        "POST",
        &format!("/api/agents/{agent_id}/start"),
        json!({}),
    )
    .await;
    assert_eq!(start_status, StatusCode::BAD_GATEWAY);
    assert_eq!(start_error["error"], "simulated startup failure");
    assert_eq!(
        store.list_agents().await.expect("agents should list").len(),
        1,
        "a Runtime failure must not erase the persistent Agent identity"
    );
}

#[tokio::test]
async fn task_routes_support_numbering_claims_transitions_and_dependencies() {
    let store = Store::in_memory().await.expect("store should open");
    let app = router(store, Arc::new(FakeRuntime));

    let (_, channel) = json_request(
        app.clone(),
        "POST",
        "/api/channels",
        json!({"name": "task-api"}),
    )
    .await;
    let channel_id = channel["id"].as_str().expect("channel id");
    let (_, agent) = json_request(
        app.clone(),
        "POST",
        "/api/agents",
        json!({
            "channel_id": channel_id,
            "name": "builder",
            "runtime": "fake",
            "model": "test-model"
        }),
    )
    .await;
    let agent_id = agent["id"].as_str().expect("agent id");

    let (first_status, first) = json_request(
        app.clone(),
        "POST",
        &format!("/api/channels/{channel_id}/tasks"),
        json!({"title": "prepare"}),
    )
    .await;
    assert_eq!(first_status, StatusCode::CREATED);
    assert_eq!(first["taskNumber"], 1);
    assert_eq!(first["status"], "todo");
    let (_, second) = json_request(
        app.clone(),
        "POST",
        &format!("/api/channels/{channel_id}/tasks"),
        json!({"title": "ship"}),
    )
    .await;
    assert_eq!(second["taskNumber"], 2);

    let (dependency_status, dependencies) = json_request(
        app.clone(),
        "POST",
        &format!("/api/channels/{channel_id}/tasks/2/dependencies"),
        json!({"dependsOn": 1}),
    )
    .await;
    assert_eq!(dependency_status, StatusCode::CREATED);
    assert_eq!(dependencies["dependsOn"], json!([1]));

    let (blocked_status, blocked) = json_request(
        app.clone(),
        "POST",
        &format!("/api/channels/{channel_id}/tasks/2/claim"),
        json!({"agentId": agent_id}),
    )
    .await;
    assert_eq!(blocked_status, StatusCode::CONFLICT);
    assert!(blocked["error"]
        .as_str()
        .expect("blocked error")
        .contains("unmet dependencies"));

    let (claim_status, claimed) = json_request(
        app.clone(),
        "POST",
        &format!("/api/channels/{channel_id}/tasks/1/claim"),
        json!({"agentId": agent_id}),
    )
    .await;
    assert_eq!(claim_status, StatusCode::OK);
    assert_eq!(claimed["status"], "in_progress");
    assert_eq!(claimed["assigneeName"], "builder");
    let (done_status, done) = json_request(
        app.clone(),
        "POST",
        &format!("/api/channels/{channel_id}/tasks/1/status"),
        json!({"status": "done", "progress": "verified"}),
    )
    .await;
    assert_eq!(done_status, StatusCode::OK);
    assert_eq!(done["progress"], "verified");

    let (dependent_status, dependent) = json_request(
        app.clone(),
        "POST",
        &format!("/api/channels/{channel_id}/tasks/2/claim"),
        json!({"agentId": agent_id}),
    )
    .await;
    assert_eq!(dependent_status, StatusCode::OK);
    assert_eq!(dependent["status"], "in_progress");

    let (_, in_progress) = json_request(
        app,
        "GET",
        &format!("/api/channels/{channel_id}/tasks?status=in_progress"),
        json!({}),
    )
    .await;
    assert_eq!(in_progress.as_array().map(Vec::len), Some(1));
    assert_eq!(in_progress[0]["taskNumber"], 2);
}

#[tokio::test]
async fn runtime_history_routes_match_the_existing_web_contract() {
    let store = Store::in_memory().await.expect("store should open");
    let channel = store
        .create_channel("history-api")
        .await
        .expect("channel should persist");
    let agent = store
        .create_agent(
            channel.id,
            "historian",
            "fake",
            Some("test-model"),
            AgentStatus::Running,
        )
        .await
        .expect("agent should persist");
    let message = store
        .append_message(channel.id, None, MessageRole::User, "record this")
        .await
        .expect("message should persist");
    let started_at = Utc::now();
    let session = store
        .create_agent_session(
            agent.id,
            Some(channel.id),
            "session-web",
            Some("launch-web"),
            None,
            "chat",
            started_at,
        )
        .await
        .expect("session should persist");
    let turn = store
        .upsert_agent_turn(&NewAgentTurn {
            agent_id: agent.id,
            session_id: session.session_id.clone(),
            launch_id: session.launch_id.clone(),
            turn_number: 1,
            started_at,
            ended_at: Some(started_at + ChronoDuration::milliseconds(250)),
            input_tokens: 10,
            output_tokens: 5,
            cost_usd: 0.001,
            context_window: 100_000,
            entries: json!([{"kind": "text", "text": "recorded"}]),
            session_type: "chat".to_owned(),
            channel_id: Some(channel.id),
            source_message_id: Some(message.id),
        })
        .await
        .expect("turn should persist");
    store
        .insert_agent_activity(
            agent.id,
            Some(session.id),
            Some(&session.session_id),
            "working",
            Some("recording"),
            &["recording".to_owned()],
            session.launch_id.as_deref(),
            started_at,
        )
        .await
        .expect("activity should persist");

    let app = router(store, Arc::new(FakeRuntime));
    let agent_id = agent.id;

    let (sessions_status, sessions) = json_request(
        app.clone(),
        "GET",
        &format!("/api/agents/{agent_id}/sessions?limit=20&type=chat"),
        json!(null),
    )
    .await;
    assert_eq!(sessions_status, StatusCode::OK);
    assert_eq!(sessions[0]["sessionId"], "session-web");
    assert_eq!(sessions[0]["turnCount"], 1);
    assert_eq!(sessions[0]["inputTokens"], 10);

    let (current_status, current) = json_request(
        app.clone(),
        "GET",
        &format!("/api/agents/{agent_id}/sessions/current"),
        json!(null),
    )
    .await;
    assert_eq!(current_status, StatusCode::OK);
    assert_eq!(current["id"], session.id.to_string());

    let (turns_status, turns) = json_request(
        app.clone(),
        "GET",
        &format!("/api/agents/{agent_id}/sessions/session-web/turns?limit=120&offset=0"),
        json!(null),
    )
    .await;
    assert_eq!(turns_status, StatusCode::OK);
    assert_eq!(turns[0]["id"], turn.id.to_string());
    assert_eq!(turns[0]["durationMs"], 250);
    assert_eq!(turns[0]["messageRef"]["messageId"], message.id.to_string());

    let (turn_status, loaded_turn) = json_request(
        app.clone(),
        "GET",
        &format!("/api/agents/{agent_id}/turns/{}", turn.id),
        json!(null),
    )
    .await;
    assert_eq!(turn_status, StatusCode::OK);
    assert_eq!(loaded_turn["entries"][0]["kind"], "text");

    let (activity_status, activity) = json_request(
        app,
        "GET",
        &format!("/api/agents/{agent_id}/activity?limit=50&offset=0"),
        json!(null),
    )
    .await;
    assert_eq!(activity_status, StatusCode::OK);
    assert_eq!(activity[0]["activity"], "working");
    assert_eq!(activity[0]["sessionRowId"], session.id.to_string());
}

#[tokio::test]
async fn failed_runtime_delivery_is_accepted_and_retried_from_sqlite() {
    let store = Store::in_memory().await.expect("store should open");
    let app = router_with_delivery_config(
        store,
        Arc::new(FlakyRuntime::default()),
        DeliveryConfig {
            batch_size: 8,
            max_attempts: 3,
            poll_interval: Duration::from_millis(5),
            attempt_timeout: Duration::from_secs(1),
            base_backoff: Duration::from_millis(5),
            max_backoff: Duration::from_millis(5),
        },
    );

    let (_, channel) = json_request(
        app.clone(),
        "POST",
        "/api/channels",
        json!({"name": "retry"}),
    )
    .await;
    let channel_id = channel["id"].as_str().expect("channel id");
    let _ = json_request(
        app.clone(),
        "POST",
        "/api/agents",
        json!({
            "channel_id": channel_id,
            "name": "flaky",
            "runtime": "fake",
            "model": "test-model"
        }),
    )
    .await;

    let (status, posted) = json_request(
        app.clone(),
        "POST",
        &format!("/api/channels/{channel_id}/messages"),
        json!({"content": "retry me"}),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(posted["replies"], json!([]));
    assert!(matches!(
        posted["pending_deliveries"][0]["state"].as_str(),
        Some("pending" | "in_flight")
    ));
    assert_eq!(posted["pending_deliveries"][0]["attempts"], 1);

    let mut messages = json!([]);
    for _ in 0..100 {
        let (_, current) = json_request(
            app.clone(),
            "GET",
            &format!("/api/channels/{channel_id}/messages"),
            json!({}),
        )
        .await;
        messages = current;
        if messages.as_array().map(Vec::len) == Some(2) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(messages.as_array().map(Vec::len), Some(2));
    assert_eq!(messages[1]["content"], "recovered: retry me");

    let (_, stats) = json_request(app, "GET", "/api/deliveries/stats", json!({})).await;
    assert_eq!(stats["pending"], 0);
    assert_eq!(stats["in_flight"], 0);
    assert_eq!(stats["exhausted"], 0);
}

#[tokio::test]
async fn timed_out_delivery_stops_runtime_before_retrying_once() {
    let store = Store::in_memory().await.expect("store should open");
    let runtime = Arc::new(TimeoutOnceRuntime::default());
    let app = router_with_delivery_config(
        store,
        runtime.clone(),
        DeliveryConfig {
            batch_size: 8,
            max_attempts: 3,
            poll_interval: Duration::from_millis(5),
            attempt_timeout: Duration::from_millis(10),
            base_backoff: Duration::from_millis(5),
            max_backoff: Duration::from_millis(5),
        },
    );
    let (_, channel) = json_request(
        app.clone(),
        "POST",
        "/api/channels",
        json!({"name": "timeout-retry"}),
    )
    .await;
    let channel_id = channel["id"].as_str().expect("channel id");
    let _ = json_request(
        app.clone(),
        "POST",
        "/api/agents",
        json!({
            "channel_id": channel_id,
            "name": "slow-once",
            "runtime": "fake",
            "model": "test-model"
        }),
    )
    .await;

    let (status, _) = json_request(
        app.clone(),
        "POST",
        &format!("/api/channels/{channel_id}/messages"),
        json!({"content": "do not duplicate"}),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);

    let mut messages = json!([]);
    for _ in 0..100 {
        let (_, current) = json_request(
            app.clone(),
            "GET",
            &format!("/api/channels/{channel_id}/messages"),
            json!({}),
        )
        .await;
        messages = current;
        if messages.as_array().map(Vec::len) == Some(2) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    tokio::time::sleep(Duration::from_millis(120)).await;
    let (_, final_messages) = json_request(
        app,
        "GET",
        &format!("/api/channels/{channel_id}/messages"),
        json!({}),
    )
    .await;
    assert_eq!(messages.as_array().map(Vec::len), Some(2));
    assert_eq!(final_messages.as_array().map(Vec::len), Some(2));
    assert_eq!(
        final_messages[1]["content"],
        "retried safely: do not duplicate"
    );
    assert_eq!(runtime.calls.load(Ordering::Relaxed), 2);
    assert_eq!(runtime.stops.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn startup_releases_and_retries_delivery_left_in_flight_by_previous_process() {
    let temp = tempfile::tempdir().expect("temp directory");
    let database_path = temp.path().join("cocli.sqlite3");
    let store = Store::open(&database_path).await.expect("store opens");
    let channel = store.create_channel("restart").await.expect("channel");
    let agent = store
        .create_agent(
            channel.id,
            "echo",
            "fake",
            Some("test-model"),
            AgentStatus::Running,
        )
        .await
        .expect("agent");
    let message = store
        .append_message(channel.id, None, MessageRole::User, "resume delivery")
        .await
        .expect("message");
    store
        .enqueue_deliveries(&message, &[agent.id])
        .await
        .expect("enqueue");
    let reserved = store
        .reserve_due_deliveries(1, 3, chrono::Utc::now())
        .await
        .expect("reserve before crash");
    assert_eq!(reserved.len(), 1);
    drop(store);

    let reopened = Store::open(&database_path).await.expect("reopen");
    let app = router_with_delivery_config(
        reopened,
        Arc::new(FakeRuntime),
        DeliveryConfig {
            batch_size: 8,
            max_attempts: 3,
            poll_interval: Duration::from_millis(5),
            attempt_timeout: Duration::from_secs(1),
            base_backoff: Duration::from_millis(5),
            max_backoff: Duration::from_millis(5),
        },
    );

    let mut messages = json!([]);
    for _ in 0..100 {
        let (_, current) = json_request(
            app.clone(),
            "GET",
            &format!("/api/channels/{}/messages", channel.id),
            json!({}),
        )
        .await;
        messages = current;
        if messages.as_array().map(Vec::len) == Some(2) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    assert_eq!(messages.as_array().map(Vec::len), Some(2));
    assert_eq!(messages[1]["content"], "echo: resume delivery");
    let (_, stats) = json_request(app, "GET", "/api/deliveries/stats", json!({})).await;
    assert_eq!(stats["pending"], 0);
    assert_eq!(stats["in_flight"], 0);
}

#[tokio::test]
async fn panicking_runtime_task_is_deferred_instead_of_sticking_in_flight() {
    let store = Store::in_memory().await.expect("store opens");
    let app = router_with_delivery_config(
        store,
        Arc::new(PanicOnceRuntime::default()),
        DeliveryConfig {
            batch_size: 8,
            max_attempts: 3,
            poll_interval: Duration::from_millis(5),
            attempt_timeout: Duration::from_secs(1),
            base_backoff: Duration::from_millis(5),
            max_backoff: Duration::from_millis(5),
        },
    );
    let (_, channel) = json_request(
        app.clone(),
        "POST",
        "/api/channels",
        json!({"name": "panic-retry"}),
    )
    .await;
    let channel_id = channel["id"].as_str().expect("channel id");
    let _ = json_request(
        app.clone(),
        "POST",
        "/api/agents",
        json!({
            "channel_id": channel_id,
            "name": "panic-once",
            "runtime": "fake",
            "model": "test-model"
        }),
    )
    .await;

    let (status, posted) = json_request(
        app.clone(),
        "POST",
        &format!("/api/channels/{channel_id}/messages"),
        json!({"content": "survive panic"}),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_ne!(posted["pending_deliveries"][0]["state"], "exhausted");

    let mut messages = json!([]);
    for _ in 0..100 {
        let (_, current) = json_request(
            app.clone(),
            "GET",
            &format!("/api/channels/{channel_id}/messages"),
            json!({}),
        )
        .await;
        messages = current;
        if messages.as_array().map(Vec::len) == Some(2) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(messages.as_array().map(Vec::len), Some(2));
    assert_eq!(
        messages[1]["content"],
        "recovered after panic: survive panic"
    );
}

#[tokio::test]
async fn local_bridge_routes_support_message_inbox_history_and_working_state() {
    let store = Store::in_memory().await.expect("store opens");
    let app = router(store.clone(), Arc::new(FakeRuntime));
    let (_, channel) = json_request(
        app.clone(),
        "POST",
        "/api/channels",
        json!({"name": "bridge"}),
    )
    .await;
    let channel_id = channel["id"].as_str().expect("channel id");
    let (_, first) = json_request(
        app.clone(),
        "POST",
        "/api/agents",
        json!({
            "channel_id": channel_id,
            "name": "first",
            "runtime": "fake",
            "model": "test-model"
        }),
    )
    .await;
    let (_, second) = json_request(
        app.clone(),
        "POST",
        "/api/agents",
        json!({
            "channel_id": channel_id,
            "name": "second",
            "runtime": "fake",
            "model": "test-model"
        }),
    )
    .await;
    let first_id = first["id"].as_str().expect("first id");
    let second_id = second["id"].as_str().expect("second id");
    let first_token = store
        .agent_bridge_token(first_id.parse().expect("first uuid"))
        .await
        .expect("first token query")
        .expect("first token");
    let second_token = store
        .agent_bridge_token(second_id.parse().expect("second uuid"))
        .await
        .expect("second token query")
        .expect("second token");
    let (missing_auth, _) = json_request(
        app.clone(),
        "GET",
        &format!("/api/bridge/agents/{second_id}/inbox"),
        json!({}),
    )
    .await;
    assert_eq!(missing_auth, StatusCode::UNAUTHORIZED);
    let (cross_agent_auth, _) = bridge_json_request(
        app.clone(),
        "GET",
        &format!("/api/bridge/agents/{second_id}/inbox"),
        json!({}),
        &first_token,
    )
    .await;
    assert_eq!(cross_agent_auth, StatusCode::UNAUTHORIZED);

    let (send_status, sent) = bridge_json_request(
        app.clone(),
        "POST",
        &format!("/api/bridge/agents/{first_id}/messages"),
        json!({"target": "#bridge", "content": "peer update"}),
        &first_token,
    )
    .await;
    assert_eq!(send_status, StatusCode::CREATED);
    assert_eq!(sent["content"], "peer update");
    assert_eq!(sent["agent_id"], first_id);

    let (_, inbox) = bridge_json_request(
        app.clone(),
        "GET",
        &format!("/api/bridge/agents/{second_id}/inbox?limit=10"),
        json!({}),
        &second_token,
    )
    .await;
    assert_eq!(inbox["messages"].as_array().map(Vec::len), Some(1));
    assert_eq!(inbox["messages"][0]["content"], "peer update");
    let (_, consumed) = bridge_json_request(
        app.clone(),
        "GET",
        &format!("/api/bridge/agents/{second_id}/inbox?limit=10"),
        json!({}),
        &second_token,
    )
    .await;
    assert_eq!(consumed["messages"], json!([]));
    let (_, own_inbox) = bridge_json_request(
        app.clone(),
        "GET",
        &format!("/api/bridge/agents/{first_id}/inbox"),
        json!({}),
        &first_token,
    )
    .await;
    assert_eq!(own_inbox["messages"], json!([]));

    let (missing_target_status, missing_target_error) = bridge_json_request(
        app.clone(),
        "GET",
        &format!("/api/bridge/agents/{second_id}/history?limit=10"),
        json!({}),
        &second_token,
    )
    .await;
    assert_eq!(missing_target_status, StatusCode::BAD_REQUEST);
    assert_eq!(missing_target_error["error"], "channel target is required");

    let (_, history) = bridge_json_request(
        app.clone(),
        "GET",
        &format!("/api/bridge/agents/{second_id}/history?channel=bridge&limit=10"),
        json!({}),
        &second_token,
    )
    .await;
    assert_eq!(history["channel"]["name"], "bridge");
    assert_eq!(history["messages"][0]["content"], "peer update");

    let (create_tasks_status, created_tasks) = bridge_json_request(
        app.clone(),
        "POST",
        &format!("/api/bridge/agents/{second_id}/tasks"),
        json!({"channel": "bridge", "tasks": [{"title": "prepare"}, {"title": "ship"}]}),
        &second_token,
    )
    .await;
    assert_eq!(create_tasks_status, StatusCode::CREATED);
    assert_eq!(created_tasks["tasks"][0]["taskNumber"], 1);
    assert_eq!(created_tasks["tasks"][1]["taskNumber"], 2);
    let (_, dependency) = bridge_json_request(
        app.clone(),
        "POST",
        &format!("/api/bridge/agents/{second_id}/tasks/dependencies"),
        json!({"channel": "bridge", "task_number": 2, "depends_on": 1}),
        &second_token,
    )
    .await;
    assert_eq!(dependency["dependsOn"], json!([1]));
    let (_, blocked_claim) = bridge_json_request(
        app.clone(),
        "POST",
        &format!("/api/bridge/agents/{second_id}/tasks/claim"),
        json!({"channel": "bridge", "task_numbers": [2]}),
        &second_token,
    )
    .await;
    assert_eq!(blocked_claim["results"][0]["success"], false);
    assert!(blocked_claim["results"][0]["reason"]
        .as_str()
        .expect("claim reason")
        .contains("unmet dependencies"));
    let (_, first_claim) = bridge_json_request(
        app.clone(),
        "POST",
        &format!("/api/bridge/agents/{second_id}/tasks/claim"),
        json!({"channel": "bridge", "task_numbers": [1]}),
        &second_token,
    )
    .await;
    assert_eq!(first_claim["results"][0]["success"], true);
    let (_, completed) = bridge_json_request(
        app.clone(),
        "POST",
        &format!("/api/bridge/agents/{second_id}/tasks/update-status"),
        json!({"channel": "bridge", "task_number": 1, "status": "done", "progress": "verified"}),
        &second_token,
    )
    .await;
    assert_eq!(completed["status"], "done");
    let (_, second_claim) = bridge_json_request(
        app.clone(),
        "POST",
        &format!("/api/bridge/agents/{second_id}/tasks/claim"),
        json!({"channel": "bridge", "task_numbers": [2]}),
        &second_token,
    )
    .await;
    assert_eq!(second_claim["results"][0]["success"], true);
    let (_, tasks) = bridge_json_request(
        app.clone(),
        "GET",
        &format!("/api/bridge/agents/{second_id}/tasks?channel=bridge&status=all"),
        json!({}),
        &second_token,
    )
    .await;
    assert_eq!(tasks["tasks"].as_array().map(Vec::len), Some(2));
    let (_, message_claim) = bridge_json_request(
        app.clone(),
        "POST",
        &format!("/api/bridge/agents/{second_id}/tasks/claim"),
        json!({"channel": "bridge", "message_ids": [sent["id"]]}),
        &second_token,
    )
    .await;
    assert_eq!(message_claim["results"][0]["success"], true);
    assert_eq!(message_claim["results"][0]["created"], true);
    assert_eq!(message_claim["results"][0]["task"]["messageId"], sent["id"]);

    let (_, working) = bridge_json_request(
        app.clone(),
        "POST",
        &format!("/api/bridge/agents/{second_id}/working"),
        json!({
            "summary": "implement MCP",
            "channelName": "bridge",
            "taskNumber": 3,
            "nextStepHint": "run protocol tests"
        }),
        &second_token,
    )
    .await;
    assert_eq!(working["state"]["summary"], "implement MCP");
    assert_eq!(working["state"]["task_number"], 3);
    let (_, current) = bridge_json_request(
        app.clone(),
        "GET",
        &format!("/api/bridge/agents/{second_id}/working"),
        json!({}),
        &second_token,
    )
    .await;
    assert_eq!(current["state"]["next_step_hint"], "run protocol tests");
    let (_, cleared) = bridge_json_request(
        app.clone(),
        "POST",
        &format!("/api/bridge/agents/{second_id}/working/clear"),
        json!({}),
        &second_token,
    )
    .await;
    assert_eq!(cleared["cleared"], true);
    let (_, empty) = bridge_json_request(
        app,
        "GET",
        &format!("/api/bridge/agents/{second_id}/working"),
        json!({}),
        &second_token,
    )
    .await;
    assert!(empty["state"].is_null());
}

#[tokio::test]
async fn runtime_control_routes_expose_status_and_typed_unsupported_errors() {
    let store = Store::in_memory().await.expect("store should open");
    let app = router(store, Arc::new(FakeRuntime));

    let (_, channel) = json_request(
        app.clone(),
        "POST",
        "/api/channels",
        json!({"name": "controls"}),
    )
    .await;
    let channel_id = channel["id"].as_str().expect("channel id");
    let (_, agent) = json_request(
        app.clone(),
        "POST",
        "/api/agents",
        json!({
            "channel_id": channel_id,
            "name": "fake",
            "runtime": "fake",
            "model": "test-model"
        }),
    )
    .await;
    let agent_id = agent["id"].as_str().expect("agent id");

    let (status_code, status) = json_request(
        app.clone(),
        "GET",
        &format!("/api/agents/{agent_id}/runtime"),
        json!({}),
    )
    .await;
    assert_eq!(status_code, StatusCode::OK);
    assert_eq!(status["agent_id"], agent_id);
    assert_eq!(status["running"], false);
    assert_eq!(status["tier"], "healthy");

    let (metrics_status, metrics) =
        json_request(app.clone(), "GET", "/api/metrics", json!({})).await;
    assert_eq!(metrics_status, StatusCode::OK);
    assert_eq!(metrics["counters"], json!({}));
    assert_eq!(metrics["gauges"], json!({}));

    let (steer_status, steer_error) = json_request(
        app.clone(),
        "POST",
        &format!("/api/agents/{agent_id}/turn/steer"),
        json!({"input": "redirect"}),
    )
    .await;
    assert_eq!(steer_status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(steer_error["error"]
        .as_str()
        .expect("steer error")
        .contains("not supported"));

    let (fork_status, fork_error) = json_request(
        app.clone(),
        "POST",
        &format!("/api/agents/{agent_id}/thread/fork"),
        json!({}),
    )
    .await;
    assert_eq!(fork_status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(fork_error["error"]
        .as_str()
        .expect("fork error")
        .contains("not supported"));

    let (probe_status, probe_error) = json_request(
        app,
        "POST",
        &format!("/api/agents/{agent_id}/recovery/probe"),
        json!({}),
    )
    .await;
    assert_eq!(probe_status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(probe_error["error"]
        .as_str()
        .expect("probe error")
        .contains("not supported"));
}
