//! Orphaned agent runtime reaper.
//!
//! Source-of-truth: `daemon/agent/agent_pidfile.go:92-154` (Go `ReapOrphanedAgents`).
//!
//! At Rust-daemon startup, scan the per-agent pidfile directory shared with the
//! Go daemon (namespace `chatrs/agents/`) and apply the 5-way decision matrix:
//!
//! | pid state               | action                                      |
//! |-------------------------|---------------------------------------------|
//! | dead                    | remove pidfile (stale)                      |
//! | alive, ppid = 1 (init)  | SIGKILL + remove pidfile (true orphan)      |
//! | alive, ppid = self      | SIGKILL + remove pidfile (defensive)        |
//! | alive, ppid alive & ≠self | leave both alone (foreign daemon owns it) |
//! | alive, ppid = 0 (unknown) | leave both alone (conservative)           |
//!
//! See `feedback_concurrent_daemon_collateral.md` in user memory for history
//! of the cross-daemon collateral-kill bug this 5-way matrix fixes.

use cocli_pidfile::agent_pid_dir;
use std::fs;

/// Result of one reap scan. Useful for tests + tracing.
#[derive(Debug, Default, Clone)]
pub struct ReapStats {
    /// Number of `.pid` files inspected.
    pub scanned: usize,
    /// Live orphan processes SIGKILL'd + pidfile removed.
    pub reaped: usize,
    /// Pidfiles for already-dead PIDs that were swept.
    pub cleaned_stale: usize,
    /// Live processes left alone because some OTHER live daemon owns them.
    pub skipped_foreign: usize,
    /// Live processes left alone because ppid lookup returned 0 (ps failed).
    pub skipped_unknown_ppid: usize,
}

/// Process-inspection dependencies, injectable so tests can drive deterministic
/// ppid/alive matrices without spawning real subprocesses.
pub trait ReapDeps {
    fn is_process_alive(&self, pid: u32) -> bool;
    fn force_kill(&self, pid: u32);
    /// Returns 0 if parent cannot be determined.
    fn process_parent(&self, pid: u32) -> u32;
    fn self_pid(&self) -> u32;
}

/// Production `ReapDeps` backed by `sysinfo` + `nix::sys::signal`.
pub struct DefaultReapDeps;

impl ReapDeps for DefaultReapDeps {
    fn is_process_alive(&self, pid: u32) -> bool {
        use sysinfo::{Pid, System};
        let mut sys = System::new();
        sys.refresh_process(Pid::from_u32(pid));
        sys.process(Pid::from_u32(pid)).is_some()
    }

    fn force_kill(&self, pid: u32) {
        #[cfg(unix)]
        {
            let _ = nix::sys::signal::kill(
                nix::unistd::Pid::from_raw(pid as i32),
                nix::sys::signal::Signal::SIGKILL,
            );
        }
        #[cfg(windows)]
        {
            use sysinfo::{Pid, Signal, System};
            let mut sys = System::new();
            sys.refresh_process(Pid::from_u32(pid));
            if let Some(process) = sys.process(Pid::from_u32(pid)) {
                let _ = process.kill_with(Signal::Kill);
            }
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = pid;
        }
    }

    fn process_parent(&self, pid: u32) -> u32 {
        use sysinfo::{Pid, System};
        let mut sys = System::new();
        sys.refresh_process(Pid::from_u32(pid));
        sys.process(Pid::from_u32(pid))
            .and_then(|p| p.parent())
            .map(|p| p.as_u32())
            .unwrap_or(0)
    }

    fn self_pid(&self) -> u32 {
        std::process::id()
    }
}

/// Reap orphaned agent runtimes using the production `DefaultReapDeps`.
pub fn reap_orphaned_agents() -> std::io::Result<ReapStats> {
    reap_with(&DefaultReapDeps)
}

/// Reap with a custom dependencies impl (used by tests).
pub fn reap_with(deps: &dyn ReapDeps) -> std::io::Result<ReapStats> {
    let mut stats = ReapStats::default();
    let dir = agent_pid_dir()?;
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Ok(stats),
    };
    let self_pid = deps.self_pid();

    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let path = entry.path();
        let name = file_name.to_string_lossy();

        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        if !name.ends_with(".pid") {
            continue;
        }
        stats.scanned += 1;

        let pid: u32 = match fs::read_to_string(&path)
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .filter(|p: &u32| *p > 0)
        {
            Some(p) => p,
            None => {
                // Unreadable or malformed pidfile — sweep it like Go does.
                let _ = fs::remove_file(&path);
                continue;
            }
        };

        if !deps.is_process_alive(pid) {
            let _ = fs::remove_file(&path);
            stats.cleaned_stale += 1;
            continue;
        }

        let ppid = deps.process_parent(pid);
        match ppid {
            0 => {
                tracing::warn!(
                    component = "reaper",
                    pid,
                    file = %name,
                    "orphan reap skipped: cannot determine parent"
                );
                stats.skipped_unknown_ppid += 1;
            }
            p if p == 1 || p == self_pid || !deps.is_process_alive(p) => {
                // Truly orphaned: reparented to init, owned by this daemon
                // (defensive — shouldn't happen at startup), or parent died
                // since the alive check ran.
                deps.force_kill(pid);
                let _ = fs::remove_file(&path);
                tracing::warn!(
                    component = "reaper",
                    pid,
                    ppid,
                    file = %name,
                    "reaped orphaned agent runtime"
                );
                stats.reaped += 1;
            }
            _ => {
                // Some OTHER live daemon owns this PID. Pre-fix would have
                // SIGKILL'd it (cross-daemon collateral kill). Leave alone.
                tracing::debug!(
                    component = "reaper",
                    pid,
                    ppid,
                    file = %name,
                    "orphan reap skipped: foreign daemon owns runtime"
                );
                stats.skipped_foreign += 1;
            }
        }
    }
    Ok(stats)
}
