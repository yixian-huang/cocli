//! Spawn helpers for the Cursor Agent CLI runtime.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tokio::process::{Child, Command};

/// Inputs required to spawn one Cursor headless turn.
pub struct SpawnContext<'a> {
    pub cursor_binary: &'a Path,
    pub working_dir: &'a Path,
    pub bridge_binary: &'a Path,
    pub agent_id: &'a str,
    pub server_url: &'a str,
    pub auth_token: &'a str,
    pub model: &'a str,
    pub resume_session: Option<&'a str>,
    pub prompt: &'a str,
    pub no_bridge: bool,
}

/// Spawn `cursor-agent` in slock-like per-turn headless mode.
pub fn spawn_cursor(ctx: &SpawnContext) -> std::io::Result<Child> {
    if !ctx.no_bridge {
        write_cursor_mcp_config(
            ctx.working_dir,
            ctx.bridge_binary,
            ctx.agent_id,
            ctx.server_url,
            ctx.auth_token,
        )?;
    }

    let mut cmd = Command::new(ctx.cursor_binary);
    cmd.current_dir(ctx.working_dir);
    cmd.args(build_spawn_args(ctx));
    cmd.env("FORCE_COLOR", "0").env("NO_COLOR", "1");
    cmd.stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    cmd.spawn()
}

/// Build Cursor CLI args in a pure, testable form.
pub fn build_spawn_args(ctx: &SpawnContext) -> Vec<String> {
    let mut args = vec![
        "--print".to_string(),
        "--output-format".to_string(),
        "stream-json".to_string(),
        "--yolo".to_string(),
        "--approve-mcps".to_string(),
        "--trust".to_string(),
    ];

    if !ctx.model.is_empty() {
        args.push("--model".to_string());
        args.push(ctx.model.to_string());
    }
    if let Some(session_id) = ctx.resume_session {
        args.push("--resume".to_string());
        args.push(session_id.to_string());
    }
    if !ctx.prompt.is_empty() {
        args.push(ctx.prompt.to_string());
    }
    args
}

/// Write `<work_dir>/.cursor/mcp.json` with the platform chat bridge.
pub fn write_cursor_mcp_config(
    work_dir: &Path,
    bridge_binary: &Path,
    agent_id: &str,
    server_url: &str,
    auth_token: &str,
) -> std::io::Result<PathBuf> {
    use cocli_bridge_config::bridge_args;

    #[derive(Serialize)]
    struct McpServer<'a> {
        args: Vec<String>,
        command: &'a str,
    }

    #[derive(Serialize)]
    struct Root<'a> {
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
            args: bridge_args(agent_id, server_url, auth_token),
            command,
        },
    );
    let bytes = serde_json::to_vec_pretty(&Root {
        mcp_servers: servers,
    })
    .map_err(std::io::Error::other)?;

    let dir = work_dir.join(".cursor");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("mcp.json");
    std::fs::write(&path, bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))?;
    }
    Ok(path)
}
