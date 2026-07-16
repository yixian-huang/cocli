//! `ChatrsDriver` — `Driver` impl for the chatrs Rust agent CLI.
//!
//! Ports Go `daemon/drivers/chatrs.go` (404 LOC). Stateless driver: no
//! sub-traits (no `as_*` overrides), subprocess-only stdout JSONL. Per-trait
//! values verified against chatrs.go lines noted inline.
//!
//! `prepare_workspace` is a no-op; `.cocli/agent.json` is written from
//! `spawn` instead so the writer sees `SpawnConfig`'s `server_url` and
//! `auth_token` (the trait's `prepare_workspace` doesn't get them).

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use cocli_driver_core::types::{
    BusyDeliveryMode, DriverAgentConfig, EnvPropagation, MessageMode, SkillCompatibility,
    SpawnConfig,
};
use cocli_driver_core::{Driver, DriverError, DriverEvent};

use crate::events::parse_line;
use crate::spawn::{
    build_chatrs_child_env, extract_chatrs_settings, spawn_chatrs, write_chatrs_agent_json,
    SpawnContext,
};
use crate::stdin::encode_stdin_message;

pub struct ChatrsDriver {
    /// Path to the `chatrs` CLI binary (resolved at daemon boot, normally
    /// `bin/chatrs` next to `bin/cocli-daemon-rs`).
    chatrs_binary: PathBuf,
    /// Path to the `cocli-bridge` MCP server binary. chatrs reads it from
    /// the `BRIDGE_BIN_PATH` env var at spawn time (set by the actor); we
    /// keep the path threaded through `ChatrsDriver` so a future Phase 2c
    /// move into `.cocli/agent.json` is a one-line change.
    bridge_binary: PathBuf,
}

impl ChatrsDriver {
    pub fn new(chatrs_binary: PathBuf, bridge_binary: PathBuf) -> Self {
        Self {
            chatrs_binary,
            bridge_binary,
        }
    }
}

#[async_trait]
impl Driver for ChatrsDriver {
    fn name(&self) -> &str {
        // Matches Go `chatrs.go:29` — runtime ID is "chatrs", not "chatry".
        "chatrs"
    }

    fn mcp_tool_prefix(&self) -> &str {
        // Matches Go `chatrs.go:41`.
        "mcp__chat__"
    }

    fn busy_delivery_mode(&self) -> BusyDeliveryMode {
        // Matches Go `chatrs.go:65` (`BusyDeliveryMode: "gated"`).
        // chatrs *technically* accepts mid-turn stdin but the daemon gates
        // for delivery-ordering safety in V1.
        BusyDeliveryMode::Gated
    }

    fn env_propagation(&self) -> EnvPropagation {
        // Matches Go `chatrs.go:63` (`EnvPropagation: EnvInherit`).
        EnvPropagation::Inherit
    }

    fn skill_compatibility(&self) -> SkillCompatibility {
        // Matches Go `chatrs.go:54` (`SkillUnsupported`). chatrs V1 has no
        // skill loading mechanism.
        SkillCompatibility::Unsupported
    }

    fn context_window_tokens(&self) -> Option<u32> {
        // Go `chatrs.go:47` returns 200_000, but the value is profile-
        // dependent. Leave `None` here per Phase 2b task instructions —
        // exposing it requires per-profile threading the Go code hand-
        // wires. Phase 2c followup.
        None
    }

    fn prepare_workspace(
        &self,
        _work_dir: &Path,
        _config: &DriverAgentConfig,
        _agent_id: &str,
        _system_prompt: &str,
    ) -> Result<(), DriverError> {
        // chatrs writes `.cocli/agent.json` inside `spawn` (needs
        // SpawnConfig.server_url + auth_token which `prepare_workspace`
        // doesn't receive). No-op here.
        Ok(())
    }

    fn spawn(&self, cfg: &SpawnConfig) -> Result<tokio::process::Child, DriverError> {
        // Mirrors Go `chatrs.go::PrepareWorkspace` (lines 83-91): read
        // profile_name + write_enabled from EnvVars. write_enabled is
        // TRUE only when the value is exactly "true" (string match);
        // defaults are profile="anthropic", write_enabled=false.
        //
        // SECURITY: defaulting write_enabled=false (not true) closes the
        // codex-flagged regression where read-only agents got write
        // access from hardcoded defaults on PR #9.
        let (profile_name, write_enabled) = extract_chatrs_settings(cfg.env_vars);

        let workspace_dir_str = cfg.working_dir.to_string_lossy();

        let agent_json_path = write_chatrs_agent_json(
            cfg.working_dir,
            &profile_name,
            cfg.model,
            write_enabled,
            &workspace_dir_str,
            &self.bridge_binary,
            cfg.agent_id,
            cfg.server_url,
            cfg.auth_token,
            cfg.system_prompt,
        )
        .map_err(DriverError::Io)?;

        let child_env = build_chatrs_child_env(
            cfg.env_vars,
            &self.bridge_binary,
            cfg.auth_token,
            cfg.working_dir,
            cfg.agent_id,
        );

        spawn_chatrs(&SpawnContext {
            chatrs_binary: &self.chatrs_binary,
            working_dir: cfg.working_dir,
            agent_json_path: &agent_json_path,
            env_vars: &child_env,
        })
        .map_err(DriverError::Io)
    }

    fn parse_event(&self, line: &str) -> Vec<DriverEvent> {
        // chatrs emits one semantic event per line. Empty / malformed lines
        // become `Unknown` (preserves 1:1 line/event count).
        vec![parse_line(line).into()]
    }

    fn encode_stdin_message(
        &self,
        text: &str,
        session_id: Option<&str>,
        mode: MessageMode,
    ) -> Option<String> {
        // Per chatrs.go:382-393: user -> {"kind":"user",...},
        // notification -> {"kind":"system",...}.
        Some(encode_stdin_message(text, session_id, mode))
    }

    fn supports_turn_cancel(&self) -> bool {
        // chatrs supports SIGINT-based cancel (its async runtime handles
        // the signal and unwinds the in-flight turn cleanly).
        true
    }

    // supports_turn_steer / turn_steer use defaults (false / Err(
    // TurnSteerUnsupported)). Matches Go `chatrs.go:397-399`.

    fn skill_search_paths(&self, _workspace: &Path) -> Vec<PathBuf> {
        // chatrs V1 has no skill mechanism — empty list.
        // Matches Go `chatrs.go:50-52` (`SkillSearchPaths` returns
        // empty `SkillPaths{}`).
        Vec::new()
    }

    fn is_turn_exit(&self) -> bool {
        false
    }
}
