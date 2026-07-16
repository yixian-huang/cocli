#![cfg(unix)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;

use cocli_agent::state::Idle;
use cocli_agent::{AgentActor, StartCfg};
use cocli_driver_core::types::{
    BusyDeliveryMode, DriverAgentConfig, EnvPropagation, MessageMode, SkillCompatibility,
    SpawnConfig, TurnStatus,
};
use cocli_driver_core::{Driver, DriverError, DriverEvent};
use cocli_pidfile::TestPidDirGuard;
use cocli_protocol::types::DeliveryMessage;
use cocli_protocol::AgentDeliverMsg;
use cocli_runtime_pool::RuntimeRegistry;
use tokio::process::Command;
use tokio::sync::{broadcast, mpsc};

struct FakeLifecycleDriver;

#[async_trait::async_trait]
impl Driver for FakeLifecycleDriver {
    fn name(&self) -> &str {
        "fake"
    }

    fn mcp_tool_prefix(&self) -> &str {
        "mcp__fake__"
    }

    fn busy_delivery_mode(&self) -> BusyDeliveryMode {
        BusyDeliveryMode::Direct
    }

    fn env_propagation(&self) -> EnvPropagation {
        EnvPropagation::Inherit
    }

    fn skill_compatibility(&self) -> SkillCompatibility {
        SkillCompatibility::Supported
    }

    fn prepare_workspace(
        &self,
        work_dir: &Path,
        _config: &DriverAgentConfig,
        _agent_id: &str,
        _system_prompt: &str,
    ) -> Result<(), DriverError> {
        std::fs::write(work_dir.join("prepared"), b"yes").map_err(DriverError::Io)
    }

    fn spawn(&self, _cfg: &SpawnConfig) -> Result<tokio::process::Child, DriverError> {
        Command::new("/bin/sh")
            .arg("-c")
            .arg(
                "printf 'session\\n'; while IFS= read -r line; do printf 'text:%s\\n' \"$line\"; printf 'turn\\n'; done",
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(DriverError::Io)
    }

    fn parse_event(&self, line: &str) -> Vec<DriverEvent> {
        if line == "session" {
            vec![DriverEvent::SessionStarted {
                session_id: "fake-session".to_string(),
            }]
        } else if let Some(text) = line.strip_prefix("text:") {
            vec![DriverEvent::TextDelta {
                text: text.to_string(),
            }]
        } else if line == "turn" {
            vec![DriverEvent::TurnEnd {
                status: TurnStatus::Completed,
                input_tokens: 1,
                output_tokens: 1,
                cost_usd: 0.0,
                cache_creation_tokens: 0,
                cache_read_tokens: 0,
                context_window_tokens: 1024,
            }]
        } else {
            vec![DriverEvent::Unknown]
        }
    }

    fn encode_stdin_message(
        &self,
        text: &str,
        _session_id: Option<&str>,
        _mode: MessageMode,
    ) -> Option<String> {
        Some(text.to_string())
    }

    fn supports_turn_cancel(&self) -> bool {
        true
    }

    fn skill_search_paths(&self, _workspace: &Path) -> Vec<PathBuf> {
        Vec::new()
    }
}

struct PromptBeforeSessionDriver;

#[async_trait::async_trait]
impl Driver for PromptBeforeSessionDriver {
    fn name(&self) -> &str {
        "prompt-before-session"
    }

    fn mcp_tool_prefix(&self) -> &str {
        "mcp__prompt_before_session__"
    }

    fn busy_delivery_mode(&self) -> BusyDeliveryMode {
        BusyDeliveryMode::Direct
    }

    fn requires_initial_prompt(&self) -> bool {
        true
    }

    fn env_propagation(&self) -> EnvPropagation {
        EnvPropagation::Inherit
    }

    fn skill_compatibility(&self) -> SkillCompatibility {
        SkillCompatibility::Supported
    }

    fn prepare_workspace(
        &self,
        _work_dir: &Path,
        _config: &DriverAgentConfig,
        _agent_id: &str,
        _system_prompt: &str,
    ) -> Result<(), DriverError> {
        Ok(())
    }

    fn spawn(&self, _cfg: &SpawnConfig) -> Result<tokio::process::Child, DriverError> {
        Command::new("/bin/sh")
            .arg("-c")
            .arg(
                "IFS= read -r line; printf 'session\\n'; printf 'text:%s\\n' \"$line\"; printf 'turn\\n'; while IFS= read -r line; do printf 'text:%s\\n' \"$line\"; printf 'turn\\n'; done",
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(DriverError::Io)
    }

    fn parse_event(&self, line: &str) -> Vec<DriverEvent> {
        FakeLifecycleDriver.parse_event(line)
    }

    fn encode_stdin_message(
        &self,
        text: &str,
        _session_id: Option<&str>,
        _mode: MessageMode,
    ) -> Option<String> {
        Some(text.to_string())
    }

    fn supports_turn_cancel(&self) -> bool {
        true
    }

    fn skill_search_paths(&self, _workspace: &Path) -> Vec<PathBuf> {
        Vec::new()
    }
}

#[tokio::test(flavor = "current_thread")]
async fn actor_uses_core_driver_for_prepare_spawn_parse_and_delivery() {
    let temp = tempfile::tempdir().unwrap();
    let _pid_guard = TestPidDirGuard::new(temp.path().join("pids").as_path());
    let mut registry = RuntimeRegistry::new();
    registry.register(Arc::new(FakeLifecycleDriver));

    let (_command_tx, command_rx) = mpsc::channel(4);
    let (outbound_tx, _outbound_rx) = mpsc::channel(4);
    let (state_tx, _state_rx) = mpsc::channel(4);
    let (obs_tx, _obs_rx) = broadcast::channel(4);
    let actor = AgentActor::<Idle> {
        id: "agent-core-contract".to_string(),
        mailbox: command_rx,
        outbound: outbound_tx,
        state_tx,
        obs_tx,
        state: Idle,
    };
    let mut running = actor
        .start(StartCfg {
            registry: Arc::new(registry),
            runtime_name: "fake".to_string(),
            workspace_root: temp.path().join("workspaces"),
            server_url: "ws://localhost:8080".to_string(),
            auth_token: "test-token".to_string(),
            channel_id: uuid::Uuid::nil(),
            channel_name: String::new(),
            model: "fake-model".to_string(),
            launch_id: "launch-1".to_string(),
            resume_session: None,
            system_prompt: String::new(),
            env_vars: HashMap::new(),
        })
        .await
        .unwrap();

    assert_eq!(running.state.session_id, "fake-session");
    assert!(temp
        .path()
        .join("workspaces/agent-core-contract/prepared")
        .exists());

    running
        .deliver(AgentDeliverMsg {
            agent_id: "agent-core-contract".to_string(),
            message: DeliveryMessage {
                sender_name: "alice".to_string(),
                sender_type: "user".to_string(),
                content: "hello".to_string(),
                channel_name: "test".to_string(),
                ..Default::default()
            },
            ..Default::default()
        })
        .await
        .unwrap();

    let event = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        running.state.event_rx.recv(),
    )
    .await
    .unwrap()
    .unwrap();
    match event {
        DriverEvent::TextDelta { text } => {
            assert_eq!(text, "[test] alice: hello");
        }
        other => panic!("unexpected event: {other:?}"),
    }

    let _stopping = running.stop(true);
}

#[tokio::test(flavor = "current_thread")]
async fn actor_sends_required_initial_prompt_before_waiting_for_session() {
    let temp = tempfile::tempdir().unwrap();
    let _pid_guard = TestPidDirGuard::new(temp.path().join("pids").as_path());
    let mut registry = RuntimeRegistry::new();
    registry.register(Arc::new(PromptBeforeSessionDriver));

    let (_command_tx, command_rx) = mpsc::channel(4);
    let (outbound_tx, _outbound_rx) = mpsc::channel(4);
    let (state_tx, _state_rx) = mpsc::channel(4);
    let (obs_tx, _obs_rx) = broadcast::channel(4);
    let actor = AgentActor::<Idle> {
        id: "agent-prompt-before-session".to_string(),
        mailbox: command_rx,
        outbound: outbound_tx,
        state_tx,
        obs_tx,
        state: Idle,
    };

    let mut running = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        actor.start(StartCfg {
            registry: Arc::new(registry),
            runtime_name: "prompt-before-session".to_string(),
            workspace_root: temp.path().join("workspaces"),
            server_url: "ws://localhost:8080".to_string(),
            auth_token: "test-token".to_string(),
            channel_id: uuid::Uuid::nil(),
            channel_name: String::new(),
            model: "fake-model".to_string(),
            launch_id: "launch-2".to_string(),
            resume_session: None,
            system_prompt: "bootstrap".to_string(),
            env_vars: HashMap::new(),
        }),
    )
    .await
    .expect("actor should not wait for a session before writing the initial prompt")
    .unwrap();

    let event = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        running.state.event_rx.recv(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(matches!(
        event,
        DriverEvent::TextDelta { text } if text == "bootstrap"
    ));

    let _stopping = running.stop(true);
}
