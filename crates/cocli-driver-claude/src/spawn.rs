//! Spawn helper for the claude CLI.
//!
//! Claude Code's `--print --input-format stream-json` mode is persistent:
//! each user turn is written as one JSON line on stdin and output arrives as
//! newline-delimited stream-json.

use tokio::process::{Child, Command};

pub struct SpawnContext<'a> {
    pub claude_binary: &'a std::path::Path,
    pub working_dir: &'a std::path::Path,
    pub model: &'a str,
    /// Path to `.mcp-config.json` written by the caller (bridge config).
    /// `None` skips the `--mcp-config` flag (parity with Go `NoBridge=true`).
    pub mcp_config: Option<&'a std::path::Path>,
    /// Existing claude `session_id` for `--resume`. `None` starts a fresh
    /// session.
    pub resume_session: Option<&'a str>,
    /// Persistent platform contract. Used as the prompt fallback for wake
    /// respawns, where the actor passes the wake notification in
    /// `SpawnConfig.system_prompt`.
    pub system_prompt: &'a str,
    /// Per-spawn user prompt. Preferred over `system_prompt` on initial start.
    pub initial_prompt: &'a str,
    /// Per-agent env vars to set on the child Command. Empty slice means
    /// claude inherits the parent process's environment unchanged.
    pub env_vars: &'a [(String, String)],
}

pub fn spawn_claude(ctx: &SpawnContext) -> std::io::Result<Child> {
    let mut cmd = Command::new(ctx.claude_binary);
    cmd.current_dir(ctx.working_dir);
    cmd.args(build_spawn_args(ctx));
    cmd.envs(ctx.env_vars.iter().cloned());
    cmd.stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    cmd.spawn()
}

/// Build the claude CLI argument vector (pure, no spawn) so the persistent
/// contract is unit-testable.
pub fn build_spawn_args(ctx: &SpawnContext) -> Vec<String> {
    let mut args = vec![
        "--print".to_string(),
        "--allow-dangerously-skip-permissions".to_string(),
        "--dangerously-skip-permissions".to_string(),
        "--verbose".to_string(),
        "--permission-mode".to_string(),
        "bypassPermissions".to_string(),
        "--output-format".to_string(),
        "stream-json".to_string(),
        "--input-format".to_string(),
        "stream-json".to_string(),
        "--include-partial-messages".to_string(),
    ];
    if !ctx.model.is_empty() {
        args.push("--model".to_string());
        args.push(ctx.model.to_string());
    }
    if let Some(mcp) = ctx.mcp_config {
        args.push("--mcp-config".to_string());
        args.push(mcp.to_string_lossy().to_string());
    }
    if let Some(sid) = ctx.resume_session {
        args.push("--resume".to_string());
        args.push(sid.to_string());
    }
    args
}

/// Phase 2a-prime: write the canonical `.mcp-config.json` for a claude
/// agent. Returns the absolute path. Byte-for-byte identical to Phase 2a's
/// `cocli_bridge_config::write_mcp_config` output (pretty-printed JSON,
/// mode 0o644 on unix, field order `command` then `args`) — see
/// `tests/driver_impl.rs::
/// claude_spawn_writes_mcp_config_byte_identical_to_phase2a`.
///
/// Uses explicit `Serialize` structs (not `serde_json::json!`) because the
/// `json!` macro builds a `Map` that alphabetizes keys by default, which
/// would re-order `command` after `args` and break byte parity.
pub fn write_claude_mcp_config(
    work_dir: &std::path::Path,
    bridge_binary: &std::path::Path,
    agent_id: &str,
    server_url: &str,
    auth_token: &str,
) -> std::io::Result<std::path::PathBuf> {
    use cocli_bridge_config::bridge_args;
    use serde::Serialize;
    use std::collections::BTreeMap;

    #[derive(Serialize)]
    struct McpServer<'a> {
        command: &'a str,
        args: Vec<String>,
    }

    #[derive(Serialize)]
    struct McpRoot<'a> {
        #[serde(rename = "mcpServers")]
        mcp_servers: BTreeMap<String, McpServer<'a>>,
    }

    let command = bridge_binary.to_str().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "bridge_binary path is not valid UTF-8",
        )
    })?;

    let mut servers = BTreeMap::new();
    servers.insert(
        "chat".to_string(),
        McpServer {
            command,
            args: bridge_args(agent_id, server_url, auth_token),
        },
    );
    let root = McpRoot {
        mcp_servers: servers,
    };
    // Pretty-printed (matches Phase 2a's `to_vec_pretty`).
    let bytes = serde_json::to_vec_pretty(&root)?;

    let path = work_dir.join(".mcp-config.json");
    std::fs::write(&path, bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))?;
    }
    Ok(path)
}
