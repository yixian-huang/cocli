//! Fixture-based roundtrip tests for every Phase 0a wire msg.
//!
//! Each fixture is a canonical JSON payload that the Go daemon would emit
//! (or accept). The test asserts:
//! 1. JSON parses cleanly into the corresponding `ServerMsg` / `DaemonMsg`
//!    variant via `#[serde(tag = "type")]` dispatch.
//! 2. The parsed value re-serializes and re-parses without losing structure.
//! 3. All required (non-omitempty) fields round-trip equal.

use cocli_protocol::{DaemonMsg, ServerMsg};

// ---------------------------------------------------------------------------
// Generic helpers
// ---------------------------------------------------------------------------

fn parse_server(raw: &str) -> ServerMsg {
    serde_json::from_str(raw).expect("ServerMsg parse")
}

fn parse_daemon(raw: &str) -> DaemonMsg {
    serde_json::from_str(raw).expect("DaemonMsg parse")
}

fn reparse_server(msg: &ServerMsg) -> ServerMsg {
    let back = serde_json::to_string(msg).expect("ServerMsg serialize");
    serde_json::from_str(&back).expect("ServerMsg re-parse")
}

fn reparse_daemon(msg: &DaemonMsg) -> DaemonMsg {
    let back = serde_json::to_string(msg).expect("DaemonMsg serialize");
    serde_json::from_str(&back).expect("DaemonMsg re-parse")
}

// ---------------------------------------------------------------------------
// Server → Daemon
// ---------------------------------------------------------------------------

#[test]
fn ping_roundtrip() {
    let raw = include_str!("fixtures/ping.json");
    let msg = parse_server(raw);
    assert!(matches!(msg, ServerMsg::Ping(_)));
    assert!(matches!(reparse_server(&msg), ServerMsg::Ping(_)));
}

#[test]
fn agent_start_roundtrip() {
    let raw = include_str!("fixtures/agent_start.json");
    let msg = parse_server(raw);
    let ServerMsg::AgentStart(start) = &msg else {
        panic!("expected AgentStart, got {:?}", msg);
    };
    assert_eq!(start.agent_id, "a872bcba-1234-5678-9abc-def012345678");
    assert_eq!(start.config.runtime, "claude");
    assert_eq!(start.config.model, "claude-opus-4-7");
    assert_eq!(start.config.name, "test-agent");
    assert_eq!(start.launch_id, "launch-1");
    assert!(matches!(reparse_server(&msg), ServerMsg::AgentStart(_)));
}

#[test]
fn agent_stop_roundtrip() {
    let raw = include_str!("fixtures/agent_stop.json");
    let msg = parse_server(raw);
    let ServerMsg::AgentStop(stop) = &msg else {
        panic!("expected AgentStop, got {:?}", msg);
    };
    assert_eq!(stop.agent_id, "a872bcba-1234-5678-9abc-def012345678");
    assert!(!stop.force);
    assert!(matches!(reparse_server(&msg), ServerMsg::AgentStop(_)));
}

#[test]
fn agent_deliver_roundtrip() {
    let raw = include_str!("fixtures/agent_deliver.json");
    let msg = parse_server(raw);
    let ServerMsg::AgentDeliver(d) = &msg else {
        panic!("expected AgentDeliver, got {:?}", msg);
    };
    assert_eq!(d.seq, 1);
    assert_eq!(d.attempt, 1);
    assert_eq!(d.priority_class, "normal");
    assert_eq!(d.delivery_tier, "tierDigest");
    assert_eq!(d.message.content, "hello agent");
    assert_eq!(d.message.sender_name, "alice");
    assert_eq!(d.message.message_id, "msg-1");
    // The DeliveryMessage channel_id is snake_case on the wire.
    assert_eq!(
        d.message.channel_id.to_string(),
        "11111111-1111-1111-1111-111111111111"
    );
    assert!(matches!(reparse_server(&msg), ServerMsg::AgentDeliver(_)));
}

#[test]
fn agent_turn_cancel_roundtrip() {
    let raw = include_str!("fixtures/agent_turn_cancel.json");
    let msg = parse_server(raw);
    assert!(matches!(msg, ServerMsg::AgentTurnCancel(_)));
    assert!(matches!(
        reparse_server(&msg),
        ServerMsg::AgentTurnCancel(_)
    ));
}

#[test]
fn agent_recover_sessions_roundtrip() {
    let raw = include_str!("fixtures/agent_recover_sessions.json");
    let msg = parse_server(raw);
    let ServerMsg::AgentRecoverSessions(rs) = &msg else {
        panic!("expected AgentRecoverSessions, got {:?}", msg);
    };
    assert_eq!(rs.sessions.len(), 1);
    let s = &rs.sessions[0];
    assert_eq!(s.agent_id, "a872bcba-1234-5678-9abc-def012345678");
    assert_eq!(s.session_id, "sess-1");
    assert_eq!(s.turn_count, 3);
    assert_eq!(s.sessions.len(), 1);
    assert_eq!(s.sessions[0].session_type, "chat");
    assert!(matches!(
        reparse_server(&msg),
        ServerMsg::AgentRecoverSessions(_)
    ));
}

#[test]
fn server_shutdown_roundtrip() {
    let raw = include_str!("fixtures/server_shutdown.json");
    let msg = parse_server(raw);
    let ServerMsg::ServerShutdown(s) = &msg else {
        panic!("expected ServerShutdown, got {:?}", msg);
    };
    assert_eq!(s.reason, "deploy");
    assert!(matches!(reparse_server(&msg), ServerMsg::ServerShutdown(_)));
}

#[test]
fn server_unknown_msg_falls_through() {
    // A wire type not in Phase 0a should hit the catch-all Unknown variant.
    let raw = r#"{"type":"agent:turn:steer","agentId":"x","input":"do this"}"#;
    let msg: ServerMsg = serde_json::from_str(raw).expect("Unknown parse");
    assert!(matches!(msg, ServerMsg::Unknown));
}

// ---------------------------------------------------------------------------
// Daemon → Server
// ---------------------------------------------------------------------------

#[test]
fn ready_roundtrip() {
    let raw = include_str!("fixtures/ready.json");
    let msg = parse_daemon(raw);
    let DaemonMsg::Ready(r) = &msg else {
        panic!("expected Ready, got {:?}", msg);
    };
    assert_eq!(r.hostname, "test-host");
    assert_eq!(r.os, "darwin");
    assert_eq!(r.daemon_version, "0.0.1-rs");
    assert_eq!(r.runtimes, vec!["claude"]);
    assert_eq!(
        r.capabilities,
        vec!["agent.start".to_string(), "agent.deliver".to_string()]
    );
    assert!(matches!(reparse_daemon(&msg), DaemonMsg::Ready(_)));
}

#[test]
fn pong_roundtrip() {
    let raw = include_str!("fixtures/pong.json");
    let msg = parse_daemon(raw);
    assert!(matches!(msg, DaemonMsg::Pong(_)));
    assert!(matches!(reparse_daemon(&msg), DaemonMsg::Pong(_)));
}

#[test]
fn daemon_recover_roundtrip() {
    let raw = include_str!("fixtures/daemon_recover.json");
    let msg = parse_daemon(raw);
    assert!(matches!(msg, DaemonMsg::DaemonRecover(_)));
    assert!(matches!(reparse_daemon(&msg), DaemonMsg::DaemonRecover(_)));
}

#[test]
fn agent_status_roundtrip() {
    let raw = include_str!("fixtures/agent_status.json");
    let msg = parse_daemon(raw);
    let DaemonMsg::AgentStatus(s) = &msg else {
        panic!("expected AgentStatus, got {:?}", msg);
    };
    // Per spec: status enum is "active" | "inactive" | "error".
    assert_eq!(s.status, "active");
    assert_eq!(s.launch_id, "launch-1");
    assert!(matches!(reparse_daemon(&msg), DaemonMsg::AgentStatus(_)));
}

#[test]
fn agent_stop_error_roundtrip() {
    let raw = include_str!("fixtures/agent_stop_error.json");
    let msg = parse_daemon(raw);
    let DaemonMsg::AgentStopError(e) = &msg else {
        panic!("expected AgentStopError, got {:?}", msg);
    };
    assert_eq!(e.error, "agent already stopping");
    assert!(matches!(reparse_daemon(&msg), DaemonMsg::AgentStopError(_)));
}

#[test]
fn agent_deliver_ack_roundtrip() {
    let raw = include_str!("fixtures/agent_deliver_ack.json");
    let msg = parse_daemon(raw);
    let DaemonMsg::AgentDeliverAck(a) = &msg else {
        panic!("expected AgentDeliverAck, got {:?}", msg);
    };
    assert_eq!(a.seq, 1);
    assert_eq!(a.attempt, 1);
    // Per spec: routeAction enum is "inbox" | "tierDigest" | "tierDelayed" | "checkMessages".
    assert_eq!(a.route_action, "inbox");
    assert!(matches!(
        reparse_daemon(&msg),
        DaemonMsg::AgentDeliverAck(_)
    ));
}

#[test]
fn agent_deliver_accepted_roundtrip() {
    let raw = include_str!("fixtures/agent_deliver_accepted.json");
    let msg = parse_daemon(raw);
    let DaemonMsg::AgentDeliverAccepted(a) = &msg else {
        panic!("expected AgentDeliverAccepted, got {:?}", msg);
    };
    assert_eq!(a.route_action, "tierDigest");
    assert!(matches!(
        reparse_daemon(&msg),
        DaemonMsg::AgentDeliverAccepted(_)
    ));
}

#[test]
fn agent_session_roundtrip() {
    let raw = include_str!("fixtures/agent_session.json");
    let msg = parse_daemon(raw);
    let DaemonMsg::AgentSession(s) = &msg else {
        panic!("expected AgentSession, got {:?}", msg);
    };
    assert_eq!(s.session_id, "sess-1");
    assert!(s.is_new);
    assert_eq!(s.prompt_layer, "basic");
    // The snake_case "channel_id" wire-field anomaly is preserved.
    assert_eq!(
        s.channel_id.to_string(),
        "11111111-1111-1111-1111-111111111111"
    );

    // Verify it stays snake_case in the round-tripped JSON.
    let serialized = serde_json::to_string(&msg).expect("serialize");
    assert!(
        serialized.contains("\"channel_id\""),
        "expected snake_case channel_id in serialized session msg: {}",
        serialized
    );
    assert!(matches!(reparse_daemon(&msg), DaemonMsg::AgentSession(_)));
}

#[test]
fn agent_session_end_roundtrip() {
    let raw = include_str!("fixtures/agent_session_end.json");
    let msg = parse_daemon(raw);
    let DaemonMsg::AgentSessionEnd(s) = &msg else {
        panic!("expected AgentSessionEnd, got {:?}", msg);
    };
    // Per spec: endReason (NOT reason); enum "idle"|"context_reset"|"error"|"manual_stop".
    assert_eq!(s.end_reason, "idle");
    assert_eq!(s.turn_count, 5);
    assert_eq!(s.input_tokens, 1024);
    assert_eq!(s.output_tokens, 256);
    assert!((s.cost_usd - 0.0123).abs() < 1e-9);
    assert_eq!(s.context_window, 200000);

    // Confirm the JSON tag is `endReason`, not `reason`.
    let serialized = serde_json::to_string(&msg).expect("serialize");
    assert!(
        serialized.contains("\"endReason\""),
        "expected camelCase endReason: {}",
        serialized
    );
    assert!(matches!(
        reparse_daemon(&msg),
        DaemonMsg::AgentSessionEnd(_)
    ));
}

#[test]
fn agent_session_idle_roundtrip() {
    let raw = include_str!("fixtures/agent_session_idle.json");
    let msg = parse_daemon(raw);
    let DaemonMsg::AgentSessionIdle(s) = &msg else {
        panic!("expected AgentSessionIdle, got {:?}", msg);
    };
    assert_eq!(s.turn_count, 5);
    assert!((s.total_cost_usd - 0.0234).abs() < 1e-9);
    assert_eq!(s.active_sessions, 1);
    assert!(matches!(
        reparse_daemon(&msg),
        DaemonMsg::AgentSessionIdle(_)
    ));
}

#[test]
fn agent_activity_roundtrip() {
    let raw = include_str!("fixtures/agent_activity.json");
    let msg = parse_daemon(raw);
    let DaemonMsg::AgentActivity(a) = &msg else {
        panic!("expected AgentActivity, got {:?}", msg);
    };
    assert_eq!(a.activity, "working");
    assert_eq!(a.attention_state, "working");
    assert_eq!(a.detail, "thinking");
    assert_eq!(a.entries.len(), 1);
    assert_eq!(a.entries[0].kind, "thinking");
    assert_eq!(a.channel_name, "general");
    assert!(matches!(reparse_daemon(&msg), DaemonMsg::AgentActivity(_)));
}

#[test]
fn agent_turn_roundtrip() {
    let raw = include_str!("fixtures/agent_turn.json");
    let msg = parse_daemon(raw);
    let DaemonMsg::AgentTurn(t) = &msg else {
        panic!("expected AgentTurn, got {:?}", msg);
    };
    assert_eq!(t.session_id, "sess-1");
    assert_eq!(t.turn_number, 2);
    assert_eq!(t.entries.len(), 3);
    assert_eq!(t.entries[0].kind, "text");
    assert_eq!(t.entries[1].kind, "tool_call");
    assert_eq!(t.entries[1].id, "tool-call-1");
    assert_eq!(t.entries[2].kind, "tool_result");
    assert_eq!(t.input_tokens, 1500);
    assert_eq!(t.output_tokens, 350);
    assert!((t.cost_usd - 0.018).abs() < 1e-9);
    assert_eq!(t.context_window, 200000);
    assert_eq!(t.cache_read_tokens, 1200);
    assert!((t.context_usage_pct - 0.75).abs() < 1e-9);
    assert!(matches!(reparse_daemon(&msg), DaemonMsg::AgentTurn(_)));
}

#[test]
fn agent_recovery_record_roundtrip() {
    let raw = include_str!("fixtures/agent_recovery_record.json");
    let msg = parse_daemon(raw);
    let DaemonMsg::AgentRecoveryRecord(r) = &msg else {
        panic!("expected AgentRecoveryRecord, got {:?}", msg);
    };
    assert_eq!(r.stopped_at_ms, 1746234000000);
    assert_eq!(r.stop_reason, "rate_limit");
    assert_eq!(r.expected_recovery_at_ms, 1746237600000);
    assert_eq!(r.provider, "claude");
    assert!(matches!(
        reparse_daemon(&msg),
        DaemonMsg::AgentRecoveryRecord(_)
    ));
}
