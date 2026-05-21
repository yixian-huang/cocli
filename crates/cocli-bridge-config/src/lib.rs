//! Writes `.mcp-config.json` for a claude-driven agent.
//!
//! Schema mirrors Go `daemon/drivers/claude.go:99-128` byte-for-byte:
//! ```json
//! {
//!   "mcpServers": {
//!     "chat": {
//!       "command": "<bridge_binary>",
//!       "args": ["--agent-id", "<id>", "--server-url", "<url>", "--auth-token", "<tok>"]
//!     }
//!   }
//! }
//! ```

use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::io;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// Inputs required to render the MCP config for the claude-bridge.
pub struct BridgeConfig<'a> {
    pub bridge_binary: &'a Path,
    pub agent_id: &'a str,
    pub server_url: &'a str,
    pub auth_token: &'a str,
}

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

/// Writes `.mcp-config.json` to `work_dir` and returns the resulting absolute path.
///
/// Schema parity with Go `daemon/drivers/claude.go:99-128`:
/// the only MCP server entry is named `"chat"`; args are positional
/// (`--agent-id <id> --server-url <url> --auth-token <tok>`).
pub fn write_mcp_config(work_dir: &Path, cfg: &BridgeConfig) -> io::Result<PathBuf> {
    let command = cfg.bridge_binary.to_str().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "bridge_binary path is not valid UTF-8",
        )
    })?;

    let mut servers = BTreeMap::new();
    servers.insert(
        "chat".to_string(),
        McpServer {
            command,
            args: vec![
                "--agent-id".into(),
                cfg.agent_id.into(),
                "--server-url".into(),
                cfg.server_url.into(),
                "--auth-token".into(),
                cfg.auth_token.into(),
            ],
        },
    );
    let root = McpRoot {
        mcp_servers: servers,
    };
    let json = serde_json::to_vec_pretty(&root)?;

    let path = work_dir.join(".mcp-config.json");
    fs::write(&path, json)?;
    #[cfg(unix)]
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644))?;
    Ok(path)
}
