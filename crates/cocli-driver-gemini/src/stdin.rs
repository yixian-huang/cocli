//! Stdin encoder for the gemini CLI runtime.
//!
//! Gemini's headless mode (`-p <prompt>`) accepts the initial prompt as a
//! CLI argument. For every subsequent turn the actor uses the turn-exit
//! respawn pattern (spec §3, §7): queue deliveries, kill child on exit,
//! respawn with wake-notification prompt via `-p` + `--resume <sid>`.
//! Stdin is never written — `encode_stdin_message` returns None.

pub use cocli_driver_core::encode_stdin_turn_exit as encode_stdin_message;
