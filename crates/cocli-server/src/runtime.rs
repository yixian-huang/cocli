use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use cocli_agent::state::Idle;
use cocli_agent::{AgentActor, StartCfg};
use cocli_api::{RuntimeError, RuntimeInfo, RuntimeService};
use cocli_driver_claude::ClaudeDriver;
use cocli_driver_codex::CodexDriver;
use cocli_driver_core::types::TurnStatus;
use cocli_driver_core::{Driver, DriverEvent};
use cocli_driver_cursor::CursorDriver;
use cocli_driver_gemini::GeminiDriver;
use cocli_protocol::types::DeliveryMessage;
use cocli_protocol::AgentDeliverMsg;
use cocli_runtime_pool::{
    initial_oss_runtime_specs, RuntimeCapabilities, RuntimeCatalog, RuntimeCatalogEntry,
    RuntimeProbe, RuntimeRegistry, SystemRuntimeProbe,
};
use cocli_store::{Agent, Message};
use tokio::sync::{broadcast, mpsc, Mutex};
use uuid::Uuid;

const INITIALIZATION_PROMPT: &str = "\
You are a local cocli agent. Reply to tasks in plain text. \
Do not call messaging or collaboration tools. \
For this initialization turn, reply with exactly READY.";

/// Inputs used to discover and run local CLI runtimes.
#[derive(Clone, Debug)]
pub struct LocalRuntimeConfig {
    /// Parent directory for per-agent runtime workspaces.
    pub workspace_root: PathBuf,
    /// Loopback URL passed to the optional MCP bridge.
    pub server_url: String,
    /// Local bridge capability token.
    pub auth_token: String,
    /// Maximum time allowed for one initialization or task turn.
    pub turn_timeout: Duration,
}

impl LocalRuntimeConfig {
    /// Creates a local runtime configuration with a two-minute turn budget.
    pub fn new(workspace_root: PathBuf, server_url: String) -> Self {
        Self {
            workspace_root,
            server_url,
            auth_token: String::new(),
            turn_timeout: Duration::from_secs(120),
        }
    }
}

/// Runtime service backed by the shared registry and `AgentActor` lifecycle.
pub struct LocalRuntimeService {
    registry: Arc<RuntimeRegistry>,
    catalog: Vec<RuntimeInfo>,
    config: LocalRuntimeConfig,
    execution_lock: Mutex<()>,
}

impl LocalRuntimeService {
    /// Discovers the OSS runtime matrix from `PATH`.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeSetupError`] when the current executable path cannot
    /// be inspected while resolving the optional bridge binary.
    pub fn discover(config: LocalRuntimeConfig) -> Result<Self, RuntimeSetupError> {
        let probe = SystemRuntimeProbe::from_environment();
        let bridge = resolve_bridge(&probe)?;
        let specs = initial_oss_runtime_specs();
        let mut registry = RuntimeRegistry::new();

        for spec in &specs {
            let Some(binary) = probe.resolve_binary(&spec.command) else {
                continue;
            };
            if let Some(driver) = create_driver(&spec.name, binary, bridge.clone()) {
                registry.register(driver);
            }
        }

        let catalog = registry.discover(&specs, &probe);
        Ok(Self::from_catalog(registry, catalog, config))
    }

    fn from_catalog(
        registry: RuntimeRegistry,
        catalog: RuntimeCatalog,
        config: LocalRuntimeConfig,
    ) -> Self {
        Self {
            registry: Arc::new(registry),
            catalog: catalog.runtimes.iter().map(runtime_info).collect(),
            config,
            execution_lock: Mutex::new(()),
        }
    }

    async fn run_reply(&self, agent: &Agent, message: &Message) -> Result<String, RuntimeError> {
        let (_command_tx, command_rx) = mpsc::channel(4);
        let (outbound_tx, _outbound_rx) = mpsc::channel(16);
        let (state_tx, _state_rx) = mpsc::channel(4);
        let (obs_tx, _obs_rx) = broadcast::channel(16);
        let actor = AgentActor::<Idle> {
            id: agent.id.to_string(),
            mailbox: command_rx,
            outbound: outbound_tx,
            state_tx,
            obs_tx,
            state: Idle,
        };
        let start = actor.start(StartCfg {
            registry: Arc::clone(&self.registry),
            runtime_name: agent.runtime.clone(),
            workspace_root: self.config.workspace_root.clone(),
            server_url: self.config.server_url.clone(),
            auth_token: self.config.auth_token.clone(),
            channel_id: agent.channel_id,
            channel_name: "local".to_owned(),
            model: agent.model.clone().unwrap_or_default(),
            launch_id: Uuid::new_v4().to_string(),
            resume_session: None,
            system_prompt: INITIALIZATION_PROMPT.to_owned(),
            env_vars: HashMap::new(),
        });
        let mut running = tokio::time::timeout(self.config.turn_timeout, start)
            .await
            .map_err(|_| RuntimeError::Delivery("runtime startup timed out".to_owned()))?
            .map_err(|error| RuntimeError::Delivery(error.to_string()))?;

        if let Err(error) = wait_for_turn(&mut running, self.config.turn_timeout, false).await {
            let _stopping = running.stop(true);
            return Err(error);
        }

        let delivery = AgentDeliverMsg {
            agent_id: agent.id.to_string(),
            message: DeliveryMessage {
                channel_id: agent.channel_id,
                sender_name: "user".to_owned(),
                sender_type: "user".to_owned(),
                content: message.content.clone(),
                channel_name: "local".to_owned(),
                channel_type: "channel".to_owned(),
                ..Default::default()
            },
            ..Default::default()
        };
        if let Err(error) = running.deliver(delivery).await {
            let _stopping = running.stop(true);
            return Err(RuntimeError::Delivery(error));
        }

        let reply = wait_for_turn(&mut running, self.config.turn_timeout, true).await;
        let _stopping = running.stop(true);
        reply
    }
}

#[async_trait]
impl RuntimeService for LocalRuntimeService {
    async fn list(&self) -> Vec<RuntimeInfo> {
        self.catalog.clone()
    }

    async fn reply(&self, agent: &Agent, message: &Message) -> Result<String, RuntimeError> {
        let _guard = self.execution_lock.lock().await;
        self.run_reply(agent, message).await
    }
}

async fn wait_for_turn(
    running: &mut AgentActor<cocli_agent::state::Running>,
    timeout: Duration,
    collect_text: bool,
) -> Result<String, RuntimeError> {
    tokio::time::timeout(timeout, async {
        let mut text = String::new();
        let mut last_error = None;
        while let Some(event) = running.state.event_rx.recv().await {
            match event {
                DriverEvent::TextDelta { text: delta } if collect_text => text.push_str(&delta),
                DriverEvent::Error { message, .. } => last_error = Some(message),
                DriverEvent::TurnEnd { status, .. } => {
                    if matches!(status, TurnStatus::Failed | TurnStatus::Cancelled) {
                        return Err(RuntimeError::Delivery(
                            last_error.unwrap_or_else(|| format!("runtime turn ended: {status:?}")),
                        ));
                    }
                    if collect_text && text.trim().is_empty() {
                        return Err(RuntimeError::Delivery(last_error.unwrap_or_else(|| {
                            "runtime completed without a text reply".to_owned()
                        })));
                    }
                    return Ok(text);
                }
                DriverEvent::Write { data } => running
                    .write_driver_request(&data)
                    .await
                    .map_err(RuntimeError::Delivery)?,
                _ => {}
            }
        }
        Err(RuntimeError::Delivery(last_error.unwrap_or_else(|| {
            "runtime exited before completing the turn".to_owned()
        })))
    })
    .await
    .map_err(|_| RuntimeError::Delivery("runtime turn timed out".to_owned()))?
}

fn resolve_bridge(probe: &dyn RuntimeProbe) -> Result<PathBuf, RuntimeSetupError> {
    if let Some(binary) = probe.resolve_binary(Path::new("cocli-bridge")) {
        return Ok(binary);
    }
    let sibling = std::env::current_exe()
        .map_err(RuntimeSetupError::CurrentExecutable)?
        .with_file_name("cocli-bridge");
    Ok(sibling)
}

fn create_driver(name: &str, binary: PathBuf, bridge: PathBuf) -> Option<Arc<dyn Driver>> {
    match name {
        "claude" => Some(Arc::new(ClaudeDriver::new(binary, bridge))),
        "cursor" => Some(Arc::new(CursorDriver::new(binary, bridge))),
        "codex" => Some(Arc::new(CodexDriver::new(binary, bridge))),
        "gemini" => Some(Arc::new(GeminiDriver::new(binary, bridge))),
        _ => None,
    }
}

fn runtime_info(entry: &RuntimeCatalogEntry) -> RuntimeInfo {
    RuntimeInfo {
        name: entry.name.clone(),
        installed: entry.installed && entry.unavailable_reason.is_none(),
        binary: entry
            .binary
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned()),
        version: entry.version.clone(),
        models: entry.models.iter().map(|model| model.id.clone()).collect(),
        capabilities: entry
            .capabilities
            .as_ref()
            .map(capability_names)
            .unwrap_or_default(),
        unavailable_reason: entry.unavailable_reason.clone(),
    }
}

fn capability_names(capabilities: &RuntimeCapabilities) -> Vec<String> {
    let mut names = vec![
        format!("delivery:{}", capabilities.busy_delivery_mode),
        format!("env:{}", capabilities.env_propagation),
        format!("skills:{}", capabilities.skill_compatibility),
    ];
    if capabilities.supports_turn_cancel {
        names.push("turn_cancel".to_owned());
    }
    if capabilities.supports_turn_steer {
        names.push("turn_steer".to_owned());
    }
    if capabilities.supports_thread_fork {
        names.push("thread_fork".to_owned());
    }
    if capabilities.is_turn_exit {
        names.push("turn_exit".to_owned());
    }
    names
}

/// Errors raised while assembling the local runtime service.
#[derive(Debug, thiserror::Error)]
pub enum RuntimeSetupError {
    /// The current executable path could not be inspected.
    #[error("failed to inspect the current executable while resolving cocli-bridge")]
    CurrentExecutable(#[source] std::io::Error),
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::process::Stdio;

    use cocli_driver_core::types::{
        BusyDeliveryMode, DriverAgentConfig, EnvPropagation, MessageMode, SkillCompatibility,
        SpawnConfig,
    };
    use cocli_driver_core::{DriverError, DriverEvent};
    use cocli_pidfile::TestPidDirGuard;
    use cocli_runtime_pool::{RuntimeModel, RuntimeSpec};
    use tempfile::tempdir;
    use tokio::process::Command;

    use super::*;

    struct FakeDriver;

    #[async_trait]
    impl Driver for FakeDriver {
        fn name(&self) -> &str {
            "fake"
        }

        fn mcp_tool_prefix(&self) -> &str {
            ""
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
                    session_id: "session-1".to_owned(),
                }]
            } else if let Some(text) = line.strip_prefix("text:") {
                vec![DriverEvent::TextDelta {
                    text: text.to_owned(),
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
            Some(text.to_owned())
        }

        fn supports_turn_cancel(&self) -> bool {
            true
        }

        fn skill_search_paths(&self, _workspace: &Path) -> Vec<PathBuf> {
            Vec::new()
        }
    }

    struct BootstrapWriteDriver;

    #[async_trait]
    impl Driver for BootstrapWriteDriver {
        fn name(&self) -> &str {
            "bootstrap-write"
        }

        fn mcp_tool_prefix(&self) -> &str {
            ""
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
                vec![
                    DriverEvent::SessionStarted {
                        session_id: "session-2".to_owned(),
                    },
                    DriverEvent::Write {
                        data: "bootstrap".to_owned(),
                    },
                ]
            } else {
                FakeDriver.parse_event(line)
            }
        }

        fn encode_stdin_message(
            &self,
            text: &str,
            _session_id: Option<&str>,
            _mode: MessageMode,
        ) -> Option<String> {
            Some(text.to_owned())
        }

        fn supports_turn_cancel(&self) -> bool {
            true
        }

        fn skill_search_paths(&self, _workspace: &Path) -> Vec<PathBuf> {
            Vec::new()
        }
    }

    #[tokio::test]
    async fn completes_a_reply_through_agent_actor() {
        let temp = tempdir().expect("temp directory");
        let _pid_guard = TestPidDirGuard::new(&temp.path().join("pids"));
        let mut registry = RuntimeRegistry::new();
        registry.register(Arc::new(FakeDriver));
        let catalog = registry.discover(
            &[RuntimeSpec::new("fake", "/bin/sh")
                .with_models(vec![RuntimeModel::new("test-model", "Test Model")])],
            &SystemRuntimeProbe::with_path(None),
        );
        let service = LocalRuntimeService::from_catalog(
            registry,
            catalog,
            LocalRuntimeConfig {
                workspace_root: temp.path().join("workspaces"),
                server_url: "http://127.0.0.1:8090".to_owned(),
                auth_token: String::new(),
                turn_timeout: Duration::from_secs(2),
            },
        );
        let agent = Agent {
            id: Uuid::new_v4(),
            channel_id: Uuid::new_v4(),
            name: "builder".to_owned(),
            runtime: "fake".to_owned(),
            model: Some("test-model".to_owned()),
            status: cocli_store::AgentStatus::Running,
            created_at: chrono::Utc::now(),
        };
        let message = Message {
            id: Uuid::new_v4(),
            channel_id: agent.channel_id,
            seq: 1,
            agent_id: None,
            role: cocli_store::MessageRole::User,
            content: "ship it".to_owned(),
            created_at: chrono::Utc::now(),
        };

        let reply = service
            .reply(&agent, &message)
            .await
            .expect("runtime reply");

        assert!(reply.contains("ship it"));
        assert_eq!(service.list().await[0].models, vec!["test-model"]);
    }

    #[tokio::test]
    async fn executes_driver_requested_bootstrap_write_before_waiting_for_turn() {
        let temp = tempdir().expect("temp directory");
        let _pid_guard = TestPidDirGuard::new(&temp.path().join("pids"));
        let mut registry = RuntimeRegistry::new();
        registry.register(Arc::new(BootstrapWriteDriver));
        let service = LocalRuntimeService::from_catalog(
            registry,
            RuntimeCatalog::default(),
            LocalRuntimeConfig {
                workspace_root: temp.path().join("workspaces"),
                server_url: "http://127.0.0.1:8090".to_owned(),
                auth_token: String::new(),
                turn_timeout: Duration::from_secs(2),
            },
        );
        let agent = Agent {
            id: Uuid::new_v4(),
            channel_id: Uuid::new_v4(),
            name: "builder".to_owned(),
            runtime: "bootstrap-write".to_owned(),
            model: Some("test-model".to_owned()),
            status: cocli_store::AgentStatus::Running,
            created_at: chrono::Utc::now(),
        };
        let message = Message {
            id: Uuid::new_v4(),
            channel_id: agent.channel_id,
            seq: 1,
            agent_id: None,
            role: cocli_store::MessageRole::User,
            content: "ship it".to_owned(),
            created_at: chrono::Utc::now(),
        };

        let reply = service
            .reply(&agent, &message)
            .await
            .expect("runtime reply");

        assert!(reply.contains("ship it"));
    }
}
