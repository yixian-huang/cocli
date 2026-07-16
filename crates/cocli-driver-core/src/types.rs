//! Runtime-neutral supporting types used by the driver contract.

use std::path::Path;

/// Per-spawn configuration passed to [`crate::Driver::spawn`] and
/// [`crate::ProcessFactory::new_process`].
pub struct SpawnConfig<'a> {
    pub working_dir: &'a Path,
    pub model: &'a str,
    pub mcp_config: Option<&'a Path>,
    pub resume_session: Option<&'a str>,
    pub agent_id: &'a str,
    pub server_url: &'a str,
    pub auth_token: &'a str,
    pub system_prompt: &'a str,
    pub initial_prompt: &'a str,
    pub env_vars: &'a [(String, String)],
}

/// Narrow per-driver view of agent configuration. Keeping this type in core
/// avoids coupling drivers to protocol or persistence crates.
pub struct DriverAgentConfig<'a> {
    pub runtime: &'a str,
    pub model: &'a str,
    pub working_runtime: &'a str,
    pub working_model: &'a str,
    pub env_vars: &'a [(String, String)],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusyDeliveryMode {
    /// Buffer until a safe turn boundary.
    Gated,
    /// Inject directly through stdin or runtime RPC.
    Direct,
    /// Send only a count or wake notification while busy.
    Notify,
    /// Queue until the turn-exit runtime can be respawned.
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvPropagation {
    Inherit,
    SettingsCopy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageMode {
    User,
    Notification,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillCompatibility {
    Unsupported,
    Uncertain,
    Supported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlatformActionTransport {
    #[default]
    Mcp,
    Cli,
    Hybrid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitCodeClass {
    Normal,
    AuthFailed,
    ConfigError,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnStatus {
    Completed,
    Cancelled,
    Failed,
    MaxSteps,
    Unknown(String),
}

pub fn normalize_turn_status(raw: &str) -> TurnStatus {
    match raw.trim().to_lowercase().as_str() {
        "completed" | "succeeded" | "success" => TurnStatus::Completed,
        "cancelled" | "canceled" | "interrupted" => TurnStatus::Cancelled,
        "failed" | "error" | "errored" => TurnStatus::Failed,
        "max_steps_reached" | "max_steps" | "step_limit" | "step_limit_reached" => {
            TurnStatus::MaxSteps
        }
        "" => TurnStatus::Unknown(String::new()),
        _ => TurnStatus::Unknown(raw.to_string()),
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct GcStats {
    pub removed: usize,
    pub freed_bytes: u64,
}
