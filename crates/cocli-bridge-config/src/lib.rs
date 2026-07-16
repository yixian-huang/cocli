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

/// Canonical MCP server entry used by runtime-specific config writers.
pub struct McpServerEntry<'a> {
    pub command: &'a Path,
    pub args: Vec<String>,
}

/// Build the canonical bridge argument vector.
pub fn bridge_args(agent_id: &str, server_url: &str, auth_token: &str) -> Vec<String> {
    vec![
        "--agent-id".into(),
        agent_id.into(),
        "--server-url".into(),
        bridge_http_base_url(server_url),
        "--auth-token".into(),
        auth_token.into(),
    ]
}

/// Normalize a daemon websocket URL to the HTTP base URL used by the bridge.
pub fn bridge_http_base_url(server_url: &str) -> String {
    let raw = server_url.trim();
    let Ok(mut url) = url::Url::parse(raw) else {
        return raw.trim_end_matches('/').to_string();
    };

    match url.scheme() {
        "ws" => {
            let _ = url.set_scheme("http");
        }
        "wss" => {
            let _ = url.set_scheme("https");
        }
        _ => {}
    }

    url.set_query(None);
    url.set_fragment(None);

    let path = url.path().trim_end_matches('/').to_string();
    if path == "/daemon/connect" || path.ends_with("/daemon/connect") {
        url.set_path(path.trim_end_matches("/daemon/connect"));
    }

    url.as_str().trim_end_matches('/').to_string()
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

#[cfg(test)]
mod bridge_args_tests {
    use super::*;

    #[test]
    fn bridge_args_use_http_base_url() {
        assert_eq!(
            bridge_args(
                "agent-1",
                "wss://cocli.example.com/base/daemon/connect?key=k#frag",
                "token"
            ),
            vec![
                "--agent-id",
                "agent-1",
                "--server-url",
                "https://cocli.example.com/base",
                "--auth-token",
                "token",
            ]
        );
    }

    #[test]
    fn malformed_bridge_url_is_preserved_without_trailing_slash() {
        assert_eq!(bridge_http_base_url("not a url///"), "not a url");
    }
}
