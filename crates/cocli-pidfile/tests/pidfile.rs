use cocli_pidfile::{
    agent_pid_dir, agent_pid_file, read_agent_pidfile, remove_agent_pidfile, sanitize_agent_id,
    write_agent_pidfile, TestPidDirGuard,
};
use std::fs;

#[test]
fn write_then_read_roundtrip() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let _guard = TestPidDirGuard::new(tmp.path());

    let aid = "test-roundtrip";
    write_agent_pidfile(aid, 12345).expect("write");

    let path = agent_pid_file(aid).expect("path");
    assert!(path.exists(), "pidfile not created at {}", path.display());

    let raw = fs::read_to_string(&path).expect("read");
    assert_eq!(raw, "12345\n", "expected newline-terminated decimal PID");

    let pid = read_agent_pidfile(aid).expect("read api").expect("Some(pid)");
    assert_eq!(pid, 12345);

    // remove
    remove_agent_pidfile(aid).expect("remove");
    assert!(!path.exists(), "pidfile not removed");

    // remove is idempotent
    remove_agent_pidfile(aid).expect("remove idempotent");

    // read after remove returns None
    assert!(read_agent_pidfile(aid).expect("read missing").is_none());
}

#[test]
fn sanitize_corner_cases() {
    // Empty / whitespace → "unknown"
    assert_eq!(sanitize_agent_id(""), "unknown");
    assert_eq!(sanitize_agent_id("   "), "unknown");
    assert_eq!(sanitize_agent_id("\t\n "), "unknown");

    // Already-clean IDs pass through.
    assert_eq!(sanitize_agent_id("agent-1_2"), "agent-1_2");
    assert_eq!(sanitize_agent_id("ABC-xyz_42"), "ABC-xyz_42");

    // Disallowed ASCII (path separators, dots, spaces) → "_".
    assert_eq!(sanitize_agent_id("a/b/c"), "a_b_c");
    assert_eq!(sanitize_agent_id("a.b"), "a_b");
    assert_eq!(sanitize_agent_id("a b"), "a_b");
    assert_eq!(sanitize_agent_id("../etc/passwd"), "___etc_passwd");

    // Leading/trailing whitespace stripped before sanitisation.
    assert_eq!(sanitize_agent_id("  foo  "), "foo");

    // Non-ASCII runes: one underscore each (matches Go WriteByte('_') per rune).
    assert_eq!(sanitize_agent_id("中文agent"), "__agent");
    assert_eq!(sanitize_agent_id("中"), "_");
}

#[test]
fn path_uses_chatrs_namespace_when_no_override() {
    // This test verifies the *default* path; do NOT install a guard.
    // Other tests run on different threads with their own override and
    // do not affect this thread's PID_DIR_OVERRIDE (it stays None).
    let dir = agent_pid_dir().expect("dir");
    let dir_s = dir.to_string_lossy().to_string();
    let sep = std::path::MAIN_SEPARATOR;
    assert!(
        dir_s.ends_with(&format!("{sep}chatrs{sep}agents")),
        "agent_pid_dir does not end with chatrs/agents: {}",
        dir_s
    );

    let path = agent_pid_file("test").expect("file path");
    let s = path.to_string_lossy();
    assert!(
        s.contains(&format!("{sep}chatrs{sep}agents{sep}")),
        "path does not contain /chatrs/agents/: {}",
        s
    );
    assert!(s.ends_with("test.pid"));
    assert!(
        !s.contains(&format!("{sep}cocli{sep}")),
        "namespace leaked to cocli: {}",
        s
    );
}

#[test]
fn guard_isolates_concurrent_test_threads() {
    // Confirm two simultaneous guards in this thread don't bleed. We
    // can't easily simulate two threads from inside one test, but the
    // nested-guard restore behaviour is the property we want.
    let tmp_outer = tempfile::tempdir().expect("tempdir");
    let _outer = TestPidDirGuard::new(tmp_outer.path());
    let outer_dir = agent_pid_dir().expect("dir");
    assert_eq!(outer_dir, tmp_outer.path());

    {
        let tmp_inner = tempfile::tempdir().expect("tempdir");
        let _inner = TestPidDirGuard::new(tmp_inner.path());
        let inner_dir = agent_pid_dir().expect("dir");
        assert_eq!(inner_dir, tmp_inner.path());
    } // _inner dropped → previous override restored

    let after_drop = agent_pid_dir().expect("dir");
    assert_eq!(
        after_drop,
        tmp_outer.path(),
        "Drop of inner guard should restore outer override"
    );
}
