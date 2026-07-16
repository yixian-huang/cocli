//! Spawn helper for the gemini CLI runtime + session-GC + settings.json
//! writer.
//!
//! Mirrors Go `daemon/drivers/gemini.go`:
//!   - `Spawn` (line 242-343) — CLI args + env (FORCE_COLOR=0 / NO_COLOR=1)
//!   - `PrepareWorkspace` (line 225-240) — `.gemini/` dir + optional GEMINI.md
//!   - `GCSessionFiles` (line 110-160) — prune `~/.gemini/tmp/<slug>/chats/*.json`

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use cocli_driver_core::prompt_arg;
use cocli_driver_core::types::GcStats;
use tokio::process::{Child, Command};

/// Inputs that gemini's spawn needs that aren't carried directly by
/// `cocli-driver-core::types::SpawnConfig`. The driver impl populates
/// this from `SpawnConfig` + the driver's own state (binary path).
pub struct SpawnContext<'a> {
    pub gemini_binary: &'a Path,
    pub working_dir: &'a Path,
    pub model: &'a str,
    pub resume_session: Option<&'a str>,
    /// Persistent platform contract written to `GEMINI.md`; also used as
    /// the `-p` fallback only when no initial prompt is supplied.
    pub system_prompt: &'a str,
    /// Per-spawn user prompt for headless mode. When non-empty, this becomes
    /// the `-p <prompt>` argument while `system_prompt` stays available to
    /// the workspace contract file.
    pub initial_prompt: &'a str,
    /// When true, skip the `--allowed-mcp-server-names chat` flag and the
    /// settings.json write (parity with Go `NoBridge=true`).
    pub no_bridge: bool,
}

/// Build the gemini-cli `Command`. Flag order (see `gemini_args`):
///   1. `--approval-mode yolo`
///   2. `--output-format stream-json`
///   3. `--skip-trust` (trust the workspace for the session; keeps yolo from
///      being downgraded in an untrusted dir — diverges from Go gemini.go,
///      which predates gemini-cli's folder-trust gate)
///   4. (bridge) `--allowed-mcp-server-names chat`
///   5. (model)  `--model <model>`
///   6. (resume) `--resume <sid>`
///   7. (prompt) `-p <initial_prompt>` when non-empty, otherwise
///      `-p <system_prompt>` when non-empty
///
/// Sets `FORCE_COLOR=0` + `NO_COLOR=1` so stream-json stdout stays clean.
pub fn spawn_gemini(ctx: &SpawnContext) -> std::io::Result<Child> {
    let mut cmd = Command::new(ctx.gemini_binary);
    cmd.current_dir(ctx.working_dir);
    cmd.args(gemini_args(ctx));
    cmd.env("FORCE_COLOR", "0").env("NO_COLOR", "1");
    cmd.stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    cmd.spawn()
}

/// Build the gemini-cli argument vector (pure, no spawn) so the flag set is
/// unit-testable. See `spawn_gemini` for the canonical order + rationale.
fn gemini_args(ctx: &SpawnContext) -> Vec<String> {
    let mut args = vec![
        "--approval-mode".to_string(),
        "yolo".to_string(),
        "--output-format".to_string(),
        "stream-json".to_string(),
        // Trust the agent workspace for this session. Gemini's folder-trust
        // gate otherwise downgrades `--approval-mode yolo` to "default" in an
        // untrusted dir and the headless turn hangs awaiting approval.
        "--skip-trust".to_string(),
    ];
    if !ctx.no_bridge {
        // Defense in depth — gemini-cli merges ~/.gemini/settings.json which
        // is user-controlled. Pinning the allowlist to "chat" prevents any
        // user-installed MCP server from sneaking into a daemon-managed
        // agent's tool surface. See gemini.go:301-310.
        args.push("--allowed-mcp-server-names".to_string());
        args.push("chat".to_string());
    }
    if !ctx.model.is_empty() {
        args.push("--model".to_string());
        args.push(ctx.model.to_string());
    }
    if let Some(sid) = ctx.resume_session {
        args.push("--resume".to_string());
        args.push(sid.to_string());
    }
    if let Some(prompt) = prompt_arg(ctx.initial_prompt, ctx.system_prompt) {
        args.push("-p".to_string());
        args.push(prompt.to_string());
    }
    args
}

/// Write `<work_dir>/.gemini/settings.json` with the canonical
/// `mcpServers.chat` entry + per-agent env vars.
///
/// Bytes are deterministic and byte-equivalent to Go's
/// `json.MarshalIndent(map[string]any{...}, "", "  ")` output: top-level
/// `mcpServers` only key; inside `chat`, fields ordered alphabetically as
/// `args`, `command`, `env`; `env` map sorted by key (BTreeMap).
///
/// The Go driver writes this file inside `Spawn` (after reading any existing
/// settings.json and overwriting the `mcpServers` key). For Phase 2b parity
/// the Rust driver writes it from `Driver::spawn` via this helper. Calling
/// this on a workspace with no pre-existing settings.json produces the
/// fixture compared in `tests/driver_impl.rs::gemini_settings_json_byte_identical_to_go`.
pub fn write_gemini_settings_json(
    work_dir: &Path,
    bridge_binary: &Path,
    agent_id: &str,
    server_url: &str,
    auth_token: &str,
    env_vars: &[(String, String)],
) -> std::io::Result<PathBuf> {
    use cocli_bridge_config::bridge_args;
    use serde::Serialize;

    // Field order matches Go's alphabetized `map[string]any` marshaling:
    // args, command, env.
    #[derive(Serialize)]
    struct McpServer<'a> {
        args: Vec<String>,
        command: &'a str,
        env: BTreeMap<String, String>,
    }

    // gemini-cli >= 0.43 denies configured MCP servers in headless
    // (`--output-format stream-json`) mode unless `mcp.autoAllowInHeadless`
    // is set (PR #27215 / issue #26021). The daemon always runs gemini
    // headless, so opt in here. Declared before `mcpServers` so the key
    // order is deterministic (`mcp`, then `mcpServers`).
    #[derive(Serialize)]
    struct McpPolicy {
        #[serde(rename = "autoAllowInHeadless")]
        auto_allow_in_headless: bool,
    }

    #[derive(Serialize)]
    struct McpRoot<'a> {
        mcp: McpPolicy,
        #[serde(rename = "mcpServers")]
        mcp_servers: BTreeMap<String, McpServer<'a>>,
    }

    let command = bridge_binary.to_str().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "bridge_binary path is not valid UTF-8",
        )
    })?;

    let env: BTreeMap<String, String> = env_vars.iter().cloned().collect();
    let mut servers = BTreeMap::new();
    servers.insert(
        "chat".to_string(),
        McpServer {
            args: bridge_args(agent_id, server_url, auth_token),
            command,
            env,
        },
    );
    let root = McpRoot {
        mcp: McpPolicy {
            auto_allow_in_headless: true,
        },
        mcp_servers: servers,
    };
    let bytes = serde_json::to_vec_pretty(&root).map_err(std::io::Error::other)?;

    let settings_dir = work_dir.join(".gemini");
    std::fs::create_dir_all(&settings_dir)?;
    let path = settings_dir.join("settings.json");
    std::fs::write(&path, bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))?;
    }
    Ok(path)
}

/// Insert `path` into a gemini-cli `trustedFolders.json` map (value
/// `"TRUST_FOLDER"`), preserving existing entries. Idempotent. Empty or
/// unparseable input is treated as an empty map. Output is 2-space pretty
/// JSON, matching gemini-cli's own writer.
fn merge_trusted_folder(existing: &str, path: &str) -> String {
    let mut map: serde_json::Map<String, serde_json::Value> = serde_json::from_str(existing)
        .ok()
        .and_then(|v: serde_json::Value| match v {
            serde_json::Value::Object(m) => Some(m),
            _ => None,
        })
        .unwrap_or_default();
    map.insert(
        path.to_string(),
        serde_json::Value::String("TRUST_FOLDER".to_string()),
    );
    serde_json::to_string_pretty(&serde_json::Value::Object(map))
        .unwrap_or_else(|_| String::from("{}"))
}

/// Register `work_dir` as a persistently-trusted folder in `trusted_file`.
///
/// gemini-cli 0.40.1 only loads project-scoped `mcpServers` (our bridge,
/// from `<work_dir>/.gemini/settings.json`) when the folder is listed in
/// `~/.gemini/trustedFolders.json`; the session-only `--skip-trust` flag
/// covers the approval-mode downgrade but NOT project-MCP loading. The path
/// is lowercased because gemini-cli normalizes trust keys to lowercase when
/// it compares the process cwd.
///
/// Read-merge-(atomic)write. Concurrency: two simultaneous spawns could
/// drop one entry; `prepare_workspace` re-registers on every (re)spawn so it
/// self-heals on the next turn.
fn register_trusted_folder_at(trusted_file: &Path, work_dir: &str) -> std::io::Result<()> {
    let lowercased = work_dir.to_lowercase();
    let existing = std::fs::read_to_string(trusted_file).unwrap_or_default();
    let merged = merge_trusted_folder(&existing, &lowercased);
    if let Some(parent) = trusted_file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Atomic replace: write to a unique temp in the same dir, then rename.
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = trusted_file.with_file_name(format!("trustedFolders.json.{nanos}.tmp"));
    std::fs::write(&tmp, merged.as_bytes())?;
    std::fs::rename(&tmp, trusted_file)?;
    Ok(())
}

/// Register `work_dir` in the user's `~/.gemini/trustedFolders.json` so
/// gemini-cli loads the project bridge MCP server (see
/// `register_trusted_folder_at`).
pub fn register_gemini_trusted_workspace(work_dir: &Path) -> std::io::Result<()> {
    let home = dirs::home_dir()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "home dir not found"))?;
    let trusted = home.join(".gemini").join("trustedFolders.json");
    let work_dir_str = work_dir.to_str().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "work_dir is not valid UTF-8",
        )
    })?;
    register_trusted_folder_at(&trusted, work_dir_str)
}

/// Prune `~/.gemini/tmp/<slug>/chats/*.{json,jsonl}` whose mtime is older
/// than `max_age`. Mirrors Go `gemini.go::GCSessionFiles` (line 110-160).
///
/// - `home == ""` or `max_age <= 0` → no-op (returns zeros, no error).
/// - Missing `~/.gemini/tmp` → no-op (not an error; gemini-cli may not have
///   run on this host yet).
/// - Per slug: missing `chats/` subdir is skipped (not all slugs persist
///   session files).
/// - Non-`.json`/`.jsonl` files are ignored even when stale.
/// - Files whose mtime is **after** the cutoff are skipped (active session).
pub fn gc_gemini_session_files(home: &Path, max_age: Duration) -> std::io::Result<GcStats> {
    if home.as_os_str().is_empty() || max_age.is_zero() {
        return Ok(GcStats::default());
    }
    let tmp_root = home.join(".gemini").join("tmp");
    let cutoff = match SystemTime::now().checked_sub(max_age) {
        Some(t) => t,
        None => return Ok(GcStats::default()),
    };

    let entries = match std::fs::read_dir(tmp_root) {
        Ok(e) => e,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(GcStats::default());
        }
        Err(err) => return Err(err),
    };

    let mut removed: usize = 0;
    let mut freed_bytes: u64 = 0;

    for slug_entry in entries.flatten() {
        let slug_path = slug_entry.path();
        let is_dir = slug_entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        if !is_dir {
            continue;
        }
        let chats_dir = slug_path.join("chats");
        let chats = match std::fs::read_dir(&chats_dir) {
            Ok(c) => c,
            Err(_) => continue, // chats/ may not exist for every slug
        };
        for file_entry in chats.flatten() {
            let path = file_entry.path();
            let meta = match file_entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };
            if meta.is_dir() {
                continue;
            }
            let name = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n,
                None => continue,
            };
            if !name.ends_with(".json") && !name.ends_with(".jsonl") {
                continue;
            }
            let mtime = match meta.modified() {
                Ok(t) => t,
                Err(_) => continue,
            };
            if mtime > cutoff {
                continue;
            }
            let size = meta.len();
            if std::fs::remove_file(&path).is_ok() {
                removed += 1;
                freed_bytes += size;
            }
        }
    }

    Ok(GcStats {
        removed,
        freed_bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx<'a>(system_prompt: &'a str, initial_prompt: &'a str) -> SpawnContext<'a> {
        SpawnContext {
            gemini_binary: Path::new("/bin/false"),
            working_dir: Path::new("/tmp"),
            model: "gemini-test",
            resume_session: None,
            system_prompt,
            initial_prompt,
            no_bridge: false,
        }
    }

    #[test]
    fn gemini_args_use_headless_prompt_arg_precedence() {
        let with_initial = ctx("PLATFORM CONTRACT", "BOOTSTRAP TURN");
        let args = gemini_args(&with_initial);
        assert!(args.windows(2).any(|w| w == ["-p", "BOOTSTRAP TURN"]));

        let system_only = ctx("PLATFORM CONTRACT", "");
        let args = gemini_args(&system_only);
        assert!(args.windows(2).any(|w| w == ["-p", "PLATFORM CONTRACT"]));

        let empty = ctx("", "");
        let args = gemini_args(&empty);
        assert!(!args.contains(&"-p".to_string()));
    }

    #[test]
    fn gemini_args_include_skip_trust_in_canonical_order() {
        // `--skip-trust` trusts the spawn cwd for the session. Without it,
        // gemini's folder-trust gate downgrades `--approval-mode yolo` to
        // "default" in an untrusted agent workspace ("Approval mode
        // overridden to default because the current folder is not trusted")
        // and the headless turn hangs waiting for an approval that can never
        // arrive. Observed in the 2026-05-30 smoke. It sits with the
        // always-on flags, before the bridge/model/resume/prompt conditionals.
        let c = ctx("", "BOOT");
        let args = gemini_args(&c);
        let expected: Vec<String> = [
            "--approval-mode",
            "yolo",
            "--output-format",
            "stream-json",
            "--skip-trust",
            "--allowed-mcp-server-names",
            "chat",
            "--model",
            "gemini-test",
            "-p",
            "BOOT",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(args, expected);
    }

    #[test]
    fn merge_trusted_folder_adds_to_empty_input() {
        let out = merge_trusted_folder("", "/ws/a");
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["/ws/a"], "TRUST_FOLDER");
    }

    #[test]
    fn merge_trusted_folder_preserves_existing_entries() {
        let out = merge_trusted_folder(r#"{"/keep/me":"TRUST_FOLDER"}"#, "/ws/a");
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["/keep/me"], "TRUST_FOLDER");
        assert_eq!(v["/ws/a"], "TRUST_FOLDER");
    }

    #[test]
    fn merge_trusted_folder_is_idempotent() {
        let once = merge_trusted_folder("{}", "/ws/a");
        let twice = merge_trusted_folder(&once, "/ws/a");
        assert_eq!(once, twice);
    }

    #[test]
    fn register_trusted_folder_lowercases_key_and_persists() {
        let dir = tempfile::tempdir().unwrap();
        let tf = dir.path().join(".gemini").join("trustedFolders.json");
        register_trusted_folder_at(&tf, "/Users/Me/Agent WS").unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&tf).unwrap()).unwrap();
        // gemini compares lowercased cwd → key must be lowercased.
        assert_eq!(v["/users/me/agent ws"], "TRUST_FOLDER");
    }

    #[test]
    fn register_trusted_folder_preserves_and_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let tf = dir.path().join("trustedFolders.json");
        std::fs::write(&tf, r#"{"/pre/existing":"TRUST_FOLDER"}"#).unwrap();
        register_trusted_folder_at(&tf, "/ws/a").unwrap();
        register_trusted_folder_at(&tf, "/ws/a").unwrap(); // twice
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&tf).unwrap()).unwrap();
        assert_eq!(v["/pre/existing"], "TRUST_FOLDER");
        assert_eq!(v["/ws/a"], "TRUST_FOLDER");
        assert_eq!(v.as_object().unwrap().len(), 2);
    }
}
