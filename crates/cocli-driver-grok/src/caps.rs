//! Shared static `Driver` capability answers for grok (registry + per-process).

use std::path::{Path, PathBuf};

use cocli_driver_core::types::{
    BusyDeliveryMode, EnvPropagation, PlatformActionTransport, SkillCompatibility,
};
use cocli_driver_core::DriverError;

use crate::usage::{context_window_for_model, grok_home_dir};
use crate::{encode_stdin_message, write_grok_agents_md};

pub const NAME: &str = "grok";
pub const MCP_TOOL_PREFIX: &str = "chat__";
pub const DEFAULT_MODEL_FOR_REGISTRY: &str = "grok-composer-2.5-fast";

pub fn requires_initial_prompt() -> bool {
    true
}

pub fn is_turn_exit() -> bool {
    true
}

pub fn defers_session_id_to_turn_end() -> bool {
    true
}

pub fn busy_delivery_mode() -> BusyDeliveryMode {
    BusyDeliveryMode::None
}

pub fn env_propagation() -> EnvPropagation {
    EnvPropagation::Inherit
}

pub fn skill_compatibility() -> SkillCompatibility {
    SkillCompatibility::Supported
}

pub fn platform_action_transport() -> PlatformActionTransport {
    PlatformActionTransport::Cli
}

pub fn registry_context_window_tokens() -> Option<u32> {
    Some(context_window_for_model(
        &grok_home_dir(),
        DEFAULT_MODEL_FOR_REGISTRY,
    ))
}

pub fn prepare_workspace(work_dir: &Path, system_prompt: &str) -> Result<(), DriverError> {
    let grok_dir = work_dir.join(".grok");
    std::fs::create_dir_all(grok_dir).map_err(DriverError::Io)?;
    write_grok_agents_md(work_dir, system_prompt).map_err(DriverError::Io)?;
    Ok(())
}

pub fn skill_search_paths(workspace: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = vec![workspace.join(".grok").join("skills")];
    if let Some(home) = dirs::home_dir() {
        paths.push(home.join(".grok").join("skills"));
        paths.push(home.join(".agents").join("skills"));
    }
    paths
}

pub fn encode_stdin(
    text: &str,
    session_id: Option<&str>,
    mode: cocli_driver_core::types::MessageMode,
) -> Option<String> {
    encode_stdin_message(text, session_id, mode)
}

pub fn supports_turn_cancel() -> bool {
    true
}

pub fn supports_turn_steer() -> bool {
    false
}

pub fn supports_thread_fork() -> bool {
    false
}
