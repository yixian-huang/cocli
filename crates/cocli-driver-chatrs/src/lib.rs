//! Chatrs driver — `Driver` impl + JSONL parser + stdin encoder + spawn helpers.
//!
//! Ports `daemon/drivers/chatrs.go`. Subprocess-only (no JSON-RPC state
//! machine); stdout JSONL with `"kind"` discriminator.

pub mod conv;
pub mod driver;
pub mod events;
pub mod spawn;
pub mod stdin;

pub use driver::ChatrsDriver;
pub use events::{parse_line, ChatrsEvent};
pub use spawn::{
    build_chatrs_child_env, extract_chatrs_settings, spawn_chatrs, write_chatrs_agent_json,
    SpawnContext,
};
pub use stdin::{encode_stdin_message, encode_user_message};
