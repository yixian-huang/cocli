//! Grok Build driver — `Driver` impl + JSONL/streaming-json parser + spawn helpers.
//!
//! Ports headless turn-exit model from docs (https://docs.x.ai/build/cli/headless-scripting).
//! Uses `grok -p` + `--output-format streaming-json` + `--always-approve`.
//! Mirrors Go driver (to be added) and Rust patterns from cocli-driver-gemini.

pub mod caps;
pub mod conv;
pub mod driver;
pub mod errors;
pub mod events;
pub mod spawn;
pub mod stdin;
pub mod usage;

pub use driver::GrokDriver;
pub use events::{parse_line, GrokEvent};
pub use spawn::{spawn_grok, write_grok_agents_md, SpawnContext};
pub use stdin::encode_stdin_message;
pub use usage::{
    context_window_for_model, grok_home_dir, list_models_from_cache, GrokCachedModel,
    GrokTurnUsage, GrokUsageContext,
};
