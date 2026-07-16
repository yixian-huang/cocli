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
    let body = serde_json::from_slice(&bytes).expect("response should be JSON");
    (status, body)
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
