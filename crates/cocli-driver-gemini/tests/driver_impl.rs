//! Validate that `GeminiDriver` implements `Driver` + `ExitCodeClassifier`
//! + `SessionFileGC` correctly.
//!
//! Tests are grouped by surface:
//!   1. Capability / data getters (name, prefix, busy mode, etc.)
//!   2. `prepare_workspace` side effects
//!   3. `spawn`-time fixture compare against Go's JSON output
//!   4. `ExitCodeClassifier::classify_exit_code` (41 / 52 / 130 / other)
//!   5. `SessionFileGC::gc_session_files` mtime-based reaping
//!   6. `parse_event` smoke (Error severity → ErrorSeverity mapping)

use std::time::{Duration, SystemTime};

use cocli_driver_core::types::{
    BusyDeliveryMode, DriverAgentConfig, EnvPropagation, ExitCodeClass, MessageMode,
    SkillCompatibility, TurnStatus,
};
use cocli_driver_core::{Driver, DriverError, DriverEvent, ErrorSeverity};
use cocli_driver_gemini::GeminiDriver;

fn driver_for_test() -> GeminiDriver {
    GeminiDriver::new(
        std::path::PathBuf::from("/bin/false"),
        std::path::PathBuf::from("/opt/cocli/bin/cocli-bridge"),
    )
}

// ─── 1. Capability / data getters ─────────────────────────────────────────

#[test]
fn gemini_name_is_gemini() {
    assert_eq!(driver_for_test().name(), "gemini");
}

#[test]
fn gemini_mcp_tool_prefix_is_single_underscore() {
    assert_eq!(driver_for_test().mcp_tool_prefix(), "mcp_chat_");
}

#[test]
fn gemini_busy_delivery_mode_is_direct() {
    assert_eq!(
        driver_for_test().busy_delivery_mode(),
        BusyDeliveryMode::Direct
    );
}

#[test]
fn gemini_env_propagation_is_settings_copy() {
    assert_eq!(
        driver_for_test().env_propagation(),
        EnvPropagation::SettingsCopy
    );
}

#[test]
fn gemini_skill_compatibility_is_uncertain() {
    assert_eq!(
        driver_for_test().skill_compatibility(),
        SkillCompatibility::Uncertain
    );
}

#[test]
fn gemini_context_window_is_1m() {
    assert_eq!(driver_for_test().context_window_tokens(), Some(1_000_000));
}

#[test]
fn gemini_requires_initial_prompt_is_true() {
    // gemini-cli's `-p <prompt>` mode is mandatory for daemon-spawned
    // headless operation. Task 0's actor step 5.5 threads system_prompt
    // through SpawnConfig; our spawn passes it as `-p`.
    assert!(driver_for_test().requires_initial_prompt());
}

#[test]
fn gemini_extra_system_prompt_is_non_empty_and_pinned() {
    let drv = driver_for_test();
    let s = drv.extra_system_prompt_section();
    assert!(!s.is_empty(), "expected non-empty extra system prompt");
    assert!(s.contains("STRICT"), "expected STRICT keyword: {s}");
    assert!(
        s.contains("send_message"),
        "expected send_message keyword: {s}"
    );
}

#[test]
fn gemini_supports_turn_cancel_true_steer_false() {
    let drv = driver_for_test();
    assert!(drv.supports_turn_cancel());
    assert!(!drv.supports_turn_steer());
}

#[test]
fn gemini_turn_steer_returns_unsupported() {
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
fn gemini_is_turn_exit_true() {
    let drv = driver_for_test();
    assert!(drv.is_turn_exit(), "gemini-cli is a turn-exit runtime");
}

#[test]
fn gemini_encode_user_message_returns_none_for_user_mode() {
    // Phase 2c #1: now that is_turn_exit() = true and actor::deliver
    // short-circuits BEFORE calling encode_stdin_message for turn-exit
    // drivers, the function correctly returns None for all modes. Gemini-cli
    // does not read piped stdin in `-p` headless mode.
    use cocli_driver_core::types::MessageMode;
    let drv = driver_for_test();
    let result = drv.encode_stdin_message("hello", None, MessageMode::User);
    assert!(
        result.is_none(),
        "turn-exit driver must return None — actor short-circuits before calling this"
    );
}

#[test]
fn gemini_encode_notification_returns_none() {
    // Notification mode keeps the legacy `None` semantics — gemini has
    // no inbox semantics and the actor's `deliver()` doesn't invoke this
    // path today (always User). Documents intent for future callers.
    let drv = driver_for_test();
    assert!(drv
        .encode_stdin_message("ping", Some("sid"), MessageMode::Notification)
        .is_none());
}

#[test]
fn gemini_skill_search_paths_include_workspace_and_global() {
    let drv = driver_for_test();
    let workspace = std::path::Path::new("/tmp/test-workspace");
    let paths = drv.skill_search_paths(workspace);
    assert!(paths
        .iter()
        .any(|p| p.ends_with(".gemini/skills") && p.starts_with(workspace)));
    if let Some(home) = dirs::home_dir() {
        assert!(paths
            .iter()
            .any(|p| p.ends_with(".gemini/skills") && p.starts_with(&home)));
    }
}

#[test]
fn gemini_as_exit_code_classifier_returns_some() {
    let drv = driver_for_test();
    assert!(drv.as_exit_code_classifier().is_some());
}

#[test]
fn gemini_as_session_file_gc_returns_some() {
    let drv = driver_for_test();
    assert!(drv.as_session_file_gc().is_some());
}

// ─── 2. `prepare_workspace` side effects ──────────────────────────────────

#[test]
fn gemini_prepare_workspace_creates_gemini_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let drv = driver_for_test();
    let cfg = DriverAgentConfig {
        runtime: "gemini",
        model: "gemini-2.5-pro",
        working_runtime: "gemini",
        working_model: "gemini-2.5-pro",
        env_vars: &[],
    };
    drv.prepare_workspace(tmp.path(), &cfg, "agent-x", "")
        .unwrap();
    assert!(
        tmp.path().join(".gemini").is_dir(),
        ".gemini dir should exist"
    );
    // No system prompt → no GEMINI.md written.
    assert!(!tmp.path().join("GEMINI.md").exists());
}

#[test]
fn gemini_prepare_workspace_writes_gemini_md_when_system_prompt_present() {
    let tmp = tempfile::tempdir().unwrap();
    let drv = driver_for_test();
    let cfg = DriverAgentConfig {
        runtime: "gemini",
        model: "gemini-2.5-pro",
        working_runtime: "gemini",
        working_model: "gemini-2.5-pro",
        env_vars: &[],
    };
    let prompt = "You are an agent named alice.";
    drv.prepare_workspace(tmp.path(), &cfg, "agent-x", prompt)
        .unwrap();
    let md = tmp.path().join("GEMINI.md");
    assert!(md.exists());
    assert_eq!(std::fs::read_to_string(&md).unwrap(), prompt);
}

// ─── 3. `spawn`-time settings.json byte-identical to Go ──────────────────

/// Raw-byte fixture compare: GeminiDriver writes `<work_dir>/.gemini/settings.json`
/// whose bytes exactly equal what Go's `gemini.go::Spawn` produces via
/// `json.MarshalIndent(map[string]any{...}, "", "  ")` for the same inputs.
///
/// The expected fixture below was captured from a side-by-side comparison
/// of Rust's `serde_json::to_vec_pretty` against the equivalent Go snippet
/// (see commit message). Both produce the same alphabetized field order
/// (args, command, env) inside `chat`, and the same BTreeMap-sorted env
/// keys.
///
/// This test exercises `write_gemini_settings_json` directly because
/// `Driver::spawn` also calls `spawn_gemini` which attempts to start the
/// CLI binary — comparing the file write path avoids the spawn cost.
#[test]
fn gemini_settings_json_byte_identical_to_go() {
    let tmp = tempfile::tempdir().unwrap();
    let work_dir = tmp.path();
    let bridge = std::path::PathBuf::from("/opt/cocli/bin/cocli-bridge");
    let env = vec![
        ("COCLI_AGENT_ID".to_string(), "agent-xyz".to_string()),
        ("DAEMON_LOCAL_TOKEN".to_string(), "tok-local".to_string()),
    ];

    let p = cocli_driver_gemini::write_gemini_settings_json(
        work_dir,
        &bridge,
        "agent-xyz",
        "ws://127.0.0.1:8090",
        "1hz_tok_test",
        &env,
    )
    .expect("write settings.json");

    assert!(p.exists());
    assert_eq!(
        p,
        work_dir.join(".gemini").join("settings.json"),
        "writer should place file at <work_dir>/.gemini/settings.json"
    );

    let actual = std::fs::read(&p).expect("read settings.json");

    // Pretty 2-space JSON. The `mcp.autoAllowInHeadless` block is REQUIRED
    // for gemini-cli >= 0.43: headless (`--output-format stream-json`) denies
    // configured MCP servers by default unless this opt-in is set (gemini-cli
    // PR #27215 / issue #26021). This diverges from Go gemini.go (which
    // predates the gate); the Go daemon is being retired. Key order is
    // struct-declaration order: `mcp` then `mcpServers`.
    let expected: &[u8] = b"{\n  \"mcp\": {\n    \"autoAllowInHeadless\": true\n  },\n  \"mcpServers\": {\n    \"chat\": {\n      \"args\": [\n        \"--agent-id\",\n        \"agent-xyz\",\n        \"--server-url\",\n        \"http://127.0.0.1:8090\",\n        \"--auth-token\",\n        \"1hz_tok_test\"\n      ],\n      \"command\": \"/opt/cocli/bin/cocli-bridge\",\n      \"env\": {\n        \"COCLI_AGENT_ID\": \"agent-xyz\",\n        \"DAEMON_LOCAL_TOKEN\": \"tok-local\"\n      }\n    }\n  }\n}";

    assert_eq!(
        actual, expected,
        "settings.json must carry mcp.autoAllowInHeadless + the chat bridge server"
    );
}

#[test]
fn gemini_settings_json_includes_env_vars() {
    // Codex PR #11 review regression: `GeminiDriver::spawn` used to pass
    // `&[]` to write_gemini_settings_json, silently dropping per-agent
    // env vars under EnvPropagation::SettingsCopy. Spawn now threads
    // cfg.env_vars from SpawnConfig. This test pins the JSON shape that
    // results when env_vars are non-empty: every (key,value) appears
    // inside mcpServers.chat.env, sorted alphabetically by key.
    let tmp = tempfile::tempdir().unwrap();
    let bridge = std::path::PathBuf::from("/opt/cocli/bin/cocli-bridge");
    let env = vec![
        ("ZZ_LAST".to_string(), "z".to_string()),
        ("AA_FIRST".to_string(), "a".to_string()),
        ("OPENAI_API_KEY".to_string(), "sk-test".to_string()),
    ];
    let p = cocli_driver_gemini::write_gemini_settings_json(
        tmp.path(),
        &bridge,
        "agent-env",
        "ws://localhost:8090",
        "tok",
        &env,
    )
    .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap();
    let env_obj = v["mcpServers"]["chat"]["env"]
        .as_object()
        .expect("env must be an object");
    assert_eq!(env_obj.len(), 3, "all env vars must be present");
    assert_eq!(env_obj["AA_FIRST"], "a");
    assert_eq!(env_obj["ZZ_LAST"], "z");
    assert_eq!(env_obj["OPENAI_API_KEY"], "sk-test");

    // BTreeMap-backed sort: keys appear alphabetically in the serialized
    // bytes (matches Go's json.MarshalIndent of map[string]any{...}).
    let raw = std::fs::read_to_string(tmp.path().join(".gemini").join("settings.json")).unwrap();
    let aa = raw.find("AA_FIRST").expect("AA_FIRST in raw");
    let openai = raw.find("OPENAI_API_KEY").expect("OPENAI_API_KEY in raw");
    let zz = raw.find("ZZ_LAST").expect("ZZ_LAST in raw");
    assert!(aa < openai && openai < zz, "env keys must be sorted");
}

#[test]
fn gemini_settings_json_with_empty_env_still_serializes_env_field() {
    let tmp = tempfile::tempdir().unwrap();
    let work_dir = tmp.path();
    let bridge = std::path::PathBuf::from("/opt/cocli/bin/cocli-bridge");
    let p = cocli_driver_gemini::write_gemini_settings_json(
        work_dir,
        &bridge,
        "agent-1",
        "ws://localhost:8090",
        "tok",
        &[],
    )
    .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap();
    let chat = &v["mcpServers"]["chat"];
    assert_eq!(chat["command"], "/opt/cocli/bin/cocli-bridge");
    assert!(chat["args"].is_array());
    // Empty env is still serialized as an empty object (matches Go's
    // map[string]any{} marshaling: `"env": {}`). Critical for parity —
    // bridge-side code may assume the key exists.
    assert!(chat["env"].is_object());
    assert_eq!(chat["env"].as_object().unwrap().len(), 0);
}

// ─── 4. `ExitCodeClassifier::classify_exit_code` ──────────────────────────

#[test]
fn gemini_classify_exit_code_41_is_auth_failed() {
    let drv = driver_for_test();
    let clf = drv.as_exit_code_classifier().unwrap();
    assert_eq!(clf.classify_exit_code(41), ExitCodeClass::AuthFailed);
}

#[test]
fn gemini_classify_exit_code_52_is_config_error() {
    let drv = driver_for_test();
    let clf = drv.as_exit_code_classifier().unwrap();
    assert_eq!(clf.classify_exit_code(52), ExitCodeClass::ConfigError);
}

#[test]
fn gemini_classify_exit_code_130_is_cancelled() {
    let drv = driver_for_test();
    let clf = drv.as_exit_code_classifier().unwrap();
    assert_eq!(clf.classify_exit_code(130), ExitCodeClass::Cancelled);
}

#[test]
fn gemini_classify_exit_code_other_is_normal() {
    let drv = driver_for_test();
    let clf = drv.as_exit_code_classifier().unwrap();
    // Pin the "fall-through" branch: 0 success, 1 generic, 42 input-error,
    // 255 high code — all must route to Normal so the daemon's existing
    // retry/auto-recover pipeline handles them.
    for code in [0, 1, 42, 100, 255] {
        assert_eq!(
            clf.classify_exit_code(code),
            ExitCodeClass::Normal,
            "exit code {code} should fall through to Normal"
        );
    }
}

// ─── 5. `SessionFileGC::gc_session_files` reaper ──────────────────────────

#[test]
fn gemini_session_file_gc_missing_tmp_is_no_op() {
    let tmp = tempfile::tempdir().unwrap();
    // No .gemini at all
    let drv = driver_for_test();
    let gc = drv.as_session_file_gc().unwrap();
    let stats = gc
        .gc_session_files(tmp.path(), Duration::from_secs(7 * 24 * 60 * 60))
        .expect("missing tmp should be no-op");
    assert_eq!(stats.removed, 0);
    assert_eq!(stats.freed_bytes, 0);
}

#[test]
fn gemini_session_file_gc_zero_max_age_is_no_op() {
    let tmp = tempfile::tempdir().unwrap();
    let chats = tmp
        .path()
        .join(".gemini")
        .join("tmp")
        .join("slug")
        .join("chats");
    std::fs::create_dir_all(&chats).unwrap();
    let f = chats.join("old.json");
    std::fs::write(&f, b"{}").unwrap();
    // Backdate so it would normally be eligible.
    let very_old = SystemTime::now()
        .checked_sub(Duration::from_secs(365 * 24 * 60 * 60))
        .unwrap();
    set_mtime(&f, very_old);

    let drv = driver_for_test();
    let gc = drv.as_session_file_gc().unwrap();
    let stats = gc
        .gc_session_files(tmp.path(), Duration::ZERO)
        .expect("zero max_age should no-op");
    assert_eq!(stats.removed, 0);
    assert!(f.exists(), "file must still exist after zero-maxAge GC");
}

#[test]
fn gemini_session_file_gc_removes_old_files() {
    let tmp = tempfile::tempdir().unwrap();
    let tmp_root = tmp.path().join(".gemini").join("tmp");
    let old_slug = tmp_root.join("slug_old").join("chats");
    let new_slug = tmp_root.join("slug_new").join("chats");
    std::fs::create_dir_all(&old_slug).unwrap();
    std::fs::create_dir_all(&new_slug).unwrap();

    // Old .json file (30 days ago).
    let old_json = old_slug.join("session-old.json");
    std::fs::write(&old_json, b"{\"old\":true}").unwrap();
    let thirty_days_ago = SystemTime::now()
        .checked_sub(Duration::from_secs(30 * 24 * 60 * 60))
        .unwrap();
    set_mtime(&old_json, thirty_days_ago);

    // Old .jsonl variant — should also be pruned.
    let old_jsonl = old_slug.join("session-old.jsonl");
    std::fs::write(&old_jsonl, b"{}\n").unwrap();
    set_mtime(&old_jsonl, thirty_days_ago);

    // Recent .json file (1 hour ago) — must NOT be deleted.
    let recent = new_slug.join("session-recent.json");
    std::fs::write(&recent, b"{\"recent\":true}").unwrap();
    let one_hour_ago = SystemTime::now()
        .checked_sub(Duration::from_secs(60 * 60))
        .unwrap();
    set_mtime(&recent, one_hour_ago);

    // Non-chat file (.txt) — must NOT be deleted even when stale.
    let non_chat = old_slug.join("notes.txt");
    std::fs::write(&non_chat, b"ignore me").unwrap();
    set_mtime(&non_chat, thirty_days_ago);

    let drv = driver_for_test();
    let gc = drv.as_session_file_gc().unwrap();
    let stats = gc
        .gc_session_files(tmp.path(), Duration::from_secs(14 * 24 * 60 * 60))
        .expect("GC should succeed");

    assert_eq!(stats.removed, 2, "should remove old .json + old .jsonl");
    assert!(stats.freed_bytes > 0, "freed bytes should be positive");

    assert!(!old_json.exists(), "old .json should be deleted");
    assert!(!old_jsonl.exists(), "old .jsonl should be deleted");
    assert!(recent.exists(), "recent file should be kept");
    assert!(
        non_chat.exists(),
        "non-chat file (wrong extension) should be kept even when stale"
    );
}

#[test]
fn gemini_session_file_gc_ignores_slug_without_chats_subdir() {
    let tmp = tempfile::tempdir().unwrap();
    // slug exists but no chats/ inside — simulates fresh slug created
    // before any session was saved.
    std::fs::create_dir_all(tmp.path().join(".gemini").join("tmp").join("slug_empty")).unwrap();

    let drv = driver_for_test();
    let gc = drv.as_session_file_gc().unwrap();
    let stats = gc
        .gc_session_files(tmp.path(), Duration::from_secs(24 * 60 * 60))
        .expect("should not error");
    assert_eq!(stats.removed, 0);
}

#[test]
fn gemini_session_file_gc_empty_home_is_no_op() {
    let drv = driver_for_test();
    let gc = drv.as_session_file_gc().unwrap();
    let stats = gc
        .gc_session_files(std::path::Path::new(""), Duration::from_secs(60))
        .expect("empty home should no-op");
    assert_eq!(stats.removed, 0);
    assert_eq!(stats.freed_bytes, 0);
}

/// Helper to backdate a file's mtime so the GC tests don't need to
/// `sleep` for real time to pass. Uses portable `std::fs::FileTimes`
/// (stable since 1.75) for the mtime; the access-time match isn't
/// strictly required for our GC logic (we read `metadata.modified()`).
fn set_mtime(path: &std::path::Path, when: SystemTime) {
    let times = std::fs::FileTimes::new()
        .set_modified(when)
        .set_accessed(when);
    let f = std::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .expect("open for set_mtime");
    f.set_times(times).expect("set_times");
}

// ─── 6. parse_event smoke (severity mapping into DriverEvent::Error) ──────

#[test]
fn gemini_parse_event_error_severity_maps_to_enum() {
    let drv = driver_for_test();
    let line = r#"{"type":"error","severity":"warning","message":"Loop detected"}"#;
    let evs = drv.parse_event(line);
    assert_eq!(evs.len(), 1);
    match &evs[0] {
        DriverEvent::Error {
            message, severity, ..
        } => {
            assert_eq!(message, "Loop detected");
            assert_eq!(*severity, Some(ErrorSeverity::Warning));
        }
        other => panic!("expected DriverEvent::Error, got {other:?}"),
    }
}

#[test]
fn gemini_parse_event_result_with_status_error_emits_error_then_turn_end() {
    let drv = driver_for_test();
    let line = r#"{"type":"result","status":"error","error":{"type":"INVALID_STREAM","message":"Invalid stream: malformed tool call"},"stats":{"input_tokens":100,"output_tokens":20}}"#;
    let evs = drv.parse_event(line);
    assert_eq!(
        evs.len(),
        2,
        "result with non-success status should emit Error + TurnEnd"
    );
    match &evs[0] {
        DriverEvent::Error {
            message, severity, ..
        } => {
            assert!(message.contains("Invalid stream"));
            // We classify non-success result errors as severity::Error so
            // the actor can route through the standard error-display
            // path instead of the recoverable-warning path.
            assert_eq!(*severity, Some(ErrorSeverity::Error));
        }
        other => panic!("first event should be Error, got {other:?}"),
    }
    match &evs[1] {
        DriverEvent::TurnEnd {
            status,
            input_tokens,
            output_tokens,
            ..
        } => {
            assert_eq!(*status, TurnStatus::Failed);
            assert_eq!(*input_tokens, 100);
            assert_eq!(*output_tokens, 20);
        }
        other => panic!("second event should be TurnEnd, got {other:?}"),
    }
}

#[test]
fn gemini_parse_event_result_success_emits_turn_end_only() {
    let drv = driver_for_test();
    let line = r#"{"type":"result","status":"success","stats":{"input_tokens":50,"output_tokens":10,"cached":5}}"#;
    let evs = drv.parse_event(line);
    assert_eq!(evs.len(), 1);
    match &evs[0] {
        DriverEvent::TurnEnd {
            status,
            input_tokens,
            output_tokens,
            cache_read_tokens,
            cost_usd,
            ..
        } => {
            assert_eq!(*status, TurnStatus::Completed);
            assert_eq!(*input_tokens, 50);
            assert_eq!(*output_tokens, 10);
            assert_eq!(*cache_read_tokens, 5);
            // gemini-cli does NOT emit total_cost_usd.
            assert_eq!(*cost_usd, 0.0);
        }
        other => panic!("expected TurnEnd, got {other:?}"),
    }
}
