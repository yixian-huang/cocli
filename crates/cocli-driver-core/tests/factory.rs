use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU32;
use std::sync::Arc;

use async_trait::async_trait;
use cocli_driver_core::types::{
    BusyDeliveryMode, DriverAgentConfig, EnvPropagation, MessageMode, SkillCompatibility,
    SpawnConfig,
};
use cocli_driver_core::{Driver, DriverError, DriverEvent, ProcessFactory};

struct StatelessFake;

#[async_trait]
impl Driver for StatelessFake {
    fn name(&self) -> &str {
        "stateless"
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
        SkillCompatibility::Unsupported
    }

    fn prepare_workspace(
        &self,
        _: &Path,
        _: &DriverAgentConfig,
        _: &str,
        _: &str,
    ) -> Result<(), DriverError> {
        Ok(())
    }

    fn spawn(&self, _: &SpawnConfig) -> Result<tokio::process::Child, DriverError> {
        Err(DriverError::Other("test fake".to_string()))
    }

    fn parse_event(&self, _: &str) -> Vec<DriverEvent> {
        vec![DriverEvent::Unknown]
    }

    fn encode_stdin_message(&self, _: &str, _: Option<&str>, _: MessageMode) -> Option<String> {
        Some(String::new())
    }

    fn supports_turn_cancel(&self) -> bool {
        false
    }

    fn skill_search_paths(&self, _: &Path) -> Vec<PathBuf> {
        Vec::new()
    }
}

struct StatefulFactory;
struct StatefulProcess {
    _counter: AtomicU32,
}

#[async_trait]
impl Driver for StatefulFactory {
    fn name(&self) -> &str {
        "stateful-factory"
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
        SkillCompatibility::Unsupported
    }

    fn prepare_workspace(
        &self,
        _: &Path,
        _: &DriverAgentConfig,
        _: &str,
        _: &str,
    ) -> Result<(), DriverError> {
        Ok(())
    }

    fn spawn(&self, _: &SpawnConfig) -> Result<tokio::process::Child, DriverError> {
        Err(DriverError::Other(
            "factory cannot spawn directly".to_string(),
        ))
    }

    fn parse_event(&self, _: &str) -> Vec<DriverEvent> {
        vec![DriverEvent::Unknown]
    }

    fn encode_stdin_message(&self, _: &str, _: Option<&str>, _: MessageMode) -> Option<String> {
        None
    }

    fn supports_turn_cancel(&self) -> bool {
        false
    }

    fn skill_search_paths(&self, _: &Path) -> Vec<PathBuf> {
        Vec::new()
    }

    fn as_process_factory(&self) -> Option<&dyn ProcessFactory> {
        Some(self)
    }
}

impl ProcessFactory for StatefulFactory {
    fn new_process(&self, _: &SpawnConfig) -> Box<dyn Driver> {
        Box::new(StatefulProcess {
            _counter: AtomicU32::new(0),
        })
    }
}

#[async_trait]
impl Driver for StatefulProcess {
    fn name(&self) -> &str {
        "stateful-process"
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
        SkillCompatibility::Unsupported
    }

    fn prepare_workspace(
        &self,
        _: &Path,
        _: &DriverAgentConfig,
        _: &str,
        _: &str,
    ) -> Result<(), DriverError> {
        Ok(())
    }

    fn spawn(&self, _: &SpawnConfig) -> Result<tokio::process::Child, DriverError> {
        Err(DriverError::Other("test fake spawn".to_string()))
    }

    fn parse_event(&self, _: &str) -> Vec<DriverEvent> {
        vec![DriverEvent::Unknown]
    }

    fn encode_stdin_message(&self, _: &str, _: Option<&str>, _: MessageMode) -> Option<String> {
        None
    }

    fn supports_turn_cancel(&self) -> bool {
        false
    }

    fn skill_search_paths(&self, _: &Path) -> Vec<PathBuf> {
        Vec::new()
    }
}

#[test]
fn stateless_driver_does_not_expose_process_factory() {
    let driver: Arc<dyn Driver> = Arc::new(StatelessFake);

    assert!(driver.as_process_factory().is_none());
}

#[test]
fn stateful_driver_exposes_process_factory() {
    let driver: Arc<dyn Driver> = Arc::new(StatefulFactory);

    assert!(driver.as_process_factory().is_some());
}

#[test]
fn process_factory_returns_fresh_driver_instances() {
    let driver: Arc<dyn Driver> = Arc::new(StatefulFactory);
    let factory = driver
        .as_process_factory()
        .expect("stateful driver exposes a process factory");
    let config = SpawnConfig {
        working_dir: Path::new("/tmp"),
        model: "model",
        mcp_config: None,
        resume_session: None,
        agent_id: "agent-1",
        server_url: "http://127.0.0.1",
        auth_token: "token",
        system_prompt: "",
        initial_prompt: "",
        env_vars: &[],
    };

    let first = factory.new_process(&config);
    let second = factory.new_process(&config);

    assert_ne!(
        first.as_ref() as *const _ as *const (),
        second.as_ref() as *const _ as *const ()
    );
}
