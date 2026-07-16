//! Validate that `ChatrsDriver` implements `Driver` correctly.
//!
//! Mirrors the structure of `cocli-driver-claude/tests/driver_impl.rs`. The
//! load-bearing test here is `chatrs_spawn_writes_agent_json_byte_identical_to_go`
//! which asserts that `.cocli/agent.json` matches Go's `MarshalIndent`
//! output byte-for-byte for the same inputs.

use cocli_driver_chatrs::{
    build_chatrs_child_env, extract_chatrs_settings, write_chatrs_agent_json, ChatrsDriver,
};
use cocli_driver_core::types::{
    BusyDeliveryMode, DriverAgentConfig, EnvPropagation, MessageMode, SkillCompatibility,
    SpawnConfig,
};
use cocli_driver_core::{Driver, DriverError, DriverEvent};

fn driver_for_test() -> ChatrsDriver {
    ChatrsDriver::new(
        std::path::PathBuf::from("/bin/false"),
        std::path::PathBuf::from("/opt/cocli/bin/cocli-bridge"),
    )
}

#[test]
fn chatrs_name_is_chatrs() {
    // Per chatrs.go:29 (NOT "chatry").
    assert_eq!(driver_for_test().name(), "chatrs");
}

#[test]
fn chatrs_mcp_tool_prefix_is_double_underscore() {
    assert_eq!(driver_for_test().mcp_tool_prefix(), "mcp__chat__");
}

#[test]
fn chatrs_busy_delivery_mode_is_gated() {
    assert_eq!(
        driver_for_test().busy_delivery_mode(),
        BusyDeliveryMode::Gated
    );
}

#[test]
fn chatrs_env_propagation_is_inherit() {
    assert_eq!(driver_for_test().env_propagation(), EnvPropagation::Inherit);
}

#[test]
fn chatrs_skill_compatibility_is_unsupported() {
    assert_eq!(
        driver_for_test().skill_compatibility(),
        SkillCompatibility::Unsupported
    );
}

#[test]
fn chatrs_requires_initial_prompt_is_false() {
    // Per chatrs.go:37 — chatrs idles waiting for first stdin user message,
    // no bootstrap prompt needed.
    assert!(!driver_for_test().requires_initial_prompt());
}

#[test]
fn chatrs_is_turn_exit_false() {
    let drv = driver_for_test();
    assert!(!drv.is_turn_exit(), "chatrs is a persistent-stdin runtime");
}

#[test]
fn chatrs_supports_turn_cancel_is_true() {
    assert!(driver_for_test().supports_turn_cancel());
}

#[test]
fn chatrs_supports_turn_steer_is_false() {
    assert!(!driver_for_test().supports_turn_steer());
}

#[test]
fn chatrs_turn_steer_returns_unsupported() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    assert!(matches!(
        rt.block_on(driver_for_test().turn_steer("anything")),
        Err(DriverError::TurnSteerUnsupported)
    ));
}

#[test]
fn chatrs_context_window_is_none() {
    // Phase 2b leaves this `None` — chatrs's 200k figure is profile-
    // dependent; Phase 2c exposes per-profile values.
    assert_eq!(driver_for_test().context_window_tokens(), None);
}

#[test]
fn chatrs_skill_search_paths_is_empty() {
    let drv = driver_for_test();
    let paths = drv.skill_search_paths(std::path::Path::new("/tmp/ws"));
    assert!(paths.is_empty());
}

#[test]
fn encode_user_message_user_mode() {
    let drv = driver_for_test();
    let s = drv
        .encode_stdin_message("hi", None, MessageMode::User)
        .expect("chatrs returns Some");
    assert!(s.contains(r#""kind":"user""#), "got {s:?}");
    assert!(s.contains(r#""text":"hi""#));
    // No trailing newline (actor adds it).
    assert!(!s.ends_with('\n'));
}

#[test]
fn encode_user_message_notification_mode() {
    let drv = driver_for_test();
    let s = drv
        .encode_stdin_message("alert", None, MessageMode::Notification)
        .expect("chatrs returns Some");
    assert!(s.contains(r#""kind":"system""#), "got {s:?}");
    assert!(s.contains(r#""text":"alert""#));
}

#[test]
fn encode_user_message_ignores_session_id() {
    // chatrs tracks session state internally; the encoder ignores any
    // session_id passed in.
    let drv = driver_for_test();
    let s = drv
        .encode_stdin_message("hi", Some("sess-zzz"), MessageMode::User)
        .unwrap();
    assert!(!s.contains("session_id"), "got {s:?}");
    assert!(!s.contains("sess-zzz"));
}

#[test]
fn parse_event_init_maps_to_session_started() {
    let drv = driver_for_test();
    let evs = drv.parse_event(r#"{"kind":"init","session_id":"sess-1"}"#);
    assert_eq!(evs.len(), 1);
    match &evs[0] {
        DriverEvent::SessionStarted { session_id } => assert_eq!(session_id, "sess-1"),
        other => panic!("expected SessionStarted, got {other:?}"),
    }
}

#[test]
fn parse_event_tool_done_success_maps_to_tool_done() {
    let drv = driver_for_test();
    let evs = drv.parse_event(r#"{"kind":"tool_done","id":"call-1","ok":true,"output":"OK"}"#);
    assert_eq!(evs.len(), 1);
    match &evs[0] {
        DriverEvent::ToolDone { id, result, error } => {
            assert_eq!(id, "call-1");
            assert_eq!(result, "OK");
            assert!(error.is_none());
        }
        other => panic!("expected ToolDone, got {other:?}"),
    }
}

#[test]
fn parse_event_tool_done_failure_maps_to_tool_done_with_error() {
    let drv = driver_for_test();
    let evs = drv.parse_event(
        r#"{"kind":"tool_done","id":"call-2","ok":false,"output":"permission denied"}"#,
    );
    assert_eq!(evs.len(), 1);
    match &evs[0] {
        DriverEvent::ToolDone { id, result, error } => {
            assert_eq!(id, "call-2");
            assert!(result.is_empty());
            assert_eq!(error.as_deref(), Some("permission denied"));
        }
        other => panic!("expected ToolDone, got {other:?}"),
    }
}

#[test]
fn parse_event_turn_end_normalizes_status_to_completed() {
    use cocli_driver_core::types::TurnStatus;
    let drv = driver_for_test();
    let evs = drv.parse_event(
        r#"{"kind":"turn_end","status":"completed","input_tokens":10,"output_tokens":5,"cost_usd":0.001}"#,
    );
    assert_eq!(evs.len(), 1);
    match &evs[0] {
        DriverEvent::TurnEnd { status, .. } => assert_eq!(status, &TurnStatus::Completed),
        other => panic!("expected TurnEnd, got {other:?}"),
    }
}

#[test]
fn parse_event_rate_limit_overage_defaults_to_unset() {
    let drv = driver_for_test();
    let evs = drv.parse_event(
        r#"{"kind":"rate_limit","type":"five_hour","status":"limited","resets":1746700000}"#,
    );
    assert_eq!(evs.len(), 1);
    match &evs[0] {
        DriverEvent::RateLimit {
            limit_type,
            overage_status,
            overage_resets,
            is_using_overage,
            ..
        } => {
            assert_eq!(limit_type, "five_hour");
            assert!(overage_status.is_none());
            assert!(overage_resets.is_none());
            assert!(!*is_using_overage);
        }
        other => panic!("expected RateLimit, got {other:?}"),
    }
}

#[test]
fn parse_event_unknown_maps_to_unknown() {
    let drv = driver_for_test();
    let evs = drv.parse_event("not even json");
    assert_eq!(evs.len(), 1);
    assert!(matches!(evs[0], DriverEvent::Unknown));
}

#[test]
fn prepare_workspace_is_noop() {
    // chatrs writes .cocli/agent.json in `spawn` (needs SpawnConfig fields
    // that prepare_workspace doesn't get) — confirm `prepare_workspace`
    // doesn't create the file.
    let tmp = tempfile::tempdir().unwrap();
    let drv = driver_for_test();
    let cfg = DriverAgentConfig {
        runtime: "chatrs",
        model: "claude-haiku-4-5",
        working_runtime: "chatrs",
        working_model: "claude-haiku-4-5",
        env_vars: &[],
    };
    drv.prepare_workspace(tmp.path(), &cfg, "agent-x", "")
        .unwrap();
    let agent_json = tmp.path().join(".cocli").join("agent.json");
    assert!(
        !agent_json.exists(),
        "prepare_workspace must not write .cocli/agent.json — that happens in spawn"
    );
}

/// Raw-byte fixture compare: `write_chatrs_agent_json` must produce
/// `.cocli/agent.json` whose bytes exactly equal Go `chatrs.go::
/// PrepareWorkspace` output for the same inputs.
///
/// Captured from Go's `json.MarshalIndent(map[string]any{...}, "", "  ")`
/// for inputs:
/// - workspace_dir = "/tmp/cocli-chatrs-test"
/// - profile_name  = "anthropic"
/// - model         = "claude-haiku-4-5"
/// - write_enabled = false
/// - agent_id      = "agent-123"
/// - system_prompt = "You are helpful."
///
/// Go marshals `map[string]any` with alphabetized keys; the Rust impl uses
/// a `Serialize` struct with fields declared in alphabetical order so the
/// output matches byte-for-byte.
#[test]
fn chatrs_spawn_writes_agent_json_byte_identical_to_go() {
    let tmp = tempfile::tempdir().unwrap();

    // Use a fixed workspace_dir string so the byte fixture stays stable;
    // the actual file lands inside `tmp.path()/.cocli/agent.json`.
    let workspace_dir = "/tmp/cocli-chatrs-test";

    let bridge_binary = std::path::PathBuf::from("/opt/cocli/bin/cocli-bridge");
    let path = write_chatrs_agent_json(
        tmp.path(),
        "anthropic",        // profile_name
        "claude-haiku-4-5", // model
        false,              // write_enabled
        workspace_dir,      // workspace_dir (string, NOT tmp.path() — fixed for byte parity)
        &bridge_binary,
        "agent-123", // agent_id
        "ws://127.0.0.1:8090",
        "1hz_tok_test",
        "You are helpful.", // system_prompt
    )
    .expect("write agent.json");

    assert_eq!(
        path,
        tmp.path().join(".cocli").join("agent.json"),
        "writer must land at .cocli/agent.json (matches Go chatrs.go:74)"
    );

    let expected_bytes: &[u8] = b"{\n  \"agent_id\": \"agent-123\",\n  \"max_iterations\": 50,\n  \"model\": \"claude-haiku-4-5\",\n  \"profile_name\": \"anthropic\",\n  \"system_prompt\": \"You are helpful.\",\n  \"workspace_dir\": \"/tmp/cocli-chatrs-test\",\n  \"write_enabled\": false\n}";

    let actual_bytes = std::fs::read(&path).expect("read .cocli/agent.json");

    assert_eq!(
        actual_bytes, expected_bytes,
        "chatrs .cocli/agent.json must be byte-identical to Go's MarshalIndent output"
    );
}

/// `ChatrsDriver::spawn` writes `.cocli/agent.json` to the workspace, even
/// when the spawn itself fails (we use `/bin/false` as the binary).
#[tokio::test]
async fn chatrs_spawn_writes_agent_json_side_effect() {
    let tmp = tempfile::tempdir().unwrap();
    let drv = driver_for_test();

    let cfg = SpawnConfig {
        working_dir: tmp.path(),
        model: "claude-haiku-4-5",
        mcp_config: None,
        resume_session: None,
        agent_id: "agent-side-effect",
        server_url: "ws://127.0.0.1:8090",
        auth_token: "1hz_tok_test",
        system_prompt: "",
        initial_prompt: "",
        env_vars: &[],
    };

    // Spawn may or may not succeed (depends on /bin/false existing); we
    // only care that .cocli/agent.json got written.
    let _ = drv.spawn(&cfg);

    let agent_json = tmp.path().join(".cocli").join("agent.json");
    assert!(
        agent_json.exists(),
        "ChatrsDriver::spawn must write .cocli/agent.json"
    );
    let body = std::fs::read_to_string(agent_json).unwrap();
    assert!(body.contains(r#""agent_id": "agent-side-effect""#));
    assert!(body.contains(r#""model": "claude-haiku-4-5""#));
}

// ─── extract_chatrs_settings ──────────────────────────────────────────────────

#[test]
fn chatrs_extract_settings_reads_profile_name_from_env() {
    let env = vec![("CHATRS_PROFILE_NAME".to_string(), "openrouter".to_string())];
    let (profile, write_enabled) = extract_chatrs_settings(&env);
    assert_eq!(profile, "openrouter");
    assert!(
        !write_enabled,
        "write_enabled defaults to false when env missing"
    );
}

#[test]
fn chatrs_extract_settings_write_enabled_only_when_exactly_true() {
    // Matches Go behavior: `EnvVars["CHATRS_WRITE_ENABLED"] == "true"`.
    // Any other string (including uppercase / common truthy values) -> false.
    for (val, expected) in [
        ("true", true),
        ("True", false),
        ("TRUE", false),
        ("1", false),
        ("yes", false),
        ("", false),
        (" true ", false),
    ] {
        let env = vec![("CHATRS_WRITE_ENABLED".to_string(), val.to_string())];
        let (_, write_enabled) = extract_chatrs_settings(&env);
        assert_eq!(
            write_enabled, expected,
            "CHATRS_WRITE_ENABLED={val:?} should produce write_enabled={expected}"
        );
    }
}

#[test]
fn chatrs_extract_settings_defaults_when_env_missing() {
    // No CHATRS_* keys -> profile="anthropic" (Go default at chatrs.go:85),
    // write_enabled=false (SECURITY: not true).
    let (profile, write_enabled) = extract_chatrs_settings(&[]);
    assert_eq!(profile, "anthropic");
    assert!(!write_enabled);
}

#[test]
fn chatrs_extract_settings_empty_profile_name_falls_back_to_anthropic() {
    // Mirrors Go's `if profileName == "" { profileName = "anthropic" }`.
    let env = vec![("CHATRS_PROFILE_NAME".to_string(), "".to_string())];
    let (profile, _) = extract_chatrs_settings(&env);
    assert_eq!(profile, "anthropic");
}

#[test]
fn chatrs_spawn_writes_write_enabled_false_when_env_missing() {
    // SECURITY regression test: hardcoded `write_enabled=true` previously
    // granted write access to read-only agents. Verify the JSON ends up
    // with write_enabled=false when no env vars are supplied.
    let tmp = tempfile::tempdir().unwrap();
    let drv = driver_for_test();

    let cfg = SpawnConfig {
        working_dir: tmp.path(),
        model: "claude-haiku-4-5",
        mcp_config: None,
        resume_session: None,
        agent_id: "agent-readonly",
        server_url: "ws://127.0.0.1:8090",
        auth_token: "1hz_tok_test",
        system_prompt: "",
        initial_prompt: "",
        env_vars: &[],
    };
    let _ = drv.spawn(&cfg);

    let agent_json = tmp.path().join(".cocli").join("agent.json");
    let body = std::fs::read_to_string(agent_json).unwrap();
    assert!(
        body.contains(r#""write_enabled": false"#),
        "default must be write_enabled=false (security): got {body}"
    );
    assert!(
        body.contains(r#""profile_name": "anthropic""#),
        "default profile must be anthropic: got {body}"
    );
}

#[test]
fn chatrs_spawn_writes_write_enabled_true_when_env_says_true() {
    let tmp = tempfile::tempdir().unwrap();
    let drv = driver_for_test();
    let env = vec![
        ("CHATRS_WRITE_ENABLED".to_string(), "true".to_string()),
        ("CHATRS_PROFILE_NAME".to_string(), "openai".to_string()),
    ];

    let cfg = SpawnConfig {
        working_dir: tmp.path(),
        model: "claude-haiku-4-5",
        mcp_config: None,
        resume_session: None,
        agent_id: "agent-writer",
        server_url: "ws://127.0.0.1:8090",
        auth_token: "1hz_tok_test",
        system_prompt: "",
        initial_prompt: "",
        env_vars: &env,
    };
    let _ = drv.spawn(&cfg);

    let agent_json = tmp.path().join(".cocli").join("agent.json");
    let body = std::fs::read_to_string(agent_json).unwrap();
    assert!(body.contains(r#""write_enabled": true"#), "got {body}");
    assert!(body.contains(r#""profile_name": "openai""#), "got {body}");
}

// ─── build_chatrs_child_env ───────────────────────────────────────────────────

#[test]
fn chatrs_build_child_env_injects_bridge_vars() {
    // Mirrors Go chatrs.go:150-156: child env must contain BRIDGE_BIN_PATH,
    // BRIDGE_SCOPED_TOKEN, BRIDGE_WORKSPACE_DIR, CHATRS_AGENT_ID populated
    // from spawn-time inputs.
    let env = build_chatrs_child_env(
        &[],
        std::path::Path::new("/opt/cocli/bin/cocli-bridge"),
        "1hz_scoped_abc",
        std::path::Path::new("/tmp/ws"),
        "agent-xyz",
    );
    let kv: std::collections::HashMap<_, _> = env.into_iter().collect();
    assert_eq!(
        kv.get("BRIDGE_BIN_PATH").map(String::as_str),
        Some("/opt/cocli/bin/cocli-bridge")
    );
    assert_eq!(
        kv.get("BRIDGE_SCOPED_TOKEN").map(String::as_str),
        Some("1hz_scoped_abc")
    );
    assert_eq!(
        kv.get("BRIDGE_WORKSPACE_DIR").map(String::as_str),
        Some("/tmp/ws")
    );
    assert_eq!(
        kv.get("CHATRS_AGENT_ID").map(String::as_str),
        Some("agent-xyz")
    );
}

#[test]
fn chatrs_build_child_env_preserves_user_env_vars() {
    // Env vars injected upstream by the actor/router
    // (CHATRS_PROVIDER_KEY, CHATRS_PROFILE_NAME, BRIDGE_SERVER_URL, optional
    // local-proxy token, etc.) must survive into the child env so chatrs and
    // its bridge can read them.
    let user_env = vec![
        ("CHATRS_PROVIDER_KEY".to_string(), "sk-test-123".to_string()),
        ("CHATRS_PROFILE_NAME".to_string(), "openai".to_string()),
        (
            "BRIDGE_SERVER_URL".to_string(),
            "http://127.0.0.1:9999".to_string(),
        ),
        (
            "DAEMON_LOCAL_TOKEN".to_string(),
            "local_tok_xyz".to_string(),
        ),
    ];
    let env = build_chatrs_child_env(
        &user_env,
        std::path::Path::new("/opt/cocli/bin/cocli-bridge"),
        "1hz_tok",
        std::path::Path::new("/tmp/ws"),
        "agent-xyz",
    );
    let kv: std::collections::HashMap<_, _> = env.into_iter().collect();
    assert_eq!(
        kv.get("CHATRS_PROVIDER_KEY").map(String::as_str),
        Some("sk-test-123")
    );
    assert_eq!(
        kv.get("CHATRS_PROFILE_NAME").map(String::as_str),
        Some("openai")
    );
    // Critically: BRIDGE_SERVER_URL flows through from upstream env and is
    // NOT clobbered by build_chatrs_child_env.
    assert_eq!(
        kv.get("BRIDGE_SERVER_URL").map(String::as_str),
        Some("http://127.0.0.1:9999")
    );
    assert_eq!(
        kv.get("DAEMON_LOCAL_TOKEN").map(String::as_str),
        Some("local_tok_xyz")
    );
}

#[test]
fn chatrs_build_child_env_internal_vars_override_user_on_clash() {
    // Mirrors Go's `append(cmd.Env, "BRIDGE_BIN_PATH=...")` semantics:
    // later entries win when os/exec resolves the env. The Rust impl
    // uses `Command::envs()` which behaves the same way (later .envs()
    // calls override earlier ones for matching keys).
    let user_env = vec![(
        "BRIDGE_BIN_PATH".to_string(),
        "/tmp/sneaky-bridge".to_string(),
    )];
    let env = build_chatrs_child_env(
        &user_env,
        std::path::Path::new("/opt/cocli/bin/cocli-bridge"),
        "tok",
        std::path::Path::new("/tmp/ws"),
        "a-id",
    );
    // The output Vec has TWO entries for BRIDGE_BIN_PATH; verify the
    // last one wins (which is what Command::envs() observes).
    let last_bridge_bin = env
        .iter()
        .rfind(|(k, _)| k == "BRIDGE_BIN_PATH")
        .map(|(_, v)| v.as_str());
    assert_eq!(last_bridge_bin, Some("/opt/cocli/bin/cocli-bridge"));
}
