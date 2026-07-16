//! Verify codex spawn `Command` args match the Go wire shape
//! (`daemon/drivers/codex.go:309-321`). We use `build_spawn_args` so the
//! check is hermetic — no real codex binary required.

use std::path::Path;

use cocli_driver_codex::{build_spawn_args, SpawnContext};

fn ctx<'a>(bridge: &'a Path, agent_id: &'a str) -> SpawnContext<'a> {
    SpawnContext {
        codex_binary: Path::new("/usr/bin/codex"),
        bridge_binary: bridge,
        working_dir: Path::new("/tmp/workdir"),
        model: "gpt-5",
        agent_id,
        server_url: "ws://127.0.0.1:8090",
        auth_token: "1hz_tok_test",
        no_bridge: false,
        system_prompt: "",
        env_vars: &[],
    }
}

#[test]
fn spawn_args_start_with_app_server_listen_stdio() {
    let bridge = Path::new("/opt/cocli/bin/cocli-bridge");
    let c = ctx(bridge, "agent-xyz");
    let args = build_spawn_args(&c);
    assert_eq!(args[0], "app-server");
    assert_eq!(args[1], "--listen");
    assert_eq!(args[2], "stdio://");
}

#[test]
fn spawn_args_include_mcp_servers_chat_command_flag() {
    let bridge = Path::new("/opt/cocli/bin/cocli-bridge");
    let c = ctx(bridge, "agent-xyz");
    let args = build_spawn_args(&c);
    // -c mcp_servers.chat.command=/opt/cocli/bin/cocli-bridge
    let expected = "mcp_servers.chat.command=/opt/cocli/bin/cocli-bridge";
    let pairs: Vec<(usize, &String)> = args.iter().enumerate().collect();
    let idx = pairs
        .iter()
        .position(|(_, v)| v.as_str() == expected)
        .expect("expected mcp command flag");
    assert_eq!(args[idx - 1], "-c");
}

#[test]
fn spawn_args_mcp_servers_chat_args_is_json_array() {
    let bridge = Path::new("/opt/cocli/bin/cocli-bridge");
    let c = ctx(bridge, "agent-xyz");
    let args = build_spawn_args(&c);
    // Find the `mcp_servers.chat.args=...` flag value.
    let kv = args
        .iter()
        .find(|s| s.starts_with("mcp_servers.chat.args="))
        .expect("expected mcp_servers.chat.args= flag");
    let json_part = kv
        .strip_prefix("mcp_servers.chat.args=")
        .expect("strip prefix");
    let parsed: Vec<String> = serde_json::from_str(json_part).expect("valid json array");
    assert_eq!(
        parsed,
        vec![
            "--agent-id".to_string(),
            "agent-xyz".to_string(),
            "--server-url".to_string(),
            "http://127.0.0.1:8090".to_string(),
            "--auth-token".to_string(),
            "1hz_tok_test".to_string(),
        ]
    );
}

#[test]
fn spawn_args_include_startup_and_tool_timeouts() {
    let bridge = Path::new("/opt/cocli/bin/cocli-bridge");
    let c = ctx(bridge, "agent-xyz");
    let args = build_spawn_args(&c);
    assert!(args
        .iter()
        .any(|s| s == "mcp_servers.chat.startup_timeout_sec=30"));
    assert!(args
        .iter()
        .any(|s| s == "mcp_servers.chat.tool_timeout_sec=300"));
    assert!(args.iter().any(|s| s == "mcp_servers.chat.enabled=true"));
    assert!(args.iter().any(|s| s == "mcp_servers.chat.required=true"));
}

#[test]
fn spawn_args_omit_mcp_flags_when_no_bridge() {
    let bridge = Path::new("/opt/cocli/bin/cocli-bridge");
    let mut c = ctx(bridge, "agent-xyz");
    c.no_bridge = true;
    let args = build_spawn_args(&c);
    assert_eq!(args.len(), 3); // just app-server / --listen / stdio://
    assert!(!args.iter().any(|s| s.contains("mcp_servers")));
}

#[test]
fn spawn_writes_agents_md_when_system_prompt_set() {
    use cocli_driver_codex::spawn_codex;
    let tmp = tempfile::tempdir().unwrap();
    // Use /usr/bin/false so spawn returns Err quickly without
    // leaving a zombie process.
    let c = SpawnContext {
        codex_binary: Path::new("/usr/bin/false"),
        bridge_binary: Path::new("/opt/cocli/bin/cocli-bridge"),
        working_dir: tmp.path(),
        model: "gpt-5",
        agent_id: "a",
        server_url: "ws://x",
        auth_token: "t",
        no_bridge: false,
        system_prompt: "You are codex.",
        env_vars: &[],
    };
    // spawn may error (false exits immediately); AGENTS.md must be
    // written BEFORE the spawn attempt.
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let _ = runtime.block_on(async { spawn_codex(&c) });
    let agents_md = tmp.path().join("AGENTS.md");
    assert!(agents_md.exists(), "AGENTS.md should be written");
    let body = std::fs::read_to_string(&agents_md).unwrap();
    assert_eq!(body, "You are codex.");
}

#[test]
fn spawn_skips_agents_md_when_system_prompt_empty() {
    use cocli_driver_codex::spawn_codex;
    let tmp = tempfile::tempdir().unwrap();
    let c = SpawnContext {
        codex_binary: Path::new("/usr/bin/false"),
        bridge_binary: Path::new("/opt/cocli/bin/cocli-bridge"),
        working_dir: tmp.path(),
        model: "gpt-5",
        agent_id: "a",
        server_url: "ws://x",
        auth_token: "t",
        no_bridge: false,
        system_prompt: "",
        env_vars: &[],
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let _ = runtime.block_on(async { spawn_codex(&c) });
    assert!(!tmp.path().join("AGENTS.md").exists());
}
