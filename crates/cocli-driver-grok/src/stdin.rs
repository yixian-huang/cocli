//! Stdin encoder for the grok CLI runtime.
//!
//! Grok (headless `-p` + streaming-json) is a turn-exit runtime like gemini.
//! The actor short-circuits before calling encode_stdin_message for
//! is_turn_exit() drivers; we return None for all modes (notifications and
//! user deliveries go via next respawn's `-p` prompt).

pub use cocli_driver_core::encode_stdin_turn_exit as encode_stdin_message;
