//! `CodexDriver` (factory) and `CodexProcessDriver` (per-process) — both
//! implement the `Driver` trait. The factory creates a fresh per-process
//! driver on every `Driver::as_process_factory().new_process()` call;
//! the per-process driver owns thread/turn state and the JSON-RPC
//! request-ID counter.
//!
//! Wire-shape parity verified against Go `daemon/drivers/codex.go`:
//! - mcp_tool_prefix `mcp_chat_` (codex.go:47)
//! - busy_delivery_mode = Direct on per-process (codex.go's app-server
//!   accepts JSON-RPC during turns via turn/steer)
//! - context window 256k (codex.go:51)
//! - per-process prepare_workspace runs `git init` when `.git` is
//!   missing (codex.go:64) — no file written. The factory hook is a
//!   no-op stub; the actor invokes the per-process variant.
//! - app-server CLI args + `-c mcp_servers.chat.*` flags
//!   (codex.go:309-321).
//! - Init JSON-RPC handshake (codex.go:341).
//! - turn/start, turn/steer, turn/interrupt encoders (codex.go:1163,
//!   1197, 1240).
//! - Skill paths (codex.go:53, 281).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use cocli_driver_core::event::ErrorSeverity;
use cocli_driver_core::subtraits::{
    ProcessFactory, ProcessInitializer, StdinBinder, TurnInterruptor,
};
use cocli_driver_core::types::{
    BusyDeliveryMode, DriverAgentConfig, EnvPropagation, MessageMode, SkillCompatibility,
    SpawnConfig,
};
use cocli_driver_core::{Driver, DriverError, DriverEvent};
use tokio::io::AsyncWriteExt;

use crate::events::parse_line;
use crate::skill_probe;
use crate::spawn::{spawn_codex, SpawnContext};
use crate::stdin::{
    encode_initialize, encode_thread_fork, encode_thread_resume, encode_thread_start,
    encode_turn_interrupt, encode_turn_start, encode_turn_steer, encode_user_message,
};
use crate::types::RateLimitSnapshot;

const THREAD_FORK_RESPONSE_TIMEOUT: Duration = Duration::from_secs(10);

/// Budget for initialize → thread/start|resume → first `turn/started`.
/// Mirrors Go `handshakeTimeout` (codex.go:127-132). Var-equivalent for
/// tests via [`test_hooks::set_handshake_timeout_ms`].
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(120);

/// Test-only override (milliseconds). Zero in production.
static HANDSHAKE_TIMEOUT_OVERRIDE_MS: AtomicU64 = AtomicU64::new(0);

/// Bounds the FIFO of pending steers. Oldest entries drop when exceeded;
/// the daemon delivery queue remains the source of truth for unsent work.
const INFLIGHT_STEER_CAP: usize = 32;

/// Default bootstrap when no `initial_prompt` was supplied at spawn.
/// Mirrors Go `advanceToTurn` (codex.go:546-548).
const DEFAULT_BOOTSTRAP_PROMPT: &str =
    "You have just started. Use check_messages to see if there are any pending messages.";

/// Factory driver — held as the per-runtime singleton in the daemon's
/// driver registry. Always delegates spawn / encode / parse to a fresh
/// `CodexProcessDriver` via `as_process_factory()`.
pub struct CodexDriver {
    codex_binary: PathBuf,
    bridge_binary: PathBuf,
}

impl CodexDriver {
    pub fn new(codex_binary: PathBuf, bridge_binary: PathBuf) -> Self {
        Self {
            codex_binary,
            bridge_binary,
        }
    }
}

#[async_trait]
impl Driver for CodexDriver {
    fn name(&self) -> &str {
        "codex"
    }

    fn mcp_tool_prefix(&self) -> &str {
        // codex.go:47
        "mcp_chat_"
    }

    fn busy_delivery_mode(&self) -> BusyDeliveryMode {
        // Factory placeholder — per-process overrides to Direct. The
        // factory is never the runtime an actor talks to; this value
        // exists only because the trait is total.
        BusyDeliveryMode::Gated
    }

    fn env_propagation(&self) -> EnvPropagation {
        EnvPropagation::Inherit
    }

    fn skill_compatibility(&self) -> SkillCompatibility {
        SkillCompatibility::Supported
    }

    fn context_window_tokens(&self) -> Option<u32> {
        // codex.go:51
        Some(256_000)
    }

    fn prepare_workspace(
        &self,
        _work_dir: &Path,
        _config: &DriverAgentConfig,
        _agent_id: &str,
        _system_prompt: &str,
    ) -> Result<(), DriverError> {
        // Factory-level no-op. The actor's lifecycle calls
        // `as_process_factory().new_process(cfg)` THEN
        // `per_process.prepare_workspace(...)` — so the per-process
        // driver below is the one that runs `git init`. The factory's
        // hook is never invoked by the actor and is kept only because
        // `Driver` is a total trait.
        Ok(())
    }

    fn spawn(&self, _cfg: &SpawnConfig) -> Result<tokio::process::Child, DriverError> {
        // Factory stub — production path goes through
        // as_process_factory().new_process().spawn(); preserve the Go
        // shape (codex.go:78) where calling Spawn on the factory works
        // by delegating, but the daemon does NOT take this path.
        Err(DriverError::Other(
            "CodexDriver is a factory; spawn via as_process_factory()".to_string(),
        ))
    }

    fn parse_event(&self, _line: &str) -> Vec<DriverEvent> {
        // codex.go:84 — factory ParseLine returns nil.
        Vec::new()
    }

    fn encode_stdin_message(
        &self,
        _text: &str,
        _session_id: Option<&str>,
        _mode: MessageMode,
    ) -> Option<String> {
        // codex.go:85-87 — factory EncodeStdinMessage returns "".
        None
    }

    fn requires_initial_prompt(&self) -> bool {
        // Router builds the check_messages bootstrap; the actor writes it
        // after the JSON-RPC handshake completes (thread id is required
        // before encode_stdin_message can emit turn/start).
        true
    }

    fn supports_turn_cancel(&self) -> bool {
        // codex app-server supports turn/interrupt (per-process); the
        // factory advertises the capability so the daemon's router can
        // decide pre-spawn.
        true
    }

    fn skill_search_paths(&self, workspace: &Path) -> Vec<PathBuf> {
        skill_paths_for(workspace)
    }

    async fn probe_skills(
        &self,
        workspace: &Path,
    ) -> Result<Option<cocli_driver_core::NativeSkillProbe>, DriverError> {
        skill_probe::probe_skills(&self.codex_binary, workspace)
            .await
            .map(Some)
    }

    fn as_process_factory(&self) -> Option<&dyn ProcessFactory> {
        Some(self)
    }

    fn is_turn_exit(&self) -> bool {
        false
    }
}

impl ProcessFactory for CodexDriver {
    fn new_process(&self, _cfg: &SpawnConfig) -> Box<dyn Driver> {
        Box::new(CodexProcessDriver::new(
            self.codex_binary.clone(),
            self.bridge_binary.clone(),
        ))
    }
}

/// Per-process driver. Owns the JSON-RPC handshake state, thread ID,
/// active turn ID, and the captured stdin handle used by `turn_steer` /
/// `interrupt_turn`.
pub struct CodexProcessDriver {
    codex_binary: PathBuf,
    bridge_binary: PathBuf,
    /// Monotonic JSON-RPC request ID. id=1 is reserved for `initialize`;
    /// id=2 is reserved for the `thread/start` (or `thread/resume`)
    /// chained by the handshake state machine. `encode_stdin_message`,
    /// `turn_steer`, `interrupt_turn` fetch-add starting at id=3.
    request_id: AtomicU64,
    state: Arc<Mutex<CodexState>>,
    stdin: Arc<Mutex<Option<tokio::process::ChildStdin>>>,
    /// Params captured at `spawn()` time so the handshake state machine
    /// can build the `thread/start` / `thread/resume` JSON when the
    /// `initialize` response arrives. Mirrors codex.go's SpawnContext
    /// stash (codex.go:101-113).
    spawn_params: Arc<Mutex<Option<SpawnParams>>>,
    /// B5 runtime-drift detection: one `[runtime_drift]` error per session.
    drift_emitted: AtomicBool,
}

#[derive(Default)]
struct CodexState {
    /// Set at `spawn()` for structured logs (Go `agentID` on codex driver).
    agent_id: String,
    thread_id: String,
    active_turn_id: String,
    rate_limits: Option<RateLimitSnapshot>,
    /// Carried from `thread/tokenUsage/updated` and consumed on the next
    /// `turn/completed` (codex.go:629).
    pending_tokens: Option<PendingTokens>,
    /// Pending synchronous JSON-RPC control-call responses keyed by request id.
    /// Handshake responses remain phase-driven; this map is for calls like
    /// `thread/fork` that must return a response payload to the caller.
    pending_control: HashMap<u64, std_mpsc::Sender<serde_json::Value>>,
    /// JSON-RPC handshake state. Codex review issue 1: without a state
    /// machine here, the `initialize` response was silently dropped,
    /// `thread/start` was never sent, and no `SessionStarted` event ever
    /// surfaced — `AgentActor::start` timed out at 15s.
    handshake: HandshakeState,
    /// Steer texts written to stdin but not yet confirmed (no turn-end,
    /// no rejection). Used to pair `activeTurnNotSteerable` errors with
    /// the message that was rejected (codex.go:169, 1486).
    inflight_steers: Vec<PendingSteer>,
    /// Steer texts rejected as not-steerable, replayed as `turn/start` on
    /// the next `turn/completed` boundary (codex.go:171-175, 645-667).
    rejected_steers: Vec<String>,
    /// Drop this sender to cancel the handshake-timeout watchdog once the
    /// first `turn/started` lands (Go `phaseRunning`, codex.go:605-613).
    handshake_cancel: Option<std::sync::mpsc::Sender<()>>,
}

/// A `turn/steer` we wrote but have not yet seen confirmed or rejected.
struct PendingSteer {
    text: String,
    #[allow(dead_code)]
    sent_at: std::time::Instant,
}

/// Stages of the JSON-RPC handshake. See codex.go phaseInit / phaseInitWait
/// / phaseThreadWait / phaseTurnWait / phaseRunning.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
enum HandshakeState {
    /// `initialize` written to stdin in `write_init_sequence`; awaiting
    /// the JSON-RPC response with id=1.
    #[default]
    AwaitingInitializeResponse,
    /// `thread/start` (or `thread/resume`) written via `DriverEvent::Write`;
    /// awaiting the response with id=2.
    AwaitingThreadStartResponse,
    /// Thread is open; bootstrap `turn/start` Write emitted (Go
    /// `phaseTurnWait`). `encode_stdin_message` stays gated until Running.
    Ready,
    /// First `turn/started` received — matches Go `phaseRunning`. The
    /// handshake-timeout monitor treats this as success.
    Running,
}

/// Subset of `SpawnConfig` captured at spawn time so the handshake state
/// machine has enough context to build `thread/start` / `thread/resume`
/// when the `initialize` response arrives.
struct SpawnParams {
    work_dir: PathBuf,
    model: String,
    system_prompt: String,
    /// Router-built bootstrap user turn (check_messages callout).
    initial_prompt: String,
    resume_session: Option<String>,
}

impl CodexState {
    fn record_inflight_steer_locked(&mut self, text: &str) {
        self.inflight_steers.push(PendingSteer {
            text: text.to_string(),
            sent_at: std::time::Instant::now(),
        });
        if self.inflight_steers.len() > INFLIGHT_STEER_CAP {
            let over = self.inflight_steers.len() - INFLIGHT_STEER_CAP;
            self.inflight_steers.drain(0..over);
        }
    }

    fn promote_rejected_steer_locked(&mut self) -> Option<String> {
        if self.inflight_steers.is_empty() {
            return None;
        }
        let oldest = self.inflight_steers.remove(0);
        self.rejected_steers.push(oldest.text.clone());
        Some(oldest.text)
    }

    fn dequeue_rejected_replay_locked(&mut self) -> Option<String> {
        if self.rejected_steers.is_empty() {
            return None;
        }
        Some(self.rejected_steers.remove(0))
    }

    fn clear_steer_queues_locked(&mut self) {
        self.inflight_steers.clear();
        self.rejected_steers.clear();
    }
}

#[derive(Default, Clone, Copy)]
struct PendingTokens {
    input: u64,
    output: u64,
    cached: u64,
    /// Captured from `thread/tokenUsage/updated.tokenUsage.modelContextWindow`.
    context_window: u64,
}

impl CodexProcessDriver {
    pub fn new(codex_binary: PathBuf, bridge_binary: PathBuf) -> Self {
        Self {
            codex_binary,
            bridge_binary,
            // id=1 reserved for initialize, id=2 reserved for the
            // chained thread/start (or thread/resume) the handshake
            // emits when initialize response arrives.
            request_id: AtomicU64::new(3),
            state: Arc::new(Mutex::new(CodexState::default())),
            stdin: Arc::new(Mutex::new(None)),
            spawn_params: Arc::new(Mutex::new(None)),
            drift_emitted: AtomicBool::new(false),
        }
    }

    fn emit_drift_if_first(&self, reason: &str) -> Vec<DriverEvent> {
        if self.drift_emitted.swap(true, Ordering::SeqCst) {
            return Vec::new();
        }
        tracing::warn!(
            component = "drivers/codex",
            reason = reason,
            "codex driver: runtime shape drift"
        );
        vec![DriverEvent::Error {
            message: format!("[runtime_drift] codex driver: {reason}"),
            code: Some("runtime_drift".to_string()),
            severity: Some(ErrorSeverity::Warning),
            http_status: None,
        }]
    }

    fn next_request_id(&self) -> u64 {
        self.request_id.fetch_add(1, Ordering::SeqCst)
    }

    fn agent_id(&self) -> String {
        self.state.lock().unwrap().agent_id.clone()
    }

    /// Mid-turn delivery requires `phaseRunning` (Go `EncodeStdinMessage`
    /// gate). `turn/steer` and `interrupt_turn` share the same guard.
    fn steer_session_locked(g: &CodexState) -> Result<(), DriverError> {
        if g.thread_id.is_empty() || g.handshake != HandshakeState::Running {
            return Err(DriverError::TurnSteerUnavailable);
        }
        Ok(())
    }

    fn steer_turn_ids_locked(g: &CodexState) -> Result<(String, String), DriverError> {
        Self::steer_session_locked(g)?;
        if g.active_turn_id.is_empty() {
            return Err(DriverError::TurnSteerNoActiveTurn);
        }
        Ok((g.thread_id.clone(), g.active_turn_id.clone()))
    }

    async fn write_stdin_bytes(&self, bytes: &[u8]) -> Result<(), DriverError> {
        StdinBinder::write_stdin(self, bytes)
            .await
            .map_err(DriverError::Io)
    }

    /// Test hook: directly set the thread ID + active turn ID so unit
    /// tests can drive turn_steer / interrupt_turn without running the
    /// full handshake. Sets `HandshakeState::Running`.
    #[doc(hidden)]
    pub fn set_state_for_test(&self, thread_id: &str, active_turn_id: &str) {
        let mut g = self.state.lock().unwrap();
        g.thread_id = thread_id.to_string();
        g.active_turn_id = active_turn_id.to_string();
        g.handshake = HandshakeState::Running;
    }

    /// Test hook: thread + active turn set but handshake still `Ready`
    /// (`phaseTurnWait`) — delivery paths must stay gated.
    #[doc(hidden)]
    pub fn set_handshake_ready_for_test(&self, thread_id: &str, active_turn_id: &str) {
        let mut g = self.state.lock().unwrap();
        g.thread_id = thread_id.to_string();
        g.active_turn_id = active_turn_id.to_string();
        g.handshake = HandshakeState::Ready;
    }

    /// Test hook: prime the spawn-time params so handshake-driven write
    /// emission can be exercised without actually spawning a process.
    #[doc(hidden)]
    pub fn set_spawn_params_for_test(
        &self,
        work_dir: &Path,
        model: &str,
        system_prompt: &str,
        resume_session: Option<&str>,
    ) {
        self.set_spawn_params_with_prompt_for_test(
            work_dir,
            model,
            system_prompt,
            None,
            resume_session,
        );
    }

    /// Test hook: same as [`set_spawn_params_for_test`] but with an explicit
    /// bootstrap prompt for handshake `turn/start` Write emission.
    #[doc(hidden)]
    pub fn set_spawn_params_with_prompt_for_test(
        &self,
        work_dir: &Path,
        model: &str,
        system_prompt: &str,
        initial_prompt: Option<&str>,
        resume_session: Option<&str>,
    ) {
        *self.spawn_params.lock().unwrap() = Some(SpawnParams {
            work_dir: work_dir.to_path_buf(),
            model: model.to_string(),
            system_prompt: system_prompt.to_string(),
            initial_prompt: initial_prompt
                .unwrap_or(DEFAULT_BOOTSTRAP_PROMPT)
                .to_string(),
            resume_session: resume_session.map(|s| s.to_string()),
        });
    }

    /// Returns `Some(events)` when `line` is a JSON-RPC response that the
    /// handshake state machine consumes. Returns `None` for non-responses
    /// (notifications, errors, malformed lines) so the caller can fall
    /// through to the regular parser.
    ///
    /// The machine mirrors codex.go's `handleResponse` (codex.go:449):
    /// - id=1 (initialize) → write `thread/start` (or `thread/resume`)
    ///   via `DriverEvent::Write`, transition to `AwaitingThreadStartResponse`.
    /// - id=2 (thread/start | thread/resume) → extract `result.thread.id`,
    ///   transition to `Ready`, emit `DriverEvent::SessionStarted`.
    ///
    /// On resume failure (response carries `error`), we fall back to a
    /// fresh `thread/start` mirroring codex.go:466-470.
    fn maybe_handle_handshake_response(&self, line: &str) -> Option<Vec<DriverEvent>> {
        let raw: serde_json::Value = serde_json::from_str(line.trim()).ok()?;
        // Response shape: numeric id present, no method field.
        let id = raw.get("id").and_then(|v| v.as_u64())?;
        if raw.get("method").and_then(|v| v.as_str()).is_some() {
            return None;
        }

        if let Some(tx) = {
            let mut g = self.state.lock().unwrap();
            g.pending_control.remove(&id)
        } {
            let _ = tx.send(raw);
            return Some(Vec::new());
        }

        // Snapshot state under the lock, then release before doing work.
        let (current_phase, spawn_params_ready) = {
            let g = self.state.lock().unwrap();
            (
                g.handshake.clone(),
                self.spawn_params.lock().unwrap().is_some(),
            )
        };

        match (current_phase, id) {
            (HandshakeState::AwaitingInitializeResponse, 1) => {
                // Check for error response — codex would not return an error
                // for `initialize` in practice, but be defensive.
                if raw.get("error").is_some() {
                    return Some(vec![DriverEvent::Error {
                        message: format!(
                            "codex initialize failed: {}",
                            raw.get("error")
                                .and_then(|e| e.get("message"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("unknown")
                        ),
                        code: Some("initialize_failed".to_string()),
                        severity: Some(cocli_driver_core::event::ErrorSeverity::Error),
                        http_status: None,
                    }]);
                }
                if !spawn_params_ready {
                    // No spawn params — this happens in tests that exercise
                    // parse_event without invoking spawn. We can't build
                    // thread/start without work_dir, so just absorb.
                    return Some(Vec::new());
                }
                let data = self.build_thread_request_json();
                {
                    let mut g = self.state.lock().unwrap();
                    g.handshake = HandshakeState::AwaitingThreadStartResponse;
                    // If we're resuming, codex's response is acked but the
                    // thread id is already known from spawn params — set it
                    // now so a delivery between thread/resume-write and the
                    // ack lands on the right thread. Mirrors codex.go:514.
                    let sp = self.spawn_params.lock().unwrap();
                    if let Some(p) = sp.as_ref() {
                        if let Some(resume) = &p.resume_session {
                            g.thread_id.clone_from(resume);
                        }
                    }
                }
                Some(vec![DriverEvent::Write { data }])
            }
            (HandshakeState::AwaitingThreadStartResponse, 2) => {
                // Error path: if we were resuming, fall back to fresh start.
                if raw.get("error").is_some() {
                    let was_resume = self
                        .spawn_params
                        .lock()
                        .unwrap()
                        .as_ref()
                        .and_then(|p| p.resume_session.clone())
                        .is_some();
                    if was_resume {
                        // Clear resume_session + thread_id, build a fresh
                        // thread/start, and emit it. Stay in
                        // AwaitingThreadStartResponse for the next ack.
                        {
                            let mut g = self.state.lock().unwrap();
                            g.thread_id.clear();
                        }
                        if let Some(p) = self.spawn_params.lock().unwrap().as_mut() {
                            p.resume_session = None;
                        }
                        let data = self.build_thread_request_json();
                        return Some(vec![DriverEvent::Write { data }]);
                    }
                    return Some(vec![DriverEvent::Error {
                        message: format!(
                            "codex thread/start failed: {}",
                            raw.get("error")
                                .and_then(|e| e.get("message"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("unknown")
                        ),
                        code: Some("thread_start_failed".to_string()),
                        severity: Some(cocli_driver_core::event::ErrorSeverity::Error),
                        http_status: None,
                    }]);
                }
                // Success: extract thread id from result (Go threadIDFromResult).
                let thread_id = raw
                    .get("result")
                    .and_then(thread_id_from_result)
                    .unwrap_or_else(|| {
                        // Resume path may not echo thread.id — fall back to
                        // the one stashed at spawn time / set above.
                        self.state.lock().unwrap().thread_id.clone()
                    });
                if thread_id.is_empty() {
                    return Some(vec![DriverEvent::Error {
                        message: "codex thread/start response missing thread.id".to_string(),
                        code: Some("thread_start_no_id".to_string()),
                        severity: Some(cocli_driver_core::event::ErrorSeverity::Error),
                        http_status: None,
                    }]);
                }
                {
                    let mut g = self.state.lock().unwrap();
                    g.thread_id.clone_from(&thread_id);
                    g.handshake = HandshakeState::Ready;
                }
                let mut out = vec![DriverEvent::SessionStarted {
                    session_id: thread_id,
                }];
                if let Some(data) = self.build_bootstrap_turn_start() {
                    out.push(DriverEvent::Write { data });
                }
                Some(out)
            }
            // Stray response while in Ready (or unmatched id) — ignore.
            (_, _) => Some(Vec::new()),
        }
    }

    /// Build the JSON-RPC line for either `thread/start` (no resume) or
    /// `thread/resume` (resume requested at spawn time). Always uses
    /// request id = 2. Mirrors codex.go::advanceToThread.
    fn build_thread_request_json(&self) -> String {
        let sp = self.spawn_params.lock().unwrap();
        let Some(p) = sp.as_ref() else {
            // Shouldn't be reachable in production (spawn always
            // populates), but be safe.
            return encode_thread_start("", "", "", 2);
        };
        if let Some(resume) = &p.resume_session {
            encode_thread_resume(resume, 2)
        } else {
            let work_dir_s = p.work_dir.to_string_lossy();
            encode_thread_start(&work_dir_s, &p.system_prompt, &p.model, 2)
        }
    }

    fn clear_pending_control(&self, request_id: u64) {
        let _ = self
            .state
            .lock()
            .unwrap()
            .pending_control
            .remove(&request_id);
    }

    /// First user turn after thread open — mirrors Go `advanceToTurn`.
    fn build_bootstrap_turn_start(&self) -> Option<String> {
        let thread_id = {
            let g = self.state.lock().unwrap();
            let tid = g.thread_id.trim().to_string();
            if tid.is_empty() {
                return None;
            }
            tid
        };
        let prompt = self
            .spawn_params
            .lock()
            .unwrap()
            .as_ref()
            .map(|p| {
                let trimmed = p.initial_prompt.trim();
                if trimmed.is_empty() {
                    DEFAULT_BOOTSTRAP_PROMPT.to_string()
                } else {
                    trimmed.to_string()
                }
            })
            .unwrap_or_else(|| DEFAULT_BOOTSTRAP_PROMPT.to_string());
        let id = self.next_request_id();
        Some(encode_turn_start(&prompt, &thread_id, id))
    }

    fn build_turn_start_replay(&self, text: &str) -> Option<String> {
        let thread_id = {
            let g = self.state.lock().unwrap();
            let tid = g.thread_id.trim().to_string();
            if tid.is_empty() {
                return None;
            }
            tid
        };
        let id = self.next_request_id();
        Some(encode_turn_start(text, &thread_id, id))
    }

    fn arm_handshake_timeout_monitor(&self) {
        let mut g = self.state.lock().unwrap();
        if g.handshake_cancel.is_some() {
            return;
        }
        let (cancel_tx, cancel_rx) = std::sync::mpsc::channel();
        g.handshake_cancel = Some(cancel_tx);
        let timeout = handshake_timeout_duration();
        drop(g);

        let state = self.state.clone();
        let stdin = self.stdin.clone();
        std::thread::spawn(move || {
            if let Err(std::sync::mpsc::RecvTimeoutError::Timeout) = cancel_rx.recv_timeout(timeout)
            {
                fire_handshake_timeout(state, stdin, timeout);
            }
        });
    }

    fn complete_handshake_monitor(&self) {
        let mut g = self.state.lock().unwrap();
        g.handshake = HandshakeState::Running;
        g.handshake_cancel.take();
    }

    /// Test hook: whether the bound stdin handle is still present.
    #[doc(hidden)]
    pub fn stdin_is_bound_for_test(&self) -> bool {
        self.stdin.lock().unwrap().is_some()
    }
}

fn handshake_timeout_duration() -> Duration {
    let ms = HANDSHAKE_TIMEOUT_OVERRIDE_MS.load(Ordering::SeqCst);
    if ms > 0 {
        Duration::from_millis(ms)
    } else {
        HANDSHAKE_TIMEOUT
    }
}

fn fire_handshake_timeout(
    state: Arc<Mutex<CodexState>>,
    stdin: Arc<Mutex<Option<tokio::process::ChildStdin>>>,
    timeout: Duration,
) {
    let phase = {
        let mut g = state.lock().unwrap();
        if g.handshake == HandshakeState::Running {
            return;
        }
        let phase = g.handshake.clone();
        g.handshake_cancel.take();
        phase
    };
    tracing::error!(
        component = "drivers/codex",
        ?phase,
        timeout_secs = timeout.as_secs(),
        "codex handshake timed out; closing stdin to terminate process"
    );
    stdin.lock().unwrap().take();
}

/// Hidden hooks for integration tests (handshake timeout override).
#[doc(hidden)]
pub mod test_hooks {
    use std::sync::atomic::Ordering;
    use std::sync::{Mutex, MutexGuard};

    static HOOKS_MUTEX: Mutex<()> = Mutex::new(());
    static ASYNC_HOOKS_MUTEX: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    /// Serialize tests that override handshake timeout (shared atomic).
    pub fn lock_hooks() -> MutexGuard<'static, ()> {
        HOOKS_MUTEX
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Async variant for tests that must hold the hook lock across `.await`.
    pub async fn lock_hooks_async() -> tokio::sync::MutexGuard<'static, ()> {
        ASYNC_HOOKS_MUTEX.lock().await
    }

    pub fn set_handshake_timeout_ms(ms: u64) {
        super::HANDSHAKE_TIMEOUT_OVERRIDE_MS.store(ms, Ordering::SeqCst);
    }

    pub fn reset_handshake_timeout_ms() {
        super::HANDSHAKE_TIMEOUT_OVERRIDE_MS.store(0, Ordering::SeqCst);
    }
}

#[async_trait]
impl Driver for CodexProcessDriver {
    fn name(&self) -> &str {
        "codex"
    }

    fn mcp_tool_prefix(&self) -> &str {
        "mcp_chat_"
    }

    fn requires_initial_prompt(&self) -> bool {
        // Bootstrap turn/start is emitted by the handshake state machine
        // (`advanceToTurn` parity) — the actor must not double-write.
        false
    }

    fn busy_delivery_mode(&self) -> BusyDeliveryMode {
        // codex.go:40 — app-server mode accepts JSON-RPC on stdin at
        // any time (turn/steer during an active turn).
        BusyDeliveryMode::Direct
    }

    fn env_propagation(&self) -> EnvPropagation {
        EnvPropagation::Inherit
    }

    fn skill_compatibility(&self) -> SkillCompatibility {
        SkillCompatibility::Supported
    }

    fn context_window_tokens(&self) -> Option<u32> {
        Some(256_000)
    }

    fn prepare_workspace(
        &self,
        work_dir: &Path,
        _config: &DriverAgentConfig,
        _agent_id: &str,
        _system_prompt: &str,
    ) -> Result<(), DriverError> {
        // codex.go:62-73 — codex requires a git repo in the workdir.
        // **This is the path the actor actually invokes** (after
        // `as_process_factory().new_process(cfg)`); the factory-level
        // hook is a no-op stub. Codex review issue 2 — initial port had
        // git init on the factory and the per-process as a no-op, which
        // meant `git init` never ran.
        let git_dir = work_dir.join(".git");
        if !git_dir.exists() {
            let status = std::process::Command::new("git")
                .arg("init")
                .current_dir(work_dir)
                .status()
                .map_err(DriverError::Io)?;
            if !status.success() {
                return Err(DriverError::Other(format!(
                    "git init for codex workspace exited with {status}"
                )));
            }
        }
        Ok(())
    }

    fn spawn(&self, cfg: &SpawnConfig) -> Result<tokio::process::Child, DriverError> {
        // Stash params the handshake state machine needs to build the
        // `thread/start` / `thread/resume` request when the `initialize`
        // response arrives. Mirrors codex.go's SpawnContext capture
        // (codex.go:101-113).
        {
            let mut g = self.state.lock().unwrap();
            g.agent_id = cfg.agent_id.to_string();
        }
        {
            let mut g = self.spawn_params.lock().unwrap();
            *g = Some(SpawnParams {
                work_dir: cfg.working_dir.to_path_buf(),
                model: cfg.model.to_string(),
                system_prompt: cfg.system_prompt.to_string(),
                initial_prompt: cfg.initial_prompt.to_string(),
                resume_session: cfg.resume_session.map(|s| s.to_string()),
            });
        }
        spawn_codex(&SpawnContext {
            codex_binary: &self.codex_binary,
            bridge_binary: &self.bridge_binary,
            working_dir: cfg.working_dir,
            model: cfg.model,
            agent_id: cfg.agent_id,
            server_url: cfg.server_url,
            auth_token: cfg.auth_token,
            no_bridge: false,
            system_prompt: cfg.system_prompt,
            // Codex review issue 4: env_vars was dropped. Phase 2b actor
            // integration (Task 1e, merged via PR #13) populates
            // SpawnConfig.env_vars from AgentConfig; passing &[] here
            // silently discarded per-agent env (API keys, model
            // overrides) and broke any agent that depended on them.
            env_vars: cfg.env_vars,
        })
        .map_err(DriverError::Io)
    }

    fn parse_event(&self, line: &str) -> Vec<DriverEvent> {
        // Handshake state machine: responses (id present, no method) are
        // not surfaced by the stateless parser, but the driver MUST act
        // on them to chain initialize → thread/start → SessionStarted.
        // Codex review issue 1.
        if let Some(handshake_out) = self.maybe_handle_handshake_response(line) {
            return handshake_out;
        }
        let events = parse_line(line);
        let mut out: Vec<DriverEvent> = Vec::new();
        for ev in events {
            match ev {
                crate::events::CodexEvent::SessionStarted { ref session_id } => {
                    let mut g = self.state.lock().unwrap();
                    if g.thread_id.is_empty() {
                        g.thread_id.clone_from(session_id);
                    }
                    drop(g);
                    out.extend(Vec::<DriverEvent>::from(ev));
                }
                crate::events::CodexEvent::TurnStarted { ref turn_id } => {
                    self.complete_handshake_monitor();
                    // Codex review issue 3: without this update the
                    // encoder always emits turn/start, never turn/steer,
                    // so mid-turn deliveries spawn a colliding turn.
                    if let Some(tid) = turn_id {
                        tracing::info!(
                            agent_id = %self.agent_id(),
                            turn_id = %tid,
                            "codex: turn/started"
                        );
                        let mut g = self.state.lock().unwrap();
                        g.active_turn_id.clone_from(tid);
                    } else {
                        tracing::info!(agent_id = %self.agent_id(), "codex: turn/started");
                    }
                    out.extend(Vec::<DriverEvent>::from(ev.clone()));
                }
                crate::events::CodexEvent::Thinking => {
                    out.extend(Vec::<DriverEvent>::from(
                        crate::events::CodexEvent::Thinking,
                    ));
                }
                crate::events::CodexEvent::TokenUsage {
                    input_tokens,
                    output_tokens,
                    cached_input_tokens,
                    model_context_window,
                } => {
                    let mut g = self.state.lock().unwrap();
                    g.pending_tokens = Some(PendingTokens {
                        input: input_tokens,
                        output: output_tokens,
                        cached: cached_input_tokens,
                        context_window: model_context_window,
                    });
                    // No DriverEvent — silently absorbed.
                }
                crate::events::CodexEvent::TurnEnd {
                    status,
                    input_tokens: _,
                    output_tokens: _,
                    context_window: _,
                    cache_read_tokens: _,
                } => {
                    let (merged, replay_text) = {
                        let mut g = self.state.lock().unwrap();
                        g.active_turn_id.clear();
                        let merged = g.pending_tokens.take();
                        let replay_text = g.dequeue_rejected_replay_locked();
                        (merged, replay_text)
                    };
                    let (i, o, c, cw) = match merged {
                        Some(p) => (
                            // Same merge as codex.go:639 — cached prefix
                            // counts toward "tokens occupying the model
                            // context window for the next turn".
                            p.input + p.cached,
                            p.output,
                            p.cached,
                            p.context_window,
                        ),
                        None => (0, 0, 0, 0),
                    };
                    // Codex `thread/tokenUsage/updated` carries token counts
                    // only — no `cost_usd` / `totalCostUsd` field (Go parity:
                    // codex.go `tokenUsage` struct). Per-turn cost stays 0.
                    out.push(DriverEvent::TurnEnd {
                        status: cocli_driver_core::types::normalize_turn_status(&status),
                        input_tokens: i,
                        output_tokens: o,
                        cost_usd: 0.0,
                        cache_creation_tokens: 0,
                        cache_read_tokens: c,
                        context_window_tokens: cw,
                    });
                    if let Some(text) = replay_text {
                        tracing::info!(
                            agent_id = %self.agent_id(),
                            text_bytes = text.len(),
                            "codex replaying previously-rejected steer as turn/start"
                        );
                        if let Some(data) = self.build_turn_start_replay(&text) {
                            out.push(DriverEvent::Write { data });
                        }
                    }
                }
                crate::events::CodexEvent::RateLimits { snapshot } => {
                    let bucket_reached = snapshot.bucket_reached();
                    {
                        let mut g = self.state.lock().unwrap();
                        g.rate_limits = Some(snapshot.clone());
                    }
                    if bucket_reached {
                        out.push(crate::conv::event_rate_limit_from_snapshot(&snapshot));
                    }
                }
                crate::events::CodexEvent::Error {
                    ref message,
                    ref info,
                    http_status,
                    will_retry,
                } => {
                    if will_retry {
                        tracing::debug!(
                            agent_id = %self.agent_id(),
                            message = %message,
                            info = %info.as_str(),
                            http_status,
                            "codex transient error suppressed (willRetry=true)"
                        );
                        continue;
                    }
                    let info_canon = info.canon();
                    if info_canon == "activeturnnotsteerable" {
                        let rejected = {
                            let mut g = self.state.lock().unwrap();
                            g.promote_rejected_steer_locked()
                        };
                        if let Some(text) = rejected {
                            tracing::info!(
                                agent_id = %self.agent_id(),
                                text_bytes = text.len(),
                                "codex turn/steer rejected, queued for replay on next turn/completed"
                            );
                        }
                    }
                    let is_rate_limit = crate::types::is_rate_limit_message(message)
                        || info_canon == "usagelimitexceeded";
                    out.extend(Vec::<DriverEvent>::from(ev.clone()));
                    if is_rate_limit {
                        // Re-enrich the trailing RateLimit (added by the
                        // pure mapping) with the real snapshot when we
                        // have one. Replace the placeholder.
                        let snap = {
                            let g = self.state.lock().unwrap();
                            g.rate_limits.clone()
                        };
                        if let Some(snap) = snap {
                            if let Some(last) = out.last_mut() {
                                if matches!(last, DriverEvent::RateLimit { .. }) {
                                    *last = crate::conv::event_rate_limit_from_snapshot(&snap);
                                }
                            }
                        }
                    }
                }
                crate::events::CodexEvent::Unknown { ref reason } => {
                    out.extend(self.emit_drift_if_first(reason));
                }
                other => out.extend(Vec::<DriverEvent>::from(other)),
            }
        }
        out
    }

    fn encode_stdin_message(
        &self,
        text: &str,
        _session_id: Option<&str>,
        _mode: MessageMode,
    ) -> Option<String> {
        let (thread_id, active_turn_id, running) = {
            let g = self.state.lock().unwrap();
            (
                g.thread_id.clone(),
                g.active_turn_id.clone(),
                g.handshake == HandshakeState::Running,
            )
        };
        if thread_id.is_empty() || !running {
            // codex.go:1141 — threadID set AND phaseRunning required.
            return None;
        }
        let is_steer = !active_turn_id.is_empty();
        let id = self.next_request_id();
        let encoded = encode_user_message(text, &thread_id, &active_turn_id, id);
        if is_steer {
            tracing::info!(
                agent_id = %self.agent_id(),
                thread_id = %thread_id,
                turn_id = %active_turn_id,
                request_id = id,
                "codex: turn/steer encoded"
            );
            let mut g = self.state.lock().unwrap();
            g.record_inflight_steer_locked(text);
        } else {
            tracing::info!(
                agent_id = %self.agent_id(),
                thread_id = %thread_id,
                request_id = id,
                "codex: turn/start encoded"
            );
        }
        Some(encoded)
    }

    fn supports_turn_cancel(&self) -> bool {
        true
    }

    fn supports_turn_steer(&self) -> bool {
        // codex-process is the ONLY runtime in Phase 2b that supports
        // mid-turn preempt via turn/steer.
        true
    }

    fn supports_thread_fork(&self) -> bool {
        true
    }

    async fn turn_steer(&self, input: &str) -> Result<(), DriverError> {
        let (thread_id, active_turn_id) = {
            let g = self.state.lock().unwrap();
            Self::steer_turn_ids_locked(&g)?
        };
        let id = self.next_request_id();
        let line = encode_turn_steer(input, &thread_id, &active_turn_id, id);
        tracing::info!(
            agent_id = %self.agent_id(),
            thread_id = %thread_id,
            turn_id = %active_turn_id,
            request_id = id,
            "codex: turn/steer encoded"
        );
        {
            let mut g = self.state.lock().unwrap();
            g.record_inflight_steer_locked(input);
        }
        let mut bytes = line.into_bytes();
        bytes.push(b'\n');
        self.write_stdin_bytes(&bytes).await?;
        Ok(())
    }

    async fn fork_thread(&self, thread_id: &str) -> Result<String, DriverError> {
        let requested_thread_id = {
            let g = self.state.lock().unwrap();
            let trimmed = thread_id.trim();
            if trimmed.is_empty() {
                g.thread_id.trim().to_string()
            } else {
                trimmed.to_string()
            }
        };
        if requested_thread_id.is_empty() {
            return Err(DriverError::Other(
                "thread fork unavailable: missing thread id".to_string(),
            ));
        }

        let request_id = self.next_request_id();
        let (tx, rx) = std_mpsc::channel();
        {
            let mut g = self.state.lock().unwrap();
            g.pending_control.insert(request_id, tx);
        }

        let line = encode_thread_fork(&requested_thread_id, request_id);
        let mut bytes = line.into_bytes();
        bytes.push(b'\n');
        if let Err(err) = self.write_stdin_bytes(&bytes).await {
            self.clear_pending_control(request_id);
            return Err(err);
        }

        let response = match tokio::task::spawn_blocking(move || {
            rx.recv_timeout(THREAD_FORK_RESPONSE_TIMEOUT)
        })
        .await
        {
            Ok(Ok(response)) => response,
            Ok(Err(std_mpsc::RecvTimeoutError::Timeout)) => {
                self.clear_pending_control(request_id);
                return Err(DriverError::Other(format!(
                    "thread fork timed out after {}s",
                    THREAD_FORK_RESPONSE_TIMEOUT.as_secs()
                )));
            }
            Ok(Err(std_mpsc::RecvTimeoutError::Disconnected)) => {
                return Err(DriverError::Other(
                    "thread fork response channel closed".to_string(),
                ));
            }
            Err(join_err) => {
                self.clear_pending_control(request_id);
                return Err(DriverError::Other(format!(
                    "thread fork wait task failed: {join_err}"
                )));
            }
        };

        if let Some(message) = response
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|v| v.as_str())
        {
            return Err(DriverError::Other(format!("thread fork failed: {message}")));
        }

        let new_thread_id = response
            .get("result")
            .and_then(thread_id_from_result)
            .ok_or_else(|| {
                DriverError::Other("thread fork response missing thread id".to_string())
            })?;
        {
            let mut g = self.state.lock().unwrap();
            g.thread_id.clone_from(&new_thread_id);
            g.active_turn_id.clear();
            // Pending steer queues belong to the old thread; drop them so
            // replays do not fire against a fresh forked context.
            g.clear_steer_queues_locked();
        }
        Ok(new_thread_id)
    }

    fn skill_search_paths(&self, workspace: &Path) -> Vec<PathBuf> {
        skill_paths_for(workspace)
    }

    async fn probe_skills(
        &self,
        workspace: &Path,
    ) -> Result<Option<cocli_driver_core::NativeSkillProbe>, DriverError> {
        skill_probe::probe_skills(&self.codex_binary, workspace)
            .await
            .map(Some)
    }

    fn as_process_initializer(&self) -> Option<&dyn ProcessInitializer> {
        Some(self)
    }
    fn as_stdin_binder(&self) -> Option<&dyn StdinBinder> {
        Some(self)
    }
    fn as_turn_interruptor(&self) -> Option<&dyn TurnInterruptor> {
        Some(self)
    }

    fn is_turn_exit(&self) -> bool {
        false
    }
}

impl ProcessInitializer for CodexProcessDriver {
    fn write_init_sequence(&self, stdin: &mut dyn std::io::Write) -> std::io::Result<()> {
        // request id=1 is reserved for initialize (constructor set
        // request_id counter starting at 2).
        let line = encode_initialize(1);
        writeln!(stdin, "{line}")?;
        stdin.flush()
    }
}

#[async_trait]
impl StdinBinder for CodexProcessDriver {
    fn bind_stdin(&self, stdin: tokio::process::ChildStdin) {
        *self.stdin.lock().unwrap() = Some(stdin);
        // Arm after stdin is bound (Go sets d.stdin in WriteInitSequence).
        self.arm_handshake_timeout_monitor();
    }
    async fn write_stdin(&self, bytes: &[u8]) -> std::io::Result<()> {
        // Take the stdin out of the std::sync::Mutex so we don't hold
        // the (non-Send) guard across `.await`. Drop the lock before
        // awaiting, then put the stdin back.
        let mut stdin = {
            let mut guard = self.stdin.lock().unwrap();
            guard.take().ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::NotConnected, "stdin not bound")
            })?
        };
        let result = async {
            stdin.write_all(bytes).await?;
            stdin.flush().await?;
            Ok::<(), std::io::Error>(())
        }
        .await;
        // Restore stdin regardless of write success — the next call
        // can retry; only an unrecoverable I/O error from the OS-level
        // pipe being broken would force us to drop it, and that surfaces
        // through subsequent writes anyway.
        *self.stdin.lock().unwrap() = Some(stdin);
        result
    }
}

#[async_trait]
impl TurnInterruptor for CodexProcessDriver {
    async fn interrupt_turn(&self) -> Result<(), DriverError> {
        let (thread_id, active_turn_id) = {
            let g = self.state.lock().unwrap();
            Self::steer_turn_ids_locked(&g)?
        };
        let id = self.next_request_id();
        tracing::info!(
            agent_id = %self.agent_id(),
            thread_id = %thread_id,
            turn_id = %active_turn_id,
            request_id = id,
            "codex: turn/interrupt encoded"
        );
        let line = encode_turn_interrupt(&thread_id, &active_turn_id, id);
        let mut bytes = line.into_bytes();
        bytes.push(b'\n');
        self.write_stdin_bytes(&bytes).await?;
        Ok(())
    }
}

/// Codex's skill paths — workspace-scoped + global, in priority order.
/// Mirrors Go `CodexDriver::SkillSearchPaths` (codex.go:53). The Go
/// version splits into Workspace / Global; we flatten with workspace
/// entries first to preserve precedence.
fn skill_paths_for(workspace: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = vec![
        workspace.join(".codex").join("skills"),
        workspace.join(".agents").join("skills"),
    ];
    if let Some(home) = dirs::home_dir() {
        paths.push(home.join(".codex").join("skills"));
        paths.push(home.join(".codex").join("skills").join(".system"));
        paths.push(home.join(".agents").join("skills"));
    }
    paths
}

/// Extract thread id from a JSON-RPC `result` payload.
/// Mirrors Go `threadIDFromResult` (codex.go:1027).
pub(crate) fn thread_id_from_result(result: &serde_json::Value) -> Option<String> {
    if let Some(tid) = result.get("threadId").and_then(|v| v.as_str()) {
        let tid = tid.trim();
        if !tid.is_empty() {
            return Some(tid.to_string());
        }
    }
    if let Some(tid) = result
        .get("thread")
        .and_then(|t| t.get("id"))
        .and_then(|v| v.as_str())
    {
        let tid = tid.trim();
        if !tid.is_empty() {
            return Some(tid.to_string());
        }
    }
    if let Some(tid) = result
        .get("data")
        .and_then(|d| d.get("thread"))
        .and_then(|t| t.get("id"))
        .and_then(|v| v.as_str())
    {
        let tid = tid.trim();
        if !tid.is_empty() {
            return Some(tid.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::thread_id_from_result;

    #[test]
    fn thread_id_from_result_prefers_thread_id_field() {
        let result = serde_json::json!({"threadId": "thread-from-field"});
        assert_eq!(
            thread_id_from_result(&result).as_deref(),
            Some("thread-from-field")
        );
    }

    #[test]
    fn thread_id_from_result_reads_thread_object_id() {
        let result = serde_json::json!({"thread": {"id": "thread-from-object"}});
        assert_eq!(
            thread_id_from_result(&result).as_deref(),
            Some("thread-from-object")
        );
    }

    #[test]
    fn thread_id_from_result_reads_data_thread_id() {
        let result = serde_json::json!({"data": {"thread": {"id": "thread-from-data"}}});
        assert_eq!(
            thread_id_from_result(&result).as_deref(),
            Some("thread-from-data")
        );
    }

    #[test]
    fn thread_id_from_result_rejects_blank_values() {
        let result = serde_json::json!({"threadId": "   "});
        assert!(thread_id_from_result(&result).is_none());
    }
}
