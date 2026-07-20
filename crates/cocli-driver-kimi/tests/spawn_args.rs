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
    let working_dir = Path::new("/tmp/ws");
    let agent_file = working_dir
        .join(".cocli-kimi-agent.yaml")
        .to_string_lossy()
        .into_owned();
    let mcp_file = working_dir
        .join(".cocli-kimi-mcp.json")
        .to_string_lossy()
        .into_owned();
    assert_eq!(
        args,
        vec![
            "--wire".to_string(),
            "--yolo".to_string(),
            "--agent-file".to_string(),
            agent_file,
            "--mcp-config-file".to_string(),
            mcp_file,
            "--session".to_string(),
            "sid".to_string(),
            "--model".to_string(),
            "model-x".to_string(),
        ]
    );
}
