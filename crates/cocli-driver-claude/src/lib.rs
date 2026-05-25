//! Claude driver: stream-json parser + stdin encoder + Driver trait impl.

pub mod driver;
pub mod events;
pub mod process;
pub mod spawn;
pub mod stdin;

pub use driver::ClaudeDriver;
pub use events::{parse_line, parse_line_to_events, ClaudeEvent};
pub use process::ClaudeProcess;
pub use spawn::{spawn_claude, SpawnContext as LegacySpawnContext};
pub use stdin::encode_user_message;
