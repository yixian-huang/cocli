//! Driver implementation for Cursor Agent CLI.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use cocli_driver_core::types::{
    BusyDeliveryMode, DriverAgentConfig, EnvPropagation, MessageMode, SkillCompatibility,
    SpawnConfig,
};
use cocli_driver_core::{Driver, DriverError, DriverEvent};

use crate::events::parse_line;
use crate::spawn::{spawn_cursor, SpawnContext};

pub struct CursorDriver {
    cursor_binary: PathBuf,
    bridge_binary: PathBuf,
}

impl CursorDriver {
    pub fn new(cursor_binary: PathBuf, bridge_binary: PathBuf) -> Self {
        Self {
            cursor_binary,
            bridge_binary,
        }
    }
}

#[async_trait]
impl Driver for CursorDriver {
    fn name(&self) -> &str {
        "cursor"
    }

    fn mcp_tool_prefix(&self) -> &str {
        "mcp__chat__"
    }

    fn requires_initial_prompt(&self) -> bool {
        true
    }

    fn busy_delivery_mode(&self) -> BusyDeliveryMode {
        BusyDeliveryMode::Direct
    }

    fn env_propagation(&self) -> EnvPropagation {
        EnvPropagation::SettingsCopy
    }

    fn skill_compatibility(&self) -> SkillCompatibility {
        SkillCompatibility::Uncertain
    }

    fn prepare_workspace(
        &self,
        work_dir: &Path,
        _config: &DriverAgentConfig,
        _agent_id: &str,
        _system_prompt: &str,
    ) -> Result<(), DriverError> {
        std::fs::create_dir_all(work_dir.join(".cursor")).map_err(DriverError::Io)
    }

    fn spawn(&self, cfg: &SpawnConfig) -> Result<tokio::process::Child, DriverError> {
        spawn_cursor(&SpawnContext {
            cursor_binary: &self.cursor_binary,
            working_dir: cfg.working_dir,
            bridge_binary: &self.bridge_binary,
            agent_id: cfg.agent_id,
            server_url: cfg.server_url,
            auth_token: cfg.auth_token,
            model: cfg.model,
            resume_session: cfg.resume_session,
            prompt: cfg.initial_prompt,
            no_bridge: false,
        })
        .map_err(DriverError::Io)
    }

    fn parse_event(&self, line: &str) -> Vec<DriverEvent> {
        parse_line(line)
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

    fn is_turn_exit(&self) -> bool {
        true
    }

    fn skill_search_paths(&self, workspace: &Path) -> Vec<PathBuf> {
        let mut paths = vec![
            workspace.join(".cursor").join("skills"),
            workspace.join(".agents").join("skills"),
        ];
        if let Some(home) = dirs::home_dir() {
            paths.push(home.join(".cursor").join("skills"));
            paths.push(home.join(".agents").join("skills"));
        }
        paths
    }
}
