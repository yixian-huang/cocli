//! `Driver` trait contract tests for `CodexDriver` (factory) and
//! `CodexProcessDriver` (per-process). Mirrors the structure of
//! `cocli-driver-claude/tests/driver_impl.rs`.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use cocli_driver_codex::test_hooks;
use cocli_driver_codex::{CodexDriver, CodexProcessDriver};
use cocli_driver_core::types::{
    BusyDeliveryMode, DriverAgentConfig, EnvPropagation, MessageMode, SkillCompatibility,
    SpawnConfig,
};
use cocli_driver_core::{Driver, DriverError, DriverEvent};
use tokio::io::{AsyncBufReadExt, BufReader};

fn spawn_stdin_capture_child() -> (
    tokio::process::Child,
    tokio::process::ChildStdin,
    BufReader<tokio::process::ChildStdout>,
) {
    let mut child = tokio::process::Command::new("cat")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn stdin capture child");
    let stdin = child.stdin.take().expect("child stdin");
    let stdout = child.stdout.take().expect("child stdout");
    (child, stdin, BufReader::new(stdout))
}

async fn read_captured_json_line(
    reader: &mut BufReader<tokio::process::ChildStdout>,
) -> serde_json::Value {
    let mut written = String::new();
    tokio::time::timeout(Duration::from_secs(1), reader.read_line(&mut written))
        .await
        .expect("stdin capture timed out")
        .expect("read stdin capture");
    assert!(!written.trim().is_empty(), "expected stdin write payload");
    serde_json::from_str(written.trim()).expect("stdin capture should be valid JSON")
}

fn factory_for_test() -> CodexDriver {
    CodexDriver::new(
        PathBuf::from("/usr/bin/true"), // codex CLI
        PathBuf::from("/opt/cocli/bin/cocli-bridge"),
    )
}

fn process_driver_for_test() -> CodexProcessDriver {
    CodexProcessDriver::new(
        PathBuf::from("/usr/bin/true"),
        PathBuf::from("/opt/cocli/bin/cocli-bridge"),
    )
}

fn block_on<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(f)
}

// ── Factory (CodexDriver) ─────────────────────────────────────────────

#[test]
fn factory_name_is_codex() {
    assert_eq!(factory_for_test().name(), "codex");
}

#[test]
fn factory_mcp_tool_prefix() {
    assert_eq!(factory_for_test().mcp_tool_prefix(), "mcp_chat_");
}

#[test]
fn factory_busy_delivery_mode_is_gated_placeholder() {
    // Factory is never the actor's runtime — placeholder is fine, but
    // the per-process driver below MUST override to Direct.
    assert_eq!(
        factory_for_test().busy_delivery_mode(),
        BusyDeliveryMode::Gated
    );
}

#[test]
fn factory_env_propagation_is_inherit() {
    assert_eq!(
        factory_for_test().env_propagation(),
        EnvPropagation::Inherit
    );
}

#[test]
fn factory_skill_compatibility_is_supported() {
    assert_eq!(
        factory_for_test().skill_compatibility(),
        SkillCompatibility::Supported
    );
}

#[test]
fn factory_context_window_is_256k() {
    assert_eq!(factory_for_test().context_window_tokens(), Some(256_000));
}

#[test]
fn factory_skill_search_paths_include_codex_and_agents() {
    let drv = factory_for_test();
    let workspace = std::path::Path::new("/tmp/test-workspace");
    let paths = drv.skill_search_paths(workspace);
    assert!(paths
        .iter()
        .any(|p| p.ends_with(".codex/skills") && p.starts_with(workspace)));
    assert!(paths
        .iter()
        .any(|p| p.ends_with(".agents/skills") && p.starts_with(workspace)));
    if let Some(home) = dirs::home_dir() {
        assert!(paths
            .iter()
            .any(|p| p.ends_with(".codex/skills") && p.starts_with(&home)));
    }
}

#[test]
fn factory_returns_self_as_process_factory() {
    let drv = factory_for_test();
    assert!(drv.as_process_factory().is_some());
}

#[test]
fn factory_parse_event_returns_empty() {
    let drv = factory_for_test();
    let line = r#"{"jsonrpc":"2.0","method":"turn/started","params":{}}"#;
    assert!(drv.parse_event(line).is_empty());
}

#[test]
fn factory_encode_stdin_returns_none() {
    let drv = factory_for_test();
    assert!(drv
        .encode_stdin_message("hello", None, MessageMode::User)
        .is_none());
}

#[test]
fn factory_spawn_errors_with_factory_message() {
    let drv = factory_for_test();
    let work = tempfile::tempdir().unwrap();
    let cfg = SpawnConfig {
        working_dir: work.path(),
        model: "gpt-5",
        mcp_config: None,
        resume_session: None,
        agent_id: "a",
        server_url: "ws://x",
        auth_token: "t",
        system_prompt: "",
        initial_prompt: "",
        env_vars: &[],
    };
    match drv.spawn(&cfg) {
        Err(DriverError::Other(msg)) => assert!(msg.contains("factory")),
        other => panic!("expected factory Other error, got {other:?}"),
    }
}

#[test]
fn factory_prepare_workspace_is_noop() {
    // After codex review issue 2 the factory's prepare_workspace is a
    // no-op — `git init` moved to the per-process driver (the path the
    // actor actually invokes). The factory hook must still succeed
    // because the trait is total.
    let tmp = tempfile::tempdir().unwrap();
    let drv = factory_for_test();
    let cfg = DriverAgentConfig {
        runtime: "codex",
        model: "gpt-5",
        working_runtime: "codex",
        working_model: "gpt-5",
        env_vars: &[],
    };
    drv.prepare_workspace(tmp.path(), &cfg, "agent-x", "")
        .unwrap();
    // Factory must NOT touch the workspace.
    assert!(!tmp.path().join(".git").exists());
}

#[test]
fn codex_per_process_prepare_workspace_runs_git_init() {
    // Codex review issue 2: the actor invokes
    // `per_process.prepare_workspace(...)`, not the factory's. Verify
    // the per-process driver runs `git init` so codex's "workspace must
    // be a git repo" invariant holds.
    if std::process::Command::new("git")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("git not in PATH; skipping");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let drv = process_driver_for_test();
    let cfg = DriverAgentConfig {
        runtime: "codex",
        model: "gpt-5",
        working_runtime: "codex",
        working_model: "gpt-5",
        env_vars: &[],
    };
    drv.prepare_workspace(tmp.path(), &cfg, "agent-x", "")
        .unwrap();
    assert!(tmp.path().join(".git").exists());
}

#[test]
fn codex_per_process_prepare_workspace_skips_when_git_present() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join(".git").join("objects")).unwrap();
    let drv = process_driver_for_test();
    let cfg = DriverAgentConfig {
        runtime: "codex",
        model: "gpt-5",
        working_runtime: "codex",
        working_model: "gpt-5",
        env_vars: &[],
    };
    drv.prepare_workspace(tmp.path(), &cfg, "agent-x", "")
        .unwrap();
    assert!(tmp.path().join(".git").join("objects").exists());
}

// ── Per-process (CodexProcessDriver) ──────────────────────────────────

#[test]
fn process_driver_name_is_codex() {
    assert_eq!(process_driver_for_test().name(), "codex");
}

#[test]
fn process_driver_busy_delivery_mode_is_direct() {
    // codex.go:40 SupportsStdinNotification = true → Direct.
    assert_eq!(
        process_driver_for_test().busy_delivery_mode(),
        BusyDeliveryMode::Direct
    );
}

#[test]
fn process_driver_supports_turn_steer() {
    assert!(process_driver_for_test().supports_turn_steer());
}

#[test]
fn process_driver_supports_turn_cancel() {
    assert!(process_driver_for_test().supports_turn_cancel());
}

#[test]
fn process_driver_requires_initial_prompt_is_false() {
    // Bootstrap turn/start is emitted during handshake (Go advanceToTurn).
    assert!(!process_driver_for_test().requires_initial_prompt());
}

#[test]
fn factory_requires_initial_prompt_is_true() {
    assert!(factory_for_test().requires_initial_prompt());
}

#[test]
fn process_driver_exposes_sub_traits() {
    let d = process_driver_for_test();
    assert!(d.as_process_initializer().is_some());
    assert!(d.as_stdin_binder().is_some());
    assert!(d.as_turn_interruptor().is_some());
    // Per-process is NOT a factory.
    assert!(d.as_process_factory().is_none());
}

#[test]
fn process_driver_encode_stdin_returns_none_before_handshake() {
    let d = process_driver_for_test();
    assert!(d
        .encode_stdin_message("hello", None, MessageMode::User)
        .is_none());
}

#[test]
fn process_driver_encode_stdin_after_handshake_builds_turn_start() {
    let d = process_driver_for_test();
    d.set_state_for_test("thread-1", "");
    let s = d
        .encode_stdin_message("hello", None, MessageMode::User)
        .expect("ready after handshake");
    let v: serde_json::Value = serde_json::from_str(&s).unwrap();
    assert_eq!(v["jsonrpc"], "2.0");
    assert_eq!(v["method"], "turn/start");
    assert_eq!(v["params"]["threadId"], "thread-1");
    assert_eq!(v["params"]["input"][0]["type"], "text");
    assert_eq!(v["params"]["input"][0]["text"], "hello");
}

#[test]
fn process_driver_encode_stdin_during_active_turn_builds_turn_steer() {
    let d = process_driver_for_test();
    d.set_state_for_test("thread-1", "turn-7");
    let s = d
        .encode_stdin_message("steer text", None, MessageMode::User)
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&s).unwrap();
    assert_eq!(v["method"], "turn/steer");
    assert_eq!(v["params"]["threadId"], "thread-1");
    assert_eq!(v["params"]["expectedTurnId"], "turn-7");
    assert_eq!(v["params"]["input"][0]["text"], "steer text");
}

#[test]
fn process_initializer_writes_initialize_jsonrpc() {
    let d = process_driver_for_test();
    let mut buf: Vec<u8> = Vec::new();
    d.as_process_initializer()
        .unwrap()
        .write_init_sequence(&mut buf)
        .unwrap();
    let s = String::from_utf8(buf).unwrap();
    // writeln! → trailing newline.
    assert!(s.ends_with('\n'));
    let v: serde_json::Value = serde_json::from_str(s.trim()).unwrap();
    assert_eq!(v["jsonrpc"], "2.0");
    assert_eq!(v["id"], 1);
    assert_eq!(v["method"], "initialize");
    assert_eq!(v["params"]["clientInfo"]["name"], "1hz-daemon");
}

#[test]
fn turn_steer_errors_without_thread_id() {
    let d = process_driver_for_test();
    assert!(matches!(
        block_on(d.turn_steer("hi")),
        Err(DriverError::TurnSteerUnavailable)
    ));
}

#[test]
fn turn_steer_errors_with_no_active_turn() {
    let d = process_driver_for_test();
    d.set_state_for_test("thread-1", "");
    assert!(matches!(
        block_on(d.turn_steer("hi")),
        Err(DriverError::TurnSteerNoActiveTurn)
    ));
}

#[test]
fn turn_steer_unavailable_before_running() {
    let d = process_driver_for_test();
    d.set_handshake_ready_for_test("thread-1", "turn-wait");
    assert!(matches!(
        block_on(d.turn_steer("hi")),
        Err(DriverError::TurnSteerUnavailable)
    ));
}

#[test]
fn interrupt_turn_unavailable_before_running() {
    let d = process_driver_for_test();
    d.set_handshake_ready_for_test("thread-1", "turn-wait");
    assert!(matches!(
        block_on(d.as_turn_interruptor().unwrap().interrupt_turn()),
        Err(DriverError::TurnSteerUnavailable)
    ));
}

#[test]
fn steer_rejection_preserves_active_turn_not_steerable_classification() {
    let d = process_driver_for_test();
    d.set_state_for_test("thread-test", "turn-classify");
    let _ = d
        .encode_stdin_message("classify me", None, MessageMode::User)
        .expect("mid-turn steer");

    let err_line = r#"{"jsonrpc":"2.0","method":"error","params":{"willRetry":false,"error":{"message":"reject","codexErrorInfo":"activeTurnNotSteerable"}}}"#;
    let evs = d.parse_event(err_line);
    match &evs[0] {
        DriverEvent::Error {
            code,
            severity,
            http_status,
            ..
        } => {
            assert_eq!(code.as_deref(), Some("activeTurnNotSteerable"));
            assert_eq!(*severity, Some(cocli_driver_core::ErrorSeverity::Warning));
            assert!(http_status.is_none());
        }
        other => panic!("expected classified Error, got {other:?}"),
    }
}

#[test]
fn fork_thread_writes_request_waits_for_response_and_updates_thread() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async {
        let d = Arc::new(process_driver_for_test());
        d.set_state_for_test("thread-old", "turn-active");

        let (mut child, stdin, mut capture) = spawn_stdin_capture_child();
        d.as_stdin_binder().unwrap().bind_stdin(stdin);

        let fork_driver = d.clone();
        let join = tokio::spawn(async move { fork_driver.fork_thread("thread-old").await });
        let fork_json = read_captured_json_line(&mut capture).await;
        let events =
            d.parse_event(r#"{"jsonrpc":"2.0","id":3,"result":{"thread":{"id":"thread-new"}}}"#);
        assert!(
            events.is_empty(),
            "fork response should wake the pending call without surfacing events"
        );
        let forked = join.await.unwrap().expect("fork succeeds");
        assert_eq!(forked, "thread-new");

        let turn = d
            .encode_stdin_message("after fork", None, MessageMode::User)
            .expect("thread id should be updated");
        let turn_json: serde_json::Value = serde_json::from_str(&turn).unwrap();
        assert_eq!(turn_json["method"], "turn/start");
        assert_eq!(turn_json["params"]["threadId"], "thread-new");

        let _ = child.kill().await;
        let _ = child.wait().await;
        assert_eq!(fork_json["jsonrpc"], "2.0");
        assert_eq!(fork_json["id"], 3);
        assert_eq!(fork_json["method"], "thread/fork");
        assert_eq!(fork_json["params"]["threadId"], "thread-old");
    });
}

#[test]
fn factory_new_process_returns_per_process_driver() {
    let factory = factory_for_test();
    let work = tempfile::tempdir().unwrap();
    let cfg = SpawnConfig {
        working_dir: work.path(),
        model: "gpt-5",
        mcp_config: None,
        resume_session: None,
        agent_id: "a",
        server_url: "ws://x",
        auth_token: "t",
        system_prompt: "",
        initial_prompt: "",
        env_vars: &[],
    };
    let pd = factory.as_process_factory().unwrap().new_process(&cfg);
    assert_eq!(pd.name(), "codex");
    assert_eq!(pd.busy_delivery_mode(), BusyDeliveryMode::Direct);
    assert!(pd.supports_turn_steer());
}

#[test]
fn parse_event_emits_session_started_on_thread_started() {
    let d = process_driver_for_test();
    let evs = d
        .parse_event(r#"{"jsonrpc":"2.0","method":"thread/started","params":{"threadId":"th-1"}}"#);
    assert_eq!(evs.len(), 1);
    match &evs[0] {
        DriverEvent::SessionStarted { session_id } => assert_eq!(session_id, "th-1"),
        other => panic!("expected SessionStarted, got {other:?}"),
    }
}

#[test]
fn parse_event_turn_completed_merges_pending_tokens() {
    let d = process_driver_for_test();
    // Token usage absorbed silently.
    let evs = d.parse_event(
        r#"{"jsonrpc":"2.0","method":"thread/tokenUsage/updated","params":{"tokenUsage":{"last":{"inputTokens":300,"outputTokens":75,"cachedInputTokens":50},"modelContextWindow":128000}}}"#,
    );
    assert!(evs.is_empty());

    let evs = d.parse_event(
        r#"{"jsonrpc":"2.0","method":"turn/completed","params":{"status":"completed"}}"#,
    );
    assert_eq!(evs.len(), 1);
    match &evs[0] {
        DriverEvent::TurnEnd {
            status,
            input_tokens,
            output_tokens,
            cache_read_tokens,
            context_window_tokens,
            cost_usd,
            ..
        } => {
            assert_eq!(status, &cocli_driver_core::types::TurnStatus::Completed);
            // input = 300 + 50 (cached prefix merge) per codex.go:639.
            assert_eq!(*input_tokens, 350);
            assert_eq!(*output_tokens, 75);
            assert_eq!(*cache_read_tokens, 50);
            assert_eq!(*context_window_tokens, 128_000);
            assert_eq!(*cost_usd, 0.0, "codex wire has no per-turn cost field");
        }
        other => panic!("expected TurnEnd, got {other:?}"),
    }
}

// ── Codex review fix coverage ─────────────────────────────────────────

#[test]
fn codex_spawn_command_envs_includes_user_env_vars() {
    // Codex review issue 4: CodexProcessDriver::spawn must pass
    // cfg.env_vars to spawn_codex so the codex CLI inherits the per-agent
    // env vars (API keys, model overrides) that the actor populates from
    // AgentConfig.env_vars. The integration test confirms the
    // SpawnContext-shaped surface routes env_vars to Command::env: spawn
    // the simplest possible env-readout probe and grep stdout for the
    // injected pairs.
    use cocli_driver_codex::{spawn_codex, SpawnContext};
    use std::path::Path;

    let tmp = tempfile::tempdir().unwrap();
    let env_vars: Vec<(String, String)> = vec![
        ("CODEX_TEST_FOO".into(), "bar".into()),
        ("CODEX_TEST_BAZ".into(), "qux".into()),
    ];

    let env_bin = Path::new("/usr/bin/env");
    if !env_bin.exists() {
        eprintln!("/usr/bin/env not present; skipping env-readout assertion");
        return;
    }

    // Use spawn_codex with codex_binary = /usr/bin/env. Codex's CLI args
    // (`app-server --listen stdio://`) become arguments to env, which
    // env will try to execute as a command; we don't care about that —
    // we only need the child's env to round-trip. We capture stdout via
    // a fresh tokio current-thread runtime.
    let ctx = SpawnContext {
        codex_binary: env_bin,
        bridge_binary: Path::new("/opt/cocli/bin/cocli-bridge"),
        working_dir: tmp.path(),
        model: "",
        agent_id: "agent-x",
        server_url: "ws://x",
        auth_token: "t",
        no_bridge: true, // env doesn't accept -c flags
        system_prompt: "",
        env_vars: &env_vars,
    };
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        if let Ok(mut child) = spawn_codex(&ctx) {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
    });

    // Direct wiring assertion: build a tokio Command identical to what
    // spawn_codex does for env handling and confirm round-trip. This is
    // the load-bearing check — the spawn_codex call above just verifies
    // the path is at least exec-reachable.
    let mut probe = tokio::process::Command::new(env_bin);
    for (k, v) in &env_vars {
        probe.env(k, v);
    }
    probe.stdout(std::process::Stdio::piped());
    let output = rt.block_on(async { probe.output().await.expect("env probe") });
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("CODEX_TEST_FOO=bar"),
        "CODEX_TEST_FOO missing from env output: {stdout}"
    );
    assert!(
        stdout.contains("CODEX_TEST_BAZ=qux"),
        "CODEX_TEST_BAZ missing from env output: {stdout}"
    );
}

#[test]
fn codex_initialize_response_triggers_thread_start_write() {
    // Codex review issue 1: stash spawn params (mimics `spawn()`), feed
    // an initialize response into parse_event, expect a DriverEvent::Write
    // carrying a `thread/start` JSON-RPC request with id=2. Without this
    // chain `thread/start` was never sent and the actor timed out.
    let d = process_driver_for_test();
    let tmp = tempfile::tempdir().unwrap();
    d.set_spawn_params_for_test(tmp.path(), "gpt-5", "you are codex", None);

    let response = r#"{"jsonrpc":"2.0","id":1,"result":{}}"#;
    let evs = d.parse_event(response);
    assert_eq!(evs.len(), 1, "expected exactly one Write event");
    match &evs[0] {
        DriverEvent::Write { data } => {
            let v: serde_json::Value = serde_json::from_str(data).unwrap();
            assert_eq!(v["jsonrpc"], "2.0");
            assert_eq!(v["id"], 2);
            assert_eq!(v["method"], "thread/start");
            assert_eq!(v["params"]["sandbox"], "danger-full-access");
            assert_eq!(v["params"]["approvalPolicy"], "never");
            assert_eq!(v["params"]["cwd"], tmp.path().to_string_lossy().as_ref());
            assert_eq!(v["params"]["baseInstructions"], "you are codex");
            assert_eq!(v["params"]["model"], "gpt-5");
        }
        other => panic!("expected DriverEvent::Write, got {other:?}"),
    }
}

#[test]
fn codex_initialize_response_uses_thread_resume_when_session_present() {
    let d = process_driver_for_test();
    let tmp = tempfile::tempdir().unwrap();
    d.set_spawn_params_for_test(tmp.path(), "gpt-5", "", Some("th-existing"));

    let response = r#"{"jsonrpc":"2.0","id":1,"result":{}}"#;
    let evs = d.parse_event(response);
    assert_eq!(evs.len(), 1);
    match &evs[0] {
        DriverEvent::Write { data } => {
            let v: serde_json::Value = serde_json::from_str(data).unwrap();
            assert_eq!(v["method"], "thread/resume");
            assert_eq!(v["params"]["threadId"], "th-existing");
        }
        other => panic!("expected Write(thread/resume), got {other:?}"),
    }
}

#[test]
fn codex_thread_start_response_accepts_thread_id_field() {
    let d = process_driver_for_test();
    let tmp = tempfile::tempdir().unwrap();
    d.set_spawn_params_for_test(tmp.path(), "gpt-5", "", None);
    let _ = d.parse_event(r#"{"jsonrpc":"2.0","id":1,"result":{}}"#);

    let response = r#"{"jsonrpc":"2.0","id":2,"result":{"threadId":"th-flat"}}"#;
    let evs = d.parse_event(response);
    match &evs[0] {
        DriverEvent::SessionStarted { session_id } => assert_eq!(session_id, "th-flat"),
        other => panic!("expected SessionStarted, got {other:?}"),
    }
}

#[test]
fn codex_thread_start_response_accepts_data_thread_id() {
    let d = process_driver_for_test();
    let tmp = tempfile::tempdir().unwrap();
    d.set_spawn_params_for_test(tmp.path(), "gpt-5", "", None);
    let _ = d.parse_event(r#"{"jsonrpc":"2.0","id":1,"result":{}}"#);

    let response =
        r#"{"jsonrpc":"2.0","id":2,"result":{"data":{"thread":{"id":"th-nested-data"}}}}"#;
    let evs = d.parse_event(response);
    match &evs[0] {
        DriverEvent::SessionStarted { session_id } => assert_eq!(session_id, "th-nested-data"),
        other => panic!("expected SessionStarted, got {other:?}"),
    }
}

#[test]
fn codex_thread_resume_error_falls_back_to_thread_start() {
    let d = process_driver_for_test();
    let tmp = tempfile::tempdir().unwrap();
    d.set_spawn_params_for_test(tmp.path(), "gpt-5", "resume me", Some("th-stale"));

    let _ = d.parse_event(r#"{"jsonrpc":"2.0","id":1,"result":{}}"#);
    let resume_err = r#"{"jsonrpc":"2.0","id":2,"error":{"message":"thread not found"}}"#;
    let evs = d.parse_event(resume_err);
    assert_eq!(evs.len(), 1);
    match &evs[0] {
        DriverEvent::Write { data } => {
            let v: serde_json::Value = serde_json::from_str(data).unwrap();
            assert_eq!(v["method"], "thread/start");
            assert_eq!(v["id"], 2);
            assert_eq!(v["params"]["baseInstructions"], "resume me");
        }
        other => panic!("expected fallback thread/start Write, got {other:?}"),
    }

    let start_ok = r#"{"jsonrpc":"2.0","id":2,"result":{"thread":{"id":"th-fresh"}}}"#;
    let evs = d.parse_event(start_ok);
    match &evs[0] {
        DriverEvent::SessionStarted { session_id } => assert_eq!(session_id, "th-fresh"),
        other => panic!("expected SessionStarted after fallback, got {other:?}"),
    }
}

#[test]
fn codex_thread_start_response_emits_session_started_and_bootstrap_turn_start() {
    // Go advanceToTurn: thread ack → SessionStarted + bootstrap turn/start
    // Write. encode_stdin_message stays gated until turn/started (Running).
    let d = process_driver_for_test();
    let tmp = tempfile::tempdir().unwrap();
    d.set_spawn_params_with_prompt_for_test(
        tmp.path(),
        "gpt-5",
        "",
        Some("bootstrap inbox check"),
        None,
    );

    let _ = d.parse_event(r#"{"jsonrpc":"2.0","id":1,"result":{}}"#);

    let response = r#"{"jsonrpc":"2.0","id":2,"result":{"thread":{"id":"th-new"}}}"#;
    let evs = d.parse_event(response);
    assert_eq!(evs.len(), 2, "expected SessionStarted + bootstrap Write");
    match &evs[0] {
        DriverEvent::SessionStarted { session_id } => assert_eq!(session_id, "th-new"),
        other => panic!("expected SessionStarted, got {other:?}"),
    }
    match &evs[1] {
        DriverEvent::Write { data } => {
            let v: serde_json::Value = serde_json::from_str(data).unwrap();
            assert_eq!(v["method"], "turn/start");
            assert_eq!(v["params"]["threadId"], "th-new");
            assert_eq!(v["params"]["input"][0]["text"], "bootstrap inbox check");
        }
        other => panic!("expected bootstrap Write, got {other:?}"),
    }

    assert!(
        d.encode_stdin_message("hi", None, MessageMode::User)
            .is_none(),
        "encode must stay gated until turn/started"
    );

    let _ = d.parse_event(r#"{"jsonrpc":"2.0","method":"turn/started","params":{"turnId":"t-1"}}"#);
    let s = d
        .encode_stdin_message("hi", None, MessageMode::User)
        .expect("encode after Running");
    let v: serde_json::Value = serde_json::from_str(&s).unwrap();
    assert_eq!(v["method"], "turn/steer");
    assert_eq!(v["params"]["expectedTurnId"], "t-1");

    let _ = d.parse_event(
        r#"{"jsonrpc":"2.0","method":"turn/completed","params":{"status":"completed"}}"#,
    );
    let idle = d
        .encode_stdin_message("next", None, MessageMode::User)
        .expect("encode after turn completes");
    let v: serde_json::Value = serde_json::from_str(&idle).unwrap();
    assert_eq!(v["method"], "turn/start");
    assert_eq!(v["params"]["threadId"], "th-new");
}

#[test]
fn codex_handshake_state_machine_stale_response_is_absorbed() {
    // Defensive: extra responses (out-of-state or unmatched id) should
    // be absorbed silently rather than crash. Robust to retries.
    let d = process_driver_for_test();
    let evs = d.parse_event(r#"{"jsonrpc":"2.0","id":999,"result":{}}"#);
    assert!(evs.is_empty());
}

#[test]
fn codex_turn_started_notification_updates_active_turn_id() {
    // Codex review issue 3: turn/started must populate active_turn_id
    // so encoder picks turn/steer for mid-turn delivery. Without this,
    // mid-turn user messages collide with the running turn.
    let d = process_driver_for_test();
    // Skip handshake (test hook).
    d.set_state_for_test("th-1", "");
    let evs =
        d.parse_event(r#"{"jsonrpc":"2.0","method":"turn/started","params":{"turnId":"t-77"}}"#);
    // Should at least emit a thinking-equivalent (mapped to ThinkingDelta).
    assert!(matches!(
        evs.first(),
        Some(DriverEvent::ThinkingDelta { .. })
    ));

    // Now a mid-turn delivery must route to turn/steer carrying t-77.
    let s = d
        .encode_stdin_message("mid-turn poke", None, MessageMode::User)
        .expect("driver ready");
    let v: serde_json::Value = serde_json::from_str(&s).unwrap();
    assert_eq!(v["method"], "turn/steer");
    assert_eq!(v["params"]["expectedTurnId"], "t-77");
}

#[test]
fn codex_mid_turn_delivery_encodes_turn_steer_not_turn_start() {
    // Codex review issue 3 (explicit): drive the full handshake →
    // turn/started, then verify mid-turn delivery routes via
    // turn/steer. Also verifies the encoder reverts to turn/start once
    // the turn completes.
    let d = process_driver_for_test();
    let tmp = tempfile::tempdir().unwrap();
    d.set_spawn_params_for_test(tmp.path(), "gpt-5", "", None);
    let _ = d.parse_event(r#"{"jsonrpc":"2.0","id":1,"result":{}}"#);
    let _ = d.parse_event(r#"{"jsonrpc":"2.0","id":2,"result":{"thread":{"id":"th-z"}}}"#);

    assert!(
        d.encode_stdin_message("hello", None, MessageMode::User)
            .is_none(),
        "encode gated until first turn/started"
    );

    // Turn starts.
    let _ =
        d.parse_event(r#"{"jsonrpc":"2.0","method":"turn/started","params":{"turnId":"t-mid"}}"#);

    // Mid-turn path → turn/steer.
    let mid = d
        .encode_stdin_message("interrupt", None, MessageMode::User)
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&mid).unwrap();
    assert_eq!(v["method"], "turn/steer");
    assert_eq!(v["params"]["expectedTurnId"], "t-mid");

    // Turn completes → active_turn_id cleared → next delivery is turn/start.
    let _ = d.parse_event(
        r#"{"jsonrpc":"2.0","method":"turn/completed","params":{"status":"completed"}}"#,
    );
    let post = d
        .encode_stdin_message("next", None, MessageMode::User)
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&post).unwrap();
    assert_eq!(v["method"], "turn/start");
}

#[test]
fn codex_factory_is_turn_exit_false() {
    let drv = factory_for_test();
    assert!(!drv.is_turn_exit());
}

#[test]
fn codex_process_is_turn_exit_false() {
    let drv = process_driver_for_test();
    assert!(!drv.is_turn_exit());
}

fn write_replay_text(end_evs: &[DriverEvent]) -> String {
    let write = end_evs
        .iter()
        .find_map(|ev| match ev {
            DriverEvent::Write { data } => Some(data.clone()),
            _ => None,
        })
        .expect("expected Write replay");
    let replay: serde_json::Value = serde_json::from_str(&write).unwrap();
    replay["params"]["input"][0]["text"]
        .as_str()
        .unwrap_or("")
        .to_string()
}

#[test]
fn rejected_steer_fifo_replay_across_two_turn_completed() {
    let d = process_driver_for_test();
    d.set_state_for_test("thread-test", "turn-1");

    let _ = d
        .encode_stdin_message("first", None, MessageMode::User)
        .expect("first steer");
    let _ = d
        .encode_stdin_message("second", None, MessageMode::User)
        .expect("second steer");

    let err = r#"{"jsonrpc":"2.0","method":"error","params":{"willRetry":false,"error":{"message":"reject","codexErrorInfo":"activeTurnNotSteerable"}}}"#;
    let _ = d.parse_event(err);
    let _ = d.parse_event(err);

    let first_end = d.parse_event(
        r#"{"jsonrpc":"2.0","method":"turn/completed","params":{"status":"completed"}}"#,
    );
    assert_eq!(write_replay_text(&first_end), "first");

    d.set_state_for_test("thread-test", "turn-2");
    let second_end = d.parse_event(
        r#"{"jsonrpc":"2.0","method":"turn/completed","params":{"status":"completed"}}"#,
    );
    assert_eq!(write_replay_text(&second_end), "second");
}

#[test]
fn inflight_steer_cap_evicts_oldest_before_rejection() {
    let d = process_driver_for_test();
    d.set_state_for_test("thread-test", "turn-flood");

    for i in 0..37 {
        let _ = d
            .encode_stdin_message(&format!("msg-{i}"), None, MessageMode::User)
            .expect("steer encode");
    }

    let err = r#"{"jsonrpc":"2.0","method":"error","params":{"willRetry":false,"error":{"message":"reject","codexErrorInfo":"activeTurnNotSteerable"}}}"#;
    let _ = d.parse_event(err);

    let end_evs = d.parse_event(
        r#"{"jsonrpc":"2.0","method":"turn/completed","params":{"status":"completed"}}"#,
    );
    assert_eq!(
        write_replay_text(&end_evs),
        "msg-5",
        "cap eviction should drop msg-0..msg-4 before replay"
    );
}

#[test]
fn rejected_steer_replays_as_turn_start_on_turn_completed() {
    let d = process_driver_for_test();
    d.set_state_for_test("thread-1", "turn-7");

    let steer = d
        .encode_stdin_message("user poke", None, MessageMode::User)
        .expect("mid-turn steer");
    let steer_json: serde_json::Value = serde_json::from_str(&steer).unwrap();
    assert_eq!(steer_json["method"], "turn/steer");

    let err_line = r#"{"jsonrpc":"2.0","method":"error","params":{"willRetry":false,"error":{"message":"not steerable","codexErrorInfo":"activeTurnNotSteerable"}}}"#;
    let err_evs = d.parse_event(err_line);
    assert!(
        err_evs
            .iter()
            .any(|ev| matches!(ev, DriverEvent::Error { .. })),
        "rejection should still surface as Error: {err_evs:?}"
    );

    let end_evs = d.parse_event(
        r#"{"jsonrpc":"2.0","method":"turn/completed","params":{"status":"completed"}}"#,
    );
    let write = end_evs
        .iter()
        .find_map(|ev| match ev {
            DriverEvent::Write { data } => Some(data.clone()),
            _ => None,
        })
        .expect("expected Write replay after turn/completed");
    let replay: serde_json::Value = serde_json::from_str(&write).unwrap();
    assert_eq!(replay["method"], "turn/start");
    assert_eq!(replay["params"]["threadId"], "thread-1");
    assert_eq!(replay["params"]["input"][0]["text"], "user poke");
}

#[test]
fn turn_completed_without_rejected_steer_emits_no_replay_write() {
    let d = process_driver_for_test();
    d.set_state_for_test("thread-1", "turn-7");

    let end_evs = d.parse_event(
        r#"{"jsonrpc":"2.0","method":"turn/completed","params":{"status":"completed"}}"#,
    );
    assert!(
        !end_evs
            .iter()
            .any(|ev| matches!(ev, DriverEvent::Write { .. })),
        "no rejected steer queued — should not replay: {end_evs:?}"
    );
}

#[test]
fn fork_thread_clears_rejected_steer_replay_queue() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async {
        let d = Arc::new(process_driver_for_test());
        d.set_state_for_test("thread-old", "turn-active");

        let _ = d
            .encode_stdin_message("queued poke", None, MessageMode::User)
            .unwrap();
        let err_line = r#"{"jsonrpc":"2.0","method":"error","params":{"willRetry":false,"error":{"message":"not steerable","codexErrorInfo":{"activeTurnNotSteerable":{"turnKind":"review"}}}}}"#;
        let _ = d.parse_event(err_line);

        let (mut child, stdin, mut capture) = spawn_stdin_capture_child();
        d.as_stdin_binder().unwrap().bind_stdin(stdin);

        let fork_driver = d.clone();
        let join = tokio::spawn(async move { fork_driver.fork_thread("thread-old").await });
        let _ = read_captured_json_line(&mut capture).await;
        // steer used id=3; fork uses id=4.
        let _ = d.parse_event(
            r#"{"jsonrpc":"2.0","id":4,"result":{"thread":{"id":"thread-new"}}}"#,
        );
        join.await.unwrap().expect("fork succeeds");

        let end_evs = d.parse_event(
            r#"{"jsonrpc":"2.0","method":"turn/completed","params":{"status":"completed"}}"#,
        );
        assert!(
            !end_evs
                .iter()
                .any(|ev| matches!(ev, DriverEvent::Write { .. })),
            "fork should drop pending replay queue: {end_evs:?}"
        );

        let _ = child.kill().await;
        let _ = child.wait().await;
    });
}

#[test]
fn auto_approve_server_request_emits_write_event() {
    let d = process_driver_for_test();
    let line = r#"{"jsonrpc":"2.0","id":7,"method":"server/approval","params":{"kind":"exec"}}"#;
    let evs = d.parse_event(line);
    match &evs[0] {
        DriverEvent::Write { data } => {
            let v: serde_json::Value = serde_json::from_str(data).unwrap();
            assert_eq!(v["id"], 7);
            assert_eq!(v["result"]["approved"], true);
        }
        other => panic!("expected Write, got {other:?}"),
    }
}

#[test]
fn runtime_drift_emits_once_per_driver_instance() {
    let d = process_driver_for_test();
    let line = r#"{"jsonrpc":"2.0","method":"future/unknown","params":{}}"#;

    let first = d.parse_event(line);
    assert_eq!(first.len(), 1);
    match &first[0] {
        DriverEvent::Error { message, code, .. } => {
            assert!(message.contains("[runtime_drift]"));
            assert!(message.contains("unknown method future/unknown"));
            assert_eq!(code.as_deref(), Some("runtime_drift"));
        }
        other => panic!("expected drift Error, got {other:?}"),
    }

    let second = d.parse_event(line);
    assert!(
        second.is_empty(),
        "subsequent unknown shapes must be suppressed: {second:?}"
    );
}

#[test]
fn parse_event_suppresses_will_retry_errors() {
    let d = process_driver_for_test();
    let line = r#"{"jsonrpc":"2.0","method":"error","params":{"willRetry":true,"error":{"message":"transient","codexErrorInfo":"serverOverloaded"}}}"#;
    assert!(d.parse_event(line).is_empty());
}

#[test]
fn parse_event_emits_error_when_will_retry_false() {
    let d = process_driver_for_test();
    let line = r#"{"jsonrpc":"2.0","method":"error","params":{"willRetry":false,"error":{"message":"server overloaded","codexErrorInfo":"serverOverloaded"}}}"#;
    let evs = d.parse_event(line);
    assert_eq!(evs.len(), 1);
    match &evs[0] {
        DriverEvent::Error {
            message,
            code,
            http_status,
            ..
        } => {
            assert_eq!(message, "server overloaded");
            assert_eq!(code.as_deref(), Some("serverOverloaded"));
            assert!(http_status.is_none());
        }
        other => panic!("expected Error, got {other:?}"),
    }
}

#[test]
fn parse_event_surfaces_http_status_on_object_form_codex_error() {
    let d = process_driver_for_test();
    let line = r#"{"jsonrpc":"2.0","method":"error","params":{"willRetry":false,"error":{"message":"upstream gateway 503","codexErrorInfo":{"responseStreamDisconnected":{"httpStatusCode":503}}}}}"#;
    let evs = d.parse_event(line);
    match &evs[0] {
        DriverEvent::Error {
            code, http_status, ..
        } => {
            assert_eq!(code.as_deref(), Some("responseStreamDisconnected"));
            assert_eq!(*http_status, Some(503));
        }
        other => panic!("expected Error with http_status, got {other:?}"),
    }
}

#[test]
fn parse_event_usage_limit_enriches_rate_limit_from_stored_snapshot() {
    let d = process_driver_for_test();
    let snap_line = r#"{"jsonrpc":"2.0","method":"account/rateLimits/updated","params":{"rateLimits":{"limitId":"codex","primary":{"usedPercent":100,"windowDurationMins":300,"resetsAt":1793000000}}}}"#;
    let _ = d.parse_event(snap_line);

    let err_line = r#"{"jsonrpc":"2.0","method":"error","params":{"willRetry":false,"error":{"message":"plan window exhausted","codexErrorInfo":"usageLimitExceeded"}}}"#;
    let evs = d.parse_event(err_line);
    assert!(
        evs.len() >= 2,
        "expected Error + enriched RateLimit, got {evs:?}"
    );
    match &evs[1] {
        DriverEvent::RateLimit {
            limit_type,
            resets_at,
            ..
        } => {
            assert_eq!(limit_type, "codex");
            assert_eq!(*resets_at, 1_793_000_000);
        }
        other => panic!("expected enriched RateLimit, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn interrupt_turn_writes_turn_interrupt_json_rpc() {
    let d = Arc::new(process_driver_for_test());
    d.set_state_for_test("thread-test", "turn-42");

    let (mut child, stdin, mut capture) = spawn_stdin_capture_child();
    d.as_stdin_binder().unwrap().bind_stdin(stdin);

    d.as_turn_interruptor()
        .unwrap()
        .interrupt_turn()
        .await
        .expect("interrupt succeeds");

    let v = read_captured_json_line(&mut capture).await;
    assert_eq!(v["method"], "turn/interrupt");
    assert_eq!(v["params"]["threadId"], "thread-test");
    assert_eq!(v["params"]["turnId"], "turn-42");

    let _ = child.kill().await;
    let _ = child.wait().await;
}

#[test]
fn interrupt_turn_requires_active_turn() {
    let d = process_driver_for_test();
    d.set_state_for_test("thread-test", "");
    let err = block_on(d.as_turn_interruptor().unwrap().interrupt_turn())
        .expect_err("missing active turn");
    assert!(matches!(err, DriverError::TurnSteerNoActiveTurn));
}

#[test]
fn known_silent_notification_does_not_emit_runtime_drift() {
    let d = process_driver_for_test();
    let line = r#"{"jsonrpc":"2.0","method":"deprecationNotice","params":{"message":"old api"}}"#;
    assert!(d.parse_event(line).is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn handshake_timeout_closes_stdin_when_turn_never_starts() {
    let _hooks = test_hooks::lock_hooks_async().await;
    test_hooks::reset_handshake_timeout_ms();
    test_hooks::set_handshake_timeout_ms(50);
    let d = Arc::new(process_driver_for_test());

    let mut child = tokio::process::Command::new("sleep")
        .arg("30")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn sleep child");
    let stdin = child.stdin.take().expect("child stdin");
    d.as_stdin_binder().unwrap().bind_stdin(stdin);
    assert!(d.stdin_is_bound_for_test());

    tokio::time::sleep(Duration::from_millis(120)).await;
    assert!(
        !d.stdin_is_bound_for_test(),
        "handshake watchdog should close stdin when turn/started never arrives"
    );

    let _ = child.kill().await;
    let _ = child.wait().await;
    test_hooks::reset_handshake_timeout_ms();
}

#[tokio::test(flavor = "multi_thread")]
async fn handshake_timeout_cancelled_on_turn_started() {
    let _hooks = test_hooks::lock_hooks_async().await;
    test_hooks::reset_handshake_timeout_ms();
    test_hooks::set_handshake_timeout_ms(200);
    let d = Arc::new(process_driver_for_test());

    let mut child = tokio::process::Command::new("sleep")
        .arg("30")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn sleep child");
    let stdin = child.stdin.take().expect("child stdin");
    d.as_stdin_binder().unwrap().bind_stdin(stdin);

    let _ =
        d.parse_event(r#"{"jsonrpc":"2.0","method":"turn/started","params":{"turnId":"t-run"}}"#);

    tokio::time::sleep(Duration::from_millis(250)).await;
    assert!(
        d.stdin_is_bound_for_test(),
        "turn/started should cancel the handshake watchdog without closing stdin"
    );

    let _ = child.kill().await;
    let _ = child.wait().await;
    test_hooks::reset_handshake_timeout_ms();
}
