//! Shared context types passed to Driver trait methods.

use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct SpawnContext {
    pub agent_id: String,
    pub workdir: PathBuf,
    pub system_prompt: String,
    pub env_vars: HashMap<String, String>,
    /// Existing runtime-specific session id for resume on cold restart.
    pub resume_session: Option<String>,
    /// Local daemon HTTP/WS URL exposed to the agent (via bridge env).
    pub server_url: String,
    /// Scoped token for bridge auth.
    pub auth_token: String,
    pub bridge_bin_path: PathBuf,
    /// If true, skip wiring the MCP bridge (test scenarios only).
    pub no_bridge: bool,
    /// Pre-marshalled MCP bridge argv carried into runtime config.
    pub chat_bridge_args: Vec<String>,
    /// For SingleShotPerTurn drivers (gemini): the message to embed in the
    /// next spawn's CLI argument. Persistent drivers ignore this field.
    pub initial_message: Option<String>,
}

#[derive(Debug, Clone)]
pub struct OutboundMessage {
    pub kind: MessageKind,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageKind {
    User,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchMode {
    /// Long-lived stdin pump; driver writes turn messages to stdin (claude, codex, kimi, chatrs).
    Persistent,
    /// New process per turn; encode_stdin returns Empty; orchestrator re-spawns (gemini).
    SingleShotPerTurn,
}

#[derive(Debug, Clone)]
pub enum EncodedStdin {
    /// Driver produced bytes to write to stdin.
    Bytes(String),
    /// Driver does not write to stdin for this turn (SingleShotPerTurn drivers).
    Empty,
}

#[derive(Debug, Clone)]
pub enum InterruptAction {
    /// Driver wrote a turn-cancel envelope to stdin.
    WroteToStdin(String),
    /// Caller should deliver this signal to the child process.
    SignalSent(nix::sys::signal::Signal),
}
