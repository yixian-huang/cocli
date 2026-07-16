#![cfg(unix)]

use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use cocli_driver_core::types::{
    BusyDeliveryMode, DriverAgentConfig, EnvPropagation, MessageMode, SkillCompatibility,
    SpawnConfig,
};
use cocli_driver_core::{Driver, DriverError, DriverEvent};
use cocli_runtime_pool::{RuntimeModel, RuntimeRegistry, RuntimeSpec, SystemRuntimeProbe};

struct FakeDriver(&'static str);

#[async_trait::async_trait]
impl Driver for FakeDriver {
    fn name(&self) -> &str {
        self.0
    }

    fn mcp_tool_prefix(&self) -> &str {
        "mcp__fake__"
    }

    fn busy_delivery_mode(&self) -> BusyDeliveryMode {
        BusyDeliveryMode::Direct
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
        true
    }

    fn skill_search_paths(&self, _workspace: &Path) -> Vec<PathBuf> {
        Vec::new()
    }
}

fn write_executable(path: &Path, body: &str) {
    fs::write(path, body).unwrap();
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

#[test]
fn catalog_reports_binary_version_models_and_capabilities() {
    let temp = tempfile::tempdir().unwrap();
    let binary = temp.path().join("fake-runtime");
    write_executable(&binary, "#!/bin/sh\necho 'fake-runtime 1.2.3'\n");

    let mut registry = RuntimeRegistry::new();
    registry.register(Arc::new(FakeDriver("fake")));
    let spec = RuntimeSpec::new("fake", "fake-runtime")
        .with_models(vec![RuntimeModel::new("fake-model", "Fake Model")]);
    let probe = SystemRuntimeProbe::with_path(Some(OsString::from(temp.path())));
    let catalog = registry.discover(&[spec], &probe);
    let entry = catalog.get("fake").unwrap();

    assert!(entry.installed);
    assert_eq!(entry.binary.as_deref(), Some(binary.as_path()));
    assert_eq!(entry.version.as_deref(), Some("fake-runtime 1.2.3"));
    assert_eq!(entry.models[0].id, "fake-model");
    assert_eq!(
        entry.capabilities.as_ref().unwrap().busy_delivery_mode,
        "direct"
    );
    assert_eq!(entry.unavailable_reason, None);
    assert_eq!(catalog.installed_names(), vec!["fake".to_string()]);

    let json = serde_json::to_value(entry).unwrap();
    assert_eq!(json["name"], "fake");
    assert_eq!(json["installed"], true);
    assert_eq!(json["capabilities"]["mcp_tool_prefix"], "mcp__fake__");
}

#[test]
fn catalog_explains_missing_binary_unregistered_driver_and_allowlist() {
    let temp = tempfile::tempdir().unwrap();
    let installed = temp.path().join("installed-runtime");
    write_executable(&installed, "#!/bin/sh\necho installed\n");
    let probe = SystemRuntimeProbe::with_path(Some(OsString::from(temp.path())));

    let mut registry = RuntimeRegistry::new().with_allowlist(vec!["allowed".to_string()]);
    registry.register(Arc::new(FakeDriver("allowed")));
    registry.register(Arc::new(FakeDriver("disabled")));
    let catalog = registry.discover(
        &[
            RuntimeSpec::new("allowed", "missing-runtime"),
            RuntimeSpec::new("disabled", "installed-runtime"),
            RuntimeSpec::new("unregistered", "installed-runtime"),
        ],
        &probe,
    );

    assert_eq!(
        catalog
            .get("allowed")
            .unwrap()
            .unavailable_reason
            .as_deref(),
        Some("binary not found: missing-runtime")
    );
    assert_eq!(
        catalog
            .get("disabled")
            .unwrap()
            .unavailable_reason
            .as_deref(),
        Some("runtime disabled by allowlist")
    );
    assert_eq!(
        catalog
            .get("unregistered")
            .unwrap()
            .unavailable_reason
            .as_deref(),
        Some("driver not registered")
    );
}

#[test]
fn absolute_non_executable_file_is_not_installed() {
    let temp = tempfile::tempdir().unwrap();
    let binary = temp.path().join("not-executable");
    fs::write(&binary, "not executable").unwrap();

    let mut registry = RuntimeRegistry::new();
    registry.register(Arc::new(FakeDriver("fake")));
    let catalog = registry.discover(
        &[RuntimeSpec::new("fake", binary.clone())],
        &SystemRuntimeProbe::with_path(None),
    );

    assert!(!catalog.get("fake").unwrap().installed);
    assert_eq!(
        catalog.get("fake").unwrap().unavailable_reason,
        Some(format!("binary not found: {}", binary.display()))
    );
}
