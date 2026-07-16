use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use chrono::{Duration as ChronoDuration, Utc};
use cocli_api::{
    router, router_with_delivery_config, DeliveryConfig, RuntimeError, RuntimeInfo, RuntimeService,
    RuntimeSkill, RuntimeSkillCompatibility, RuntimeSkillFileContent, RuntimeSkillFileEntry,
};
use cocli_store::{
    Agent, AgentStatus, Message, MessageRole, NewAgentTurn, SkillLibraryFile, Store,
};
use serde_json::{json, Value};
use tempfile::tempdir;
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
struct FakeSkillRuntime {
    installs: Mutex<HashMap<(uuid::Uuid, String), Vec<SkillLibraryFile>>>,
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
async fn wiki_routes_match_browser_contract_and_preserve_history() {
    let store = Store::in_memory().await.expect("store should open");
    let app = router(store, Arc::new(FakeRuntime));
    let target_path = "roadmap%2Flocal-loop";
    let source_path = "notes%2Fimplementation";

    let (created_status, created) = json_request(
        app.clone(),
        "PUT",
        &format!("/api/wiki/pages/{target_path}"),
        json!({
            "title": "Local Loop",
            "content": "# Local Loop\n\nInitial.",
            "tags": ["roadmap", "local"],
            "updatedBy": "planner"
        }),
    )
    .await;
    assert_eq!(created_status, StatusCode::OK);
    assert_eq!(created["version"], 1);
    assert_eq!(created["path"], "roadmap/local-loop");

    let (source_status, _) = json_request(
        app.clone(),
        "PUT",
        &format!("/api/wiki/pages/{source_path}"),
        json!({
            "title": "Implementation",
            "content": "See [[roadmap/local-loop]].",
            "tags": ["notes"]
        }),
    )
    .await;
    assert_eq!(source_status, StatusCode::OK);

    let (list_status, listed) = json_request(
        app.clone(),
        "GET",
        "/api/wiki/pages?q=Local&tag=roadmap",
        json!({}),
    )
    .await;
    assert_eq!(list_status, StatusCode::OK);
    assert_eq!(listed["pages"].as_array().map(Vec::len), Some(1));
    assert_eq!(listed["pages"][0]["title"], "Local Loop");

    let (_, updated) = json_request(
        app.clone(),
        "PUT",
        &format!("/api/wiki/pages/{target_path}"),
        json!({
            "title": "Local Product Loop",
            "content": "# Local Product Loop\n\nComplete.",
            "tags": ["roadmap"],
            "ifVersion": 1
        }),
    )
    .await;
    assert_eq!(updated["version"], 2);

    let (conflict_status, conflict) = json_request(
        app.clone(),
        "PUT",
        &format!("/api/wiki/pages/{target_path}"),
        json!({
            "title": "Stale",
            "content": "stale",
            "ifVersion": 1
        }),
    )
    .await;
    assert_eq!(conflict_status, StatusCode::CONFLICT);
    assert!(conflict["error"]
        .as_str()
        .is_some_and(|error| error.contains("current=2")));

    let (revision_status, revisions) = json_request(
        app.clone(),
        "GET",
        &format!("/api/wiki/pages/{target_path}/revisions"),
        json!({}),
    )
    .await;
    assert_eq!(revision_status, StatusCode::OK);
    assert_eq!(revisions["revisions"][0]["version"], 2);
    assert_eq!(revisions["revisions"][1]["version"], 1);

    let (backlink_status, backlinks) = json_request(
        app.clone(),
        "GET",
        &format!("/api/wiki/pages/{target_path}/backlinks"),
        json!({}),
    )
    .await;
    assert_eq!(backlink_status, StatusCode::OK);
    assert_eq!(backlinks["backlinks"][0]["path"], "notes/implementation");

    let (revert_status, reverted) = json_request(
        app.clone(),
        "POST",
        &format!("/api/wiki/pages/{target_path}/revert"),
        json!({"version": 1}),
    )
    .await;
    assert_eq!(revert_status, StatusCode::OK);
    assert_eq!(reverted["page"]["version"], 3);
    assert_eq!(reverted["page"]["title"], "Local Loop");

    let (get_status, page) = json_request(
        app,
        "GET",
        &format!("/api/wiki/pages/{target_path}"),
        json!({}),
    )
    .await;
    assert_eq!(get_status, StatusCode::OK);
    assert_eq!(page["content"], "# Local Loop\n\nInitial.");
}

#[tokio::test]
async fn memory_routes_support_private_shared_write_read_and_move() {
    let store = Store::in_memory().await.expect("store should open");
    let app = router(store, Arc::new(FakeRuntime));
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

    let (write_status, written) = json_request(
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

    let (move_status, moved) = json_request(
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

    let (list_status, namespace) = json_request(
        app.clone(),
        "GET",
        &format!("/api/bridge/agents/{agent_id}/memory/list?scope=channel&channel_id={channel_id}"),
        json!({}),
    )
    .await;
    assert_eq!(list_status, StatusCode::OK);
    assert_eq!(namespace["entries"].as_array().map(Vec::len), Some(2));

    let (wiki_status, wiki_pages) =
        json_request(app.clone(), "GET", "/api/wiki/pages", json!({})).await;
    assert_eq!(wiki_status, StatusCode::OK);
    assert_eq!(wiki_pages["pages"].as_array().map(Vec::len), Some(0));

    let (_, other_channel) = json_request(
        app.clone(),
        "POST",
        "/api/channels",
        json!({"name": "not-a-member"}),
    )
    .await;
    let other_channel_id = other_channel["id"].as_str().expect("other channel id");
    let (forbidden_status, _) = json_request(
        app,
        "GET",
        &format!(
            "/api/bridge/agents/{agent_id}/memory/index?scope=channel&channel_id={other_channel_id}"
        ),
        json!({}),
    )
    .await;
    assert_eq!(forbidden_status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn bridge_wiki_routes_attribute_agent_writes_and_search_pages() {
    let store = Store::in_memory().await.expect("store should open");
    let app = router(store, Arc::new(FakeRuntime));
    let (_, channel) = json_request(
        app.clone(),
        "POST",
        "/api/channels",
        json!({"name": "wiki-bridge"}),
    )
    .await;
    let channel_id = channel["id"].as_str().expect("channel id");
    let (_, agent) = json_request(
        app.clone(),
        "POST",
        "/api/agents",
        json!({
            "channel_id": channel_id,
            "name": "wiki-writer",
            "runtime": "fake",
            "model": "test-model"
        }),
    )
    .await;
    let agent_id = agent["id"].as_str().expect("agent id");
    let encoded_path = "reference%2Fbridge-api";

    let (write_status, written) = json_request(
        app.clone(),
        "PUT",
        &format!("/api/bridge/agents/{agent_id}/wiki/pages/{encoded_path}"),
        json!({
            "title": "Bridge API",
            "content_md": "# Bridge API\n\nDurable.",
            "tags": ["reference"],
            "reason": "record contract"
        }),
    )
    .await;
    assert_eq!(write_status, StatusCode::OK);
    assert_eq!(written["updatedBy"], "wiki-writer");
    assert_eq!(written["path"], "reference/bridge-api");

    let (search_status, search) = json_request(
        app.clone(),
        "GET",
        &format!("/api/bridge/agents/{agent_id}/wiki/pages?q=Durable&limit=10"),
        json!({}),
    )
    .await;
    assert_eq!(search_status, StatusCode::OK);
    assert_eq!(search["pages"].as_array().map(Vec::len), Some(1));

    let (read_status, read) = json_request(
        app,
        "GET",
        &format!("/api/bridge/agents/{agent_id}/wiki/pages/{encoded_path}"),
        json!({}),
    )
    .await;
    assert_eq!(read_status, StatusCode::OK);
    assert_eq!(read["content"], "# Bridge API\n\nDurable.");
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
    let app = router(store, Arc::new(FakeRuntime));
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

    let (send_status, sent) = json_request(
        app.clone(),
        "POST",
        &format!("/api/bridge/agents/{first_id}/messages"),
        json!({"target": "#bridge", "content": "peer update"}),
    )
    .await;
    assert_eq!(send_status, StatusCode::CREATED);
    assert_eq!(sent["content"], "peer update");
    assert_eq!(sent["agent_id"], first_id);

    let (_, inbox) = json_request(
        app.clone(),
        "GET",
        &format!("/api/bridge/agents/{second_id}/inbox?limit=10"),
        json!({}),
    )
    .await;
    assert_eq!(inbox["messages"].as_array().map(Vec::len), Some(1));
    assert_eq!(inbox["messages"][0]["content"], "peer update");
    let (_, consumed) = json_request(
        app.clone(),
        "GET",
        &format!("/api/bridge/agents/{second_id}/inbox?limit=10"),
        json!({}),
    )
    .await;
    assert_eq!(consumed["messages"], json!([]));
    let (_, own_inbox) = json_request(
        app.clone(),
        "GET",
        &format!("/api/bridge/agents/{first_id}/inbox"),
        json!({}),
    )
    .await;
    assert_eq!(own_inbox["messages"], json!([]));

    let (_, history) = json_request(
        app.clone(),
        "GET",
        &format!("/api/bridge/agents/{second_id}/history?limit=10"),
        json!({}),
    )
    .await;
    assert_eq!(history["channel"]["name"], "bridge");
    assert_eq!(history["messages"][0]["content"], "peer update");

    let (create_tasks_status, created_tasks) = json_request(
        app.clone(),
        "POST",
        &format!("/api/bridge/agents/{second_id}/tasks"),
        json!({"tasks": [{"title": "prepare"}, {"title": "ship"}]}),
    )
    .await;
    assert_eq!(create_tasks_status, StatusCode::CREATED);
    assert_eq!(created_tasks["tasks"][0]["taskNumber"], 1);
    assert_eq!(created_tasks["tasks"][1]["taskNumber"], 2);
    let (_, dependency) = json_request(
        app.clone(),
        "POST",
        &format!("/api/bridge/agents/{second_id}/tasks/dependencies"),
        json!({"task_number": 2, "depends_on": 1}),
    )
    .await;
    assert_eq!(dependency["dependsOn"], json!([1]));
    let (_, blocked_claim) = json_request(
        app.clone(),
        "POST",
        &format!("/api/bridge/agents/{second_id}/tasks/claim"),
        json!({"task_numbers": [2]}),
    )
    .await;
    assert_eq!(blocked_claim["results"][0]["success"], false);
    assert!(blocked_claim["results"][0]["reason"]
        .as_str()
        .expect("claim reason")
        .contains("unmet dependencies"));
    let (_, first_claim) = json_request(
        app.clone(),
        "POST",
        &format!("/api/bridge/agents/{second_id}/tasks/claim"),
        json!({"task_numbers": [1]}),
    )
    .await;
    assert_eq!(first_claim["results"][0]["success"], true);
    let (_, completed) = json_request(
        app.clone(),
        "POST",
        &format!("/api/bridge/agents/{second_id}/tasks/update-status"),
        json!({"task_number": 1, "status": "done", "progress": "verified"}),
    )
    .await;
    assert_eq!(completed["status"], "done");
    let (_, second_claim) = json_request(
        app.clone(),
        "POST",
        &format!("/api/bridge/agents/{second_id}/tasks/claim"),
        json!({"task_numbers": [2]}),
    )
    .await;
    assert_eq!(second_claim["results"][0]["success"], true);
    let (_, tasks) = json_request(
        app.clone(),
        "GET",
        &format!("/api/bridge/agents/{second_id}/tasks?status=all"),
        json!({}),
    )
    .await;
    assert_eq!(tasks["tasks"].as_array().map(Vec::len), Some(2));
    let (_, message_claim) = json_request(
        app.clone(),
        "POST",
        &format!("/api/bridge/agents/{second_id}/tasks/claim"),
        json!({"message_ids": [sent["id"]]}),
    )
    .await;
    assert_eq!(message_claim["results"][0]["success"], true);
    assert_eq!(message_claim["results"][0]["created"], true);
    assert_eq!(message_claim["results"][0]["task"]["messageId"], sent["id"]);

    let (_, working) = json_request(
        app.clone(),
        "POST",
        &format!("/api/bridge/agents/{second_id}/working"),
        json!({
            "summary": "implement MCP",
            "channelName": "bridge",
            "taskNumber": 3,
            "nextStepHint": "run protocol tests"
        }),
    )
    .await;
    assert_eq!(working["state"]["summary"], "implement MCP");
    assert_eq!(working["state"]["task_number"], 3);
    let (_, current) = json_request(
        app.clone(),
        "GET",
        &format!("/api/bridge/agents/{second_id}/working"),
        json!({}),
    )
    .await;
    assert_eq!(current["state"]["next_step_hint"], "run protocol tests");
    let (_, cleared) = json_request(
        app.clone(),
        "POST",
        &format!("/api/bridge/agents/{second_id}/working/clear"),
        json!({}),
    )
    .await;
    assert_eq!(cleared["cleared"], true);
    let (_, empty) = json_request(
        app,
        "GET",
        &format!("/api/bridge/agents/{second_id}/working"),
        json!({}),
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
