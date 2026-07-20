//! Spawn helpers for the OpenCode CLI runtime.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tokio::process::{Child, Command};

pub struct SpawnContext<'a> {
    pub opencode_binary: &'a Path,
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

pub fn spawn_opencode(ctx: &SpawnContext) -> std::io::Result<Child> {
    let config_content = if ctx.no_bridge {
        None
    } else {
        Some(write_opencode_config(
            ctx.working_dir,
            ctx.bridge_binary,
            ctx.agent_id,
            ctx.server_url,
            ctx.auth_token,
        )?)
    };

    let mut cmd = Command::new(ctx.opencode_binary);
    cmd.current_dir(ctx.working_dir);
    cmd.args(build_spawn_args(ctx));
    cmd.env("FORCE_COLOR", "0").env("NO_COLOR", "1");
    if let Some(path) = config_content {
        let content = std::fs::read_to_string(path)?;
        cmd.env("OPENCODE_CONFIG_CONTENT", content);
    }
    cmd.stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    cmd.spawn()
}

pub fn build_spawn_args(ctx: &SpawnContext) -> Vec<String> {
    let mut args = vec![
        "run".to_string(),
        "--format".to_string(),
        "json".to_string(),
        "--pure".to_string(),
        "--dir".to_string(),
        ctx.working_dir.to_string_lossy().into_owned(),
    ];
    if !ctx.model.is_empty() {
        args.push("--model".to_string());
        args.push(ctx.model.to_string());
    }
    if let Some(session_id) = ctx.resume_session {
        args.push("--session".to_string());
        args.push(session_id.to_string());
    }
    if !ctx.prompt.is_empty() {
        args.push(ctx.prompt.to_string());
    }
    args
}

pub fn write_opencode_config(
    work_dir: &Path,
    bridge_binary: &Path,
    agent_id: &str,
    server_url: &str,
    auth_token: &str,
) -> std::io::Result<PathBuf> {
    use cocli_bridge_config::bridge_args;

    #[derive(Serialize)]
    struct McpServer {
        #[serde(rename = "type")]
        server_type: &'static str,
        command: Vec<String>,
        args: Vec<String>,
    }

    #[derive(Serialize)]
    struct Root {
        mcp: BTreeMap<String, McpServer>,
    }

    let command = bridge_binary.to_str().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "bridge_binary path is not valid UTF-8",
        )
    })?;
    let mut mcp = BTreeMap::new();
    mcp.insert(
        "chat".to_string(),
        McpServer {
            server_type: "local",
            command: vec![command.to_string()],
            args: bridge_args(agent_id, server_url, auth_token),
        },
    );
    let bytes = serde_json::to_vec_pretty(&Root { mcp }).map_err(std::io::Error::other)?;

    let path = work_dir.join("opencode.json");
    std::fs::write(&path, bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))?;
    }
    Ok(path)
}
