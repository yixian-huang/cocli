//! `KimiDriver` — persistent wire driver for kimi-code CLI.
//!
//! Kimi wire mode is JSON-RPC over stdio. The registry-level `KimiDriver`
//! is a factory; each spawned process owns its session id, request counter,
//! and stdin pipe.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use cocli_driver_core::subtraits::{
    ExitCodeClassifier, ProcessFactory, ProcessInitializer, StdinBinder, TurnInterruptor,
};
use cocli_driver_core::types::{
    BusyDeliveryMode, DriverAgentConfig, EnvPropagation, ExitCodeClass, MessageMode,
    SkillCompatibility, SpawnConfig,
};
use cocli_driver_core::{Driver, DriverError, DriverEvent};
use tokio::io::AsyncWriteExt;

use crate::events::parse_line;
use crate::spawn::{spawn_kimi, write_kimi_agents_md, SpawnContext};

const KIMI_WIRE_PROTOCOL_VERSION: &str = "1.3";

pub struct KimiDriver {
    kimi_binary: PathBuf,
    bridge_binary: PathBuf,
}

impl KimiDriver {
    pub fn new(kimi_binary: PathBuf, bridge_binary: PathBuf) -> Self {
        Self {
            kimi_binary,
            bridge_binary,
        }
    }
}

#[async_trait]
impl Driver for KimiDriver {
    fn name(&self) -> &str {
        "kimi"
    }

    fn mcp_tool_prefix(&self) -> &str {
        // Kimi-code exposes MCP tools as `mcp__<server>__<tool>`.
        // Our bridge server is named "chat" in `.kimi-code/mcp.json`.
        "mcp__chat__"
    }

    fn requires_initial_prompt(&self) -> bool {
        true
    }

    fn busy_delivery_mode(&self) -> BusyDeliveryMode {
        BusyDeliveryMode::Direct
    }

    fn env_propagation(&self) -> EnvPropagation {
        EnvPropagation::Inherit
    }

    fn skill_compatibility(&self) -> SkillCompatibility {
        SkillCompatibility::Uncertain
    }

    fn context_window_tokens(&self) -> Option<u32> {
        Some(262_144)
    }

    fn prepare_workspace(
        &self,
        work_dir: &Path,
        _config: &DriverAgentConfig,
        _agent_id: &str,
        system_prompt: &str,
    ) -> Result<(), DriverError> {
        // Write AGENTS.md for kimi-code to load as its platform contract.
        write_kimi_agents_md(work_dir, system_prompt).map_err(DriverError::Io)
    }

    fn spawn(&self, cfg: &SpawnConfig) -> Result<tokio::process::Child, DriverError> {
        self.new_process(cfg).spawn(cfg)
    }

    fn parse_event(&self, line: &str) -> Vec<DriverEvent> {
        parse_line(line)
            .into_iter()
            .flat_map(Vec::<DriverEvent>::from)
            .collect()
    }

    fn encode_stdin_message(
        &self,
        _text: &str,
        _session_id: Option<&str>,
        _mode: MessageMode,
    ) -> Option<String> {
        None
    }

    fn supports_turn_cancel(&self) -> bool {
        true
    }

    fn supports_turn_steer(&self) -> bool {
        true
    }

    fn skill_search_paths(&self, workspace: &Path) -> Vec<PathBuf> {
        let mut paths: Vec<PathBuf> = vec![workspace.join(".kimi-code").join("skills")];
        if let Some(home) = dirs::home_dir() {
            paths.push(home.join(".kimi-code").join("skills"));
        }
        paths
    }

    fn as_exit_code_classifier(&self) -> Option<&dyn ExitCodeClassifier> {
        Some(self)
    }

    fn as_process_factory(&self) -> Option<&dyn ProcessFactory> {
        Some(self)
    }
}

impl ProcessFactory for KimiDriver {
    fn new_process(&self, cfg: &SpawnConfig) -> Box<dyn Driver> {
        Box::new(KimiProcessDriver::new(
            self.kimi_binary.clone(),
            self.bridge_binary.clone(),
            cfg.resume_session
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            cfg.initial_prompt.to_string(),
        ))
    }
}

pub struct KimiProcessDriver {
    kimi_binary: PathBuf,
    bridge_binary: PathBuf,
    session_id: String,
    request_id: AtomicU64,
    stdin: Arc<Mutex<Option<tokio::process::ChildStdin>>>,
    session_announced: Arc<Mutex<bool>>,
}

impl KimiProcessDriver {
    fn new(
        kimi_binary: PathBuf,
        bridge_binary: PathBuf,
        session_id: String,
        _initial_prompt: String,
    ) -> Self {
        Self {
            kimi_binary,
            bridge_binary,
            session_id,
            request_id: AtomicU64::new(1),
            stdin: Arc::new(Mutex::new(None)),
            session_announced: Arc::new(Mutex::new(false)),
        }
    }

    fn next_request_id(&self) -> u64 {
        self.request_id.fetch_add(1, Ordering::Relaxed)
    }

    fn encode_prompt(&self, text: &str, method: &str) -> String {
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": self.next_request_id(),
            "method": method,
            "params": {
                "user_input": text
            }
        })
        .to_string()
    }

    async fn write_stdin_bytes(&self, bytes: &[u8]) -> Result<(), DriverError> {
        StdinBinder::write_stdin(self, bytes)
            .await
            .map_err(DriverError::Io)
    }
}

#[async_trait]
impl Driver for KimiProcessDriver {
    fn name(&self) -> &str {
        "kimi"
    }

    fn mcp_tool_prefix(&self) -> &str {
        "mcp__chat__"
    }

    fn busy_delivery_mode(&self) -> BusyDeliveryMode {
        BusyDeliveryMode::Direct
    }

    fn env_propagation(&self) -> EnvPropagation {
        EnvPropagation::Inherit
    }

    fn skill_compatibility(&self) -> SkillCompatibility {
        SkillCompatibility::Uncertain
    }

    fn context_window_tokens(&self) -> Option<u32> {
        Some(262_144)
    }

    fn prepare_workspace(
        &self,
        work_dir: &Path,
        _config: &DriverAgentConfig,
        _agent_id: &str,
        system_prompt: &str,
    ) -> Result<(), DriverError> {
        write_kimi_agents_md(work_dir, system_prompt).map_err(DriverError::Io)
    }

    fn spawn(&self, cfg: &SpawnConfig) -> Result<tokio::process::Child, DriverError> {
        spawn_kimi(&SpawnContext {
            kimi_binary: &self.kimi_binary,
            working_dir: cfg.working_dir,
            bridge_binary: &self.bridge_binary,
            agent_id: cfg.agent_id,
            server_url: cfg.server_url,
            auth_token: cfg.auth_token,
            model: cfg.model,
            session_id: &self.session_id,
            system_prompt: cfg.system_prompt,
            initial_prompt: cfg.initial_prompt,
            no_bridge: false,
        })
        .map_err(DriverError::Io)
    }

    fn parse_event(&self, line: &str) -> Vec<DriverEvent> {
        let mut out = Vec::new();
        if serde_json::from_str::<serde_json::Value>(line.trim()).is_err() {
            return out;
        }
        if !*self.session_announced.lock().unwrap() {
            out.push(DriverEvent::SessionStarted {
                session_id: self.session_id.clone(),
            });
            *self.session_announced.lock().unwrap() = true;
        }
        out.extend(
            parse_line(line)
                .into_iter()
                .flat_map(Vec::<DriverEvent>::from),
        );
        out
    }

    fn encode_stdin_message(
        &self,
        text: &str,
        _session_id: Option<&str>,
        mode: MessageMode,
    ) -> Option<String> {
        let method = match mode {
            MessageMode::User => "prompt",
            MessageMode::Notification => "steer",
        };
        Some(self.encode_prompt(text, method))
    }

    fn supports_turn_cancel(&self) -> bool {
        true
    }

    fn supports_turn_steer(&self) -> bool {
        true
    }

    async fn turn_steer(&self, input: &str) -> Result<(), DriverError> {
        let line = self.encode_prompt(input, "steer");
        let mut bytes = line.into_bytes();
        bytes.push(b'\n');
        self.write_stdin_bytes(&bytes).await
    }

    fn skill_search_paths(&self, workspace: &Path) -> Vec<PathBuf> {
        let mut paths: Vec<PathBuf> = vec![workspace.join(".kimi-code").join("skills")];
        if let Some(home) = dirs::home_dir() {
            paths.push(home.join(".kimi-code").join("skills"));
        }
        paths
    }

    fn as_process_initializer(&self) -> Option<&dyn ProcessInitializer> {
        Some(self)
    }

    fn as_stdin_binder(&self) -> Option<&dyn StdinBinder> {
        Some(self)
    }

    fn as_turn_interruptor(&self) -> Option<&dyn TurnInterruptor> {
        Some(self)
    }
}

impl ProcessInitializer for KimiProcessDriver {
    fn write_init_sequence(&self, stdin: &mut dyn std::io::Write) -> std::io::Result<()> {
        let init = serde_json::json!({
            "jsonrpc": "2.0",
            "id": self.next_request_id(),
            "method": "initialize",
            "params": {
                "protocol_version": KIMI_WIRE_PROTOCOL_VERSION,
                "client": { "name": "cocli-daemon-rs", "version": "1.0.0" },
                "capabilities": {
                    "supports_question": false,
                    "supports_plan_mode": false
                }
            }
        });
        writeln!(stdin, "{init}")?;
        stdin.flush()
    }
}

#[async_trait]
impl StdinBinder for KimiProcessDriver {
    fn bind_stdin(&self, stdin: tokio::process::ChildStdin) {
        *self.stdin.lock().unwrap() = Some(stdin);
    }

    async fn write_stdin(&self, bytes: &[u8]) -> std::io::Result<()> {
        let mut stdin = {
            let mut guard = self.stdin.lock().unwrap();
            guard.take().ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::NotConnected, "stdin not bound")
            })?
        };
        let result = async {
            stdin.write_all(bytes).await?;
            stdin.flush().await?;
            Ok::<(), std::io::Error>(())
        }
        .await;
        *self.stdin.lock().unwrap() = Some(stdin);
        result
    }
}

#[async_trait]
impl TurnInterruptor for KimiProcessDriver {
    async fn interrupt_turn(&self) -> Result<(), DriverError> {
        self.turn_steer("Stop the current turn.").await
    }
}

impl ExitCodeClassifier for KimiDriver {
    fn classify_exit_code(&self, code: i32) -> ExitCodeClass {
        match code {
            130 => ExitCodeClass::Cancelled,
            _ => ExitCodeClass::Normal,
        }
    }
}
