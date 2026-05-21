//! Spawn helper for the claude CLI.
//!
//! Mirrors Go `daemon/drivers/claude.go:85-130`. Same flag order, same
//! optional `--mcp-config` / `--resume` tail. `kill_on_drop(true)` ensures
//! the child is reaped if the daemon panics.

use tokio::process::{Child, Command};

pub struct SpawnContext<'a> {
    pub claude_binary: &'a std::path::Path,
    pub working_dir: &'a std::path::Path,
    pub model: &'a str,
    /// Path to `.mcp-config.json` written by the caller (bridge config).
    /// `None` skips the `--mcp-config` flag (parity with Go `NoBridge=true`).
    pub mcp_config: Option<&'a std::path::Path>,
    /// Existing claude `session_id` for `--resume`. `None` starts a fresh
    /// session.
    pub resume_session: Option<&'a str>,
}

pub fn spawn_claude(ctx: &SpawnContext) -> std::io::Result<Child> {
    let mut cmd = Command::new(ctx.claude_binary);
    cmd.current_dir(ctx.working_dir)
        .arg("--dangerously-skip-permissions")
        .arg("--verbose")
        .arg("--output-format")
        .arg("stream-json")
        .arg("--input-format")
        .arg("stream-json")
        .arg("--model")
        .arg(ctx.model);
    if let Some(mcp) = ctx.mcp_config {
        cmd.arg("--mcp-config").arg(mcp);
    }
    if let Some(sid) = ctx.resume_session {
        cmd.arg("--resume").arg(sid);
    }
    cmd.stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    cmd.spawn()
}
