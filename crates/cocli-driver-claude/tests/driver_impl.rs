//! Validate that `ClaudeDriver` implements `Driver` correctly.

use cocli_driver_claude::ClaudeDriver;
use cocli_driver_core::types::{
    BusyDeliveryMode, DriverAgentConfig, EnvPropagation, MessageMode, SkillCompatibility,
};
use cocli_driver_core::{Driver, DriverError, DriverEvent};

fn driver_for_test() -> ClaudeDriver {
    ClaudeDriver::new(
        std::path::PathBuf::from("/bin/false"),
        std::path::PathBuf::from("/opt/cocli/bin/cocli-bridge"),
    )
}

#[test]
fn parse_session_started_maps_to_driver_event() {
    let drv = driver_for_test();
    let line = r#"{"type":"system","subtype":"init","session_id":"sess-abc-123"}"#;
    let evs = drv.parse_event(line);
    assert_eq!(evs.len(), 1, "claude returns one event per line");
    match &evs[0] {
        DriverEvent::SessionStarted { session_id } => {
            assert_eq!(session_id, "sess-abc-123");
        }
        other => panic!("expected SessionStarted, got {other:?}"),
    }
}

#[test]
fn parse_unknown_line_maps_to_unknown() {
    let drv = driver_for_test();
    let line = "not even json";
    let evs = drv.parse_event(line);
    assert_eq!(evs.len(), 1);
    assert!(matches!(evs[0], DriverEvent::Unknown));
}

#[test]
fn claude_encode_stdin_message_returns_stream_json_user_message() {
    let drv = driver_for_test();
    let without_session = drv
        .encode_stdin_message("hello", None, MessageMode::User)
        .expect("claude encodes user messages");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&without_session).unwrap(),
        serde_json::json!({
            "type": "user",
            "message": {
                "role": "user",
                "content": [{ "type": "text", "text": "hello" }]
            }
        })
    );

    let with_session = drv
        .encode_stdin_message("hello", Some("sess-1"), MessageMode::Notification)
        .expect("claude encodes notification messages");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&with_session).unwrap()["session_id"],
        "sess-1"
    );
}

#[test]
fn capabilities_match_runtime() {
    let drv = driver_for_test();
    assert!(drv.supports_turn_cancel());
    assert!(!drv.supports_turn_steer());
}

#[test]
fn turn_steer_returns_unsupported() {
    let drv = driver_for_test();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    assert!(matches!(
        rt.block_on(drv.turn_steer("x")),
        Err(DriverError::TurnSteerUnsupported)
    ));
}

#[test]
fn skill_search_paths_includes_workspace_and_global() {
    let drv = driver_for_test();
    let workspace = std::path::Path::new("/tmp/test-workspace");
    let paths = drv.skill_search_paths(workspace);
    assert!(paths
        .iter()
        .any(|p| p.ends_with(".claude/skills") && p.starts_with(workspace)));
    assert!(paths
        .iter()
        .any(|p| p.ends_with(".claude/commands") && p.starts_with(workspace)));
    if let Some(home) = dirs::home_dir() {
        assert!(paths
            .iter()
            .any(|p| p.ends_with(".claude/skills") && p.starts_with(&home)));
    }
}

#[test]
fn claude_name_is_claude() {
    assert_eq!(driver_for_test().name(), "claude");
}

#[test]
fn claude_mcp_tool_prefix_is_double_underscore() {
    assert_eq!(driver_for_test().mcp_tool_prefix(), "mcp__chat__");
}

#[test]
fn claude_busy_delivery_mode_is_gated() {
    assert_eq!(
        driver_for_test().busy_delivery_mode(),
        BusyDeliveryMode::Gated
    );
}

#[test]
fn claude_env_propagation_is_inherit() {
    assert_eq!(driver_for_test().env_propagation(), EnvPropagation::Inherit);
}

#[test]
fn claude_skill_compatibility_is_supported() {
    assert_eq!(
        driver_for_test().skill_compatibility(),
        SkillCompatibility::Supported
    );
}

#[test]
fn claude_context_window_is_200k() {
    assert_eq!(driver_for_test().context_window_tokens(), Some(200_000));
}

#[test]
fn claude_requires_initial_prompt() {
    assert!(driver_for_test().requires_initial_prompt());
}

#[test]
fn claude_is_persistent_stdin_runtime() {
    let drv = driver_for_test();
    assert!(!drv.is_turn_exit(), "claude is a persistent stdin runtime");
}

#[test]
fn claude_driver_is_not_process_factory_or_stdin_binder() {
    let drv = driver_for_test();
    assert!(drv.as_process_factory().is_none());
    assert!(drv.as_process_initializer().is_none());
    assert!(drv.as_stdin_binder().is_none());
    assert!(drv.as_turn_interruptor().is_none());
}

#[test]
fn claude_driver_implements_exit_code_classifier() {
    assert!(driver_for_test().as_exit_code_classifier().is_some());
}

#[test]
fn claude_prepare_workspace_writes_settings_local_json() {
    let tmp = tempfile::tempdir().unwrap();
    let drv = driver_for_test();
    let cfg = DriverAgentConfig {
        runtime: "claude",
        model: "opus-4-7",
        working_runtime: "claude",
        working_model: "opus-4-7",
        env_vars: &[],
    };
    drv.prepare_workspace(tmp.path(), &cfg, "agent-x", "")
        .unwrap();
    let settings_path = tmp.path().join(".claude").join("settings.local.json");
    assert!(settings_path.exists());
    let body = std::fs::read_to_string(&settings_path).unwrap();
    let expected =
        r#"{"permissions":{"allow":["mcp__chat__*","Bash","Read","Write","Edit","Glob","Grep"]}}"#;
    assert_eq!(body, expected);
}

/// Raw-byte fixture compare: ClaudeDriver::spawn must write a
/// .mcp-config.json whose bytes exactly equal what cocli-bridge-config's
/// write_mcp_config produced in Phase 2a (i.e., what production has been
/// emitting). Comparing raw bytes — not JSON values — catches subtle
/// changes in serialization (compact vs pretty, key ordering, trailing
/// newline).
///
/// The expected fixture below was captured from Phase 2a's
/// `serde_json::to_vec_pretty` output (via the now-removed
/// `cocli_bridge_config::write_mcp_config`) using the inputs in this
/// test. PR2 Task 11 froze this as a static `&[u8]` so the test no
/// longer depends on the deprecated writer.
///
/// Uses `#[tokio::test]` because `spawn_claude` builds a `tokio::process::
/// Command` whose `.spawn()` requires a running tokio reactor (even
/// though the test only cares about the side-effect file write).
#[tokio::test]
async fn claude_spawn_writes_mcp_config_byte_identical_to_phase2a() {
    use std::path::PathBuf;

    let tmp = tempfile::tempdir().unwrap();
    let work_dir = tmp.path();

    let bridge_binary = PathBuf::from("/opt/cocli/bin/cocli-bridge");
    let claude_binary = PathBuf::from("/usr/bin/true"); // exits immediately
    let agent_id = "agent-xyz";
    let server_url = "ws://127.0.0.1:8090";
    let auth_token = "1hz_tok_test";

    // Phase 2a's `serde_json::to_vec_pretty` output for the inputs above,
    // captured before PR2 Task 11 removed the writer. Static fixture: any
    // future change to ClaudeDriver::spawn's MCP serialization (compact
    // vs pretty, key ordering, trailing newline) will fail this assertion.
    let expected_bytes: &[u8] = b"{\n  \"mcpServers\": {\n    \"chat\": {\n      \"command\": \"/opt/cocli/bin/cocli-bridge\",\n      \"args\": [\n        \"--agent-id\",\n        \"agent-xyz\",\n        \"--server-url\",\n        \"http://127.0.0.1:8090\",\n        \"--auth-token\",\n        \"1hz_tok_test\"\n      ]\n    }\n  }\n}";

    // Now exercise ClaudeDriver::spawn (new two-arg constructor).
    let drv = ClaudeDriver::new(claude_binary, bridge_binary);
    let cfg = cocli_driver_core::types::SpawnConfig {
        working_dir: work_dir,
        model: "opus-4-7",
        mcp_config: None, // ClaudeDriver::spawn computes the path internally
        resume_session: None,
        agent_id,
        server_url,
        auth_token,
        system_prompt: "",
        initial_prompt: "",
        env_vars: &[],
    };

    // spawn may error (we used /usr/bin/true), but .mcp-config.json must
    // be written before the spawn attempt.
    let _ = drv.spawn(&cfg);

    let mcp_path = work_dir.join(".mcp-config.json");
    assert!(
        mcp_path.exists(),
        ".mcp-config.json should be written by spawn"
    );
    let actual_bytes = std::fs::read(&mcp_path).expect("read claude output");

    assert_eq!(
        actual_bytes, expected_bytes,
        "ClaudeDriver::spawn must produce byte-identical .mcp-config.json to Phase 2a's bridge-config writer"
    );
}
