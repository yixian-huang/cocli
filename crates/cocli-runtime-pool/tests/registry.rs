use async_trait::async_trait;
use cocli_driver::{
    BusyDeliveryMode, DispatchMode, Driver, DriverSpawnResult, EnvPropagation, ExitClassification,
    Result, RuntimeCapabilities, SkillCompat, SkillPaths, SpawnContext,
};
use cocli_runtime_pool::RuntimeRegistry;
use std::path::Path;
use std::sync::Arc;

struct FakeDriver(&'static str);

#[async_trait]
impl Driver for FakeDriver {
    fn name(&self) -> &'static str {
        self.0
    }
    fn capabilities(&self) -> RuntimeCapabilities {
        RuntimeCapabilities {
            dispatch_mode: DispatchMode::Persistent,
            busy_delivery_mode: BusyDeliveryMode::GatedAfterTurn,
            env_propagation: EnvPropagation::Inherit,
            mcp_tool_prefix: "",
            requires_initial_prompt: false,
            context_window_tokens: 100_000,
            skill_compatibility: SkillCompat::Supported,
            supports_native_interrupt: true,
            supports_active_turn_steer: false,
            supports_rejected_steer_replay: false,
        }
    }
    async fn prepare_workspace(&self, _ctx: &SpawnContext) -> Result<()> {
        Ok(())
    }
    fn skill_search_paths(&self, _home: &Path) -> SkillPaths {
        SkillPaths::default()
    }
    async fn spawn(&self, _ctx: SpawnContext) -> Result<DriverSpawnResult> {
        unreachable!()
    }
    fn classify_exit_code(&self, _code: i32) -> ExitClassification {
        ExitClassification::Normal
    }
}

#[test]
fn registry_register_and_get() {
    let mut r = RuntimeRegistry::new();
    r.register(Arc::new(FakeDriver("claude")));
    r.register(Arc::new(FakeDriver("codex")));
    assert!(r.get("claude").is_some());
    assert!(r.get("codex").is_some());
    assert!(r.get("kimi").is_none());
}

#[test]
fn registry_names_returns_all() {
    let mut r = RuntimeRegistry::new();
    r.register(Arc::new(FakeDriver("claude")));
    r.register(Arc::new(FakeDriver("codex")));
    let mut names = r.names();
    names.sort();
    assert_eq!(names, vec!["claude", "codex"]);
}

#[test]
fn allowlist_filters() {
    let mut r = RuntimeRegistry::new().with_allowlist(vec!["claude".into()]);
    r.register(Arc::new(FakeDriver("claude")));
    r.register(Arc::new(FakeDriver("codex")));
    assert!(r.get("claude").is_some());
    assert!(
        r.get("codex").is_none(),
        "codex should be filtered by allowlist"
    );
}

#[test]
fn empty_allowlist_allows_none() {
    let mut r = RuntimeRegistry::new().with_allowlist(vec![]);
    r.register(Arc::new(FakeDriver("claude")));
    assert!(r.get("claude").is_none(), "empty allowlist allows nothing");
}
