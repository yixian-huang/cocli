//! Spawn helpers for the chatrs CLI.
//!
//! Mirrors Go `daemon/drivers/chatrs.go::Spawn` (lines 119-160) and
//! `PrepareWorkspace` (lines 73-113). Two helpers:
//!
//! - `write_chatrs_agent_json` — materialises `<work_dir>/.cocli/agent.json`
//!   byte-identical to Go's `json.MarshalIndent` output (alphabetical keys,
//!   2-space indent, no trailing newline, mode 0o644). Go writes from
//!   `PrepareWorkspace`; the Rust driver writes from `spawn` so it sees
//!   `SpawnConfig`'s `server_url` + `auth_token` (the trait's
//!   `prepare_workspace` doesn't get them).
//!
//! - `spawn_chatrs` — builds the `tokio::process::Command` for
//!   `bin/chatrs agent --config <path> --bridge-stdio`. `kill_on_drop(true)`
//!   reaps the child if the actor panics.

use serde::Serialize;
use tokio::process::{Child, Command};

/// Context for `spawn_chatrs`.
pub struct SpawnContext<'a> {
    pub chatrs_binary: &'a std::path::Path,
    pub working_dir: &'a std::path::Path,
    /// Path returned by `write_chatrs_agent_json` — fed via `--config`.
    pub agent_json_path: &'a std::path::Path,
    /// Env vars to set on the child Command. The caller composes these
    /// from user-supplied `SpawnConfig.env_vars` + chatrs-internal vars
    /// (`BRIDGE_BIN_PATH`, `BRIDGE_SCOPED_TOKEN`, `BRIDGE_WORKSPACE_DIR`,
    /// `CHATRS_AGENT_ID`) — mirrors Go `chatrs.go::Spawn` env block
    /// (lines 134-156). Empty slice means inherit parent env only.
    pub env_vars: &'a [(String, String)],
}

pub fn spawn_chatrs(ctx: &SpawnContext) -> std::io::Result<Child> {
    let mut cmd = Command::new(ctx.chatrs_binary);
    cmd.current_dir(ctx.working_dir)
        .arg("agent")
        .arg("--config")
        .arg(ctx.agent_json_path)
        .arg("--bridge-stdio");
    cmd.envs(ctx.env_vars.iter().cloned());
    cmd.stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    cmd.spawn()
}

/// Compose the env block for chatrs's child process.
///
/// Mirrors Go `chatrs.go::Spawn` (lines 134-156):
///
/// ```text
/// cmd.Env = append(os.Environ(), buildEnvVars(ctx.Config.EnvVars)...)
/// cmd.Env = append(cmd.Env,
///     "BRIDGE_BIN_PATH="+ctx.ChatBridgePath,
///     "BRIDGE_SCOPED_TOKEN="+ctx.AuthToken,
///     "BRIDGE_WORKSPACE_DIR="+ctx.WorkDir,
///     "CHATRS_AGENT_ID="+ctx.AgentID,
/// )
/// ```
///
/// Order: user-supplied `env_vars` first, chatrs-internal vars second.
/// On key clash the chatrs-internal vars win (Go's `append` semantics —
/// later entries override earlier ones in `os/exec`).
///
/// NOTE: deliberately does NOT set `BRIDGE_SERVER_URL` or local-proxy auth
/// vars here. Those arrive via `env_vars` from the actor/router so the daemon
/// owns the bridge transport mode (Go local proxy, Rust direct-server mode)
/// instead of each driver making that decision.
pub fn build_chatrs_child_env(
    env_vars: &[(String, String)],
    bridge_binary: &std::path::Path,
    auth_token: &str,
    working_dir: &std::path::Path,
    agent_id: &str,
) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = env_vars.to_vec();
    out.push((
        "BRIDGE_BIN_PATH".to_string(),
        bridge_binary.to_string_lossy().into_owned(),
    ));
    out.push(("BRIDGE_SCOPED_TOKEN".to_string(), auth_token.to_string()));
    out.push((
        "BRIDGE_WORKSPACE_DIR".to_string(),
        working_dir.to_string_lossy().into_owned(),
    ));
    out.push(("CHATRS_AGENT_ID".to_string(), agent_id.to_string()));
    out
}

/// Extract `(profile_name, write_enabled)` from `SpawnConfig.env_vars`.
///
/// Mirrors Go `chatrs.go::PrepareWorkspace` (lines 83-91):
/// - `CHATRS_PROFILE_NAME` → profile_name; falls back to `"anthropic"`
///   when env is missing or empty (matches Go's `if profileName == ""`
///   default).
/// - `CHATRS_WRITE_ENABLED` → write_enabled is `true` ONLY when the
///   value is exactly `"true"` (string match). Any other value
///   (including `"True"`, `"1"`, `"yes"`, `""`) yields `false`.
///
/// SECURITY: default is `false`, not `true`. Hardcoding `true` would
/// grant write access to read-only agents — codex flagged this on PR #9.
pub fn extract_chatrs_settings(env_vars: &[(String, String)]) -> (String, bool) {
    let mut profile = String::new();
    let mut write_enabled = false;
    for (k, v) in env_vars {
        match k.as_str() {
            "CHATRS_PROFILE_NAME" => profile.clone_from(v),
            "CHATRS_WRITE_ENABLED" => write_enabled = v == "true",
            _ => {}
        }
    }
    if profile.is_empty() {
        profile = "anthropic".to_string();
    }
    (profile, write_enabled)
}

/// Explicit struct (NOT `serde_json::json!`) so the serialized field order
/// is alphabetical — matches Go's `map[string]any` + `json.MarshalIndent`
/// output byte-for-byte. Go alphabetizes map keys when marshalling.
///
/// Field order MUST stay alphabetical: agent_id, max_iterations, model,
/// profile_name, system_prompt, workspace_dir, write_enabled.
#[derive(Serialize)]
struct AgentJson<'a> {
    agent_id: &'a str,
    max_iterations: u32,
    model: &'a str,
    profile_name: &'a str,
    system_prompt: &'a str,
    workspace_dir: &'a str,
    write_enabled: bool,
}

/// Write `<work_dir>/.cocli/agent.json` for chatrs. Returns the absolute
/// path to the JSON file (used as `--config` arg to `bin/chatrs agent`).
///
/// Byte-identical to Go's `PrepareWorkspace` output for the same inputs;
/// verified via the fixture-compare test in `tests/driver_impl.rs`.
///
/// `bridge_binary`, `agent_id`, `server_url`, `auth_token` are accepted for
/// parity with claude's `write_claude_mcp_config` signature even though
/// chatrs's `agent.json` doesn't currently embed them — chatrs reads them
/// from env vars injected by the actor at spawn time
/// (`BRIDGE_BIN_PATH`, `BRIDGE_SCOPED_TOKEN`, etc.) per Go chatrs.go:150-156.
/// Phase 2c may move them into `agent.json` directly; signature is shaped
/// to accommodate without breaking callers.
#[allow(clippy::too_many_arguments)]
pub fn write_chatrs_agent_json(
    work_dir: &std::path::Path,
    profile_name: &str,
    model: &str,
    write_enabled: bool,
    workspace_dir: &str,
    _bridge_binary: &std::path::Path,
    agent_id: &str,
    _server_url: &str,
    _auth_token: &str,
    system_prompt: &str,
) -> std::io::Result<std::path::PathBuf> {
    let cocli_dir = work_dir.join(".cocli");
    std::fs::create_dir_all(&cocli_dir)?;

    let cfg = AgentJson {
        agent_id,
        max_iterations: 50,
        model,
        profile_name,
        system_prompt,
        workspace_dir,
        write_enabled,
    };
    let bytes = serde_json::to_vec_pretty(&cfg)?;

    let path = cocli_dir.join("agent.json");
    std::fs::write(&path, bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))?;
    }
    Ok(path)
}
