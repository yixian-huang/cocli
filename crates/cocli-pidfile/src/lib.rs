//! Per-agent runtime PID files.
//!
//! Path layout (matches Go daemon `daemon/agent/agent_pidfile.go:17-27`):
//!     <UserCacheDir>/chatrs/agents/<sanitized-agent-id>.pid
//!
//! The `chatrs` namespace is shared with the Go daemon's daemon-wide pidlock so
//! both implementations can coexist (and so a future Rust daemon can reap
//! orphans left by a crashed Go daemon, and vice versa).

use std::cell::RefCell;
use std::fs;
use std::io::{self, Write};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// Namespace shared with the Go daemon (`pidLockDirName` in
/// `daemon/agent/agent_pidfile.go`). MUST stay "chatrs" for parity — the Rust
/// daemon is a drop-in replacement for the Go daemon, so any rename here would
/// strand pidfiles written by the other implementation.
const PID_LOCK_DIR_NAME: &str = "chatrs";
const AGENTS_SUBDIR: &str = "agents";

#[cfg(unix)]
const PIDFILE_MODE: u32 = 0o600;

thread_local! {
    /// Per-thread override for the pid directory. When `Some`, `agent_pid_dir()`
    /// returns this path instead of `<UserCacheDir>/chatrs/agents/`. Designed for
    /// tests: each test sets a unique tempdir via `TestPidDirGuard` so parallel
    /// test threads don't collide on the global cache directory. Production code
    /// never touches this; the override defaults to `None`.
    static PID_DIR_OVERRIDE: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

/// Test-only RAII guard that overrides `agent_pid_dir()` for the current thread
/// for the lifetime of the guard. Drop restores the previous override.
///
/// ```ignore
/// let tmp = tempfile::tempdir().unwrap();
/// let _guard = cocli_pidfile::TestPidDirGuard::new(tmp.path());
/// // ... any code that ends up calling agent_pid_dir() now sees `tmp.path()`.
/// ```
///
/// Marked `#[doc(hidden)]` because it has no use outside tests; we expose it
/// `pub` so dev-deps in sibling crates (e.g. `cocli-reaper`) can use it.
#[doc(hidden)]
pub struct TestPidDirGuard {
    prev: Option<PathBuf>,
}

impl TestPidDirGuard {
    pub fn new(dir: &Path) -> Self {
        let prev = PID_DIR_OVERRIDE.with(|c| c.borrow().clone());
        PID_DIR_OVERRIDE.with(|c| *c.borrow_mut() = Some(dir.to_path_buf()));
        TestPidDirGuard { prev }
    }
}

impl Drop for TestPidDirGuard {
    fn drop(&mut self) {
        let prev = self.prev.take();
        PID_DIR_OVERRIDE.with(|c| *c.borrow_mut() = prev);
    }
}

/// Returns the directory where per-agent PID files live, creating it if needed.
///
/// In tests, a `TestPidDirGuard` may install a thread-local override so each
/// test gets a unique tempdir and they can run in parallel. Production code
/// always sees the default `<UserCacheDir>/chatrs/agents/`.
pub fn agent_pid_dir() -> io::Result<PathBuf> {
    if let Some(d) = PID_DIR_OVERRIDE.with(|c| c.borrow().clone()) {
        fs::create_dir_all(&d)?;
        return Ok(d);
    }
    let base = dirs::cache_dir()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no user cache dir"))?;
    let dir = base.join(PID_LOCK_DIR_NAME).join(AGENTS_SUBDIR);
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Returns the canonical pidfile path for `agent_id` (does not check existence).
pub fn agent_pid_file(agent_id: &str) -> io::Result<PathBuf> {
    Ok(agent_pid_dir()?.join(format!("{}.pid", sanitize_agent_id(agent_id))))
}

/// Writes the runtime PID for `agent_id` so the next daemon instance can reap
/// orphans if this daemon crashes without graceful shutdown.
///
/// File format mirrors Go: decimal PID followed by `\n`. Permissions 0600 on
/// Unix (matches Go's `pidLockFilePerm`).
pub fn write_agent_pidfile(agent_id: &str, pid: u32) -> io::Result<()> {
    let path = agent_pid_file(agent_id)?;
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)?;
    writeln!(f, "{}", pid)?;
    f.sync_all()?;
    #[cfg(unix)]
    fs::set_permissions(&path, fs::Permissions::from_mode(PIDFILE_MODE))?;
    Ok(())
}

/// Reads the PID recorded for `agent_id`. Returns `Ok(None)` if the pidfile
/// doesn't exist or is empty/malformed (matches Go's tolerant behaviour during
/// orphan reaping — a corrupt pidfile is treated as "no PID recorded").
pub fn read_agent_pidfile(agent_id: &str) -> io::Result<Option<u32>> {
    let path = agent_pid_file(agent_id)?;
    match fs::read_to_string(path) {
        Ok(s) => Ok(s.trim().parse::<u32>().ok()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// Deletes the recorded PID for `agent_id`. No-op if the file doesn't exist.
pub fn remove_agent_pidfile(agent_id: &str) -> io::Result<()> {
    let path = agent_pid_file(agent_id)?;
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

/// Sanitises an agent ID for use as a filename. Mirrors Go
/// `sanitizeAgentID` in `daemon/agent/agent_pidfile.go:156-179`:
///
/// - trim ASCII whitespace
/// - empty input → `"unknown"`
/// - keep `[a-zA-Z0-9_-]` runes; replace any other rune with a single `_`
/// - if everything got stripped, → `"unknown"`
///
/// Note: Go's implementation does `b.WriteByte('_')` per non-ASCII *rune*, which
/// emits exactly one underscore per rune (NOT one per UTF-8 byte). We match that
/// here by iterating over `chars()` and pushing a single `_`.
pub fn sanitize_agent_id(id: &str) -> String {
    let trimmed = id.trim();
    if trimmed.is_empty() {
        return "unknown".to_string();
    }
    let mut s = String::with_capacity(trimmed.len());
    for ch in trimmed.chars() {
        match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => s.push(ch),
            _ => s.push('_'),
        }
    }
    if s.is_empty() {
        "unknown".to_string()
    } else {
        s
    }
}
