//! Spawn helper for the kimi-code CLI (`kimi --wire`).
//!
//! Kimi-code's wire mode is persistent: each user turn is written as a JSON-RPC
//! request on stdin.
//!
//! MCP bridge configuration is written to a per-workspace config file and
//! passed explicitly via `--mcp-config-file`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use tokio::process::{Child, Command};

pub struct SpawnContext<'a> {
    pub kimi_binary: &'a Path,
    pub working_dir: &'a Path,
    pub bridge_binary: &'a Path,
    pub agent_id: &'a str,
    pub server_url: &'a str,
    pub auth_token: &'a str,
    pub model: &'a str,
    pub session_id: &'a str,
    pub system_prompt: &'a str,
    pub initial_prompt: &'a str,
    pub no_bridge: bool,
}

/// Write `<work_dir>/AGENTS.md` from `system_prompt`. No-op when
/// `system_prompt` is empty (matches old Go kimi.go:189-194).
pub fn write_kimi_agents_md(work_dir: &Path, system_prompt: &str) -> std::io::Result<()> {
    if system_prompt.is_empty() {
        return Ok(());
    }
    let path = work_dir.join("AGENTS.md");
    std::fs::write(&path, system_prompt)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))?;
    }
    Ok(())
}

/// Write `<work_dir>/.cocli-kimi-mcp.json` so kimi-code loads ONLY the per-
/// agent bridge. Returns the absolute path.
///
/// Kimi-code MCP docs: https://moonshotai.github.io/kimi-code/en/customization/mcp.html
/// Project-level config shape:
///   `{"mcpServers": {"chat": {"command": "<bridge>", "args": [...]}}}`
pub fn write_kimi_mcp_config(
    work_dir: &Path,
    bridge_binary: &Path,
    agent_id: &str,
    server_url: &str,
    auth_token: &str,
) -> std::io::Result<PathBuf> {
    use serde::Serialize;

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
    let args = cocli_bridge_config::bridge_args(agent_id, server_url, auth_token);
    servers.insert("chat".to_string(), McpServer { command, args });
    let root = McpRoot {
        mcp_servers: servers,
    };

    // Kimi-code uses compact JSON (same as Go's json.Marshal).
    let bytes = serde_json::to_vec(&root)?;

    let path = work_dir.join(".cocli-kimi-mcp.json");
    std::fs::write(&path, bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))?;
    }
    Ok(path)
}

/// Spawn the kimi-code CLI subprocess with stdin/stdout/stderr piped.
pub fn spawn_kimi(ctx: &SpawnContext) -> std::io::Result<Child> {
    write_kimi_agents_md(ctx.working_dir, ctx.system_prompt)?;
    let agent_file = write_kimi_agent_file(ctx.working_dir)?;

    let mcp_path = if ctx.no_bridge {
        ctx.working_dir.join(".cocli-kimi-mcp.json")
    } else {
        write_kimi_mcp_config(
            ctx.working_dir,
            ctx.bridge_binary,
            ctx.agent_id,
            ctx.server_url,
            ctx.auth_token,
        )?
    };

    let mut cmd = Command::new(ctx.kimi_binary);
    cmd.current_dir(ctx.working_dir)
        .args(build_spawn_args_for_paths(ctx, &agent_file, &mcp_path));

    cmd.stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);

    cmd.spawn()
}

/// Test-only helper: build the argument vector so integration tests can
/// inspect args without spawning.
pub fn build_spawn_args(ctx: &SpawnContext) -> Vec<String> {
    let agent_file = ctx.working_dir.join(".cocli-kimi-agent.yaml");
    let mcp_file = ctx.working_dir.join(".cocli-kimi-mcp.json");
    build_spawn_args_for_paths(ctx, &agent_file, &mcp_file)
}

fn build_spawn_args_for_paths(
    ctx: &SpawnContext,
    agent_file: &Path,
    mcp_file: &Path,
) -> Vec<String> {
    let mut args = vec![
        "--wire".to_string(),
        "--yolo".to_string(),
        "--agent-file".to_string(),
        agent_file.to_string_lossy().into_owned(),
        "--mcp-config-file".to_string(),
        mcp_file.to_string_lossy().into_owned(),
        "--session".to_string(),
        ctx.session_id.to_string(),
    ];

    if !ctx.model.is_empty() {
        args.push("--model".to_string());
        args.push(ctx.model.to_string());
    }

    args
}

fn write_kimi_agent_file(work_dir: &Path) -> std::io::Result<PathBuf> {
    let path = work_dir.join(".cocli-kimi-agent.yaml");
    std::fs::write(
        &path,
        "version: 1\nagent:\n  extend: default\n  system_prompt_path: ./AGENTS.md\n",
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))?;
    }
    Ok(path)
}
