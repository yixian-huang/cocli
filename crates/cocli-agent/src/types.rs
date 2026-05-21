//! Cross-module types: actor commands + state-change events.

use cocli_protocol::AgentDeliverMsg;

/// Commands sent from `AgentRouter` to a per-agent `AgentActor` via its
/// mailbox channel.
#[allow(clippy::large_enum_variant)]
pub enum AgentCmd {
    Deliver(AgentDeliverMsg),
    TurnCancel,
    Stop { force: bool },
}

/// Lifecycle transitions emitted from each `AgentActor` back to the
/// `AgentRouter` so the router can keep its running-agents table in sync.
#[derive(Debug, Clone)]
pub enum AgentStateChange {
    /// AgentActor has been registered with the router (mailbox installed).
    /// Emitted from `handle_start` *before* the spawn task fires.
    Spawned { agent_id: String },
    /// Subprocess started successfully and a session_id was collected.
    Running {
        agent_id: String,
        session_id: String,
    },
    /// SIGTERM/SIGKILL sent; reap pending.
    Stopping { agent_id: String },
    /// Subprocess exited (graceful, kill, or crash). Router removes the
    /// mailbox + drops the delivery queue. `end_reason` is one of:
    /// "manual_stop" | "idle" | "context_reset" | "error".
    Stopped {
        agent_id: String,
        end_reason: String,
    },
}
