use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use cocli_api::{router, RuntimeError, RuntimeInfo, RuntimeService};
use cocli_driver_core::{
    McpApplyActionResult, McpApplyActionStatus, McpApplyExecutionRequest, McpApplyExecutionResult,
    McpBackupDescriptor, McpReloadResult, McpReloadStatus, McpVerificationResult,
    McpVerificationStatus,
};
use cocli_store::{Agent, AgentStatus, Message, Store};
use serde_json::{json, Value};
use tempfile::tempdir;
use tower::ServiceExt;

#[derive(Debug, Default)]
struct CountingMcpRuntime {
    apply_calls: AtomicUsize,
}

#[async_trait]
impl RuntimeService for CountingMcpRuntime {
    async fn list(&self) -> Vec<RuntimeInfo> {
        vec![RuntimeInfo {
            name: "fake".to_owned(),
            installed: true,
            binary: None,
            version: Some("portable-restore-test".to_owned()),
            models: Vec::new(),
            capabilities: Vec::new(),
            unavailable_reason: None,
        }]
    }

    async fn reply(&self, _agent: &Agent, message: &Message) -> Result<String, RuntimeError> {
        Ok(format!("echo: {}", message.content))
    }

    async fn apply_mcp(
        &self,
        _request: McpApplyExecutionRequest,
        _journal: Arc<dyn cocli_api::McpApplyJournalSink>,
    ) -> Result<McpApplyExecutionResult, RuntimeError> {
        self.apply_calls.fetch_add(1, Ordering::SeqCst);
        Ok(McpApplyExecutionResult {
            actions: vec![McpApplyActionResult {
                action_index: 0,
                runtime: "cursor".to_owned(),
                server_id: "docs".to_owned(),
                status: McpApplyActionStatus::Verified,
                reason: "should not run after portable restore".to_owned(),
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
                reason: "runtime restart intentionally deferred".to_owned(),
            }],
            verification: McpVerificationResult {
                status: McpVerificationStatus::Matched,
                observation_hash: "verified-observation".to_owned(),
                mismatches: Vec::new(),
                written_config_hashes: Default::default(),
                session_effective: Default::default(),
            },
            journal: Vec::new(),
        })
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

#[tokio::test]
async fn portable_restore_cannot_replay_governance_apply() {
    let temp = tempdir().expect("temp directory");
    let source_db = temp.path().join("source.sqlite3");
    let snapshot = temp.path().join("snapshot.sqlite3");

    let store = Store::open(&source_db).await.expect("source store");
    let source_installation_id = store.current_installation_id().to_owned();
    let channel = store
        .create_channel("portable-governance")
        .await
        .expect("channel");
    let agent = store
        .create_agent(channel.id, "governed", "fake", None, AgentStatus::Stopped)
        .await
        .expect("agent");
    let runtime = Arc::new(CountingMcpRuntime::default());
    let app = router(store.clone(), runtime.clone());

    let (profile_status, profile) = json_request(
        app.clone(),
        "POST",
        "/api/runtimes/mcp/profiles",
        json!({
            "name": "portable-docs",
            "description": "desired-state MCP profile",
            "servers": [{
                "serverId": "srv-docs",
                "runtime": "cursor",
                "alias": "docs",
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
                    "reference": "keychain://cocli/docs-token"
                }]
            }]
        }),
    )
    .await;
    assert_eq!(profile_status, StatusCode::CREATED, "{profile}");
    let profile_id = profile["id"].as_str().expect("profile id");
    let (binding_status, binding) = json_request(
        app.clone(),
        "POST",
        "/api/runtimes/mcp/bindings",
        json!({
            "profileId": profile_id,
            "targetType": "agent",
            "targetId": agent.id
        }),
    )
    .await;
    assert_eq!(binding_status, StatusCode::CREATED, "{binding}");

    let (plan_status, plan_view) = json_request(
        app.clone(),
        "POST",
        "/api/runtimes/mcp/plans",
        json!({ "agentId": agent.id }),
    )
    .await;
    assert_eq!(plan_status, StatusCode::CREATED, "{plan_view}");
    let plan = &plan_view["plan"];
    let plan_id = plan["id"].as_str().expect("plan id").to_owned();
    let plan_hash = plan["planHash"].as_str().expect("plan hash").to_owned();
    let observation_hash = plan["observationHash"]
        .as_str()
        .expect("observation hash")
        .to_owned();
    let config_hash = plan["configHash"].as_str().expect("config hash").to_owned();
    let (approve_status, approved) = json_request(
        app.clone(),
        "POST",
        &format!("/api/runtimes/mcp/plans/{plan_id}/approve"),
        json!({
            "planHash": plan_hash,
            "actor": "portable-restore-test",
            "expiresAt": "2099-07-19T10:00:00Z"
        }),
    )
    .await;
    assert_eq!(approve_status, StatusCode::OK, "{approved}");
    assert_eq!(approved["approvalStatus"], "approved");
    assert_eq!(approved["approvedButNotApplied"], true);
    let approval_id = approved["decision"]["id"]
        .as_str()
        .expect("approval id")
        .to_owned();
    let decision = store
        .get_mcp_plan_decision(&plan_id)
        .await
        .expect("read approval")
        .expect("approval exists");
    assert_eq!(decision.id.to_string(), approval_id);

    store
        .export_portable_snapshot(&snapshot)
        .await
        .expect("export sanitized snapshot");
    let apply_body = json!({
        "planHash": plan_hash,
        "observationHash": observation_hash,
        "configHash": config_hash,
        "actor": "portable-restore-test",
        "confirmHighRisk": true
    });
    let (source_apply_status, source_applied) = json_request(
        app.clone(),
        "POST",
        &format!("/api/runtimes/mcp/plans/{plan_id}/apply"),
        apply_body.clone(),
    )
    .await;
    assert_eq!(source_apply_status, StatusCode::OK, "{source_applied}");
    assert_eq!(runtime.apply_calls.load(Ordering::SeqCst), 1);
    drop(app);
    store.close().await;

    let staged = Store::open(&snapshot).await.expect("open staged snapshot");
    let restored_installation_id = staged
        .prepare_portable_restore()
        .await
        .expect("assign restored installation");
    staged.close().await;
    assert_ne!(restored_installation_id, source_installation_id);

    let restored = Store::open(&snapshot).await.expect("reopen restored store");
    assert_eq!(restored.current_installation_id(), restored_installation_id);
    assert!(restored
        .get_mcp_plan(&plan_id)
        .await
        .expect("restored plan lookup")
        .is_some());
    assert!(restored
        .get_mcp_plan_decision(&plan_id)
        .await
        .expect("restored approval lookup")
        .is_none());
    let restored_profile_id = uuid::Uuid::parse_str(profile_id).expect("profile id");
    assert!(restored
        .get_mcp_profile(restored_profile_id)
        .await
        .expect("restored profile lookup")
        .is_some());
    let restored_runtime = Arc::new(CountingMcpRuntime::default());
    let restored_app = router(restored.clone(), restored_runtime.clone());

    let (plan_after_status, plan_after) = json_request(
        restored_app.clone(),
        "GET",
        &format!("/api/runtimes/mcp/plans/{plan_id}"),
        json!({}),
    )
    .await;
    assert_eq!(plan_after_status, StatusCode::OK, "{plan_after}");

    let (apply_status, applied) = json_request(
        restored_app,
        "POST",
        &format!("/api/runtimes/mcp/plans/{plan_id}/apply"),
        apply_body,
    )
    .await;
    let error = applied
        .get("error")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    assert!(
        matches!(apply_status, StatusCode::CONFLICT | StatusCode::NOT_FOUND),
        "restored pre-restore approval must fail closed, got {apply_status}: {applied}"
    );
    assert!(
        error.contains("stale")
            || error.contains("mismatch")
            || error.contains("not found")
            || error.contains("drift")
            || error.contains("no approval"),
        "apply rejection should be stale/mismatch/not-found, got {applied}"
    );
    assert_eq!(
        restored_runtime.apply_calls.load(Ordering::SeqCst),
        0,
        "runtime apply must not execute a restored approval"
    );
    assert_ne!(plan_after["approvalStatus"], "approved");
    restored.close().await;
}
