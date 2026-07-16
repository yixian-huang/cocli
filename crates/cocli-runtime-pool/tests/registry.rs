use std::path::{Path, PathBuf};
use std::sync::Arc;

use cocli_driver_claude::ClaudeDriver;
use cocli_driver_codex::CodexDriver;
use cocli_driver_core::types::{
    BusyDeliveryMode, DriverAgentConfig, EnvPropagation, MessageMode, SkillCompatibility,
    SpawnConfig,
};
use cocli_driver_core::{Driver, DriverError, DriverEvent};
use cocli_driver_cursor::CursorDriver;
use cocli_driver_gemini::GeminiDriver;
use cocli_runtime_pool::RuntimeRegistry;

struct FakeDriver(&'static str);

#[async_trait::async_trait]
impl Driver for FakeDriver {
    fn name(&self) -> &str {
        self.0
    }

    fn mcp_tool_prefix(&self) -> &str {
        ""
    }

    fn busy_delivery_mode(&self) -> BusyDeliveryMode {
        BusyDeliveryMode::Gated
    }

    fn env_propagation(&self) -> EnvPropagation {
        EnvPropagation::Inherit
    }

    fn skill_compatibility(&self) -> SkillCompatibility {
        SkillCompatibility::Supported
    }

    fn prepare_workspace(
        &self,
        _work_dir: &Path,
        _config: &DriverAgentConfig,
        _agent_id: &str,
        _system_prompt: &str,
    ) -> Result<(), DriverError> {
        Ok(())
    }

    fn spawn(&self, _cfg: &SpawnConfig) -> Result<tokio::process::Child, DriverError> {
        Err(DriverError::Other("test fake does not spawn".to_string()))
    }

    fn parse_event(&self, _line: &str) -> Vec<DriverEvent> {
        vec![DriverEvent::Unknown]
    }

    fn encode_stdin_message(
        &self,
        text: &str,
        _session_id: Option<&str>,
        _mode: MessageMode,
    ) -> Option<String> {
        Some(text.to_string())
    }

    fn supports_turn_cancel(&self) -> bool {
        false
    }

    fn skill_search_paths(&self, _workspace: &Path) -> Vec<PathBuf> {
        Vec::new()
    }
}

#[test]
fn registry_register_get_and_overwrite() {
    let mut registry = RuntimeRegistry::new();
    registry.register(Arc::new(FakeDriver("claude")));
    registry.register(Arc::new(FakeDriver("codex")));
    registry.register(Arc::new(FakeDriver("claude")));

    assert_eq!(registry.get("claude").unwrap().name(), "claude");
    assert_eq!(registry.get("codex").unwrap().name(), "codex");
    assert!(registry.get("kimi").is_none());
    assert_eq!(
        registry.names(),
        vec!["claude".to_string(), "codex".to_string()]
    );
}

#[test]
fn allowlist_filters_without_changing_registry_snapshot() {
    let mut registry = RuntimeRegistry::new().with_allowlist(vec!["claude".into()]);
    registry.register(Arc::new(FakeDriver("claude")));
    registry.register(Arc::new(FakeDriver("codex")));

    assert!(registry.get("claude").is_some());
    assert!(registry.get("codex").is_none());
    assert_eq!(
        registry.names(),
        vec!["claude".to_string(), "codex".to_string()]
    );
}

#[test]
fn initial_oss_adapter_matrix_registers_through_core_trait_objects() {
    let binary = PathBuf::from("/bin/false");
    let bridge = PathBuf::from("/bin/false");
    let drivers: Vec<Arc<dyn Driver>> = vec![
        Arc::new(ClaudeDriver::new(binary.clone(), bridge.clone())),
        Arc::new(CursorDriver::new(binary.clone(), bridge.clone())),
        Arc::new(CodexDriver::new(binary.clone(), bridge.clone())),
        Arc::new(GeminiDriver::new(binary, bridge)),
    ];

    let mut registry = RuntimeRegistry::new();
    for driver in drivers {
        registry.register(driver);
    }

    assert_eq!(
        registry.names(),
        vec![
            "claude".to_string(),
            "codex".to_string(),
            "cursor".to_string(),
            "gemini".to_string(),
        ]
    );
    for runtime in ["claude", "cursor", "codex", "gemini"] {
        assert_eq!(registry.get(runtime).unwrap().name(), runtime);
    }
}
