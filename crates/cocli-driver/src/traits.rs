//! Driver + DriverProcess traits + RuntimeCapabilities.

use std::path::Path;

use crate::context::{DispatchMode, EncodedStdin, InterruptAction, OutboundMessage, SpawnContext};
use crate::error::Result;
use crate::event::Event;
use crate::paths::{ExitClassification, SkillPaths};

/// A long-lived driver factory: one instance per runtime in the daemon process.
#[async_trait::async_trait]
pub trait Driver: Send + Sync + 'static {
    /// Stable wire-level runtime name. Phase 1 keeps Go-side IDs verbatim:
    /// "claude" | "codex" | "gemini" | "kimi" | "chatrs". Product is "chatry"
    /// but wire ID is "chatrs" (matches Go server routing).
    fn name(&self) -> &'static str;

    /// Static per-runtime capabilities. cocli-agent routes by capability,
    /// not by name — avoids `if name == "codex"` style checks.
    fn capabilities(&self) -> RuntimeCapabilities;

    /// Pre-spawn workspace setup (codex: `git init`; gemini: write `.gemini/settings.json`).
    async fn prepare_workspace(&self, ctx: &SpawnContext) -> Result<()>;

    // Ownership note: `prepare_workspace` borrows `ctx` (read-only setup);
    // `spawn` takes ctx by value (impl may move fields into the child env
    // without cloning). cocli-agent's call pattern: `driver.prepare_workspace(&ctx).await?;`
    // then `driver.spawn(ctx).await?` — drop the borrow before the move.

    /// Skill MD search paths for this runtime.
    fn skill_search_paths(&self, home: &Path) -> SkillPaths;

    /// Spawn a fresh runtime process AND return a per-process state machine.
    /// The caller (cocli-agent) owns the returned Child for lifecycle management.
    /// For SingleShotPerTurn drivers, `ctx.initial_message` MUST be Some.
    async fn spawn(&self, ctx: SpawnContext) -> Result<DriverSpawnResult>;

    /// Classify a process exit code per runtime conventions.
    fn classify_exit_code(&self, code: i32) -> ExitClassification;
}

/// Spawn output: the live Child + the protocol state machine for it.
/// Note: not Clone/Debug because `tokio::process::Child` is neither.
pub struct DriverSpawnResult {
    pub child: tokio::process::Child,
    pub process: Box<dyn DriverProcess>,
}

/// Per-spawn protocol state machine. One instance per `Driver::spawn` call.
pub trait DriverProcess: Send {
    fn dispatch_mode(&self) -> DispatchMode;

    /// Encode an outbound message for stdin. Returns Empty for SingleShotPerTurn
    /// drivers (orchestrator must re-spawn with ctx.initial_message instead).
    fn encode_stdin(&mut self, msg: &OutboundMessage) -> Result<EncodedStdin>;

    /// Parse a single line of stdout into zero or more generic Events.
    fn parse_line(&mut self, line: &str) -> Vec<Event>;

    /// Drain pending driver-originated actions accumulated since the last call.
    /// Codex uses this for P3/G rejected-steer replay — after `turn/completed`,
    /// the driver pushes a `DriverAction::WriteStdin(...)` with the replayed
    /// `turn/start` payload, and cocli-agent forwards it to stdin.
    ///
    /// Implementers WITHOUT P3/G replay (claude/gemini/kimi/chatrs): return
    /// `Vec::new()`. Implementers WITH a buffer: use
    /// `std::mem::take(&mut self.pending_actions)` to drain AND return.
    fn take_pending_actions(&mut self) -> Vec<DriverAction>;

    /// Mid-turn steer.
    /// - codex: Ok + records inflight for P3/G journal.
    /// - kimi: Ok with kimi `steer` JSON-RPC payload (no replay journal).
    /// - claude/gemini/chatrs: Err(SteerNotSupported).
    /// Caller routes only when `capabilities().busy_delivery_mode == Steer`.
    fn steer(&mut self, input: &str) -> Result<EncodedStdin>;

    /// Interrupt active turn. Codex/kimi return WroteToStdin (RPC interrupt);
    /// others return SignalSent(SIGINT).
    fn interrupt(&mut self) -> Result<InterruptAction>;

    /// Current session ID, for cold-restart resume. Kimi returns None
    /// (wire 1.9 limitation; do not synthesize a fake ID).
    fn session_id(&self) -> Option<&str>;

    /// Lines to write to stdin immediately after spawn, before the bootstrap
    /// message. Returned in wire order; each element is a complete newline-
    /// terminated JSON line. Drivers that require explicit initialization
    /// (codex: `initialize` + `thread/start`) override this. Others return
    /// an empty Vec (default — no-op for claude / kimi / chatrs / gemini).
    fn startup_sequence(&mut self) -> Vec<String> {
        Vec::new()
    }
}

/// Driver-originated side-effects surfaced via `take_pending_actions`.
#[derive(Debug, Clone)]
pub enum DriverAction {
    /// Write payload to the runtime's stdin (codex P3/G replay).
    WriteStdin(String),
    /// Emit a synthetic Event into the trajectory.
    EmitEvent(Event),
}

/// Static per-runtime capabilities. Populated by each Driver impl. cocli-agent
/// routes by these fields, replacing `if driver_name == "codex"` style checks.
#[derive(Debug, Clone)]
pub struct RuntimeCapabilities {
    pub dispatch_mode: DispatchMode,
    pub busy_delivery_mode: BusyDeliveryMode,
    pub env_propagation: EnvPropagation,
    pub mcp_tool_prefix: &'static str,
    pub requires_initial_prompt: bool,
    pub context_window_tokens: u64,
    pub skill_compatibility: SkillCompat,
    pub supports_native_interrupt: bool,
    pub supports_active_turn_steer: bool,
    pub supports_rejected_steer_replay: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusyDeliveryMode {
    /// Route via `process.steer()` (codex, kimi).
    Steer,
    /// Queue in cocli-agent's mailbox, write to stdin after TurnEnd event
    /// (claude, chatrs).
    GatedAfterTurn,
    /// Write to stdin immediately (no busy detection — rare).
    Direct,
    /// Re-spawn with new SpawnContext.initial_message (gemini).
    Respawn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvPropagation {
    Inherit,
    File,
    MergedExplicit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillCompat {
    Supported,
    Unsupported,
}
