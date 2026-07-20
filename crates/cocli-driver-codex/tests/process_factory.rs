//! ProcessFactory contract — codex's factory must always hand out fresh
//! per-process drivers (no shared state) and the produced drivers must
//! expose the codex sub-trait set.

use std::path::PathBuf;

use cocli_driver_codex::CodexDriver;
use cocli_driver_core::types::{BusyDeliveryMode, SpawnConfig};
use cocli_driver_core::Driver;

fn factory() -> CodexDriver {
    CodexDriver::new(
        PathBuf::from("/usr/bin/true"),
        PathBuf::from("/opt/cocli/bin/cocli-bridge"),
    )
}

fn cfg(work: &std::path::Path) -> SpawnConfig<'_> {
    SpawnConfig {
        working_dir: work,
        model: "gpt-5",
        mcp_config: None,
        resume_session: None,
        agent_id: "agent-x",
        server_url: "ws://x",
        auth_token: "t",
        system_prompt: "",
        initial_prompt: "",
        env_vars: &[],
    }
}

#[test]
fn factory_advertises_process_factory_role() {
    let drv = factory();
    assert!(drv.as_process_factory().is_some());
}

#[test]
fn new_process_returns_per_process_driver_with_direct_busy_mode() {
    let drv = factory();
    let work = tempfile::tempdir().unwrap();
    let cfg = cfg(work.path());
    let pd: Box<dyn Driver> = drv.as_process_factory().unwrap().new_process(&cfg);
    assert_eq!(pd.name(), "codex");
    assert_eq!(pd.busy_delivery_mode(), BusyDeliveryMode::Direct);
    assert!(pd.supports_turn_steer());
    assert!(pd.as_process_initializer().is_some());
    assert!(pd.as_stdin_binder().is_some());
    assert!(pd.as_turn_interruptor().is_some());
}

#[test]
fn new_process_each_call_yields_fresh_state() {
    let drv = factory();
    let work = tempfile::tempdir().unwrap();
    let cfg = cfg(work.path());
    let _pd_a = drv.as_process_factory().unwrap().new_process(&cfg);
    let _pd_b = drv.as_process_factory().unwrap().new_process(&cfg);
    // We can't directly compare pointers across Box<dyn Driver>, but
    // we can assert that both drivers report fresh state — both should
    // refuse to encode a message before handshake.
    let pd_c = drv.as_process_factory().unwrap().new_process(&cfg);
    let pd_d = drv.as_process_factory().unwrap().new_process(&cfg);
    assert!(pd_c
        .encode_stdin_message("hi", None, cocli_driver_core::types::MessageMode::User,)
        .is_none());
    assert!(pd_d
        .encode_stdin_message("hi", None, cocli_driver_core::types::MessageMode::User,)
        .is_none());
}
