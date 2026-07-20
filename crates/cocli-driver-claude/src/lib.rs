//! Claude driver: stream-json parser + persistent spawn helpers + Driver impl.
//!
//! Mirrors `daemon/drivers/claude.go` (launch flags) and
//! `daemon/drivers/parse_helpers.go` (stream-json output parsing).

pub mod conv;
pub mod driver;
pub mod events;
pub mod spawn;

pub use driver::ClaudeDriver;
pub use events::{parse_line, ClaudeEvent};
pub use spawn::{spawn_claude, SpawnContext};
