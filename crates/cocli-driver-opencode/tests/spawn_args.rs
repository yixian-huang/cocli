use cocli_driver_opencode::spawn::{build_spawn_args, write_opencode_config, SpawnContext};

#[test]
fn opencode_spawn_args_match_slock_run_shape() {
    let work = std::path::Path::new("/workspace");
    let bridge = std::path::Path::new("/bin/cocli-bridge");
    let ctx = SpawnContext {
        opencode_binary: std::path::Path::new("opencode"),
        working_dir: work,
        bridge_binary: bridge,
        agent_id: "agent-1",
        server_url: "ws://localhost:8080",
        auth_token: "tok",
        model: "anthropic/claude-sonnet-4.5",
        resume_session: Some("session-1"),
        prompt: "wake payload",
        no_bridge: false,
    };

    let args = build_spawn_args(&ctx);

    assert_eq!(
        args,
        vec![
            "run",
            "--format",
            "json",
            "--pure",
            "--dir",
            "/workspace",
            "--model",
            "anthropic/claude-sonnet-4.5",
            "--session",
            "session-1",
            "wake payload",
        ]
    );
}

#[test]
fn opencode_config_writes_project_chat_bridge() {
    let tmp = tempfile::tempdir().unwrap();
    let path = write_opencode_config(
        tmp.path(),
        std::path::Path::new("/bin/cocli-bridge"),
        "agent-1",
        "ws://localhost:8080",
        "tok",
    )
    .unwrap();

    assert_eq!(path, tmp.path().join("opencode.json"));
    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(value["mcp"]["chat"]["type"], "local");
    assert_eq!(
        value["mcp"]["chat"]["command"],
        serde_json::json!(["/bin/cocli-bridge"])
    );
    assert_eq!(
        value["mcp"]["chat"]["args"],
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
