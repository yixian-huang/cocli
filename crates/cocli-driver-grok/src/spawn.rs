//! Spawn helper for the grok CLI (headless `-p` + `streaming-json` turn-exit).
//!
//! Mirrors `cocli-driver-gemini::spawn` layout: `SpawnContext` carries only
//! spawn-local fields; per-agent env vars are threaded separately because
//! Grok uses `EnvPropagation::Inherit` (gemini copies env into settings.json).
//!
//! Platform actions use injected `.cocli/bin/cocli` (CLI transport), not
//! `.grok/config.toml` MCP. AGENTS.md is written at root by prepare_workspace.

use std::path::Path;

use cocli_driver_core::prompt_arg;
use tokio::process::{Child, Command};

/// Inputs that grok's spawn needs that aren't carried directly by
/// `cocli-driver-core::types::SpawnConfig`. The driver impl populates
/// this from `SpawnConfig` + the driver's own state (binary path).
pub struct SpawnContext<'a> {
    pub grok_binary: &'a Path,
    pub working_dir: &'a Path,
    pub model: &'a str,
    pub resume_session: Option<&'a str>,
    /// Persistent platform contract written to `AGENTS.md`; also used as
    /// the `-p` fallback only when no initial prompt is supplied.
    pub system_prompt: &'a str,
    /// Per-spawn user prompt for headless mode. When non-empty, this becomes
    /// the `-p <prompt>` argument while `system_prompt` stays available to
    /// the workspace contract file.
    pub initial_prompt: &'a str,
}

/// Build the grok `Command`. Flag order (see `grok_args`):
///   1. (prompt) `-p <initial_prompt>` when non-empty, otherwise
///      `-p <system_prompt>` when non-empty
///   2. `--output-format streaming-json`
///   3. `--always-approve`
///   4. `--no-alt-screen`
///   5. (resume) `--resume <sid>`
///   6. (model)  `-m <model>`
///
/// `current_dir` is set for cwd (no `--cwd` flag). Child env is set from
/// `env_vars` so Inherit propagation can thread `XAI_API_KEY` etc.
pub fn spawn_grok(ctx: &SpawnContext, env_vars: &[(String, String)]) -> std::io::Result<Child> {
    let mut cmd = Command::new(ctx.grok_binary);
    cmd.current_dir(ctx.working_dir);
    cmd.args(grok_args(ctx));
    cmd.envs(env_vars.iter().cloned());
    cmd.stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    cmd.spawn()
}

/// Build the grok argument vector (pure, no spawn) so the flag set is
/// unit-testable. See `spawn_grok` for the canonical order + rationale.
fn grok_args(ctx: &SpawnContext) -> Vec<String> {
    let mut args: Vec<String> = vec![];
    if let Some(prompt) = prompt_arg(ctx.initial_prompt, ctx.system_prompt) {
        args.push("-p".to_string());
        args.push(prompt.to_string());
    }
    args.push("--output-format".to_string());
    args.push("streaming-json".to_string());
    args.push("--always-approve".to_string());
    args.push("--no-alt-screen".to_string());
    if let Some(sid) = ctx.resume_session {
        args.push("--resume".to_string());
        args.push(sid.to_string());
        // Resumed sessions are already bound to an agent type; passing `-m`
        // again triggers MODEL_SWITCH_INCOMPATIBLE_AGENT on turn-exit respawn.
    } else if !ctx.model.is_empty() {
        args.push("-m".to_string());
        args.push(ctx.model.to_string());
    }
    args
}

/// Write `<work_dir>/AGENTS.md` from `system_prompt`. No-op when
/// `system_prompt` is empty.
///
/// Called from `prepare_workspace` (receives the platform contract
/// `system_prompt`, which is preserved on respawns unlike the wake
/// `initial_prompt` in `SpawnConfig`).
pub fn write_grok_agents_md(work_dir: &Path, system_prompt: &str) -> std::io::Result<()> {
    if system_prompt.is_empty() {
        return Ok(());
    }
    let path = work_dir.join("AGENTS.md");
    std::fs::write(&path, system_prompt)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx<'a>(
        system_prompt: &'a str,
        initial_prompt: &'a str,
        resume_session: Option<&'a str>,
        model: &'a str,
    ) -> SpawnContext<'a> {
        SpawnContext {
            grok_binary: Path::new("/bin/grok"),
            working_dir: Path::new("/tmp/grok-ws"),
            model,
            resume_session,
            system_prompt,
            initial_prompt,
        }
    }

    #[test]
    fn grok_args_use_headless_prompt_arg_precedence() {
        let with_initial = ctx("PLATFORM CONTRACT", "BOOTSTRAP TURN", None, "grok-test");
        let args = grok_args(&with_initial);
        assert!(args.windows(2).any(|w| w == ["-p", "BOOTSTRAP TURN"]));

        let system_only = ctx("PLATFORM CONTRACT", "", None, "grok-test");
        let args = grok_args(&system_only);
        assert!(args.windows(2).any(|w| w == ["-p", "PLATFORM CONTRACT"]));

        let empty = ctx("", "", None, "grok-test");
        let args = grok_args(&empty);
        assert!(!args.contains(&"-p".to_string()));
    }

    #[test]
    fn grok_args_include_output_format_streaming_json() {
        let c = ctx("", "", None, "");
        let args = grok_args(&c);
        assert!(args.contains(&"--output-format".to_string()));
        let of_idx = args.iter().position(|a| a == "--output-format").unwrap();
        assert_eq!(args[of_idx + 1], "streaming-json");
    }

    #[test]
    fn grok_args_prefers_initial_prompt() {
        let c = ctx("PLATFORM CONTRACT", "BOOTSTRAP TURN", None, "");
        let args = grok_args(&c);
        let p_idx = args.iter().position(|a| a == "-p").unwrap();
        assert_eq!(args[p_idx + 1], "BOOTSTRAP TURN");
    }

    #[test]
    fn grok_args_falls_back_to_system_prompt_when_initial_missing() {
        let c = ctx("PLATFORM CONTRACT", "", None, "");
        let args = grok_args(&c);
        let p_idx = args.iter().position(|a| a == "-p").unwrap();
        assert_eq!(args[p_idx + 1], "PLATFORM CONTRACT");
    }

    #[test]
    fn grok_args_passes_context_reset_recovery_prompt_with_resume() {
        let recovery = concat!(
            "[System] Session context was reset to free context window.\n",
            "1. Read MEMORY.md and your memory index for ongoing work.\n",
            "2. If notes/recent_context.md exists, read it for the latest checkpoint.\n",
            "3. Use cocli message digest (or cocli message check if urgent) to catch up on inbox.\n",
            "Resume from saved state — do not re-discover from scratch unless memory is empty."
        );
        let session_id = "019efd39-8681-73a1-af63-4906ce094e29";
        let c = ctx("PLATFORM CONTRACT", recovery, Some(session_id), "");
        let args = grok_args(&c);
        let p_idx = args.iter().position(|a| a == "-p").unwrap();
        assert_eq!(args[p_idx + 1], recovery);
        assert!(args[p_idx + 1].contains("cocli message digest"));
        assert!(!args[p_idx + 1].contains("daemon restart"));
        let r_idx = args.iter().position(|a| a == "--resume").unwrap();
        assert_eq!(args[r_idx + 1], session_id);
    }

    #[test]
    fn grok_args_omits_p_flag_when_both_prompts_empty() {
        let c = ctx("", "", None, "");
        let args = grok_args(&c);
        assert!(!args.contains(&"-p".to_string()));
    }

    #[test]
    fn grok_args_includes_resume_session() {
        let c = ctx("", "BOOT", Some("019e89ac-d61e-70f2-be42-30dba3d2ff43"), "");
        let args = grok_args(&c);
        let r_idx = args.iter().position(|a| a == "--resume").unwrap();
        assert_eq!(args[r_idx + 1], "019e89ac-d61e-70f2-be42-30dba3d2ff43");
    }

    #[test]
    fn grok_args_includes_model() {
        let c = ctx("", "BOOT", None, "grok-beta");
        let args = grok_args(&c);
        let m_idx = args.iter().position(|a| a == "-m").unwrap();
        assert_eq!(args[m_idx + 1], "grok-beta");
    }

    #[test]
    fn grok_args_omits_model_when_resume_session_set() {
        let c = ctx("SYS", "INIT PROMPT", Some("sid-123"), "grok-1");
        let args = grok_args(&c);
        assert!(!args.contains(&"-m".to_string()));
        let r_idx = args.iter().position(|a| a == "--resume").unwrap();
        assert_eq!(args[r_idx + 1], "sid-123");
    }

    #[test]
    fn grok_args_canonical_order_matches_design_spike() {
        let c = ctx("SYS", "INIT PROMPT", None, "grok-1");
        let args = grok_args(&c);
        let expected: Vec<String> = vec![
            "-p".to_string(),
            "INIT PROMPT".to_string(),
            "--output-format".to_string(),
            "streaming-json".to_string(),
            "--always-approve".to_string(),
            "--no-alt-screen".to_string(),
            "-m".to_string(),
            "grok-1".to_string(),
        ];
        assert_eq!(args, expected);
    }
}
