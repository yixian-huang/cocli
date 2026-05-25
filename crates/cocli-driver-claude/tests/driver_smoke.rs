use cocli_driver::{Driver, SpawnContext};
use cocli_driver_claude::ClaudeDriver;
use std::collections::HashMap;
use std::path::PathBuf;

#[tokio::test]
async fn driver_name_is_claude() {
    let d = ClaudeDriver::new(PathBuf::from("/usr/bin/claude"), "sonnet".into());
    assert_eq!(d.name(), "claude");
}

#[tokio::test]
async fn driver_prepare_workspace_is_noop() {
    let d = ClaudeDriver::new(PathBuf::from("/usr/bin/claude"), "sonnet".into());
    let ctx = make_ctx();
    let r = d.prepare_workspace(&ctx).await;
    assert!(r.is_ok());
}

#[tokio::test]
async fn driver_skill_search_paths() {
    let d = ClaudeDriver::new(PathBuf::from("/usr/bin/claude"), "sonnet".into());
    let p = d.skill_search_paths(&PathBuf::from("/home/test"));
    assert!(p.global.iter().any(|g| g.ends_with(".claude/skills")));
    assert!(p.workspace.iter().any(|w| w == ".claude/skills"));
}

#[tokio::test]
async fn driver_classify_exit_code() {
    let d = ClaudeDriver::new(PathBuf::from("/usr/bin/claude"), "sonnet".into());
    assert_eq!(
        d.classify_exit_code(0),
        cocli_driver::ExitClassification::Normal
    );
    assert_eq!(
        d.classify_exit_code(130),
        cocli_driver::ExitClassification::Cancelled
    );
    assert_eq!(
        d.classify_exit_code(42),
        cocli_driver::ExitClassification::Crashed(42)
    );
}

#[tokio::test]
async fn driver_spawn_fails_with_invalid_binary() {
    let d = ClaudeDriver::new(PathBuf::from("/nonexistent/binary"), "sonnet".into());
    let ctx = make_ctx();
    let r = d.spawn(ctx).await;
    assert!(r.is_err(), "spawn with invalid binary should fail");
}

fn make_ctx() -> SpawnContext {
    SpawnContext {
        agent_id: "a1".into(),
        workdir: PathBuf::from("/tmp"),
        system_prompt: "test".into(),
        env_vars: HashMap::new(),
        resume_session: None,
        server_url: "ws://localhost:8090".into(),
        auth_token: "t".into(),
        bridge_bin_path: PathBuf::from("/usr/bin/cocli-bridge"),
        no_bridge: true,
        chat_bridge_args: vec![],
        initial_message: None,
    }
}
