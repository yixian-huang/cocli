//! Workspace injection for the agent-facing `cocli` CLI wrapper.
//!
//! Writes `.cocli/bin/cocli`, `.cocli/action.json`, and `.cocli/token` into
//! the agent workspace and returns env vars for spawn.

use std::path::{Path, PathBuf};

use serde::Serialize;

const ACTION_CONFIG_VERSION: &str = "2026-06-03";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionInjection {
    pub env_vars: Vec<(String, String)>,
    pub cocli_bin_dir: PathBuf,
    pub config_path: PathBuf,
}

#[derive(Serialize)]
struct ActionConfigFile<'a> {
    version: &'static str,
    agent_id: &'a str,
    server_url: &'a str,
    token_path: String,
}

/// Materialise the injected platform CLI into `work_dir/.cocli/`.
///
/// `server_url` must already be normalized to HTTP(S) for the bridge client.
pub fn inject_action_cli(
    work_dir: &Path,
    bridge_binary: &Path,
    agent_id: &str,
    server_url: &str,
    auth_token: &str,
) -> std::io::Result<ActionInjection> {
    let cocli_root = work_dir.join(".cocli");
    let bin_dir = cocli_root.join("bin");
    std::fs::create_dir_all(&bin_dir)?;

    let config_path = cocli_root.join("action.json");
    let manifest_path = cocli_root.join("actions.manifest.json");
    let token_path = cocli_root.join("token");
    let wrapper_path = bin_dir.join("cocli");

    let wrapper = crate::render_cocli_wrapper(bridge_binary);
    write_executable(&wrapper_path, wrapper.as_bytes())?;

    let cfg = ActionConfigFile {
        version: ACTION_CONFIG_VERSION,
        agent_id,
        server_url,
        token_path: token_path.to_string_lossy().into_owned(),
    };
    let cfg_bytes = serde_json::to_vec_pretty(&cfg)?;
    write_secret_file(&config_path, &cfg_bytes)?;
    write_manifest_file(&manifest_path, crate::embedded_manifest_bytes())?;
    write_secret_file(&token_path, auth_token.as_bytes())?;

    let config_str = config_path.to_string_lossy().into_owned();
    let bin_str = bin_dir.to_string_lossy().into_owned();
    let cocli_root_str = cocli_root.to_string_lossy().into_owned();
    let mut env_vars = vec![
        ("COCLI_ACTION_CONFIG".into(), config_str.clone()),
        ("COCLI_ACTION_FORMAT".into(), "json".into()),
        ("COCLI_ACTION_ROOT".into(), cocli_root_str),
    ];

    let path_key = "PATH";
    let existing_path = std::env::var_os(path_key)
        .map(|v| v.to_string_lossy().into_owned())
        .unwrap_or_default();
    let merged_path = if existing_path.is_empty() {
        bin_str.clone()
    } else {
        format!("{bin_str}:{existing_path}")
    };
    env_vars.push((path_key.into(), merged_path));

    Ok(ActionInjection {
        env_vars,
        cocli_bin_dir: bin_dir,
        config_path,
    })
}

/// Merge injected env vars into an existing spawn env list. Later entries in
/// `base` win for duplicate keys except `PATH`, which is always prepended with
/// the injected bin dir when present.
pub fn merge_action_env_vars(
    base: &[(String, String)],
    injection: &ActionInjection,
) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = base.to_vec();
    for (key, value) in &injection.env_vars {
        if key == "PATH" {
            if let Some((_, existing)) = out.iter_mut().find(|(k, _)| k == "PATH") {
                if !existing.starts_with(injection.cocli_bin_dir.to_string_lossy().as_ref()) {
                    existing.clone_from(value);
                }
                continue;
            }
        }
        if let Some((_, slot)) = out.iter_mut().find(|(k, _)| k == key) {
            slot.clone_from(value);
        } else {
            out.push((key.clone(), value.clone()));
        }
    }
    out
}

fn write_executable(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write as _;
    let mut file = std::fs::File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}

fn write_manifest_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write as _;
    let mut file = std::fs::File::create(path)?;
    file.write_all(bytes)?;
    if !bytes.ends_with(b"\n") {
        file.write_all(b"\n")?;
    }
    file.sync_all()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o644))?;
    }
    Ok(())
}

fn write_secret_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write as _;
    let mut file = std::fs::File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inject_action_cli_writes_wrapper_config_and_token() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let work_dir = tmp.path().join("agent-1");
        std::fs::create_dir_all(&work_dir).expect("mkdir work_dir");

        let injection = inject_action_cli(
            &work_dir,
            Path::new("/opt/cocli/bin/cocli-bridge"),
            "agent-1",
            "http://localhost:8090",
            "tok-abc",
        )
        .expect("inject");

        let wrapper = std::fs::read_to_string(work_dir.join(".cocli/bin/cocli")).expect("wrapper");
        assert!(wrapper.contains("/opt/cocli/bin/cocli-bridge"));
        assert!(wrapper.contains("audit.jsonl"));
        assert!(wrapper.contains("exec \"$BRIDGE\" action \"$@\""));

        let cfg_raw =
            std::fs::read_to_string(work_dir.join(".cocli/action.json")).expect("action.json");
        assert!(cfg_raw.contains("\"agent_id\": \"agent-1\""));
        assert!(cfg_raw.contains("\"server_url\": \"http://localhost:8090\""));

        let manifest_raw = std::fs::read_to_string(work_dir.join(".cocli/actions.manifest.json"))
            .expect("actions.manifest.json");
        assert!(manifest_raw.contains("\"message.send\""));
        assert!(manifest_raw.contains("\"send_message\""));

        let token = std::fs::read_to_string(work_dir.join(".cocli/token")).expect("token");
        assert_eq!(token, "tok-abc");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(work_dir.join(".cocli/token"))
                .expect("token meta")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }

        let cocli_bin = Path::new(".cocli").join("bin");
        let cocli_bin = cocli_bin.to_string_lossy();
        assert!(injection.env_vars.iter().any(|(k, v)| {
            k == "COCLI_ACTION_CONFIG"
                && Path::new(v).ends_with(Path::new(".cocli").join("action.json"))
        }));
        assert!(injection
            .env_vars
            .iter()
            .any(|(k, v)| k == "PATH" && v.contains(cocli_bin.as_ref())));
        assert!(injection
            .env_vars
            .iter()
            .any(|(k, v)| k == "COCLI_ACTION_ROOT" && Path::new(v).ends_with(".cocli")));
    }

    // Injected wrapper is a POSIX shell script; execute it only on Unix.
    #[cfg(unix)]
    #[test]
    fn injected_wrapper_audits_task_update_status() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let work_dir = tmp.path().join("agent-update");
        std::fs::create_dir_all(&work_dir).expect("mkdir work_dir");

        inject_action_cli(
            &work_dir,
            Path::new("/opt/cocli/bin/cocli-bridge"),
            "agent-update",
            "http://localhost:8090",
            "tok-abc",
        )
        .expect("inject");

        let cocli = work_dir.join(".cocli/bin/cocli");
        let _status = std::process::Command::new(cocli)
            .env("COCLI_ACTION_CONFIG", work_dir.join(".cocli/action.json"))
            .env("COCLI_ACTION_FORMAT", "json")
            .args([
                "task",
                "update-status",
                "--channel",
                "#ops",
                "--task-number",
                "1",
                "--status",
                "done",
            ])
            .status()
            .expect("run cocli");

        let audit_raw =
            std::fs::read_to_string(work_dir.join(".cocli/audit.jsonl")).unwrap_or_default();
        assert!(
            audit_raw.contains("\"action\":\"task.update_status\""),
            "audit should normalize hyphen CLI to underscore action (audit={audit_raw})"
        );
        assert!(audit_raw.contains("\"tool\":\"update_task_status\""));
    }

    #[cfg(unix)]
    #[test]
    fn injected_wrapper_appends_audit_jsonl() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let work_dir = tmp.path().join("agent-2");
        std::fs::create_dir_all(&work_dir).expect("mkdir work_dir");

        inject_action_cli(
            &work_dir,
            Path::new("/opt/cocli/bin/cocli-bridge"),
            "agent-2",
            "http://localhost:8090",
            "tok-abc",
        )
        .expect("inject");

        let cocli = work_dir.join(".cocli/bin/cocli");
        let status = std::process::Command::new(cocli)
            .env("COCLI_ACTION_CONFIG", work_dir.join(".cocli/action.json"))
            .env("COCLI_ACTION_FORMAT", "json")
            .args(["message", "digest"])
            .status()
            .expect("run cocli");
        // Bridge may be absent in CI; audit must still be written before exec fails.
        let audit_path = work_dir.join(".cocli/audit.jsonl");
        let audit_raw = std::fs::read_to_string(audit_path).unwrap_or_default();
        assert!(
            audit_raw.contains("\"action\":\"message.digest\""),
            "audit should record friendly invocation before bridge exec (status={:?}, audit={audit_raw})",
            status.code()
        );
        assert!(audit_raw.contains("\"tool\":\"message_digest\""));
    }

    #[test]
    fn merge_action_env_vars_updates_path_and_config() {
        let injection = ActionInjection {
            env_vars: vec![
                (
                    "COCLI_ACTION_CONFIG".into(),
                    "/tmp/a/.cocli/action.json".into(),
                ),
                ("PATH".into(), "/tmp/a/.cocli/bin:/usr/bin".into()),
            ],
            cocli_bin_dir: PathBuf::from("/tmp/a/.cocli/bin"),
            config_path: PathBuf::from("/tmp/a/.cocli/action.json"),
        };
        let merged = merge_action_env_vars(
            &[
                ("FOO".into(), "bar".into()),
                ("PATH".into(), "/usr/bin".into()),
            ],
            &injection,
        );
        assert!(merged.iter().any(|(k, v)| k == "FOO" && v == "bar"));
        let path = merged
            .iter()
            .find(|(k, _)| k == "PATH")
            .map(|(_, v)| v.as_str())
            .expect("PATH");
        assert_eq!(path, "/tmp/a/.cocli/bin:/usr/bin");
    }
}
