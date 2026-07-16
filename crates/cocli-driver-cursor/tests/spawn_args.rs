use cocli_driver_cursor::spawn::{build_spawn_args, write_cursor_mcp_config, SpawnContext};

#[test]
fn cursor_spawn_args_match_slock_per_turn_shape() {
    let work = std::path::Path::new("/workspace");
    let bridge = std::path::Path::new("/bin/cocli-bridge");
    let ctx = SpawnContext {
        cursor_binary: std::path::Path::new("cursor-agent"),
        working_dir: work,
        bridge_binary: bridge,
        agent_id: "agent-1",
        server_url: "ws://localhost:8080",
        auth_token: "tok",
        model: "composer-2",
        resume_session: Some("session-1"),
        prompt: "wake payload",
        no_bridge: false,
    };

    let args = build_spawn_args(&ctx);

    assert_eq!(
        args,
        vec![
            "--print",
            "--output-format",
            "stream-json",
            "--yolo",
            "--approve-mcps",
            "--trust",
            "--model",
            "composer-2",
            "--resume",
            "session-1",
            "wake payload",
        ]
    );
}

#[test]
fn cursor_mcp_config_writes_project_chat_bridge() {
    let tmp = tempfile::tempdir().unwrap();
    let path = write_cursor_mcp_config(
        tmp.path(),
        std::path::Path::new("/bin/cocli-bridge"),
        "agent-1",
        "ws://localhost:8080",
        "tok",
    )
    .unwrap();

    assert_eq!(path, tmp.path().join(".cursor/mcp.json"));
    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(value["mcpServers"]["chat"]["command"], "/bin/cocli-bridge");
    assert_eq!(
        value["mcpServers"]["chat"]["args"],
        serde_json::json!([
            "--agent-id",
            "agent-1",
            "--server-url",
            "http://localhost:8080",
            "--auth-token",
            "tok"
        ])
    );
}
