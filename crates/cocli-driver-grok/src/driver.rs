//! `GrokDriver` — `Driver` impl for the `grok` (xAI Grok Build) CLI runtime.
//!
//! Turn-exit driver using headless `-p` + `--output-format streaming-json`.
//! The registry-level `GrokDriver` is a `ProcessFactory`; each spawned
//! process owns a `GrokProcessDriver` that resolves token usage from Grok's
//! on-disk telemetry at turn end (`signals.json` + `unified.jsonl`).

use std::path::PathBuf;
use std::sync::Mutex;

use async_trait::async_trait;
use cocli_driver_core::subtraits::{ExitCodeClassifier, ProcessFactory};
use cocli_driver_core::types::{DriverAgentConfig, ExitCodeClass, MessageMode, SpawnConfig};
use cocli_driver_core::{Driver, DriverError, DriverEvent};

use crate::caps;
use crate::conv::to_driver_events;
use crate::errors::{
    classify_grok_error_message, classify_grok_exit_code, classify_grok_stderr_line, GrokErrorClass,
};
use crate::events::parse_line;
use crate::spawn::{spawn_grok, SpawnContext};
use crate::usage::{grok_home_dir, GrokUsageContext};

pub struct GrokDriver {
    grok_binary: PathBuf,
}

impl GrokDriver {
    pub fn new(grok_binary: PathBuf) -> Self {
        Self { grok_binary }
    }
}

pub struct GrokProcessDriver {
    grok_binary: PathBuf,
    usage_ctx: Mutex<GrokUsageContext>,
    context_window_tokens: u32,
    last_error_class: Mutex<Option<GrokErrorClass>>,
}

impl GrokProcessDriver {
    fn new(grok_binary: PathBuf, work_dir: PathBuf, model: String) -> Self {
        let usage_ctx = GrokUsageContext::new(grok_home_dir(), work_dir, model);
        let context_window_tokens = usage_ctx.context_window_tokens;
        Self {
            grok_binary,
            usage_ctx: Mutex::new(usage_ctx),
            context_window_tokens,
            last_error_class: Mutex::new(None),
        }
    }

    fn record_error_class(&self, class: GrokErrorClass) {
        if class != GrokErrorClass::Unknown {
            if let Ok(mut slot) = self.last_error_class.lock() {
                *slot = Some(class);
            }
        }
    }

    fn record_spawn(&self, child: &tokio::process::Child) {
        if let Some(pid) = child.id() {
            if let Ok(mut ctx) = self.usage_ctx.lock() {
                ctx.on_spawn(pid);
            }
        }
    }
}

macro_rules! impl_grok_driver_caps {
    () => {
        fn name(&self) -> &str {
            caps::NAME
        }

        fn mcp_tool_prefix(&self) -> &str {
            caps::MCP_TOOL_PREFIX
        }

        fn requires_initial_prompt(&self) -> bool {
            caps::requires_initial_prompt()
        }

        fn is_turn_exit(&self) -> bool {
            caps::is_turn_exit()
        }

        fn defers_session_id_to_turn_end(&self) -> bool {
            caps::defers_session_id_to_turn_end()
        }

        fn busy_delivery_mode(&self) -> cocli_driver_core::types::BusyDeliveryMode {
            caps::busy_delivery_mode()
        }

        fn env_propagation(&self) -> cocli_driver_core::types::EnvPropagation {
            caps::env_propagation()
        }

        fn skill_compatibility(&self) -> cocli_driver_core::types::SkillCompatibility {
            caps::skill_compatibility()
        }

        fn extra_system_prompt_section(&self) -> &str {
            ""
        }

        fn platform_action_transport(&self) -> cocli_driver_core::types::PlatformActionTransport {
            caps::platform_action_transport()
        }

        fn prepare_workspace(
            &self,
            work_dir: &std::path::Path,
            _config: &DriverAgentConfig,
            _agent_id: &str,
            system_prompt: &str,
        ) -> Result<(), DriverError> {
            caps::prepare_workspace(work_dir, system_prompt)
        }

        fn encode_stdin_message(
            &self,
            text: &str,
            session_id: Option<&str>,
            mode: MessageMode,
        ) -> Option<String> {
            caps::encode_stdin(text, session_id, mode)
        }

        fn supports_turn_cancel(&self) -> bool {
            caps::supports_turn_cancel()
        }

        fn supports_turn_steer(&self) -> bool {
            caps::supports_turn_steer()
        }

        fn supports_thread_fork(&self) -> bool {
            caps::supports_thread_fork()
        }

        fn skill_search_paths(&self, workspace: &std::path::Path) -> Vec<PathBuf> {
            caps::skill_search_paths(workspace)
        }

        fn as_exit_code_classifier(&self) -> Option<&dyn ExitCodeClassifier> {
            Some(self)
        }
    };
}

#[async_trait]
impl Driver for GrokDriver {
    impl_grok_driver_caps!();

    fn context_window_tokens(&self) -> Option<u32> {
        caps::registry_context_window_tokens()
    }

    fn spawn(&self, cfg: &SpawnConfig) -> Result<tokio::process::Child, DriverError> {
        self.new_process(cfg).spawn(cfg)
    }

    fn parse_event(&self, line: &str) -> Vec<DriverEvent> {
        to_driver_events(parse_line(line), None)
    }

    fn as_process_factory(&self) -> Option<&dyn ProcessFactory> {
        Some(self)
    }
}

impl ProcessFactory for GrokDriver {
    fn new_process(&self, cfg: &SpawnConfig) -> Box<dyn Driver> {
        Box::new(GrokProcessDriver::new(
            self.grok_binary.clone(),
            cfg.working_dir.to_path_buf(),
            cfg.model.to_string(),
        ))
    }
}

#[async_trait]
impl Driver for GrokProcessDriver {
    impl_grok_driver_caps!();

    fn context_window_tokens(&self) -> Option<u32> {
        Some(self.context_window_tokens)
    }

    fn spawn(&self, cfg: &SpawnConfig) -> Result<tokio::process::Child, DriverError> {
        let child = spawn_grok(
            &SpawnContext {
                grok_binary: &self.grok_binary,
                working_dir: cfg.working_dir,
                model: cfg.model,
                resume_session: cfg.resume_session,
                system_prompt: cfg.system_prompt,
                initial_prompt: cfg.initial_prompt,
            },
            cfg.env_vars,
        )
        .map_err(DriverError::Io)?;
        self.record_spawn(&child);
        Ok(child)
    }

    fn parse_event(&self, line: &str) -> Vec<DriverEvent> {
        let events = parse_line(line);
        for event in &events {
            if let crate::events::GrokEvent::Error { message, .. } = event {
                self.record_error_class(classify_grok_error_message(message));
            }
        }
        let usage = events.iter().find_map(|event| match event {
            crate::events::GrokEvent::TurnEnd { session_id, .. } if !session_id.is_empty() => self
                .usage_ctx
                .lock()
                .ok()
                .map(|ctx| ctx.resolve_turn_usage(session_id)),
            _ => None,
        });
        to_driver_events(events, usage)
    }

    fn classify_stderr_line(&self, line: &str) -> Option<DriverEvent> {
        let (message, class) = classify_grok_stderr_line(line)?;
        self.record_error_class(class);
        Some(DriverEvent::Error {
            message,
            code: crate::errors::grok_error_class_code(class).map(str::to_string),
            severity: None,
            http_status: None,
        })
    }
}

impl ExitCodeClassifier for GrokDriver {
    fn classify_exit_code(&self, code: i32) -> ExitCodeClass {
        classify_grok_exit_code(code, None)
    }
}

impl ExitCodeClassifier for GrokProcessDriver {
    fn classify_exit_code(&self, code: i32) -> ExitCodeClass {
        let last = self.last_error_class.lock().ok().and_then(|g| *g);
        classify_grok_exit_code(code, last)
    }
}
