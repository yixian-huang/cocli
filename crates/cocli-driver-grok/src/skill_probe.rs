use std::path::{Path, PathBuf};
use std::time::Duration;

use cocli_driver_core::{DriverError, NativeSkill, NativeSkillProbe, SkillDiscoveryEvidence};
use serde::Deserialize;
use tokio::process::Command;

const PROBE_TIMEOUT: Duration = Duration::from_secs(10);
const GROK_SKILLS_EVIDENCE: SkillDiscoveryEvidence = SkillDiscoveryEvidence {
    source: "grok_cli",
    detail: "inspect --json",
    proves_session_visibility: false,
};

#[derive(Debug, Deserialize)]
struct InspectReport {
    skills: Vec<InspectSkill>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InspectSkill {
    name: String,
    #[serde(default)]
    description: String,
    source: InspectSkillSource,
    user_invocable: Option<bool>,
    disabled: Option<bool>,
    compatibility_status: Option<String>,
}

#[derive(Debug, Deserialize)]
struct InspectSkillSource {
    #[serde(rename = "type")]
    kind: String,
    path: Option<PathBuf>,
}

pub(crate) async fn probe_skills(
    grok_binary: &Path,
    workspace: &Path,
) -> Result<NativeSkillProbe, DriverError> {
    probe_skills_with_timeout(grok_binary, workspace, PROBE_TIMEOUT).await
}

async fn probe_skills_with_timeout(
    grok_binary: &Path,
    workspace: &Path,
    timeout: Duration,
) -> Result<NativeSkillProbe, DriverError> {
    let output = tokio::time::timeout(
        timeout,
        Command::new(grok_binary)
            .arg("inspect")
            .arg("--json")
            .current_dir(workspace)
            .kill_on_drop(true)
            .output(),
    )
    .await
    .map_err(|_| DriverError::Other("grok inspect --json timed out".to_owned()))?
    .map_err(DriverError::Io)?;
    if !output.status.success() {
        return Err(DriverError::Other(format!(
            "grok inspect --json failed with {}: {}",
            output.status,
            concise_stderr(&output.stderr)
        )));
    }
    parse_inspect_report(&output.stdout)
}

fn parse_inspect_report(bytes: &[u8]) -> Result<NativeSkillProbe, DriverError> {
    let report: InspectReport = serde_json::from_slice(bytes)
        .map_err(|error| DriverError::Other(format!("decode grok inspect --json: {error}")))?;
    let mut skills = Vec::new();
    for skill in report.skills {
        skills.push(NativeSkill {
            name: skill.name,
            description: skill.description,
            path: skill.source.path,
            source: skill.source.kind.clone(),
            scope: normalize_source_scope(&skill.source.kind).to_owned(),
            enabled: effective_enabled(skill.disabled, skill.compatibility_status.as_deref()),
            user_invocable: skill.user_invocable,
        });
    }
    Ok(NativeSkillProbe {
        evidence: GROK_SKILLS_EVIDENCE,
        skills,
        issues: Vec::new(),
    })
}

fn normalize_source_scope(source: &str) -> &str {
    match source {
        "project" | "repo" | "workspace" => "repo",
        "builtin" | "bundled" | "server" | "system" | "admin" => "system",
        _ => "user",
    }
}

fn effective_enabled(disabled: Option<bool>, compatibility_status: Option<&str>) -> Option<bool> {
    if compatibility_status == Some("disabled") {
        return Some(false);
    }
    if let Some(disabled) = disabled {
        return Some(!disabled);
    }
    match compatibility_status {
        Some("enabled") => Some(true),
        Some("disabled") => Some(false),
        _ => None,
    }
}

fn concise_stderr(bytes: &[u8]) -> String {
    let stderr = String::from_utf8_lossy(bytes);
    stderr
        .chars()
        .take(500)
        .collect::<String>()
        .trim()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_current_inspect_skill_shape_and_optional_compatibility() {
        let probe = parse_inspect_report(
            br#"{
                "grokVersion": "0.2.101",
                "skills": [
                    {
                        "name": "reviewer",
                        "description": "Reviews changes",
                        "source": {"type": "user", "path": "/tmp/reviewer/SKILL.md"},
                        "userInvocable": true,
                        "disabled": false,
                        "vendor": "claude",
                        "compatibilityStatus": "disabled"
                    },
                    {
                        "name": "bundled",
                        "description": "Bundled skill",
                        "source": {"type": "bundled", "path": "/tmp/bundled/SKILL.md"},
                        "userInvocable": false
                    },
                    {
                        "name": "builtin",
                        "description": "Builtin skill",
                        "source": {"type": "builtin"},
                        "userInvocable": false,
                        "disabled": false
                    }
                ]
            }"#,
        )
        .expect("valid inspect report");

        assert_eq!(probe.evidence.source, "grok_cli");
        assert!(!probe.evidence.proves_session_visibility);
        assert_eq!(probe.skills[0].enabled, Some(false));
        assert_eq!(probe.skills[0].user_invocable, Some(true));
        assert_eq!(probe.skills[1].scope, "system");
        assert_eq!(probe.skills[1].enabled, None);
        assert_eq!(probe.skills[2].path, None);
        assert_eq!(probe.skills[2].enabled, Some(true));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn invokes_grok_inspect_json_in_the_workspace() {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().expect("temp directory");
        let binary = temp.path().join("fake-grok");
        let script = "#!/bin/sh\n[ \"$1\" = inspect ] || exit 2\n[ \"$2\" = --json ] || exit 3\nprintf '%s\\n' '{\"skills\":[]}'\n";
        fs::write(&binary, script).expect("fake grok executable");
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o755))
            .expect("executable permissions");

        let probe = probe_skills(&binary, temp.path())
            .await
            .expect("successful native probe");

        assert!(probe.skills.is_empty());
        assert_eq!(probe.evidence.detail, "inspect --json");
    }

    #[tokio::test]
    async fn missing_grok_cli_is_reported() {
        let temp = tempfile::tempdir().expect("temp directory");
        let error = probe_skills(&temp.path().join("missing-grok"), temp.path())
            .await
            .expect_err("missing executable should fail");
        assert!(matches!(error, DriverError::Io(_)));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn timeout_and_nonzero_exit_are_reported() {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().expect("temp directory");
        let slow = temp.path().join("slow-grok");
        // Hang on stdin forever so CI load cannot race a clean `sleep` exit.
        fs::write(&slow, "#!/bin/sh\nexec cat >/dev/null\n").expect("slow executable");
        fs::set_permissions(&slow, fs::Permissions::from_mode(0o755)).expect("permissions");
        let error = probe_skills_with_timeout(&slow, temp.path(), Duration::from_millis(50))
            .await
            .expect_err("probe should time out");
        let message = error.to_string().to_ascii_lowercase();
        assert!(
            message.contains("timed out")
                || message.contains("broken pipe")
                || message.contains("os error"),
            "expected bounded failure, got: {message}"
        );

        let failing = temp.path().join("failing-grok");
        fs::write(&failing, "#!/bin/sh\nprintf 'bad config' >&2\nexit 42\n")
            .expect("failing executable");
        fs::set_permissions(&failing, fs::Permissions::from_mode(0o755)).expect("permissions");
        let error = probe_skills(&failing, temp.path())
            .await
            .expect_err("nonzero exit should fail");
        assert!(error.to_string().contains("bad config"));
    }

    #[test]
    fn malformed_and_partial_json_are_handled() {
        let error = parse_inspect_report(b"{").expect_err("malformed JSON should fail");
        assert!(error.to_string().contains("decode grok inspect --json"));
        let error = parse_inspect_report(br#"{}"#).expect_err("partial JSON should fail");
        assert!(error.to_string().contains("decode grok inspect --json"));
    }
}
