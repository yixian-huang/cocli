//! `GeminiDriver` — `Driver` impl + `ExitCodeClassifier` + `SessionFileGC`
//! for the Google Gemini CLI runtime.
//!
//! Wraps the per-module helpers (`spawn_gemini`, `parse_line`, `to_driver_events`,
//! `encode_stdin_message`, `write_gemini_settings_json`, `gc_gemini_session_files`)
//! and adds the full set of capability getters + `prepare_workspace`. Values
//! mirror Go `daemon/drivers/gemini.go`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;
use cocli_driver_core::subtraits::{ExitCodeClassifier, SessionFileGC};
use cocli_driver_core::types::{
    BusyDeliveryMode, DriverAgentConfig, EnvPropagation, ExitCodeClass, GcStats, MessageMode,
    SkillCompatibility, SpawnConfig,
};
use cocli_driver_core::{Driver, DriverError, DriverEvent};

use crate::conv::to_driver_events;
use crate::events::parse_line;
use crate::spawn::{
    gc_gemini_session_files, spawn_gemini, write_gemini_settings_json, SpawnContext,
};
use crate::stdin::encode_stdin_message;

/// Gemini's extra system-prompt section. Strict turn-end-with-tool contract
/// — port of Go `daemon/drivers/gemini.go::geminiExtraSystemPromptSection`.
pub const GEMINI_EXTRA_SYSTEM_PROMPT: &str = "[Runtime contract — STRICT for gemini]\n\nEvery turn MUST end with at least one chat tool call (send_message, check_messages, create_tasks, update_task_status, ...). Pure reasoning / commentary output is NOT delivery — the platform cannot see it. If you believe you are done, call send_message with your final summary. If you need to think further, still call check_messages to signal active processing.\n\nRepeatedly reasoning without tool call will trigger empty_turn_auto_pause (L8c) and stall you. The only way to unstall is to call a tool.";

pub struct GeminiDriver {
    /// Path to the `gemini` CLI binary (resolved at daemon boot).
    gemini_binary: PathBuf,
    /// Path to the `cocli-bridge` MCP server binary; embedded into the
    /// per-agent `.gemini/settings.json` that `spawn` writes.
    bridge_binary: PathBuf,
}

impl GeminiDriver {
    pub fn new(gemini_binary: PathBuf, bridge_binary: PathBuf) -> Self {
        Self {
            gemini_binary,
            bridge_binary,
        }
    }
}

#[async_trait]
impl Driver for GeminiDriver {
    fn name(&self) -> &str {
        "gemini"
    }

    fn mcp_tool_prefix(&self) -> &str {
        // gemini.go:194 — single-underscore prefix.
        "mcp_chat_"
    }

    fn requires_initial_prompt(&self) -> bool {
        // gemini.go:192 — without `-p` gemini enters interactive REPL and
        // ignores piped stdin. Daemon must inject the bootstrap prompt;
        // actor threads SpawnConfig.initial_prompt so the driver's `spawn`
        // can pass it as `-p <prompt>`.
        true
    }

    fn is_turn_exit(&self) -> bool {
        // Phase 2c #1: gemini-cli's `-p <prompt>` mode exits after each turn
        // and must be respawned for the next message. This is the ONLY
        // is_turn_exit=true driver; all others (claude/codex/kimi/chatrs)
        // maintain persistent stdin.
        true
    }

    fn busy_delivery_mode(&self) -> BusyDeliveryMode {
        // gemini.go:207 — gemini-cli is turn-exit; the daemon delivers
        // stdin during busy by re-spawning with --resume, so writes never
        // actually hit a running process. Direct is correct.
        BusyDeliveryMode::Direct
    }

    fn env_propagation(&self) -> EnvPropagation {
        // gemini.go:205 — gemini-cli sanitizes /TOKEN|AUTH|KEY/i out of the
        // inherited env before forwarding to MCP children. Daemon must
        // copy EnvVars into .gemini/settings.json mcpServers.chat.env.
        EnvPropagation::SettingsCopy
    }

    fn extra_system_prompt_section(&self) -> &str {
        GEMINI_EXTRA_SYSTEM_PROMPT
    }

    fn skill_compatibility(&self) -> SkillCompatibility {
        // gemini.go:223 — gemini-cli supports skills as plain Markdown in
        // ~/.gemini/skills, but adoption / wiring under daemon control is
        // unverified.
        SkillCompatibility::Uncertain
    }

    fn context_window_tokens(&self) -> Option<u32> {
        // gemini.go:214 — Gemini 1M context advertised.
        Some(1_000_000)
    }

    fn prepare_workspace(
        &self,
        work_dir: &Path,
        _config: &DriverAgentConfig,
        _agent_id: &str,
        system_prompt: &str,
    ) -> Result<(), DriverError> {
        // Mirrors Go `gemini.go::PrepareWorkspace` (line 225-240):
        // create `.gemini/` and, when a system prompt is supplied, write
        // it to `<work_dir>/GEMINI.md`. The settings.json is written by
        // `spawn` (matching Go's placement in Spawn rather than
        // PrepareWorkspace).
        let settings_dir = work_dir.join(".gemini");
        std::fs::create_dir_all(settings_dir).map_err(DriverError::Io)?;
        if !system_prompt.is_empty() {
            let gemini_md = work_dir.join("GEMINI.md");
            std::fs::write(gemini_md, system_prompt).map_err(DriverError::Io)?;
        }
        // gemini-cli 0.40.1 only loads project-scoped mcpServers (our bridge,
        // written to <work_dir>/.gemini/settings.json by `spawn`) when the
        // folder is listed in ~/.gemini/trustedFolders.json. Without this the
        // agent has no chat tools and can't send/receive messages. `--skip-
        // trust` (see spawn.rs) only covers the approval-mode downgrade.
        crate::spawn::register_gemini_trusted_workspace(work_dir).map_err(DriverError::Io)?;
        Ok(())
    }

    fn spawn(&self, cfg: &SpawnConfig) -> Result<tokio::process::Child, DriverError> {
        // Write .gemini/settings.json with the canonical mcpServers.chat
        // entry + env vars (SettingsCopy mode). Bytes are deterministic and
        // byte-equivalent to Go's gemini.go::Spawn output for the same
        // inputs (validated by tests/driver_impl.rs::gemini_settings_json
        // _byte_identical_to_go).
        //
        // Codex PR #11 review: env_vars are now sourced from SpawnConfig
        // (populated by Task 1e's actor-side thread-through). Gemini's
        // EnvPropagation::SettingsCopy means the MCP child inherits these
        // ONLY via this JSON write — passing an empty slice (the previous
        // behavior) silently dropped every per-agent env var.
        let _settings_path = write_gemini_settings_json(
            cfg.working_dir,
            &self.bridge_binary,
            cfg.agent_id,
            cfg.server_url,
            cfg.auth_token,
            cfg.env_vars,
        )
        .map_err(DriverError::Io)?;

        spawn_gemini(&SpawnContext {
            gemini_binary: &self.gemini_binary,
            working_dir: cfg.working_dir,
            model: cfg.model,
            resume_session: cfg.resume_session,
            system_prompt: cfg.system_prompt,
            initial_prompt: cfg.initial_prompt,
            no_bridge: false,
        })
        .map_err(DriverError::Io)
    }

    fn parse_event(&self, line: &str) -> Vec<DriverEvent> {
        // gemini's `result` with status != "success" emits Error + TurnEnd
        // — `to_driver_events` expands the marker into two DriverEvents.
        to_driver_events(parse_line(line))
    }

    fn classify_stderr_line(&self, line: &str) -> Option<DriverEvent> {
        // gemini-cli reports 429 / quota exhaustion ONLY on stderr. Its
        // `--output-format stream-json` `result` line collapses the failure to
        // a generic `{"error":{"type":"unknown","message":"[API Error: An
        // unknown error occurred.]"}}` with no quota signal — so the stdout
        // parser cannot see it. The stderr line looks like:
        //   `... TerminalQuotaError: You have exhausted your capacity on this
        //    model. Your quota will reset after 2h2m2s.`  (cause.code = 429,
        //    reason = QUOTA_EXHAUSTED on following lines). Detect it here and
        // emit a RateLimit so the actor takes the quota-stop path (clean
        // "rate limited" stop) instead of a generic error + self-heal churn.
        classify_gemini_stderr_quota(line)
    }

    fn encode_stdin_message(
        &self,
        text: &str,
        session_id: Option<&str>,
        mode: MessageMode,
    ) -> Option<String> {
        encode_stdin_message(text, session_id, mode)
    }

    fn supports_turn_cancel(&self) -> bool {
        // gemini handles SIGINT (exit code 130) — actor sends SIGINT to
        // cancel the current turn (gemini.go::SteerTurn is unsupported
        // but cancel is).
        true
    }

    fn supports_turn_steer(&self) -> bool {
        false
    }

    async fn turn_steer(&self, _input: &str) -> Result<(), DriverError> {
        // gemini.go:605 — `SteerTurn` returns ErrTurnSteerUnsupported.
        Err(DriverError::TurnSteerUnsupported)
    }

    fn skill_search_paths(&self, workspace: &Path) -> Vec<PathBuf> {
        // gemini.go:216-221 — workspace-scoped first, then user-global.
        let mut paths: Vec<PathBuf> = vec![workspace.join(".gemini").join("skills")];
        if let Some(home) = dirs::home_dir() {
            paths.push(home.join(".gemini").join("skills"));
        }
        paths
    }

    fn as_exit_code_classifier(&self) -> Option<&dyn ExitCodeClassifier> {
        Some(self)
    }

    fn as_session_file_gc(&self) -> Option<&dyn SessionFileGC> {
        Some(self)
    }
}

/// Detect a gemini-cli quota / 429 stderr line and turn it into a
/// `RateLimit` event. Matches the well-known quota signatures; parses the
/// "quota will reset after <dur>" hint into an absolute `resets_at` (unix
/// secs) when present. Returns `None` for any non-quota stderr line.
fn classify_gemini_stderr_quota(line: &str) -> Option<DriverEvent> {
    let l = line.to_ascii_lowercase();
    let is_quota = l.contains("terminalquotaerror")
        || l.contains("quota_exhausted")
        || l.contains("exhausted your capacity")
        || l.contains("quota will reset");
    if !is_quota {
        return None;
    }
    let resets_at = parse_quota_reset_secs(line)
        .map(|secs| now_unix_secs().saturating_add(secs))
        .unwrap_or(0);
    Some(DriverEvent::RateLimit {
        limit_type: "quota".to_string(),
        // "rejected" → `rate_limit_is_limited` true → daemon takes the
        // quota-stop path (stop child, no self-heal session churn).
        status: "rejected".to_string(),
        resets_at,
        overage_status: None,
        overage_resets: None,
        is_using_overage: false,
    })
}

/// Parse the duration in a `... reset after 2h2m2s.` stderr hint into seconds.
/// Supports `d`/`h`/`m`/`s` units (e.g. `9h43m27s`, `120s`, `2h2m2s`).
fn parse_quota_reset_secs(line: &str) -> Option<i64> {
    let idx = line.find("reset after ")?;
    let rest = &line[idx + "reset after ".len()..];
    let token: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .collect();
    let mut total: i64 = 0;
    let mut num: i64 = 0;
    let mut matched_unit = false;
    for c in token.chars() {
        if c.is_ascii_digit() {
            num = num
                .saturating_mul(10)
                .saturating_add((c as u8 - b'0') as i64);
        } else {
            let mult = match c.to_ascii_lowercase() {
                'd' => 86_400,
                'h' => 3_600,
                'm' => 60,
                's' => 1,
                _ => return None,
            };
            total = total.saturating_add(num.saturating_mul(mult));
            num = 0;
            matched_unit = true;
        }
    }
    if matched_unit {
        Some(total)
    } else {
        None
    }
}

fn now_unix_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

impl ExitCodeClassifier for GeminiDriver {
    /// Maps gemini-cli's documented exit codes
    /// (`packages/core/src/utils/exitCodes.ts`):
    ///   - 41  FATAL_AUTHENTICATION_ERROR → AuthFailed
    ///   - 52  FATAL_CONFIG_ERROR         → ConfigError
    ///   - 130 FATAL_CANCELLATION_ERROR   → Cancelled
    ///   - other → Normal (so daemon's retry/auto-recover paths handle them)
    fn classify_exit_code(&self, code: i32) -> ExitCodeClass {
        match code {
            41 => ExitCodeClass::AuthFailed,
            52 => ExitCodeClass::ConfigError,
            130 => ExitCodeClass::Cancelled,
            _ => ExitCodeClass::Normal,
        }
    }
}

impl SessionFileGC for GeminiDriver {
    fn gc_session_files(&self, home: &Path, max_age: Duration) -> std::io::Result<GcStats> {
        gc_gemini_session_files(home, max_age)
    }
}

#[cfg(test)]
mod stderr_quota_tests {
    use super::*;

    #[test]
    fn parse_quota_reset_secs_handles_units() {
        // real gemini hint: "Your quota will reset after 2h2m2s."
        assert_eq!(
            parse_quota_reset_secs("Your quota will reset after 2h2m2s."),
            Some(2 * 3600 + 2 * 60 + 2)
        );
        assert_eq!(
            parse_quota_reset_secs("reset after 9h43m27s"),
            Some(9 * 3600 + 43 * 60 + 27)
        );
        assert_eq!(parse_quota_reset_secs("reset after 120s."), Some(120));
        assert_eq!(parse_quota_reset_secs("no reset hint here"), None);
    }

    #[test]
    fn classify_detects_terminal_quota_line() {
        // the exact stderr shape captured from gemini-cli on a live 429
        let line = "Error when talking to Gemini API ... TerminalQuotaError: You have \
                    exhausted your capacity on this model. Your quota will reset after 2h2m2s.";
        match classify_gemini_stderr_quota(line) {
            Some(DriverEvent::RateLimit {
                limit_type,
                status,
                resets_at,
                is_using_overage,
                ..
            }) => {
                assert_eq!(limit_type, "quota");
                assert_eq!(status, "rejected"); // → rate_limit_is_limited → quota-stop path
                assert!(resets_at > 0, "should parse a reset time from the hint");
                assert!(!is_using_overage);
            }
            other => panic!("expected RateLimit, got {other:?}"),
        }
    }

    #[test]
    fn classify_matches_reason_token_without_duration() {
        // a quota line without a parseable duration still classifies (resets_at=0)
        let line = "  reason: 'QUOTA_EXHAUSTED'";
        assert!(matches!(
            classify_gemini_stderr_quota(line),
            Some(DriverEvent::RateLimit { resets_at: 0, .. })
        ));
    }

    #[test]
    fn classify_ignores_non_quota_stderr() {
        for l in [
            "Ripgrep is not available. Falling back to GrepTool.",
            "[ERROR] [ImportProcessor] Failed to import mentions: ENOENT",
            "Invalid stream: The model returned an empty response or malformed tool call",
            "MCP issues detected. Run /mcp list for status.",
        ] {
            assert!(
                classify_gemini_stderr_quota(l).is_none(),
                "must not classify non-quota line: {l}"
            );
        }
    }
}
