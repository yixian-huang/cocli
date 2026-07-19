//! Runtime-agnostic interface implemented by every runtime adapter.

use std::path::{Path, PathBuf};

use async_trait::async_trait;

use crate::error::DriverError;
use crate::event::DriverEvent;
use crate::subtraits::{
    ExitCodeClassifier, ProcessFactory, ProcessInitializer, SessionFileGC, StdinBinder,
    TurnInterruptor,
};
use crate::types::{
    BusyDeliveryMode, DriverAgentConfig, EnvPropagation, MessageMode, NativeSkillProbe,
    PlatformActionTransport, SkillCompatibility, SkillDiscoveryEvidence, SpawnConfig,
};

#[async_trait]
pub trait Driver: Send + Sync {
    /// Runtime name as it appears in agent configuration.
    fn name(&self) -> &str;

    /// MCP tool prefix expected by this runtime.
    fn mcp_tool_prefix(&self) -> &str;

    /// Whether the runtime requires a canonical initial prompt before user
    /// messages flow.
    fn requires_initial_prompt(&self) -> bool {
        false
    }

    /// How the runtime tolerates delivery while a turn is in flight.
    fn busy_delivery_mode(&self) -> BusyDeliveryMode;

    /// How per-agent environment variables reach the runtime.
    fn env_propagation(&self) -> EnvPropagation;

    /// Optional runtime-specific system prompt section.
    fn extra_system_prompt_section(&self) -> &str {
        ""
    }

    /// Surface used by the runtime for outbound platform actions.
    fn platform_action_transport(&self) -> PlatformActionTransport {
        PlatformActionTransport::Mcp
    }

    /// Whether the local runtime should inject a `cocli` CLI wrapper.
    fn platform_action_cli_injected(&self) -> bool {
        matches!(
            self.platform_action_transport(),
            PlatformActionTransport::Cli | PlatformActionTransport::Hybrid
        )
    }

    /// Whether skills are surfaced to this runtime.
    fn skill_compatibility(&self) -> SkillCompatibility;

    /// Advertised model context window, when known.
    fn context_window_tokens(&self) -> Option<u32> {
        None
    }

    /// Materialize runtime-specific workspace state before spawn.
    fn prepare_workspace(
        &self,
        work_dir: &Path,
        config: &DriverAgentConfig,
        agent_id: &str,
        system_prompt: &str,
    ) -> Result<(), DriverError>;

    /// Spawn the runtime subprocess.
    fn spawn(&self, cfg: &SpawnConfig) -> Result<tokio::process::Child, DriverError>;

    /// Parse one stdout line into zero or more runtime-neutral events.
    fn parse_event(&self, line: &str) -> Vec<DriverEvent>;

    /// Encode one stdin message without a trailing newline.
    fn encode_stdin_message(
        &self,
        text: &str,
        session_id: Option<&str>,
        mode: MessageMode,
    ) -> Option<String>;

    /// Whether the runtime supports turn cancellation.
    fn supports_turn_cancel(&self) -> bool;

    /// Whether the runtime supports mid-turn steering.
    fn supports_turn_steer(&self) -> bool {
        false
    }

    /// Whether the runtime can fork the active thread in place.
    fn supports_thread_fork(&self) -> bool {
        false
    }

    /// Whether the runtime exits after each turn and must be respawned.
    fn is_turn_exit(&self) -> bool {
        false
    }

    /// Whether the session ID is only available on the terminal event.
    fn defers_session_id_to_turn_end(&self) -> bool {
        false
    }

    /// Execute a mid-turn steer operation.
    async fn turn_steer(&self, _input: &str) -> Result<(), DriverError> {
        Err(DriverError::TurnSteerUnsupported)
    }

    /// Fork the active runtime thread.
    async fn fork_thread(&self, _thread_id: &str) -> Result<String, DriverError> {
        Err(DriverError::Unsupported)
    }

    /// Directories scanned for installed skills, in priority order.
    fn skill_search_paths(&self, workspace: &Path) -> Vec<PathBuf>;

    /// Evidence used by skill inventory and diagnostics.
    ///
    /// Drivers can override this when a native runtime probe is available.
    fn skill_discovery_evidence(&self) -> SkillDiscoveryEvidence {
        SkillDiscoveryEvidence::FILESYSTEM
    }

    /// Ask the runtime itself which skills it currently discovers for a
    /// workspace. `Ok(None)` means the driver has no reliable native probe and
    /// callers should retain filesystem evidence.
    async fn probe_skills(
        &self,
        _workspace: &Path,
    ) -> Result<Option<NativeSkillProbe>, DriverError> {
        Ok(None)
    }

    /// Convert a stderr line into a semantic event when the runtime reports
    /// structured failures outside stdout.
    fn classify_stderr_line(&self, _line: &str) -> Option<DriverEvent> {
        None
    }

    fn as_process_factory(&self) -> Option<&dyn ProcessFactory> {
        None
    }

    fn as_process_initializer(&self) -> Option<&dyn ProcessInitializer> {
        None
    }

    fn as_stdin_binder(&self) -> Option<&dyn StdinBinder> {
        None
    }

    fn as_turn_interruptor(&self) -> Option<&dyn TurnInterruptor> {
        None
    }

    fn as_exit_code_classifier(&self) -> Option<&dyn ExitCodeClassifier> {
        None
    }

    fn as_session_file_gc(&self) -> Option<&dyn SessionFileGC> {
        None
    }
}
