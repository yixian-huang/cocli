//! `ClaudeDriver` — `Driver` impl for the claude CLI runtime.
//!
//! Wraps `spawn_claude` and `parse_line` with the full set of capability
//! getters + `prepare_workspace`. Values mirror the Go `daemon/drivers/claude.go`
//! source (mcp_tool_prefix per claude.go:56, settings.local.json body per
//! claude.go:71-83).

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use cocli_driver_core::subtraits::ExitCodeClassifier;
use cocli_driver_core::types::{
    BusyDeliveryMode, DriverAgentConfig, EnvPropagation, ExitCodeClass, MessageMode,
    SkillCompatibility, SpawnConfig,
};
use cocli_driver_core::{Driver, DriverError, DriverEvent};

use crate::events::parse_line;
use crate::spawn::{spawn_claude, SpawnContext};

pub struct ClaudeDriver {
    /// Path to the `claude` CLI binary (resolved at daemon boot).
    claude_binary: PathBuf,
    /// Path to the `cocli-bridge` MCP server binary; embedded into the
    /// per-agent `.mcp-config.json` that `spawn` writes.
    bridge_binary: PathBuf,
}

impl ClaudeDriver {
    pub fn new(claude_binary: PathBuf, bridge_binary: PathBuf) -> Self {
        Self {
            claude_binary,
            bridge_binary,
        }
    }
}

#[async_trait]
impl Driver for ClaudeDriver {
    fn name(&self) -> &str {
        "claude"
    }

    fn mcp_tool_prefix(&self) -> &str {
        // Matches Go `daemon/drivers/claude.go:56` (`McpToolPrefix = "mcp__chat__"`).
        "mcp__chat__"
    }

    fn requires_initial_prompt(&self) -> bool {
        true
    }

    fn busy_delivery_mode(&self) -> BusyDeliveryMode {
        // Claude Code accepts stream-json stdin, but slock gates writes while
        // a turn is in flight because arbitrary busy-time injection can collide
        // with active thinking/tool blocks.
        BusyDeliveryMode::Gated
    }

    fn env_propagation(&self) -> EnvPropagation {
        // claude has no driver-level Capabilities() override in Go and
        // reads env from the workspace at spawn time; inheriting the
        // daemon env is sufficient.
        EnvPropagation::Inherit
    }

    fn skill_compatibility(&self) -> SkillCompatibility {
        // claude is the canonical skill consumer.
        SkillCompatibility::Supported
    }

    fn context_window_tokens(&self) -> Option<u32> {
        // Sonnet/Opus advertise 200k tokens.
        Some(200_000)
    }

    fn prepare_workspace(
        &self,
        work_dir: &Path,
        _config: &DriverAgentConfig,
        _agent_id: &str,
        _system_prompt: &str,
    ) -> Result<(), DriverError> {
        // Mirror of Go `daemon/drivers/claude.go:71-83` — write the
        // `.claude/settings.local.json` permissions allowlist so the
        // CLI doesn't prompt for tool approval.
        let claude_dir = work_dir.join(".claude");
        std::fs::create_dir_all(&claude_dir).map_err(DriverError::Io)?;
        let settings = r#"{"permissions":{"allow":["mcp__chat__*","Bash","Read","Write","Edit","Glob","Grep"]}}"#;
        std::fs::write(claude_dir.join("settings.local.json"), settings)
            .map_err(DriverError::Io)?;
        Ok(())
    }

    fn spawn(&self, cfg: &SpawnConfig) -> Result<tokio::process::Child, DriverError> {
        // Phase 2a-prime: write .mcp-config.json here (was in actor::start).
        // Bytes are guaranteed identical to Phase 2a's
        // cocli_bridge_config::write_mcp_config — see fixture-compare test
        // in tests/driver_impl.rs. The deprecated writer still runs in
        // actor::start during PR2; Task 8 removes the duplicate.
        let mcp_config_path = crate::spawn::write_claude_mcp_config(
            cfg.working_dir,
            &self.bridge_binary,
            cfg.agent_id,
            cfg.server_url,
            cfg.auth_token,
        )
        .map_err(DriverError::Io)?;

        spawn_claude(&SpawnContext {
            claude_binary: &self.claude_binary,
            working_dir: cfg.working_dir,
            model: cfg.model,
            mcp_config: Some(&mcp_config_path),
            resume_session: cfg.resume_session,
            system_prompt: cfg.system_prompt,
            initial_prompt: cfg.initial_prompt,
            env_vars: cfg.env_vars,
        })
        .map_err(DriverError::Io)
    }

    fn parse_event(&self, line: &str) -> Vec<DriverEvent> {
        // claude always emits exactly one semantic event per stdout line.
        vec![parse_line(line).into()]
    }

    fn encode_stdin_message(
        &self,
        text: &str,
        session_id: Option<&str>,
        _mode: MessageMode,
    ) -> Option<String> {
        let mut root = serde_json::json!({
            "type": "user",
            "message": {
                "role": "user",
                "content": [{ "type": "text", "text": text }]
            }
        });
        if let Some(session_id) = session_id.filter(|s| !s.is_empty()) {
            root["session_id"] = serde_json::Value::String(session_id.to_string());
        }
        Some(root.to_string())
    }

    fn supports_turn_cancel(&self) -> bool {
        // FPC #12: claude treats SIGINT as "interrupt current inference, keep
        // session alive". Confirmed in cocli-agent::actor::turn_cancel.
        true
    }

    fn skill_search_paths(&self, workspace: &Path) -> Vec<PathBuf> {
        // Mirrors the hardcoded list that used to live in
        // cocli-agent::skills::handle_skills_list.
        let mut paths: Vec<PathBuf> = vec![
            workspace.join(".claude").join("skills"),
            workspace.join(".claude").join("commands"),
        ];
        if let Some(home) = dirs::home_dir() {
            paths.push(home.join(".claude").join("skills"));
            paths.push(home.join(".claude").join("commands"));
        }
        paths
    }

    fn as_exit_code_classifier(&self) -> Option<&dyn ExitCodeClassifier> {
        Some(self)
    }
}

impl ExitCodeClassifier for ClaudeDriver {
    fn classify_exit_code(&self, code: i32) -> ExitCodeClass {
        match code {
            130 => ExitCodeClass::Cancelled,
            _ => ExitCodeClass::Normal,
        }
    }
}
