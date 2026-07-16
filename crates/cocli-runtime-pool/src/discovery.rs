use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;

pub trait RuntimeProbe: Send + Sync {
    fn resolve_binary(&self, command: &Path) -> Option<PathBuf>;
    fn detect_version(&self, binary: &Path, args: &[String]) -> Option<String>;
}

#[derive(Debug, Clone)]
pub struct SystemRuntimeProbe {
    path: Option<OsString>,
}

impl SystemRuntimeProbe {
    pub fn from_environment() -> Self {
        Self {
            path: std::env::var_os("PATH"),
        }
    }

    pub fn with_path(path: Option<OsString>) -> Self {
        Self { path }
    }
}

impl Default for SystemRuntimeProbe {
    fn default() -> Self {
        Self::from_environment()
    }
}

impl RuntimeProbe for SystemRuntimeProbe {
    fn resolve_binary(&self, command: &Path) -> Option<PathBuf> {
        resolve_binary(command, self.path.as_deref())
    }

    fn detect_version(&self, binary: &Path, args: &[String]) -> Option<String> {
        let output = Command::new(binary).args(args).output().ok()?;
        if !output.status.success() {
            return None;
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        first_version_line(&stdout)
            .or_else(|| first_version_line(&stderr))
            .map(truncate_version)
    }
}

fn resolve_binary(command: &Path, path_env: Option<&OsStr>) -> Option<PathBuf> {
    if command.components().count() > 1 || command.is_absolute() {
        return is_executable_file(command).then(|| command.to_path_buf());
    }
    let path_env = path_env?;
    std::env::split_paths(path_env)
        .map(|directory| directory.join(command))
        .find(|candidate| is_executable_file(candidate))
}

fn first_version_line(output: &str) -> Option<String> {
    output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(ToOwned::to_owned)
}

fn truncate_version(mut version: String) -> String {
    const MAX_VERSION_CHARS: usize = 256;
    if version.chars().count() > MAX_VERSION_CHARS {
        version = version.chars().take(MAX_VERSION_CHARS).collect();
    }
    version
}

fn is_executable_file(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        path.metadata()
            .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        true
    }
}
