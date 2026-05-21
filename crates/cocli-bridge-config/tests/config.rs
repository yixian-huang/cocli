use cocli_bridge_config::{write_mcp_config, BridgeConfig};
use std::path::PathBuf;
use tempfile::tempdir;

#[test]
fn writes_expected_schema() {
    let dir = tempdir().unwrap();
    let bridge = PathBuf::from("/usr/local/bin/cocli-bridge");
    let path = write_mcp_config(
        dir.path(),
        &BridgeConfig {
            bridge_binary: &bridge,
            agent_id: "a1",
            server_url: "ws://localhost:8080",
            auth_token: "tok123",
        },
    )
    .unwrap();

    assert_eq!(path.file_name().unwrap(), ".mcp-config.json");

    let raw = std::fs::read_to_string(path).unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();

    // Top-level key must be "mcpServers".
    assert!(v.get("mcpServers").is_some(), "missing mcpServers root key");

    // The only server entry MUST be named "chat" (matches Go driver MCP server name).
    let chat = &v["mcpServers"]["chat"];
    assert!(chat.is_object(), "mcpServers.chat must be an object");

    assert_eq!(chat["command"], "/usr/local/bin/cocli-bridge");

    let args = chat["args"].as_array().expect("args is array");
    assert_eq!(args.len(), 6, "args must be exactly 6 elements");
    assert_eq!(args[0], "--agent-id");
    assert_eq!(args[1], "a1");
    assert_eq!(args[2], "--server-url");
    assert_eq!(args[3], "ws://localhost:8080");
    assert_eq!(args[4], "--auth-token");
    assert_eq!(args[5], "tok123");
}

#[test]
fn file_permissions_are_0644_on_unix() {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempdir().unwrap();
        let bridge = PathBuf::from("/tmp/bridge");
        let path = write_mcp_config(
            dir.path(),
            &BridgeConfig {
                bridge_binary: &bridge,
                agent_id: "x",
                server_url: "ws://x",
                auth_token: "t",
            },
        )
        .unwrap();
        let meta = std::fs::metadata(path).unwrap();
        let mode = meta.permissions().mode() & 0o777;
        assert_eq!(mode, 0o644, "mode={:o}", mode);
    }
}

#[test]
fn only_chat_server_present() {
    let dir = tempdir().unwrap();
    let bridge = PathBuf::from("/bin/cb");
    let path = write_mcp_config(
        dir.path(),
        &BridgeConfig {
            bridge_binary: &bridge,
            agent_id: "id",
            server_url: "ws://s",
            auth_token: "a",
        },
    )
    .unwrap();
    let raw = std::fs::read_to_string(path).unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let servers = v["mcpServers"].as_object().unwrap();
    assert_eq!(servers.len(), 1);
    assert!(servers.contains_key("chat"));
}
