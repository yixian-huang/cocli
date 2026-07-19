//! Codex driver: JSON-RPC app-server parser + stdin encoder + spawn
//! helpers + factory / per-process `Driver` impls.
//!
//! Mirrors `daemon/drivers/codex.go` (1865 LOC). The factory
//! (`CodexDriver`) is the registry-level singleton; each spawned agent
//! gets a fresh `CodexProcessDriver` with its own JSON-RPC request
//! counter, thread ID, and stdin handle for mid-turn preempts.

pub mod conv;
pub mod driver;
pub mod events;
mod known_silent;
mod skill_probe;
pub mod spawn;
pub mod stdin;
pub mod types;

pub use driver::{CodexDriver, CodexProcessDriver};
pub use events::{parse_line, CodexEvent};
pub use spawn::{build_spawn_args, spawn_codex, SpawnContext};
pub use stdin::{
    encode_auto_approve_response, encode_initialize, encode_thread_resume, encode_thread_start,
    encode_turn_interrupt, encode_turn_start, encode_turn_steer, encode_user_message,
    is_approval_method,
};
pub use types::{CodexErrorInfo, ErrorClassification, RateLimitSnapshot, RateLimitWindow};

#[doc(hidden)]
pub use driver::test_hooks;
