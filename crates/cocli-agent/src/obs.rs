//! Observation events broadcast from each `AgentActor` to interested
//! observers (HealthActor for idle detection; AgentRouter for state-table
//! upkeep). Uses `tokio::sync::broadcast` so multiple subscribers each see
//! every event.

use uuid::Uuid;

/// Observable change emitted by an `AgentActor` to the broadcast bus.
#[derive(Debug, Clone)]
pub enum AgentObservationChanged {
    /// Actor entered `Running` state (session_id collected, subprocess up).
    Started {
        agent_id: String,
        session_id: String,
        channel_id: Uuid,
        channel_name: String,
    },
    /// Any stdout line read from claude — HealthActor uses this to reset
    /// the idle timer (`last_stdout_at = now()`).
    StdoutSeen { agent_id: String },
    /// `result` line consumed (turn finished); `turn_count` is the new
    /// per-actor counter (1-based).
    TurnEnded { agent_id: String, turn_count: u64 },
    /// Subprocess exited (any reason).
    Stopped { agent_id: String },
}
