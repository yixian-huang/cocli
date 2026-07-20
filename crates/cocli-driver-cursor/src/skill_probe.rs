use std::path::Path;
use std::time::Duration;

use cocli_driver_core::{DriverError, NativeSkillProbe};
use tokio::process::Command;

const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) async fn probe_skills(
    cursor_binary: &Path,
    workspace: &Path,
) -> Result<Option<NativeSkillProbe>, DriverError> {
    probe_skills_with_timeout(cursor_binary, workspace, PROBE_TIMEOUT).await
}

async fn probe_skills_with_timeout(
    cursor_binary: &Path,
    workspace: &Path,
    timeout: Duration,
) -> Result<Option<NativeSkillProbe>, DriverError> {
    let output = tokio::time::timeout(
        timeout,
        Command::new(cursor_binary)
            .arg("--help")
            .current_dir(workspace)
            .kill_on_drop(true)
            .output(),
    )
    .await
    .map_err(|_| DriverError::Other("cursor CLI capability probe timed out".to_owned()))?
    .map_err(DriverError::Io)?;
    if !output.status.success() {
        return Err(DriverError::Other(format!(
            "cursor CLI capability probe failed with {}: {}",
            output.status,
            concise_stderr(&output.stderr)
        )));
    }
    let help = std::str::from_utf8(&output.stdout)
        .map_err(|_| DriverError::Other("cursor CLI help output is not valid UTF-8".to_owned()))?;
    if !help.contains("Usage:") || !help.contains("Cursor Agent") {
        return Err(DriverError::Other(
            "cursor CLI help output is incomplete or unrecognized".to_owned(),
        ));
    }

    // Current official Cursor CLIs expose filesystem-discovered Agent Skills but no
    // documented read-only command or protocol for listing them, and no contract that
    // binds a loaded Skill to a concrete session. A capability probe therefore returns
    // unsupported instead of starting an Agent session or fabricating native evidence.
    Ok(None)
}

fn concise_stderr(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .chars()
        .take(500)
        .collect::<String>()
        .trim()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn missing_cursor_cli_is_reported() {
        let temp = tempfile::tempdir().expect("temp");
        let error = probe_skills(&temp.path().join("missing-cursor"), temp.path())
            .await
            .expect_err("missing binary");
        assert!(matches!(error, DriverError::Io(_)));
    }

    #[cfg(unix)]
    async fn fake_probe(
        script: &str,
        timeout: Duration,
    ) -> Result<Option<NativeSkillProbe>, DriverError> {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().expect("temp");
        let binary = temp.path().join("cursor-agent");
        std::fs::write(&binary, script).expect("script");
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755))
            .expect("permissions");
        probe_skills_with_timeout(&binary, temp.path(), timeout).await
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn timeout_is_bounded() {
        // Cursor probe uses Command::output() on `--help` (no stdin). Hang without
        // relying on a short `sleep` race under CI load.
        let error = fake_probe("#!/bin/sh\nexec sleep 1000\n", Duration::from_millis(80))
            .await
            .expect_err("timeout");
        let message = error.to_string().to_ascii_lowercase();
        assert!(
            message.contains("timed out"),
            "expected timeout failure, got: {message}"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn nonzero_exit_is_reported_without_unbounded_stderr() {
        let error = fake_probe(
            "#!/bin/sh\nprintf '%s' 'unsupported help' >&2\nexit 17\n",
            Duration::from_secs(5),
        )
        .await
        .expect_err("nonzero");
        let message = error.to_string().to_ascii_lowercase();
        assert!(
            message.contains("17")
                || message.contains("exit")
                || message.contains("failed"),
            "expected process-failure detail, got: {message}"
        );
        assert!(
            message.contains("unsupported help"),
            "expected stderr fragment, got: {message}"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn malformed_utf8_is_rejected() {
        let error = fake_probe(
            r"#!/bin/sh
printf '\377'
",
            Duration::from_secs(5),
        )
        .await
        .expect_err("malformed");
        assert!(error.to_string().contains("UTF-8"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn partial_help_is_rejected() {
        let error = fake_probe(
            "#!/bin/sh\nprintf '%s\n' 'Usage: cursor-agent [options]'\n",
            Duration::from_secs(5),
        )
        .await
        .expect_err("partial");
        assert!(error.to_string().contains("incomplete"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn recognized_cli_reports_unsupported_native_contract() {
        let result = fake_probe(
            "#!/bin/sh\nprintf '%s\n' 'Usage: cursor-agent [options]' 'Start the Cursor Agent' 'Commands: mcp status create-chat'\n",
            Duration::from_secs(5),
        )
        .await
        .expect("capability probe");
        assert!(result.is_none());
    }
}
