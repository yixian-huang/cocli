use std::path::PathBuf;

use cocli_driver_core::types::{
    BusyDeliveryMode, EnvPropagation, PlatformActionTransport, SkillCompatibility,
};
use cocli_driver_core::Driver;
use serde::{Deserialize, Serialize};

use crate::{discover_runtime_models, RuntimeProbe, RuntimeRegistry};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeModel {
    pub id: String,
    pub label: String,
}

impl RuntimeModel {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSpec {
    pub name: String,
    pub command: PathBuf,
    pub version_args: Vec<String>,
    pub models: Vec<RuntimeModel>,
}

impl RuntimeSpec {
    pub fn new(name: impl Into<String>, command: impl Into<PathBuf>) -> Self {
        Self {
            name: name.into(),
            command: command.into(),
            version_args: vec!["--version".to_string()],
            models: Vec::new(),
        }
    }

    pub fn with_version_args(mut self, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.version_args = args.into_iter().map(Into::into).collect();
        self
    }

    pub fn with_command(mut self, command: impl Into<PathBuf>) -> Self {
        self.command = command.into();
        self
    }

    pub fn with_models(mut self, models: Vec<RuntimeModel>) -> Self {
        self.models = models;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeCapabilities {
    pub mcp_tool_prefix: String,
    pub requires_initial_prompt: bool,
    pub busy_delivery_mode: String,
    pub env_propagation: String,
    pub platform_action_transport: String,
    pub platform_action_cli_injected: bool,
    pub skill_compatibility: String,
    pub context_window_tokens: Option<u32>,
    pub supports_turn_cancel: bool,
    pub supports_turn_steer: bool,
    pub supports_thread_fork: bool,
    pub is_turn_exit: bool,
    pub defers_session_id_to_turn_end: bool,
    pub process_factory: bool,
    pub process_initializer: bool,
    pub stdin_binder: bool,
    pub turn_interruptor: bool,
    pub exit_code_classifier: bool,
    pub session_file_gc: bool,
}

impl RuntimeCapabilities {
    pub fn from_driver(driver: &dyn Driver) -> Self {
        Self {
            mcp_tool_prefix: driver.mcp_tool_prefix().to_string(),
            requires_initial_prompt: driver.requires_initial_prompt(),
            busy_delivery_mode: busy_delivery_mode_name(driver.busy_delivery_mode()).to_string(),
            env_propagation: env_propagation_name(driver.env_propagation()).to_string(),
            platform_action_transport: platform_action_transport_name(
                driver.platform_action_transport(),
            )
            .to_string(),
            platform_action_cli_injected: driver.platform_action_cli_injected(),
            skill_compatibility: skill_compatibility_name(driver.skill_compatibility()).to_string(),
            context_window_tokens: driver.context_window_tokens(),
            supports_turn_cancel: driver.supports_turn_cancel(),
            supports_turn_steer: driver.supports_turn_steer(),
            supports_thread_fork: driver.supports_thread_fork(),
            is_turn_exit: driver.is_turn_exit(),
            defers_session_id_to_turn_end: driver.defers_session_id_to_turn_end(),
            process_factory: driver.as_process_factory().is_some(),
            process_initializer: driver.as_process_initializer().is_some(),
            stdin_binder: driver.as_stdin_binder().is_some(),
            turn_interruptor: driver.as_turn_interruptor().is_some(),
            exit_code_classifier: driver.as_exit_code_classifier().is_some(),
            session_file_gc: driver.as_session_file_gc().is_some(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeCatalogEntry {
    pub name: String,
    pub installed: bool,
    pub binary: Option<PathBuf>,
    pub version: Option<String>,
    pub models: Vec<RuntimeModel>,
    pub capabilities: Option<RuntimeCapabilities>,
    pub unavailable_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RuntimeCatalog {
    pub runtimes: Vec<RuntimeCatalogEntry>,
}

impl RuntimeCatalog {
    /// Probe runtime binaries and versions without requiring a driver registry.
    ///
    /// This is intended for consumer startup/ready handshakes that need to
    /// report what is installed before or independently of driver activation.
    pub fn probe(specs: &[RuntimeSpec], probe: &dyn RuntimeProbe) -> Self {
        let mut runtimes = specs
            .iter()
            .map(|spec| {
                let binary = probe.resolve_binary(&spec.command);
                let version = binary
                    .as_deref()
                    .and_then(|path| probe.detect_version(path, &spec.version_args));
                RuntimeCatalogEntry {
                    name: spec.name.clone(),
                    installed: binary.is_some(),
                    binary,
                    version,
                    models: spec.models.clone(),
                    capabilities: None,
                    unavailable_reason: None,
                }
            })
            .collect::<Vec<_>>();
        runtimes.sort_by(|a, b| a.name.cmp(&b.name));
        Self { runtimes }
    }

    pub fn discover(
        registry: &RuntimeRegistry,
        specs: &[RuntimeSpec],
        probe: &dyn RuntimeProbe,
    ) -> Self {
        let mut runtimes = specs
            .iter()
            .map(|spec| discover_entry(registry, spec, probe))
            .collect::<Vec<_>>();
        runtimes.sort_by(|a, b| a.name.cmp(&b.name));
        let mut catalog = Self { runtimes };
        catalog.apply_discovered_models();
        catalog
    }

    /// Overlay live model discovery (CLI/cache/API) onto static spec fallbacks.
    ///
    /// Specs keep offline defaults; discovery replaces them when the runtime
    /// reports a non-empty launchable model set (for example `grok models`).
    pub fn apply_discovered_models(&mut self) {
        let names: Vec<String> = self
            .runtimes
            .iter()
            .filter(|entry| entry.installed && entry.unavailable_reason.is_none())
            .map(|entry| entry.name.clone())
            .collect();
        if names.is_empty() {
            return;
        }
        let live = discover_runtime_models(&names);
        for entry in &mut self.runtimes {
            if let Some(models) = live.get(&entry.name) {
                if !models.is_empty() {
                    entry.models = models.clone();
                }
            }
        }
    }

    pub fn get(&self, name: &str) -> Option<&RuntimeCatalogEntry> {
        self.runtimes.iter().find(|entry| entry.name == name)
    }

    pub fn installed_names(&self) -> Vec<String> {
        self.runtimes
            .iter()
            .filter(|entry| entry.installed && entry.unavailable_reason.is_none())
            .map(|entry| entry.name.clone())
            .collect()
    }
}

fn discover_entry(
    registry: &RuntimeRegistry,
    spec: &RuntimeSpec,
    probe: &dyn RuntimeProbe,
) -> RuntimeCatalogEntry {
    let binary = probe.resolve_binary(&spec.command);
    let driver = registry.get(&spec.name);
    let unavailable_reason = if !registry.is_registered(&spec.name) {
        Some("driver not registered".to_string())
    } else if !registry.is_allowed(&spec.name) {
        Some("runtime disabled by allowlist".to_string())
    } else if binary.is_none() {
        Some(format!("binary not found: {}", spec.command.display()))
    } else {
        None
    };
    let version = binary
        .as_deref()
        .and_then(|path| probe.detect_version(path, &spec.version_args));

    RuntimeCatalogEntry {
        name: spec.name.clone(),
        installed: binary.is_some(),
        binary,
        version,
        models: spec.models.clone(),
        capabilities: driver.as_deref().map(RuntimeCapabilities::from_driver),
        unavailable_reason,
    }
}

pub fn initial_oss_runtime_specs() -> Vec<RuntimeSpec> {
    vec![
        RuntimeSpec::new("claude", "claude").with_models(vec![
            RuntimeModel::new("sonnet", "Sonnet (200k)"),
            RuntimeModel::new("opus", "Opus (200k)"),
            RuntimeModel::new("claude-opus-4-7[1m]", "Opus 4.7 (1M)"),
            RuntimeModel::new("haiku", "Haiku (200k)"),
        ]),
        RuntimeSpec::new("cursor", "cursor-agent").with_models(vec![
            RuntimeModel::new("composer-2-fast", "Composer 2 Fast"),
            RuntimeModel::new("composer-2", "Composer 2"),
            RuntimeModel::new("auto", "Auto"),
        ]),
        RuntimeSpec::new("codex", "codex").with_models(vec![
            RuntimeModel::new("gpt-5.4", "gpt-5.4"),
            RuntimeModel::new("gpt-5.4-mini", "GPT-5.4-Mini"),
            RuntimeModel::new("gpt-5.3-codex", "gpt-5.3-codex"),
            RuntimeModel::new("gpt-5.2", "gpt-5.2"),
        ]),
        RuntimeSpec::new("gemini", "gemini").with_models(vec![
            RuntimeModel::new("gemini-2.5-pro", "Gemini 2.5 Pro"),
            RuntimeModel::new("gemini-2.5-flash", "Gemini 2.5 Flash"),
        ]),
        RuntimeSpec::new("kimi", "kimi").with_models(vec![
            RuntimeModel::new("kimi-k2", "Kimi K2 (128K)"),
            RuntimeModel::new("kimi-k1.5", "Kimi K1.5 (200K)"),
        ]),
        RuntimeSpec::new("grok", "grok").with_models(vec![
            RuntimeModel::new("grok-4.5", "Grok 4.5"),
            RuntimeModel::new("grok-build", "Grok Build"),
        ]),
        RuntimeSpec::new("chatrs", "chatrs"),
        RuntimeSpec::new("opencode", "opencode").with_models(vec![
            RuntimeModel::new("default", "Configured Default / Auto"),
            RuntimeModel::new("deepseek/deepseek-v4-pro", "DeepSeek V4 Pro (OpenCode)"),
            RuntimeModel::new(
                "openrouter/anthropic/claude-opus-4.5",
                "Claude Opus 4.5 via OpenRouter",
            ),
            RuntimeModel::new("fusecode/opus[1m]", "Opus 1M via FuseCode"),
        ]),
    ]
}

fn busy_delivery_mode_name(mode: BusyDeliveryMode) -> &'static str {
    match mode {
        BusyDeliveryMode::Gated => "gated",
        BusyDeliveryMode::Direct => "direct",
        BusyDeliveryMode::Notify => "notify",
        BusyDeliveryMode::None => "none",
    }
}

fn env_propagation_name(mode: EnvPropagation) -> &'static str {
    match mode {
        EnvPropagation::Inherit => "inherit",
        EnvPropagation::SettingsCopy => "settings_copy",
    }
}

fn skill_compatibility_name(mode: SkillCompatibility) -> &'static str {
    match mode {
        SkillCompatibility::Unsupported => "unsupported",
        SkillCompatibility::Uncertain => "uncertain",
        SkillCompatibility::Supported => "supported",
    }
}

fn platform_action_transport_name(mode: PlatformActionTransport) -> &'static str {
    match mode {
        PlatformActionTransport::Mcp => "mcp",
        PlatformActionTransport::Cli => "cli",
        PlatformActionTransport::Hybrid => "hybrid",
    }
}
