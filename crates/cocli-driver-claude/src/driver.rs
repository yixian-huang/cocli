//! ClaudeDriver: implements cocli_driver::Driver for the claude CLI.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use cocli_driver::{
    BusyDeliveryMode, DispatchMode, Driver, DriverError, DriverSpawnResult, EnvPropagation,
    ExitClassification, Result, RuntimeCapabilities, SkillCompat, SkillPaths, SpawnContext,
};

pub struct ClaudeDriver {
    pub claude_binary: PathBuf,
    pub default_model: String,
}

impl ClaudeDriver {
    pub fn new(claude_binary: PathBuf, default_model: String) -> Self {
        Self {
            claude_binary,
            default_model,
        }
    }
}

#[async_trait]
impl Driver for ClaudeDriver {
    fn name(&self) -> &'static str {
        "claude"
    }

    fn capabilities(&self) -> RuntimeCapabilities {
        RuntimeCapabilities {
            dispatch_mode: DispatchMode::Persistent,
            busy_delivery_mode: BusyDeliveryMode::GatedAfterTurn,
            env_propagation: EnvPropagation::Inherit,
            mcp_tool_prefix: "",
            requires_initial_prompt: false,
            context_window_tokens: 200_000,
            skill_compatibility: SkillCompat::Supported,
            supports_native_interrupt: true,
            supports_active_turn_steer: false,
            supports_rejected_steer_replay: false,
        }
    }

    async fn prepare_workspace(&self, _ctx: &SpawnContext) -> Result<()> {
        // Claude does not need pre-spawn workspace setup.
        Ok(())
    }

    fn skill_search_paths(&self, home: &Path) -> SkillPaths {
        SkillPaths {
            global: vec![home.join(".claude").join("skills")],
            workspace: vec![".claude/skills".into()],
        }
    }

    async fn spawn(&self, ctx: SpawnContext) -> Result<DriverSpawnResult> {
        use crate::process::ClaudeProcess;
        use crate::spawn::{spawn_claude, SpawnContext as LegacyCtx};

        let mcp_config_path: Option<PathBuf> = if ctx.no_bridge {
            None
        } else {
            // The caller is expected to have written `.mcp-config.json` into the
            // workspace via cocli-bridge-config before calling spawn. We point at it.
            Some(ctx.workdir.join(".mcp-config.json"))
        };

        let legacy = LegacyCtx {
            claude_binary: &self.claude_binary,
            working_dir: &ctx.workdir,
            model: &self.default_model,
            mcp_config: mcp_config_path.as_deref(),
            resume_session: ctx.resume_session.as_deref(),
        };

        let child = spawn_claude(&legacy).map_err(|e| DriverError::Spawn(e.to_string()))?;
        let mut process = ClaudeProcess::new();
        if let Some(sid) = ctx.resume_session {
            process.set_session_id(sid);
        }

        Ok(DriverSpawnResult {
            child,
            process: Box::new(process),
        })
    }

    fn classify_exit_code(&self, code: i32) -> ExitClassification {
        match code {
            0 => ExitClassification::Normal,
            130 => ExitClassification::Cancelled,
            other => ExitClassification::Crashed(other),
        }
    }
}
