use std::sync::Arc;

use async_trait::async_trait;
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use cocli_api::{router, RuntimeError, RuntimeInfo, RuntimeService};
use cocli_store::{Agent, Message, Store};
use serde_json::{json, Value};
use tower::ServiceExt;

#[derive(Debug)]
struct FakeRuntime;

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
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("response should be JSON")
    };
    (status, body)
}

#[tokio::test]
async fn workspace_routes_preserve_legacy_shape_and_support_share_rebind_and_detach() {
    let store = Store::in_memory().await.expect("store should open");
    let app = router(store, Arc::new(FakeRuntime));
    let (_, channel) = json_request(
        app.clone(),
        "POST",
        "/api/channels",
        json!({"name": "portable"}),
    )
    .await;
    let channel_id = channel["id"].as_str().expect("channel id");
    let (_, agent) = json_request(
        app.clone(),
        "POST",
        "/api/agents",
        json!({"name": "portable-agent", "runtime": "fake"}),
    )
    .await;
    let agent_id = agent["id"].as_str().expect("agent id");

    let (created_status, created) = json_request(
        app.clone(),
        "POST",
        &format!("/api/channels/{channel_id}/workspaces"),
        json!({
            "kind": "directory",
            "locator": "/definitely/missing/portable",
            "metadata": {"label": "Portable files"}
        }),
    )
    .await;
    assert_eq!(created_status, StatusCode::CREATED);
    assert_eq!(created["owner_type"], "channel");
    assert_eq!(created["owner_id"], channel_id);
    assert_eq!(created["kind"], "directory");
    assert_eq!(created["locator"], "/definitely/missing/portable");
    assert_eq!(created["provider_key"], "directory");
    let workspace_id = created["id"].as_str().expect("workspace id");
    let (canonical_status, canonical) = json_request(
        app.clone(),
        "GET",
        &format!("/api/workspaces/{workspace_id}"),
        json!({}),
    )
    .await;
    assert_eq!(canonical_status, StatusCode::OK);
    assert_eq!(canonical["provider_key"], "directory");
    assert!(
        canonical.get("owner_type").is_none(),
        "canonical workspace must not expose legacy owner_type"
    );
    assert!(
        canonical.get("owner_id").is_none(),
        "canonical workspace must not expose legacy owner_id"
    );
    assert!(
        canonical.get("kind").is_none(),
        "canonical workspace must not expose legacy kind"
    );
    assert!(
        canonical.get("locator").is_none(),
        "canonical workspace must not expose local binding locator"
    );

    let (unbound_status, unbound) = json_request(
        app.clone(),
        "POST",
        &format!("/api/channels/{channel_id}/workspaces"),
        json!({"kind": "managed", "metadata": {"label": "Unbound"}}),
    )
    .await;
    assert_eq!(unbound_status, StatusCode::CREATED);
    assert!(unbound.get("locator").is_some());
    assert!(unbound["locator"].is_null());
    let unbound_workspace_id = unbound["id"].as_str().expect("unbound workspace id");
    let (unbound_binding_status, unbound_binding) = json_request(
        app.clone(),
        "GET",
        &format!("/api/workspaces/{unbound_workspace_id}/binding"),
        json!({}),
    )
    .await;
    assert_eq!(unbound_binding_status, StatusCode::OK);
    assert_eq!(unbound_binding["state"], "unbound");
    assert_eq!(unbound_binding["error_code"], "binding_missing");

    let (attach_status, _) = json_request(
        app.clone(),
        "POST",
        &format!("/api/agents/{agent_id}/workspaces/{workspace_id}"),
        json!({"role": "shared"}),
    )
    .await;
    assert_eq!(attach_status, StatusCode::CREATED);
    let (_, channel_workspaces) = json_request(
        app.clone(),
        "GET",
        &format!("/api/channels/{channel_id}/workspaces"),
        json!({}),
    )
    .await;
    let (_, agent_workspaces) = json_request(
        app.clone(),
        "GET",
        &format!("/api/agents/{agent_id}/workspaces"),
        json!({}),
    )
    .await;
    assert_eq!(channel_workspaces[0]["id"], workspace_id);
    assert_eq!(agent_workspaces[0]["id"], workspace_id);

    let (update_status, updated) = json_request(
        app.clone(),
        "PUT",
        &format!("/api/workspaces/{workspace_id}"),
        json!({
            "display_name": "Moved files",
            "portable_locator": null,
            "metadata": {"label": "Moved files", "opaque": true}
        }),
    )
    .await;
    assert_eq!(update_status, StatusCode::OK);
    assert_eq!(updated["display_name"], "Moved files");
    assert_eq!(updated["metadata"]["opaque"], true);
    assert!(updated.get("owner_type").is_none());
    assert!(updated.get("owner_id").is_none());
    assert!(updated.get("kind").is_none());
    assert!(updated.get("locator").is_none());

    let (rebind_status, rebound) = json_request(
        app.clone(),
        "PUT",
        &format!("/api/workspaces/{workspace_id}/binding"),
        json!({"local_locator": "/still/missing", "secret_ref": null}),
    )
    .await;
    assert_eq!(rebind_status, StatusCode::OK);
    assert_eq!(rebound["state"], "needs_attention");
    assert_eq!(rebound["error_code"], "path_not_found");
    let (bindings_status, bindings) = json_request(
        app.clone(),
        "GET",
        &format!("/api/workspaces/{workspace_id}/bindings"),
        json!({}),
    )
    .await;
    assert_eq!(bindings_status, StatusCode::OK);
    assert_eq!(bindings.as_array().map(Vec::len), Some(1));

    let (detach_status, _) = json_request(
        app.clone(),
        "DELETE",
        &format!("/api/channels/{channel_id}/workspaces/{workspace_id}"),
        json!({}),
    )
    .await;
    assert_eq!(detach_status, StatusCode::NO_CONTENT);
    let (read_status, read) = json_request(
        app.clone(),
        "GET",
        &format!("/api/workspaces/{workspace_id}"),
        json!({}),
    )
    .await;
    assert_eq!(read_status, StatusCode::OK);
    assert_eq!(read["id"], workspace_id);
    assert!(read.get("owner_type").is_none());
    assert!(read.get("owner_id").is_none());
    assert!(read.get("kind").is_none());
    assert!(read.get("locator").is_none());

    let (delete_status, _) = json_request(
        app.clone(),
        "DELETE",
        &format!("/api/workspaces/{workspace_id}"),
        json!({}),
    )
    .await;
    assert_eq!(delete_status, StatusCode::NO_CONTENT);
    let (missing_status, _) = json_request(
        app.clone(),
        "GET",
        &format!("/api/workspaces/{workspace_id}"),
        json!({}),
    )
    .await;
    assert_eq!(missing_status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn canonical_workspace_routes_create_update_and_keep_binding_local() {
    let store = Store::in_memory().await.expect("store should open");
    let app = router(store, Arc::new(FakeRuntime));
    let (create_status, workspace) = json_request(
        app.clone(),
        "POST",
        "/api/workspaces",
        json!({
            "provider_key": "git",
            "descriptor_version": 1,
            "display_name": "Project remote",
            "portable_locator": "https://example.invalid/acme/project.git",
            "metadata": {"default_branch": "main"}
        }),
    )
    .await;
    assert_eq!(create_status, StatusCode::CREATED);
    assert_eq!(workspace["provider_key"], "git");
    assert_eq!(
        workspace["portable_locator"],
        "https://example.invalid/acme/project.git"
    );
    assert!(workspace.get("owner_type").is_none());
    assert!(workspace.get("owner_id").is_none());
    assert!(workspace.get("kind").is_none());
    assert!(workspace.get("locator").is_none());
    let workspace_id = workspace["id"].as_str().expect("workspace id");

    let (binding_status, binding) = json_request(
        app.clone(),
        "PUT",
        &format!("/api/workspaces/{workspace_id}/binding"),
        json!({"local_locator": "/definitely/missing/project-worktree"}),
    )
    .await;
    assert_eq!(binding_status, StatusCode::OK);
    assert_eq!(binding["state"], "needs_attention");
    assert_eq!(
        binding["local_locator"],
        "/definitely/missing/project-worktree"
    );

    let (read_status, read) = json_request(
        app.clone(),
        "GET",
        &format!("/api/workspaces/{workspace_id}"),
        json!({}),
    )
    .await;
    assert_eq!(read_status, StatusCode::OK);
    assert_eq!(
        read["portable_locator"],
        "https://example.invalid/acme/project.git"
    );
    assert!(read.get("locator").is_none());

    let (update_status, updated) = json_request(
        app.clone(),
        "PUT",
        &format!("/api/workspaces/{workspace_id}"),
        json!({
            "display_name": "Renamed remote",
            "portable_locator": "https://example.invalid/acme/renamed.git",
            "metadata": {"default_branch": "trunk"}
        }),
    )
    .await;
    assert_eq!(update_status, StatusCode::OK);
    assert_eq!(updated["display_name"], "Renamed remote");
    assert_eq!(
        updated["portable_locator"],
        "https://example.invalid/acme/renamed.git"
    );
    assert_eq!(updated["metadata"]["default_branch"], "trunk");
    assert!(updated.get("locator").is_none());

    let (git_path_status, _) = json_request(
        app.clone(),
        "POST",
        "/api/workspaces",
        json!({
            "provider_key": "git",
            "display_name": "Not portable",
            "portable_locator": "/Users/example/repository",
            "metadata": {}
        }),
    )
    .await;
    assert_eq!(git_path_status, StatusCode::BAD_REQUEST);

    let (directory_path_status, _) = json_request(
        app,
        "POST",
        "/api/workspaces",
        json!({
            "provider_key": "directory",
            "display_name": "Local directory",
            "portable_locator": "/Users/example/documents",
            "metadata": {}
        }),
    )
    .await;
    assert_eq!(directory_path_status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn unknown_provider_is_readable_and_reports_unavailable() {
    let store = Store::in_memory().await.expect("store should open");
    let app = router(store, Arc::new(FakeRuntime));
    let (_, channel) = json_request(
        app.clone(),
        "POST",
        "/api/channels",
        json!({"name": "future"}),
    )
    .await;
    let channel_id = channel["id"].as_str().expect("channel id");
    let (status, workspace) = json_request(
        app.clone(),
        "POST",
        "/api/workspaces",
        json!({
            "provider_key": "vendor.future",
            "display_name": "Future provider",
            "portable_locator": "vendor://portable",
            "metadata": {"opaque": {"keep": true}}
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(workspace["provider_key"], "vendor.future");
    assert!(workspace.get("kind").is_none());
    assert!(workspace.get("locator").is_none());
    assert_eq!(workspace["metadata"]["opaque"]["keep"], true);
    let workspace_id = workspace["id"].as_str().expect("workspace id");
    let (attach_status, _) = json_request(
        app.clone(),
        "POST",
        &format!("/api/channels/{channel_id}/workspaces/{workspace_id}"),
        json!({}),
    )
    .await;
    assert_eq!(attach_status, StatusCode::CREATED);
    let (_, binding) = json_request(
        app.clone(),
        "PUT",
        &format!("/api/workspaces/{workspace_id}/binding"),
        json!({"local_locator": "vendor-local://opaque"}),
    )
    .await;
    assert_eq!(binding["state"], "unavailable");
    assert_eq!(binding["error_code"], "provider_unavailable");

    let (channel_status, loaded_channel) = json_request(
        app,
        "GET",
        &format!("/api/channels/{channel_id}/workspaces"),
        json!({}),
    )
    .await;
    assert_eq!(channel_status, StatusCode::OK);
    assert_eq!(loaded_channel[0]["metadata"]["opaque"]["keep"], true);
}
