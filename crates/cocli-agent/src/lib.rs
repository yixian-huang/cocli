//! Agent lifecycle: typestate `AgentActor` + `AgentRouter` + delivery queue.
//!
//! Source-of-truth Go files:
//! - `daemon/agent/agent_manager.go` — router-level state
//! - `daemon/agent/agent_io.go` — stdin/stdout wiring
//! - `daemon/agent/agent_lifecycle.go` — start/stop sequence
//! - `daemon/agent/agent_delivery.go` — deliver / ack flow
//! - `internal/format/message.go` — message formatting (simplified bundle for Phase 0a)
//!
//! Phase 0a coverage (spec §5.9):
//! - happy-path start (spawn claude + bridge config + collect session_id)
//! - deliver (stream-json wrap + ack with route action)
//! - stop (SIGTERM / SIGKILL)
//! - router-level state table (running agents) + state-change events

pub mod actor;
pub mod context;
pub mod fork_reason;
pub mod format;
pub mod metrics;
pub mod obs;
pub mod queue;
pub mod recovery;
pub mod router;
pub mod state;
pub mod types;
pub mod watchdog;
pub mod working;
pub mod workspace;

pub use actor::{AgentActor, StartCfg};
pub use metrics::AgentMetrics;
pub use obs::AgentObservationChanged;
pub use queue::DeliveryQueue;
pub use router::{AgentRouter, DaemonConfig};
pub use state::{AgentState, Idle, Running, Starting, Stopping};
pub use types::{AgentCmd, AgentStateChange};
