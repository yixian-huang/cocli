//! Validate that `GrokDriver` implements `Driver` + `ExitCodeClassifier`
//! correctly (turn-exit, Inherit, Supported skills, chat__ prefix, etc.).
//!
//! Follows patterns from kimi/driver_impl.rs (AGENTS.md + Inherit + turn-exit)
//! and gemini/driver_impl.rs (prepare side effects, spawn writes, exit codes,
//! parse smoke). TDD: written first (will fail until driver.rs + stdin.rs impl).
//!
//! Uses answers from clarification: AGENTS.md at root (prepare), CLI transport
//! (no .grok/config.toml MCP), exit 0/1 Normal + 130/143 Cancelled, skill paths
//! include .grok + .agents, extra="", edit spawn+lib allowed for helper.

use std::path::PathBuf;

use cocli_driver_core::types::{
    BusyDeliveryMode, DriverAgentConfig, EnvPropagation, ExitCodeClass, MessageMode,
    SkillCompatibility, SpawnConfig,
};
use cocli_driver_core::{Driver, DriverError, DriverEvent};
use cocli_driver_grok::{write_grok_agents_md, GrokDriver};

fn driver_for_test() -> GrokDriver {
    GrokDriver::new(PathBuf::from("/bin/false"))
}

// ─── 1. Capability / data getters (match design + spike) ──────────────────

#[test]
fn grok_name_is_grok() {
    assert_eq!(driver_for_test().name(), "grok");
}

#[test]
fn grok_mcp_tool_prefix_is_chat_double_underscore() {
    // Historical MCP prefix from spike (chat server -> chat__send_message).
    // Production uses cocli CLI; prefix retained for compatibility checks.
    assert_eq!(driver_for_test().mcp_tool_prefix(), "chat__");
}

#[test]
fn grok_requires_initial_prompt_is_true() {
    // -p is required for headless; no REPL stdin idle.
    assert!(driver_for_test().requires_initial_prompt());
}

#[test]
fn grok_is_turn_exit_true() {
    // Per design: turn-exit like gemini/claude/kimi. Respawn per delivery.
    assert!(
        driver_for_test().is_turn_exit(),
        "grok is a turn-exit runtime"
    );
}

#[test]
fn grok_busy_delivery_mode_is_none() {
    assert_eq!(
        driver_for_test().busy_delivery_mode(),
        BusyDeliveryMode::None
    );
}

#[test]
fn grok_env_propagation_is_inherit() {
    // Grok inherits (incl XAI_API_KEY); platform actions via injected cocli CLI.
    // (No SettingsCopy sanitization like gemini.)
    assert_eq!(driver_for_test().env_propagation(), EnvPropagation::Inherit);
}

#[test]
fn grok_skill_compatibility_is_supported() {
    // Grok has native .grok/skills + AGENTS.md + ~/.agents/skills support.
    assert_eq!(
        driver_for_test().skill_compatibility(),
        SkillCompatibility::Supported
    );
}

#[test]
fn grok_context_window_uses_models_cache_default() {
    let window = driver_for_test().context_window_tokens();
    assert!(window.is_some());
    assert!(window.unwrap() >= 200_000);
}

#[test]
fn grok_extra_system_prompt_section_is_empty() {
    // Prefer AGENTS.md for platform contract (per design + spike).
    assert_eq!(driver_for_test().extra_system_prompt_section(), "");
}

#[test]
fn grok_supports_turn_cancel_true_steer_fork_false() {
    let drv = driver_for_test();
    assert!(drv.supports_turn_cancel());
    assert!(!drv.supports_turn_steer());
    assert!(!drv.supports_thread_fork());
}

#[test]
fn grok_turn_steer_returns_unsupported() {
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
fn grok_fork_thread_returns_unsupported() {
    let drv = driver_for_test();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    assert!(matches!(
        rt.block_on(drv.fork_thread("t")),
        Err(DriverError::Unsupported)
    ));
}

#[test]
fn grok_encode_stdin_message_returns_none() {
    // Turn-exit: actor never writes stdin for deliveries; prompt via -p on
    // (re)spawn. Matches gemini/claude/kimi behavior for is_turn_exit drivers.
    let drv = driver_for_test();
    assert!(drv
        .encode_stdin_message("hello", None, MessageMode::User)
        .is_none());
    assert!(drv
        .encode_stdin_message("note", Some("sid"), MessageMode::Notification)
        .is_none());
}

#[test]
fn grok_skill_search_paths_workspace_grok_then_homes() {
    let drv = driver_for_test();
    let workspace = std::path::Path::new("/tmp/grok-ws");
    let paths = drv.skill_search_paths(workspace);
    // Priority: workspace .grok/skills, ~/.grok/skills, ~/.agents/skills
    assert!(paths.len() >= 3);
    assert_eq!(paths[0], workspace.join(".grok").join("skills"));
    if let Some(home) = dirs::home_dir() {
        assert!(paths
            .iter()
            .any(|p| p == &home.join(".grok").join("skills")));
        assert!(paths
            .iter()
            .any(|p| p == &home.join(".agents").join("skills")));
    }
}

#[test]
fn grok_as_exit_code_classifier_returns_some() {
    let drv = driver_for_test();
    assert!(drv.as_exit_code_classifier().is_some());
}

// ─── 2. prepare_workspace side effects (AGENTS.md at root + .grok dir) ───

#[test]
fn grok_prepare_workspace_creates_grok_dir_and_no_agents_for_empty_prompt() {
    let tmp = tempfile::tempdir().unwrap();
    let drv = driver_for_test();
    let cfg = DriverAgentConfig {
        runtime: "grok",
        model: "grok-composer-2.5-fast",
        working_runtime: "grok",
        working_model: "grok-composer-2.5-fast",
        env_vars: &[],
    };
    drv.prepare_workspace(tmp.path(), &cfg, "agent-prep", "")
        .unwrap();
    assert!(
        tmp.path().join(".grok").is_dir(),
        "prepare must ensure .grok/ dir for skills"
    );
    assert!(
        !tmp.path().join("AGENTS.md").exists(),
        "empty system_prompt must not create AGENTS.md"
    );
}

#[test]
fn grok_prepare_workspace_writes_agents_md_at_root_when_system_prompt_present() {
    let tmp = tempfile::tempdir().unwrap();
    let drv = driver_for_test();
    let cfg = DriverAgentConfig {
        runtime: "grok",
        model: "grok-composer-2.5-fast",
        working_runtime: "grok",
        working_model: "grok-composer-2.5-fast",
        env_vars: &[],
    };
    let prompt = "You are a helpful Grok agent.\nUse chat__ tools for platform.\n";
    drv.prepare_workspace(tmp.path(), &cfg, "agent-g", prompt)
        .unwrap();
    let md = tmp.path().join("AGENTS.md");
    assert!(md.exists(), "AGENTS.md at workspace root");
    assert_eq!(std::fs::read_to_string(&md).unwrap(), prompt);
    // .grok dir also ensured
    assert!(tmp.path().join(".grok").is_dir());
}

// ─── 3. AGENTS.md writer direct (byte parity, like kimi) ──────────────────

#[test]
fn grok_agents_md_byte_identical() {
    let tmp = tempfile::tempdir().unwrap();
    let work_dir = tmp.path();
    let system_prompt = "Platform contract for Grok.\nFollow AGENTS.md rules.\n";

    write_grok_agents_md(work_dir, system_prompt).unwrap();

    let agents_md = work_dir.join("AGENTS.md");
    assert!(agents_md.exists(), "AGENTS.md must be written at root");

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
fn grok_agents_md_empty_prompt_is_noop() {
    let tmp = tempfile::tempdir().unwrap();
    let work_dir = tmp.path();
    write_grok_agents_md(work_dir, "").unwrap();
    let agents_md = work_dir.join("AGENTS.md");
    assert!(
        !agents_md.exists(),
        "empty system_prompt must NOT create AGENTS.md"
    );
}

// ─── 4. spawn skips MCP config.toml when CLI transport is default ───────────

#[tokio::test]
async fn grok_spawn_skips_config_toml_when_cli_transport_is_default() {
    let tmp = tempfile::tempdir().unwrap();
    let work_dir = tmp.path();

    // Use a real existing binary on macOS/Linux so .spawn() of the Child
    // succeeds (we ignore the child). /bin/false is absent on modern macOS.
    let grok_dummy = PathBuf::from("/usr/bin/true");
    let drv = GrokDriver::new(grok_dummy);

    let env: Vec<(String, String)> = vec![("XAI_API_KEY".to_string(), "xai-test-123".to_string())];

    let spawn_cfg = SpawnConfig {
        working_dir: work_dir,
        model: "grok-beta",
        mcp_config: None,
        resume_session: Some("019e89ac-d61e-70f2-be42-30dba3d2ff43"),
        agent_id: "grok-spawn-agent",
        server_url: "http://127.0.0.1:8090",
        auth_token: "scoped-tok-for-spawn",
        system_prompt: "PLATFORM",
        initial_prompt: "Do the thing",
        env_vars: &env,
    };

    // Write happens inside driver::spawn *before* the actual Command::spawn.
    // Spawn of child may be ignored; file must exist.
    let _ = drv.spawn(&spawn_cfg);

    let config_path = work_dir.join(".grok").join("config.toml");
    assert!(
        !config_path.exists(),
        "CLI transport must not write .grok/config.toml (MCP bridge skipped)"
    );
    assert_eq!(
        drv.platform_action_transport(),
        cocli_driver_core::PlatformActionTransport::Cli
    );
}

// ─── 5. ExitCodeClassifier (per spike + clarification: 1 treated Normal) ──

#[test]
fn grok_classify_exit_code_0_is_normal() {
    let drv = driver_for_test();
    let clf = drv.as_exit_code_classifier().unwrap();
    assert_eq!(clf.classify_exit_code(0), ExitCodeClass::Normal);
}

#[test]
fn grok_classify_exit_code_1_is_normal() {
    // Per clarification decision: treat 1 (auth/config/session-err) as Normal
    // so actor's general error + retry paths (plus the Error event) handle it.
    // Matches claude/kimi "else Normal" for non-sig.
    let drv = driver_for_test();
    let clf = drv.as_exit_code_classifier().unwrap();
    assert_eq!(clf.classify_exit_code(1), ExitCodeClass::Normal);
}

#[test]
fn grok_classify_exit_code_130_is_cancelled() {
    let drv = driver_for_test();
    let clf = drv.as_exit_code_classifier().unwrap();
    assert_eq!(clf.classify_exit_code(130), ExitCodeClass::Cancelled);
}

#[test]
fn grok_classify_exit_code_143_is_cancelled() {
    // SIGTERM per spike docs.
    let drv = driver_for_test();
    let clf = drv.as_exit_code_classifier().unwrap();
    assert_eq!(clf.classify_exit_code(143), ExitCodeClass::Cancelled);
}

#[test]
fn grok_classify_exit_code_other_is_normal() {
    let drv = driver_for_test();
    let clf = drv.as_exit_code_classifier().unwrap();
    for code in [42, 255, -1, 124] {
        assert_eq!(
            clf.classify_exit_code(code),
            ExitCodeClass::Normal,
            "exit code {code} should fall to Normal"
        );
    }
}

// ─── 6. parse_event smoke (uses conv + parse_line; end expands to 2) ──────

#[test]
fn grok_parse_event_text_delta() {
    let drv = driver_for_test();
    let line = r#"{"type":"text","data":" from grok"}"#;
    let evs = drv.parse_event(line);
    assert_eq!(evs.len(), 1);
    match &evs[0] {
        DriverEvent::TextDelta { text } => assert_eq!(text, " from grok"),
        other => panic!("expected TextDelta, got {other:?}"),
    }
}

#[test]
fn grok_parse_event_thought_delta() {
    let drv = driver_for_test();
    let line = r#"{"type":"thought","data":" using chat__send_message"}"#;
    let evs = drv.parse_event(line);
    assert_eq!(evs.len(), 1);
    match &evs[0] {
        DriverEvent::ThinkingDelta { text } => assert_eq!(text, " using chat__send_message"),
        other => panic!("expected ThinkingDelta, got {other:?}"),
    }
}

#[test]
fn grok_parse_event_end_yields_session_started_and_turn_end() {
    let drv = driver_for_test();
    let line = r#"{"type":"end","stopReason":"EndTurn","sessionId":"019e89ac-d61e-70f2-be42-30dba3d2ff43","requestId":"req-1"}"#;
    let evs = drv.parse_event(line);
    assert_eq!(evs.len(), 2, "end expands via conv");
    match &evs[0] {
        DriverEvent::SessionStarted { session_id } => {
            assert_eq!(session_id, "019e89ac-d61e-70f2-be42-30dba3d2ff43");
        }
        other => panic!("expected SessionStarted, got {other:?}"),
    }
    match &evs[1] {
        DriverEvent::TurnEnd { status, .. } => {
            use cocli_driver_core::types::TurnStatus;
            assert_eq!(*status, TurnStatus::Completed);
        }
        other => panic!("expected TurnEnd, got {other:?}"),
    }
}

#[test]
fn grok_parse_event_error() {
    let drv = driver_for_test();
    let line = r#"{"type":"error","message":"Session does not exist"}"#;
    let evs = drv.parse_event(line);
    assert_eq!(evs.len(), 1);
    match &evs[0] {
        DriverEvent::Error { message, .. } => {
            assert_eq!(message, "Session does not exist");
        }
        other => panic!("expected Error, got {other:?}"),
    }
}

#[test]
fn grok_parse_event_unknown_line_is_unknown() {
    let drv = driver_for_test();
    let evs = drv.parse_event("not json");
    assert_eq!(evs.len(), 1);
    assert!(matches!(&evs[0], DriverEvent::Unknown));
}

// ─── 7. as_* accessors for non-implemented subtraits ──────────────────────

#[test]
fn grok_driver_exposes_process_factory() {
    let drv = driver_for_test();
    assert!(drv.as_process_factory().is_some());
    assert!(drv.as_stdin_binder().is_none());
    assert!(drv.as_turn_interruptor().is_none());
    assert!(drv.as_process_initializer().is_none());
    assert!(drv.as_session_file_gc().is_none());
}

#[test]
fn grok_process_driver_resolves_turn_usage_from_signals_and_unified() {
    use cocli_driver_grok::usage::encode_grok_session_cwd;

    let tmp = tempfile::tempdir().unwrap();
    let grok_home = tmp.path().join("grok-home");
    let work_dir = tmp.path().join("workspace");
    std::fs::create_dir_all(work_dir.join(".grok")).unwrap();
    let session_id = "019eeae5-6a3f-7783-8fc3-baf0ff745a55";

    let signals_dir = grok_home
        .join("sessions")
        .join(encode_grok_session_cwd(&work_dir))
        .join(session_id);
    std::fs::create_dir_all(&signals_dir).unwrap();
    std::fs::write(
        signals_dir.join("signals.json"),
        r#"{"contextTokensUsed":13296,"contextWindowTokens":200000}"#,
    )
    .unwrap();

    std::env::set_var("GROK_HOME", grok_home.as_os_str());
    assert_eq!(
        cocli_driver_grok::grok_home_dir(),
        grok_home,
        "GROK_HOME must point at the test telemetry root"
    );
    let factory = driver_for_test();
    let spawn_cfg = SpawnConfig {
        working_dir: &work_dir,
        model: "grok-composer-2.5-fast",
        mcp_config: None,
        resume_session: None,
        agent_id: "agent-grok-usage",
        server_url: "http://127.0.0.1:8090",
        auth_token: "tok",
        system_prompt: "",
        initial_prompt: "hello",
        env_vars: &[],
    };
    let process = factory
        .as_process_factory()
        .unwrap()
        .new_process(&spawn_cfg);

    std::fs::create_dir_all(grok_home.join("logs")).unwrap();
    std::fs::write(
        grok_home.join("logs").join("unified.jsonl"),
        r#"{"msg":"shell.turn.inference_done","sid":"019eeae5-6a3f-7783-8fc3-baf0ff745a55","pid":42,"ctx":{"prompt_tokens":19393,"cached_prompt_tokens":7635,"completion_tokens":168,"reasoning_tokens":0}}"#,
    )
    .unwrap();

    let line = format!(
        r#"{{"type":"end","stopReason":"EndTurn","sessionId":"{session_id}","requestId":"req-usage"}}"#
    );
    let evs = process.parse_event(&line);
    assert_eq!(evs.len(), 2);
    match &evs[1] {
        DriverEvent::TurnEnd {
            input_tokens,
            output_tokens,
            cache_read_tokens,
            ..
        } => {
            assert_eq!(*input_tokens, 13_296);
            assert_eq!(*output_tokens, 168);
            assert_eq!(*cache_read_tokens, 7635);
        }
        other => panic!("expected TurnEnd, got {other:?}"),
    }
    assert_eq!(process.context_window_tokens(), Some(200_000));
    std::env::remove_var("GROK_HOME");
}
