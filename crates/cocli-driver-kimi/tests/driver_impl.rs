//! Driver-trait contract tests for `KimiDriver` (wire process factory).

use std::path::PathBuf;

use cocli_driver_core::types::{BusyDeliveryMode, EnvPropagation, MessageMode, SkillCompatibility};
use cocli_driver_core::{Driver, DriverEvent};
use cocli_driver_kimi::{write_kimi_agents_md, KimiDriver};

fn driver_for_test() -> KimiDriver {
    KimiDriver::new(
        PathBuf::from("/bin/false"),
        PathBuf::from("/opt/cocli/bin/cocli-bridge"),
    )
}

// ── Identity / capability getters ────────────────────────────────────────

#[test]
fn kimi_name_is_kimi() {
    assert_eq!(driver_for_test().name(), "kimi");
}

#[test]
fn kimi_mcp_tool_prefix_is_mcp_chat_double_underscore() {
    assert_eq!(driver_for_test().mcp_tool_prefix(), "mcp__chat__");
}

#[test]
fn kimi_requires_initial_prompt_is_true() {
    assert!(driver_for_test().requires_initial_prompt());
}

#[test]
fn kimi_is_persistent_wire_runtime() {
    assert!(!driver_for_test().is_turn_exit());
}

#[test]
fn kimi_busy_delivery_mode_is_direct() {
    assert_eq!(
        driver_for_test().busy_delivery_mode(),
        BusyDeliveryMode::Direct
    );
}

#[test]
fn kimi_env_propagation_is_inherit() {
    assert_eq!(driver_for_test().env_propagation(), EnvPropagation::Inherit);
}

#[test]
fn kimi_skill_compatibility_is_uncertain() {
    assert_eq!(
        driver_for_test().skill_compatibility(),
        SkillCompatibility::Uncertain
    );
}

#[test]
fn kimi_context_window_is_256k() {
    assert_eq!(driver_for_test().context_window_tokens(), Some(262_144));
}

#[test]
fn kimi_supports_turn_cancel_true() {
    assert!(driver_for_test().supports_turn_cancel());
}

#[test]
fn kimi_supports_turn_steer_true() {
    assert!(driver_for_test().supports_turn_steer());
}

#[test]
fn kimi_skill_search_paths_workspace_first() {
    let drv = driver_for_test();
    let workspace = std::path::Path::new("/tmp/workspace");
    let paths = drv.skill_search_paths(workspace);
    assert_eq!(paths.len(), 2);
    assert_eq!(paths[0], workspace.join(".kimi-code").join("skills"));
}

// ── Sub-trait accessors ──────────────────────────────────────────────────

#[test]
fn kimi_driver_is_process_factory() {
    assert!(driver_for_test().as_process_factory().is_some());
}

#[test]
fn kimi_factory_does_not_implement_process_stdin_traits() {
    assert!(driver_for_test().as_stdin_binder().is_none());
    assert!(driver_for_test().as_turn_interruptor().is_none());
    assert!(driver_for_test().as_process_initializer().is_none());
}

#[test]
fn kimi_driver_implements_exit_code_classifier() {
    assert!(driver_for_test().as_exit_code_classifier().is_some());
}

// ── encode_stdin_message ────────────────────────────────────────────────

#[test]
fn kimi_factory_encode_stdin_message_returns_none() {
    let drv = driver_for_test();
    assert!(drv
        .encode_stdin_message("hello", None, MessageMode::User)
        .is_none());
}

#[test]
fn kimi_process_encodes_prompt_and_steer_jsonrpc() {
    let factory = driver_for_test();
    let cfg = cocli_driver_core::types::SpawnConfig {
        working_dir: std::path::Path::new("/tmp/ws"),
        model: "kimi-k2",
        mcp_config: None,
        resume_session: Some("sess-1"),
        agent_id: "agent-1",
        server_url: "ws://127.0.0.1:8090",
        auth_token: "tok",
        system_prompt: "SYS",
        initial_prompt: "BOOT",
        env_vars: &[],
    };
    let process = factory.as_process_factory().unwrap().new_process(&cfg);

    let idle = process
        .encode_stdin_message("hello", Some("sess-1"), MessageMode::User)
        .expect("idle Kimi prompt request");
    let idle_json: serde_json::Value = serde_json::from_str(&idle).unwrap();
    assert_eq!(idle_json["jsonrpc"], "2.0");
    assert_eq!(idle_json["method"], "prompt");
    assert_eq!(idle_json["params"]["user_input"], "hello");

    let busy = process
        .encode_stdin_message("hello", Some("sess-1"), MessageMode::Notification)
        .expect("busy Kimi steer request");
    let busy_json: serde_json::Value = serde_json::from_str(&busy).unwrap();
    assert_eq!(busy_json["method"], "steer");
    assert_eq!(busy_json["params"]["user_input"], "hello");
}

#[test]
fn kimi_initializer_keeps_slock_protocol_1_3() {
    let factory = driver_for_test();
    let cfg = cocli_driver_core::types::SpawnConfig {
        working_dir: std::path::Path::new("/tmp/ws"),
        model: "kimi-k2",
        mcp_config: None,
        resume_session: Some("sess-1"),
        agent_id: "agent-1",
        server_url: "ws://127.0.0.1:8090",
        auth_token: "tok",
        system_prompt: "SYS",
        initial_prompt: "BOOT",
        env_vars: &[],
    };
    let process = factory.as_process_factory().unwrap().new_process(&cfg);
    let mut bytes = Vec::new();
    process
        .as_process_initializer()
        .unwrap()
        .write_init_sequence(&mut bytes)
        .unwrap();
    let line = String::from_utf8(bytes).unwrap();
    let init: serde_json::Value = serde_json::from_str(line.trim()).unwrap();

    assert_eq!(init["method"], "initialize");
    assert_eq!(init["params"]["protocol_version"], "1.3");
}

#[test]
fn kimi_process_accepts_wire_1_9_initialize_result() {
    let factory = driver_for_test();
    let cfg = cocli_driver_core::types::SpawnConfig {
        working_dir: std::path::Path::new("/tmp/ws"),
        model: "kimi-k2",
        mcp_config: None,
        resume_session: Some("sess-1"),
        agent_id: "agent-1",
        server_url: "ws://127.0.0.1:8090",
        auth_token: "tok",
        system_prompt: "SYS",
        initial_prompt: "BOOT",
        env_vars: &[],
    };
    let process = factory.as_process_factory().unwrap().new_process(&cfg);
    let evs = process.parse_event(
        r#"{"jsonrpc":"2.0","id":"init-1","result":{"protocol_version":"1.9","server":{"name":"Kimi Code CLI","version":"1.41.0"}}}"#,
    );

    assert!(matches!(
        evs.as_slice(),
        [DriverEvent::SessionStarted { session_id }] if session_id == "sess-1"
    ));
}

// ── parse_event smoke ───────────────────────────────────────────────────

#[test]
fn kimi_parse_event_maps_assistant_content_to_text_delta() {
    let drv = driver_for_test();
    let line = r#"{"role":"assistant","content":"hello world"}"#;
    let evs = drv.parse_event(line);
    assert_eq!(evs.len(), 1);
    match &evs[0] {
        DriverEvent::TextDelta { text } => assert_eq!(text, "hello world"),
        other => panic!("expected TextDelta, got {other:?}"),
    }
}

#[test]
fn kimi_parse_event_maps_session_resume_hint_to_session_started() {
    let drv = driver_for_test();
    let line = r#"{"role":"meta","type":"session.resume_hint","session_id":"sess_abc","command":"kimi -r sess_abc","content":"To resume..."}"#;
    let evs = drv.parse_event(line);
    assert_eq!(evs.len(), 1);
    match &evs[0] {
        DriverEvent::SessionStarted { session_id } => assert_eq!(session_id, "sess_abc"),
        other => panic!("expected SessionStarted, got {other:?}"),
    }
}

#[test]
fn kimi_parse_event_returns_empty_for_unknown_role() {
    let drv = driver_for_test();
    let line = r#"{"role":"user","content":"hi"}"#;
    assert!(drv.parse_event(line).is_empty());
}

#[test]
fn kimi_parse_event_returns_empty_for_blank_line() {
    let drv = driver_for_test();
    assert!(drv.parse_event("").is_empty());
    assert!(drv.parse_event("   \n").is_empty());
}

#[test]
fn kimi_parse_event_returns_empty_for_non_json() {
    let drv = driver_for_test();
    assert!(drv.parse_event("not json").is_empty());
}

// ── AGENTS.md byte-identical fixture compare ────────────────────────────

#[test]
fn kimi_agents_md_byte_identical_to_go() {
    let tmp = tempfile::tempdir().unwrap();
    let work_dir = tmp.path();
    let system_prompt = "You are a kimi agent.\nFollow the rules.\n";

    write_kimi_agents_md(work_dir, system_prompt).unwrap();

    let agents_md = work_dir.join("AGENTS.md");
    assert!(agents_md.exists(), "AGENTS.md must be written");

    let actual_bytes = std::fs::read(&agents_md).unwrap();
    assert_eq!(actual_bytes, system_prompt.as_bytes());

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::metadata(&agents_md).unwrap().permissions();
        assert_eq!(perms.mode() & 0o777, 0o644);
    }
}

#[test]
fn kimi_agents_md_empty_prompt_is_noop() {
    let tmp = tempfile::tempdir().unwrap();
    let work_dir = tmp.path();
    write_kimi_agents_md(work_dir, "").unwrap();
    let agents_md = work_dir.join("AGENTS.md");
    assert!(
        !agents_md.exists(),
        "empty system_prompt must NOT create AGENTS.md"
    );
}

// ── Exit code classifier ────────────────────────────────────────────────

#[test]
fn kimi_exit_code_130_is_cancelled() {
    use cocli_driver_core::types::ExitCodeClass;
    let drv = driver_for_test();
    let classifier = drv.as_exit_code_classifier().unwrap();
    assert_eq!(classifier.classify_exit_code(130), ExitCodeClass::Cancelled);
}

#[test]
fn kimi_exit_code_0_is_normal() {
    use cocli_driver_core::types::ExitCodeClass;
    let drv = driver_for_test();
    let classifier = drv.as_exit_code_classifier().unwrap();
    assert_eq!(classifier.classify_exit_code(0), ExitCodeClass::Normal);
    assert_eq!(classifier.classify_exit_code(1), ExitCodeClass::Normal);
    assert_eq!(classifier.classify_exit_code(42), ExitCodeClass::Normal);
}

// ── fork_thread / turn_steer unsupported ────────────────────────────────

#[test]
fn kimi_fork_thread_returns_unsupported() {
    let drv = driver_for_test();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    assert!(matches!(
        rt.block_on(drv.fork_thread("t")),
        Err(cocli_driver_core::DriverError::Unsupported)
    ));
}

#[test]
fn kimi_turn_steer_returns_unsupported() {
    let drv = driver_for_test();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    assert!(matches!(
        rt.block_on(drv.turn_steer("x")),
        Err(cocli_driver_core::DriverError::TurnSteerUnsupported)
    ));
}
