//! Spawn-args tests for kimi-code wire driver.

use std::path::Path;

use cocli_driver_kimi::spawn::build_spawn_args;
use cocli_driver_kimi::SpawnContext;

fn ctx<'a>(
    system_prompt: &'a str,
    initial_prompt: &'a str,
    resume_session: Option<&'a str>,
    model: &'a str,
) -> SpawnContext<'a> {
    SpawnContext {
        kimi_binary: Path::new("/bin/kimi"),
        working_dir: Path::new("/tmp/ws"),
        bridge_binary: Path::new("/bin/bridge"),
        agent_id: "agent-1",
        server_url: "ws://127.0.0.1:8090",
        auth_token: "tok",
        model,
        session_id: resume_session.unwrap_or("generated-session"),
        system_prompt,
        initial_prompt,
        no_bridge: true,
    }
}

#[test]
fn spawn_args_use_persistent_wire_shape() {
    let c = ctx("", "", None, "");
    let args = build_spawn_args(&c);
    assert_eq!(args[0], "--wire");
    assert!(args.contains(&"--yolo".to_string()));
    assert!(args.contains(&"--agent-file".to_string()));
    assert!(args.contains(&"--mcp-config-file".to_string()));
    assert!(args.contains(&"--session".to_string()));
    assert!(!args.contains(&"-p".to_string()));
    assert!(!args.contains(&"--output-format".to_string()));
}

#[test]
fn spawn_args_includes_resume_session() {
    let c = ctx("", "BOOT", Some("sess_123"), "");
    let args = build_spawn_args(&c);
    let s_idx = args.iter().position(|a| a == "--session").unwrap();
    assert_eq!(args[s_idx + 1], "sess_123");
}

#[test]
fn spawn_args_includes_model() {
    let c = ctx("", "BOOT", None, "kimi-k2");
    let args = build_spawn_args(&c);
    let m_idx = args.iter().position(|a| a == "--model").unwrap();
    assert_eq!(args[m_idx + 1], "kimi-k2");
}

#[test]
fn spawn_args_canonical_order() {
    let c = ctx("SYS", "INIT", Some("sid"), "model-x");
    let args = build_spawn_args(&c);
    assert_eq!(
        args,
        vec![
            "--wire",
            "--yolo",
            "--agent-file",
            "/tmp/ws/.cocli-kimi-agent.yaml",
            "--mcp-config-file",
            "/tmp/ws/.cocli-kimi-mcp.json",
            "--session",
            "sid",
            "--model",
            "model-x",
        ]
    );
}
