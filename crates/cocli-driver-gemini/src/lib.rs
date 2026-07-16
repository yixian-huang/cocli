//! Gemini driver: stream-json parser + stdin no-op + spawn helpers + Driver impl.
//!
//! Mirrors `daemon/drivers/gemini.go`:
//!   - ParseLine     → `events::parse_line` + `conv::to_driver_events`
//!   - Spawn         → `spawn::spawn_gemini`
//!   - PrepareWorkspace → `driver::GeminiDriver::prepare_workspace`
//!   - GCSessionFiles  → `spawn::gc_gemini_session_files` (via `SessionFileGC`)
//!   - ClassifyExitCode → `ExitCodeClassifier::classify_exit_code`
//!
//! Not included in workspace members yet — Task 2 (Phase 2b plan) wires
//! the new crate into `daemon-rs/Cargo.toml` after all four driver crates
//! land.

pub mod conv;
pub mod driver;
pub mod events;
pub mod spawn;
pub mod stdin;

pub use driver::{GeminiDriver, GEMINI_EXTRA_SYSTEM_PROMPT};
pub use events::{parse_line, GeminiEvent};
pub use spawn::{gc_gemini_session_files, spawn_gemini, write_gemini_settings_json, SpawnContext};
pub use stdin::encode_stdin_message;
