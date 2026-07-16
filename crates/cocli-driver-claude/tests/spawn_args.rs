//! Spawn-args tests for Claude Code persistent stream-json driver.

use std::path::Path;

use cocli_driver_claude::spawn::build_spawn_args;
use cocli_driver_claude::SpawnContext;

fn ctx<'a>(resume_session: Option<&'a str>, model: &'a str) -> SpawnContext<'a> {
    SpawnContext {
        claude_binary: Path::new("/bin/claude"),
        working_dir: Path::new("/tmp/ws"),
        model,
        mcp_config: Some(Path::new("/tmp/ws/.mcp-config.json")),
        resume_session,
        system_prompt: "PLATFORM CONTRACT",
        initial_prompt: "BOOTSTRAP TURN",
        env_vars: &[],
    }
}

#[test]
fn spawn_args_use_persistent_stream_json_stdin() {
    let args = build_spawn_args(&ctx(None, ""));
    assert_eq!(args[0], "--print");
    assert!(args.contains(&"--output-format".to_string()));
    assert!(args.contains(&"stream-json".to_string()));
    assert!(args.contains(&"--input-format".to_string()));
    assert!(!args.contains(&"BOOTSTRAP TURN".to_string()));
}

#[test]
fn spawn_args_includes_resume_session() {
    let args = build_spawn_args(&ctx(Some("sess_123"), ""));
    let idx = args.iter().position(|a| a == "--resume").unwrap();
    assert_eq!(args[idx + 1], "sess_123");
}

#[test]
fn spawn_args_includes_model_when_present() {
    let args = build_spawn_args(&ctx(None, "sonnet"));
    let idx = args.iter().position(|a| a == "--model").unwrap();
    assert_eq!(args[idx + 1], "sonnet");
}

#[test]
fn spawn_args_canonical_order() {
    let args = build_spawn_args(&ctx(Some("sid"), "sonnet"));
    let expected: Vec<String> = [
        "--print",
        "--allow-dangerously-skip-permissions",
        "--dangerously-skip-permissions",
        "--verbose",
        "--permission-mode",
        "bypassPermissions",
        "--output-format",
        "stream-json",
        "--input-format",
        "stream-json",
        "--include-partial-messages",
        "--model",
        "sonnet",
        "--mcp-config",
        "/tmp/ws/.mcp-config.json",
        "--resume",
        "sid",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    assert_eq!(args, expected);
}
