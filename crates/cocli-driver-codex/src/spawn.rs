//! Spawn helper for the codex CLI in `app-server --listen stdio://` mode.
//!
//! Mirrors Go `daemon/drivers/codex.go::Spawn` (codex.go:295). Codex
//! injects MCP via `-c mcp_servers.<name>.<key>=<value>` flags (NO
//! `.mcp-config.json` file written). The args array passed via
//! `-c mcp_servers.chat.args=<json>` is a JSON-encoded string — codex
//! parses it back at startup.

use cocli_bridge_config::bridge_args;
use tokio::process::{Child, Command};

pub struct SpawnContext<'a> {
    /// Resolved path to the codex CLI binary.
    pub codex_binary: &'a std::path::Path,
    /// Resolved path to `cocli-bridge` MCP server binary.
    pub bridge_binary: &'a std::path::Path,
    pub working_dir: &'a std::path::Path,
    pub model: &'a str,
    pub agent_id: &'a str,
    pub server_url: &'a str,
    pub auth_token: &'a str,
    /// When `true`, skip the MCP `-c` flags entirely (parity with Go
    /// `NoBridge=true`). Tests set this; production always wires the
    /// bridge.
    pub no_bridge: bool,
    /// Optional system prompt to write as `AGENTS.md` in the work dir
    /// before spawn (codex convention, codex.go:303).
    pub system_prompt: &'a str,
    /// Optional env overrides — appended to the inherited daemon env in
    /// `KEY=VAL` form.
    pub env_vars: &'a [(String, String)],
}

/// Build the codex CLI `Command` for `app-server --listen stdio://`.
///
/// Also writes `AGENTS.md` at the workspace root when
/// `system_prompt` is non-empty.
pub fn spawn_codex(ctx: &SpawnContext) -> std::io::Result<Child> {
    if !ctx.system_prompt.is_empty() {
        let path = ctx.working_dir.join("AGENTS.md");
        std::fs::write(&path, ctx.system_prompt)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))?;
        }
    }

    let mut cmd = Command::new(ctx.codex_binary);
    cmd.current_dir(ctx.working_dir);
    cmd.arg("app-server").arg("--listen").arg("stdio://");

    if !ctx.no_bridge {
        let bridge = ctx.bridge_binary.to_string_lossy().to_string();
        let args_json =
            serde_json::to_string(&bridge_args(ctx.agent_id, ctx.server_url, ctx.auth_token))
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        cmd.arg("-c")
            .arg(format!("mcp_servers.chat.command={bridge}"))
            .arg("-c")
            .arg(format!("mcp_servers.chat.args={args_json}"))
            .arg("-c")
            .arg("mcp_servers.chat.startup_timeout_sec=30")
            .arg("-c")
            .arg("mcp_servers.chat.tool_timeout_sec=300")
            .arg("-c")
            .arg("mcp_servers.chat.enabled=true")
            .arg("-c")
            .arg("mcp_servers.chat.required=true");
    }

    // Env propagation: inherit daemon env (tokio default) + custom
    // `KEY=VAL` overrides on top.
    for (k, v) in ctx.env_vars {
        cmd.env(k, v);
    }

    cmd.stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    cmd.spawn()
}

/// Test-only helper: build the same `std::process::Command` shape so
/// integration tests can inspect args without spawning. Returns the
/// formatted args list a `tokio::process::Command::new(...).args(...)`
/// would receive.
///
/// Used by `tests/spawn_args.rs` to verify the `-c mcp_servers.chat.*`
/// flag ordering matches the Go output for the same inputs.
pub fn build_spawn_args(ctx: &SpawnContext) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "app-server".to_string(),
        "--listen".to_string(),
        "stdio://".to_string(),
    ];
    if !ctx.no_bridge {
        let bridge = ctx.bridge_binary.to_string_lossy().to_string();
        let args_json =
            serde_json::to_string(&bridge_args(ctx.agent_id, ctx.server_url, ctx.auth_token))
                .expect("bridge args serialize");
        args.push("-c".to_string());
        args.push(format!("mcp_servers.chat.command={bridge}"));
        args.push("-c".to_string());
        args.push(format!("mcp_servers.chat.args={args_json}"));
        args.push("-c".to_string());
        args.push("mcp_servers.chat.startup_timeout_sec=30".to_string());
        args.push("-c".to_string());
        args.push("mcp_servers.chat.tool_timeout_sec=300".to_string());
        args.push("-c".to_string());
        args.push("mcp_servers.chat.enabled=true".to_string());
        args.push("-c".to_string());
        args.push("mcp_servers.chat.required=true".to_string());
    }
    args
}
