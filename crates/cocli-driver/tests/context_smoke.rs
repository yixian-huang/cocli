use cocli_driver::{DispatchMode, EncodedStdin, MessageKind, OutboundMessage, SpawnContext};
use std::collections::HashMap;
use std::path::PathBuf;

#[test]
fn spawn_context_constructs() {
    let ctx = SpawnContext {
        agent_id: "agent-1".into(),
        workdir: PathBuf::from("/tmp/workdir"),
        system_prompt: "be helpful".into(),
        env_vars: HashMap::new(),
        resume_session: Some("sid-9".into()),
        server_url: "ws://localhost:8090".into(),
        auth_token: "t1".into(),
        bridge_bin_path: PathBuf::from("/usr/bin/cocli-bridge"),
        no_bridge: false,
        chat_bridge_args: vec!["--agent-id".into(), "agent-1".into()],
        initial_message: None,
    };
    assert_eq!(ctx.agent_id, "agent-1");
    assert_eq!(ctx.resume_session.as_deref(), Some("sid-9"));
}

#[test]
fn dispatch_mode_persistent_default() {
    assert_eq!(DispatchMode::Persistent, DispatchMode::Persistent);
    assert_ne!(DispatchMode::Persistent, DispatchMode::SingleShotPerTurn);
}

#[test]
fn outbound_message_kinds() {
    let user = OutboundMessage {
        kind: MessageKind::User,
        text: "hi".into(),
    };
    let system = OutboundMessage {
        kind: MessageKind::System,
        text: "sys".into(),
    };
    assert_eq!(user.kind, MessageKind::User);
    assert_eq!(system.kind, MessageKind::System);
    assert_ne!(user.kind, system.kind);
}

#[test]
fn encoded_stdin_empty_vs_bytes() {
    let empty: EncodedStdin = EncodedStdin::Empty;
    let bytes = EncodedStdin::Bytes("payload".into());
    match empty {
        EncodedStdin::Empty => {}
        _ => panic!("expected empty"),
    }
    match bytes {
        EncodedStdin::Bytes(s) => assert_eq!(s, "payload"),
        _ => panic!(),
    }
}
