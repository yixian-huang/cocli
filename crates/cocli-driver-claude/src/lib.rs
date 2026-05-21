//! Claude driver: stream-json parser + stdin encoder + spawn helpers.
//!
//! Mirrors `daemon/drivers/claude.go` (launch flags) and
//! `daemon/drivers/parse_helpers.go` (stdin format).

pub mod events;
pub mod spawn;
pub mod stdin;

pub use events::{parse_line, ClaudeEvent};
pub use spawn::{spawn_claude, SpawnContext};
pub use stdin::encode_user_message;
