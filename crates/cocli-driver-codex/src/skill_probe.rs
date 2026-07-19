use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use cocli_driver_core::{
    DriverError, NativeSkill, NativeSkillIssue, NativeSkillProbe, SkillDiscoveryEvidence,
};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdout, Command};

const PROBE_TIMEOUT: Duration = Duration::from_secs(10);
const CODEX_SKILLS_EVIDENCE: SkillDiscoveryEvidence = SkillDiscoveryEvidence {
    source: "codex_app_server",
    detail: "skills/list(forceReload)",
    proves_session_visibility: false,
};

#[derive(Debug, Deserialize)]
struct SkillsListResponse {
    data: Vec<SkillsListEntry>,
}

#[derive(Debug, Deserialize)]
struct SkillsListEntry {
    skills: Vec<SkillMetadata>,
    errors: Vec<SkillErrorInfo>,
}

#[derive(Debug, Deserialize)]
struct SkillMetadata {
    name: String,
    description: String,
    path: PathBuf,
    scope: String,
    enabled: bool,
}

#[derive(Debug, Deserialize)]
struct SkillErrorInfo {
    message: String,
    path: PathBuf,
}

pub(crate) async fn probe_skills(
    codex_binary: &Path,
    workspace: &Path,
) -> Result<NativeSkillProbe, DriverError> {
    let mut child = Command::new(codex_binary)
        .arg("app-server")
        .arg("--listen")
        .arg("stdio://")
        .current_dir(workspace)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(DriverError::Io)?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| DriverError::Other("codex skill probe has no stdin".to_owned()))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| DriverError::Other("codex skill probe has no stdout".to_owned()))?;
    let mut stdout = BufReader::new(stdout);

    let result = async {
        write_message(
            &mut stdin,
            &json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "clientInfo": {"name": "cocli-skill-doctor", "version": env!("CARGO_PKG_VERSION")},
                    "capabilities": {"experimentalApi": true}
                }
            }),
        )
        .await?;
        read_response(&mut stdout, 1).await?;

        write_message(
            &mut stdin,
            &json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}),
        )
        .await?;
        write_message(
            &mut stdin,
            &json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "skills/list",
                "params": {"cwds": [workspace], "forceReload": true}
            }),
        )
        .await?;
        let response = read_response(&mut stdout, 2).await?;
        parse_skills_response(&response)
    }
    .await;

    drop(stdin);
    let _ = child.kill().await;
    let _ = child.wait().await;
    result
}

async fn write_message(
    stdin: &mut tokio::process::ChildStdin,
    message: &Value,
) -> Result<(), DriverError> {
    let mut line = serde_json::to_vec(message)
        .map_err(|error| DriverError::Other(format!("encode codex skill probe: {error}")))?;
    line.push(b'\n');
    stdin.write_all(&line).await.map_err(DriverError::Io)?;
    stdin.flush().await.map_err(DriverError::Io)
}

async fn read_response(stdout: &mut BufReader<ChildStdout>, id: u64) -> Result<Value, DriverError> {
    tokio::time::timeout(PROBE_TIMEOUT, async {
        loop {
            let mut line = String::new();
            let read = stdout.read_line(&mut line).await.map_err(DriverError::Io)?;
            if read == 0 {
                return Err(DriverError::Other(format!(
                    "codex app-server exited before response {id}"
                )));
            }
            let message: Value = serde_json::from_str(&line).map_err(|error| {
                DriverError::Other(format!("decode codex app-server response: {error}"))
            })?;
            if message.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if let Some(error) = message.get("error") {
                return Err(DriverError::Other(format!(
                    "codex app-server request {id} failed: {error}"
                )));
            }
            return Ok(message);
        }
    })
    .await
    .map_err(|_| DriverError::Other(format!("codex app-server response {id} timed out")))?
}

fn parse_skills_response(response: &Value) -> Result<NativeSkillProbe, DriverError> {
    let result = response
        .get("result")
        .ok_or_else(|| DriverError::Other("codex skills/list response has no result".to_owned()))?;
    let parsed: SkillsListResponse = serde_json::from_value(result.clone())
        .map_err(|error| DriverError::Other(format!("decode codex skills/list result: {error}")))?;
    let mut skills = Vec::new();
    let mut issues = Vec::new();
    for entry in parsed.data {
        skills.extend(entry.skills.into_iter().map(|skill| NativeSkill {
            name: skill.name,
            description: skill.description,
            path: Some(skill.path),
            source: "codex_app_server".to_owned(),
            scope: skill.scope,
            enabled: Some(skill.enabled),
            user_invocable: None,
        }));
        issues.extend(entry.errors.into_iter().map(|error| NativeSkillIssue {
            message: error.message,
            path: Some(error.path),
        }));
    }
    Ok(NativeSkillProbe {
        evidence: CODEX_SKILLS_EVIDENCE,
        skills,
        issues,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[tokio::test]
    async fn runs_the_app_server_handshake_and_skills_list_probe() {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().expect("temp directory");
        let binary = temp.path().join("fake-codex");
        let skill_path = temp.path().join("reviewer/SKILL.md");
        let encoded_path = serde_json::to_string(&skill_path).expect("encoded skill path");
        let script = format!(
            "#!/bin/sh\nread _line\nprintf '%s\\n' '{{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{{}}}}'\nread _line\nread _line\nprintf '%s\\n' '{{\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{{\"data\":[{{\"cwd\":\"/tmp\",\"skills\":[{{\"name\":\"reviewer\",\"description\":\"Reviews\",\"path\":{encoded_path},\"scope\":\"repo\",\"enabled\":true}}],\"errors\":[]}}]}}}}'\n"
        );
        fs::write(&binary, script).expect("fake codex executable");
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o755))
            .expect("executable permissions");

        let probe = probe_skills(&binary, temp.path())
            .await
            .expect("successful native probe");

        assert_eq!(probe.skills.len(), 1);
        assert_eq!(probe.skills[0].name, "reviewer");
        assert_eq!(probe.skills[0].enabled, Some(true));
    }

    #[test]
    fn parses_skills_list_without_claiming_session_visibility() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "result": {
                "data": [{
                    "cwd": "/tmp/project",
                    "skills": [{
                        "name": "reviewer",
                        "description": "Reviews changes",
                        "path": "/tmp/project/.agents/skills/reviewer/SKILL.md",
                        "scope": "repo",
                        "enabled": false
                    }],
                    "errors": [{"message": "invalid frontmatter", "path": "/tmp/bad/SKILL.md"}]
                }]
            }
        });

        let probe = parse_skills_response(&response).expect("valid response");
        assert_eq!(probe.evidence.source, "codex_app_server");
        assert!(!probe.evidence.proves_session_visibility);
        assert_eq!(probe.skills[0].enabled, Some(false));
        assert_eq!(probe.skills[0].scope, "repo");
        assert_eq!(probe.issues.len(), 1);
    }

    #[test]
    fn rejects_a_response_without_a_result() {
        let error = parse_skills_response(&json!({"id": 2})).expect_err("missing result");
        assert!(error.to_string().contains("has no result"));
    }
}
