use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use cocli_api::{
    router, router_with_delivery_config, DeliveryConfig, RuntimeError, RuntimeInfo, RuntimeService,
};
use cocli_store::{Agent, AgentStatus, Message, MessageRole, Store};
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

#[derive(Debug, Default)]
struct FlakyRuntime {
    calls: AtomicUsize,
}

#[derive(Debug, Default)]
struct PanicOnceRuntime {
    calls: AtomicUsize,
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
    assert_eq!(posted["pending_deliveries"][0]["state"], "pending");
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
