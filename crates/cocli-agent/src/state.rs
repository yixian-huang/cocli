//! Typestate markers for `AgentActor<S>`.
//!
//! Each state struct captures exactly the data valid at that point in the
//! lifecycle. The actor's `state` field changes type on transition, so
//! invalid operations (e.g. delivering to an `Idle` actor) become compile
//! errors instead of runtime panics.

use std::sync::Arc;
use std::time::Instant;

use tokio::process::{Child, ChildStdin};
use tokio::sync::{mpsc, Mutex};
use uuid::Uuid;

use cocli_driver::{Driver, DriverProcess, Event, SpawnContext};

/// Stored in `Running` for drivers with `BusyDeliveryMode::Respawn` (gemini).
/// Holds enough context to re-invoke `driver.spawn()` on each user message.
pub struct RespawnCtx {
    pub driver: Arc<dyn Driver>,
    /// Template for each new spawn — `initial_message` is overwritten per turn.
    pub spawn_ctx_template: SpawnContext,
}

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
/// **event_rx** carries generic `Event`s from the stdout pump (translated
/// from runtime-specific lines by the driver's `parse_line_to_events`).
/// The outer run-loop in `router.rs` `select!`s on `mailbox.recv()` and
/// `state.event_rx.recv()` together.
///
/// **process** is the driver-supplied per-spawn state machine. Phase A wires
/// stdin encoding through `process.encode_stdin` (Task 14); Task 15+ will
/// also route turn-cancel/interrupt through `process.interrupt`. The pump
/// task currently still uses `parse_line_to_events` directly — Task 17 will
/// promote it to `process.parse_line`, at which point `take_pending_actions`
/// drain will start surfacing P3/G replay (codex) on the actor side.
pub struct Running {
    pub process_child: Child,
    /// `None` for drivers that use `Stdio::null()` (e.g. gemini SingleShot).
    pub process_stdin: Option<ChildStdin>,
    /// Shared with the stdout-pump task for per-driver `parse_line` dispatch.
    pub process: Arc<Mutex<Box<dyn DriverProcess>>>,
    pub event_rx: mpsc::Receiver<Event>,
    pub session_id: String,
    pub channel_id: Uuid,
    pub channel_name: String,
    pub last_stdout_at: Instant,
    pub turn_count: u64,
    pub launch_id: String,
    /// Set for `BusyDeliveryMode::Respawn` drivers (gemini). Holds the driver +
    /// spawn context template so the actor can re-invoke `driver.spawn()` on
    /// each user message.
    pub respawn_ctx: Option<Arc<RespawnCtx>>,
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
