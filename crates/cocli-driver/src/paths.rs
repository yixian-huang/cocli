//! Skill search paths + process exit classification.

use std::path::PathBuf;

#[derive(Debug, Clone, Default)]
pub struct SkillPaths {
    /// Absolute global paths (e.g. `~/.claude/skills`).
    pub global: Vec<PathBuf>,
    /// Workspace-relative paths (e.g. `.claude/skills`).
    pub workspace: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitClassification {
    Normal,
    AuthFailed,
    ConfigError,
    Cancelled,
    Crashed(i32),
}
