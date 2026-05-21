//! Typestate markers for `AgentActor<S>`.
//!
//! Each state struct captures exactly the data valid at that point in the
//! lifecycle. The actor's `state` field changes type on transition, so
//! invalid operations (e.g. delivering to an `Idle` actor) become compile
//! errors instead of runtime panics.

use std::time::Instant;

use tokio::process::{Child, ChildStdin};
use tokio::sync::mpsc;
use uuid::Uuid;

use cocli_driver_claude::ClaudeEvent;

/// Marker trait for typestate phases. Each phase struct must implement this
/// so the `AgentActor<S>` generic parameter can be bounded.
pub trait AgentState: Send + 'static {
    fn name(&self) -> &'static str;
}

/// Pre-start. Actor has a mailbox but no subprocess yet.
pub struct Idle;

/// Spawn in flight: subprocess started, stdout-pump task running, awaiting
/// first `system` event with `session_id`. Phase 0a does not surface this
/// state externally — it exists only between `Idle::start` invocation and
/// the function return.
pub struct Starting {
    pub start_at: Instant,
}

/// Active session. Owns the child process + stdin half + the
/// stdout-event receiver fed by the pump task spawned during `Idle::start`.
///
/// **event_rx** carries `ClaudeEvent`s from the stdout pump. The outer
/// run-loop in `router.rs` `select!`s on `mailbox.recv()` and
/// `state.event_rx.recv()` together.
pub struct Running {
    pub claude: Child,
    pub claude_stdin: ChildStdin,
    pub event_rx: mpsc::Receiver<ClaudeEvent>,
    pub session_id: String,
    pub channel_id: Uuid,
    pub channel_name: String,
    pub last_stdout_at: Instant,
    pub turn_count: u64,
    pub launch_id: String,
}

/// SIGTERM (or SIGKILL when `force=true`) sent; awaiting reap.
pub struct Stopping {
    pub started_at: Instant,
    pub force: bool,
}

impl AgentState for Idle {
    fn name(&self) -> &'static str {
        "idle"
    }
}
impl AgentState for Starting {
    fn name(&self) -> &'static str {
        "starting"
    }
}
impl AgentState for Running {
    fn name(&self) -> &'static str {
        "running"
    }
}
impl AgentState for Stopping {
    fn name(&self) -> &'static str {
        "stopping"
    }
}
