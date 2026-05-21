//! Wire protocol types for the cocli daemon (Rust port).
//!
//! Source-of-truth Go files:
//! - `internal/protocol/daemon_msg.go` — wire msg structs + type constants
//! - `internal/types/types.go` — shared domain types embedded in msgs
//!
//! Phase 0a coverage: see spec §4.1 — `docs/superpowers/specs/2026-05-21-rust-daemon-phase0-design.md`.

pub mod daemon_msg;
pub mod server_msg;
pub mod types;

pub use daemon_msg::DaemonMsg;
pub use server_msg::ServerMsg;

// Phase 0a re-exports (spec §5.1)
pub use server_msg::{
    AgentDeliverMsg, AgentRecoverSessionsMsg, AgentStartMsg, AgentStopMsg, AgentTurnCancelMsg,
    PingMsg, ServerShutdownMsg,
};
pub use daemon_msg::{
    AgentActivityMsg, AgentDeliverAcceptedMsg, AgentDeliverAckMsg, AgentRecoveryRecordMsg,
    AgentSessionEndMsg, AgentSessionIdleMsg, AgentSessionMsg, AgentStatusMsg, AgentStopErrorMsg,
    AgentTurnMsg, DaemonRecoverMsg, PongMsg, ReadyMsg,
};
pub use types::{
    AgentConfig, ChannelSession, DeliveryMessage, FileTreeEntry, RecoverSession, RuntimeModel,
    TrajectoryEntry,
};
