//! Typestate markers for `AgentActor<S>`.
//!
//! Each state struct captures exactly the data valid at that point in the
//! lifecycle. The actor's `state` field changes type on transition, so
//! invalid operations (e.g. delivering to an `Idle` actor) become compile
//! errors instead of runtime panics.

use std::sync::Arc;
use std::time::Instant;

use std::path::PathBuf;

use tokio::process::{Child, ChildStdin};
use tokio::sync::mpsc;
use uuid::Uuid;

use cocli_driver_core::{Driver, DriverEvent};

/// Stored in `Running` for turn-exit drivers.
/// Holds enough context to re-invoke `driver.spawn()` on each user message.
pub struct RespawnCtx {
    pub driver: Arc<dyn Driver>,
    pub work_dir: PathBuf,
    pub model: String,
    pub server_url: String,
    pub auth_token: String,
    pub system_prompt: String,
    pub env_vars: Vec<(String, String)>,
}

/// Where the actor stores the subprocess stdin handle.
///
/// Stateful drivers such as Codex bind stdin into their per-process driver,
/// while ordinary persistent drivers keep it in the actor. Turn-exit drivers
/// may close stdin entirely and deliver by respawning.
pub enum ActorStdinStorage {
    Local(ChildStdin),
    ViaBinder,
    Closed,
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
/// **event_rx** carries generic `DriverEvent`s from the stdout pump.
/// The outer run-loop in `router.rs` `select!`s on `mailbox.recv()` and
/// `state.event_rx.recv()` together.
pub struct Running {
    pub child: Child,
    pub stdin_storage: ActorStdinStorage,
    /// Per-process driver. Process-factory runtimes receive a fresh instance
    /// for every spawn; stateless drivers reuse the registry instance.
    pub driver: Arc<dyn Driver>,
    pub event_rx: mpsc::Receiver<DriverEvent>,
    pub session_id: String,
    pub channel_id: Uuid,
    pub channel_name: String,
    pub last_stdout_at: Instant,
    pub turn_count: u64,
    pub launch_id: String,
    /// Set for turn-exit drivers. Holds the factory + owned spawn inputs so
    /// the actor can re-invoke the core contract on each user message.
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
