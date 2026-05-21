//! Decision-matrix tests for `cocli_reaper::reap_with`.
//!
//! The 5 paths covered (matches Go `daemon/agent/agent_pidfile.go:122-151`):
//!   1. dead pid           → cleanup_stale
//!   2. alive, ppid = init → reaped
//!   3. alive, ppid = self → reaped
//!   4. alive, ppid alive & foreign → skipped_foreign
//!   5. alive, ppid = 0    → skipped_unknown_ppid
//!
//! Each test installs a `TestPidDirGuard` pointing at a fresh `tempfile::tempdir()`,
//! so they run in parallel without sharing the global pidfile cache.

use cocli_pidfile::{write_agent_pidfile, TestPidDirGuard};
use cocli_reaper::*;
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Default)]
struct FakeDeps {
    alive: Mutex<HashMap<u32, bool>>,
    parent: Mutex<HashMap<u32, u32>>,
    self_pid_val: u32,
    killed: Mutex<Vec<u32>>,
}

impl ReapDeps for FakeDeps {
    fn is_process_alive(&self, pid: u32) -> bool {
        *self.alive.lock().unwrap().get(&pid).unwrap_or(&false)
    }
    fn force_kill(&self, pid: u32) {
        self.killed.lock().unwrap().push(pid);
        self.alive.lock().unwrap().insert(pid, false);
    }
    fn process_parent(&self, pid: u32) -> u32 {
        *self.parent.lock().unwrap().get(&pid).unwrap_or(&0)
    }
    fn self_pid(&self) -> u32 {
        self.self_pid_val
    }
}

/// Helper: install a thread-local pid-dir override pointing at a fresh tempdir.
/// Returned guard owns both the override AND the TempDir so the directory
/// outlives the test.
struct PidDir {
    _dir: tempfile::TempDir,
    _guard: TestPidDirGuard,
}

fn fresh_pid_dir() -> PidDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let guard = TestPidDirGuard::new(dir.path());
    PidDir {
        _dir: dir,
        _guard: guard,
    }
}

#[test]
fn dead_pid_swept() {
    let _pid_dir = fresh_pid_dir();
    write_agent_pidfile("dead-1", 99999).unwrap();
    let deps = FakeDeps {
        self_pid_val: 1000,
        ..Default::default()
    };
    let stats = reap_with(&deps).unwrap();
    assert_eq!(stats.scanned, 1);
    assert_eq!(stats.cleaned_stale, 1);
    assert_eq!(stats.reaped, 0);
    assert!(
        deps.killed.lock().unwrap().is_empty(),
        "must NOT kill dead pid"
    );
}

#[test]
fn alive_ppid_init_reaped() {
    let _pid_dir = fresh_pid_dir();
    write_agent_pidfile("orphan-1", 12345).unwrap();
    let deps = FakeDeps {
        self_pid_val: 1000,
        ..Default::default()
    };
    deps.alive.lock().unwrap().insert(12345, true);
    deps.parent.lock().unwrap().insert(12345, 1); // reparented to init
    let stats = reap_with(&deps).unwrap();
    assert_eq!(stats.reaped, 1);
    assert!(deps.killed.lock().unwrap().contains(&12345));
}

#[test]
fn alive_ppid_self_reaped() {
    let _pid_dir = fresh_pid_dir();
    write_agent_pidfile("self-1", 23456).unwrap();
    let deps = FakeDeps {
        self_pid_val: 1000,
        ..Default::default()
    };
    deps.alive.lock().unwrap().insert(23456, true);
    deps.parent.lock().unwrap().insert(23456, 1000); // ppid == self
    let stats = reap_with(&deps).unwrap();
    assert_eq!(stats.reaped, 1);
    assert!(deps.killed.lock().unwrap().contains(&23456));
}

#[test]
fn alive_ppid_foreign_alive_skipped() {
    let _pid_dir = fresh_pid_dir();
    write_agent_pidfile("foreign-1", 34567).unwrap();
    let deps = FakeDeps {
        self_pid_val: 1000,
        ..Default::default()
    };
    deps.alive.lock().unwrap().insert(34567, true);
    deps.alive.lock().unwrap().insert(7777, true);
    deps.parent.lock().unwrap().insert(34567, 7777);
    let stats = reap_with(&deps).unwrap();
    assert_eq!(stats.skipped_foreign, 1);
    assert_eq!(stats.reaped, 0);
    assert!(
        deps.killed.lock().unwrap().is_empty(),
        "must NOT cross-kill foreign daemon's runtime"
    );
}

#[test]
fn unknown_ppid_conservative() {
    let _pid_dir = fresh_pid_dir();
    write_agent_pidfile("unknown-1", 45678).unwrap();
    let deps = FakeDeps {
        self_pid_val: 1000,
        ..Default::default()
    };
    deps.alive.lock().unwrap().insert(45678, true);
    deps.parent.lock().unwrap().insert(45678, 0); // ps failed
    let stats = reap_with(&deps).unwrap();
    assert_eq!(stats.skipped_unknown_ppid, 1);
    assert_eq!(stats.reaped, 0);
    assert!(deps.killed.lock().unwrap().is_empty());
}
