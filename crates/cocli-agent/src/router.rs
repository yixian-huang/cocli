//! `AgentRouter` — owns the per-agent mailbox table and the delivery
//! buffer; routes ServerMsg into per-agent `AgentActor` tasks.
//!
//! Spec §5.9. Source-of-truth Go: `daemon/agent/agent_manager.go`.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use tokio::sync::{broadcast, mpsc};

use cocli_actor::{Actor, ActorResult, ShutdownToken};
use cocli_protocol::{
    daemon_msg::AgentStatusMsg,
    server_msg::{
        AgentDeliverMsg, AgentRecoverSessionsMsg, AgentStartMsg, AgentStopMsg, AgentTurnCancelMsg,
    },
    DaemonMsg, ServerMsg,
};

use crate::actor::{AgentActor, StartCfg};
use crate::obs::AgentObservationChanged;
use crate::queue::DeliveryQueue;
use crate::state::Idle;
use crate::types::{AgentCmd, AgentStateChange};
use crate::working::WorkingMemoryStore;

/// Daemon-wide config shared with each agent (paths, server URL, machine API key).
pub struct DaemonConfig {
    pub server_url: String,
    pub machine_id: String,
    pub api_key: String,
    pub claude_binary: std::path::PathBuf,
    pub bridge_binary: std::path::PathBuf,
    pub agent_workspace_root: std::path::PathBuf,
}

pub struct AgentRouter {
    cfg: Arc<DaemonConfig>,
    agents: HashMap<String, mpsc::Sender<AgentCmd>>,
    delivery_queue: DeliveryQueue,
    inbound_rx: mpsc::Receiver<ServerMsg>,
    outbound_tx: mpsc::Sender<DaemonMsg>,
    state_rx: mpsc::Receiver<AgentStateChange>,
    state_tx_template: mpsc::Sender<AgentStateChange>,
    obs_tx: broadcast::Sender<AgentObservationChanged>,
    /// Shared with `ServerConnActor` — read on reconnect for cold/hot
    /// branch decision (per plan Task 10 fix). Updated on every
    /// `Spawned` (insert) and `Stopped` (remove) state-change.
    running_registry: Arc<RwLock<HashSet<String>>>,
    /// Per-agent CurrentWork anchor (FPC #16). In-memory only; cleared on
    /// `AgentStateChange::Stopped` so a fresh process never inherits stale
    /// focus pointers. Mirrors Go `AgentProcess.currentWork` semantics in
    /// `daemon/agent/agent_working_state.go`.
    working: WorkingMemoryStore,
}

impl AgentRouter {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        cfg: Arc<DaemonConfig>,
        inbound_rx: mpsc::Receiver<ServerMsg>,
        outbound_tx: mpsc::Sender<DaemonMsg>,
        state_rx: mpsc::Receiver<AgentStateChange>,
        state_tx_template: mpsc::Sender<AgentStateChange>,
        obs_tx: broadcast::Sender<AgentObservationChanged>,
        running_registry: Arc<RwLock<HashSet<String>>>,
    ) -> Self {
        Self {
            cfg,
            agents: HashMap::new(),
            delivery_queue: DeliveryQueue::new(),
            inbound_rx,
            outbound_tx,
            state_rx,
            state_tx_template,
            obs_tx,
            running_registry,
            working: WorkingMemoryStore::new(),
        }
    }

    /// Snapshot of currently-registered agent IDs.
    pub fn running_agents_snapshot(&self) -> Vec<String> {
        self.agents.keys().cloned().collect()
    }
}

#[async_trait]
impl Actor for AgentRouter {
    fn name(&self) -> &'static str {
        "agent-router"
    }

    async fn run(mut self, mut shutdown: ShutdownToken) -> ActorResult<()> {
        loop {
            tokio::select! {
                Some(msg) = self.inbound_rx.recv() => match msg {
                    ServerMsg::AgentStart(m)             => self.handle_start(m).await,
                    ServerMsg::AgentStop(m)              => self.handle_stop(m).await,
                    ServerMsg::AgentDeliver(m)           => self.handle_deliver(m).await,
                    ServerMsg::AgentTurnCancel(m)        => self.handle_turn_cancel(m).await,
                    ServerMsg::AgentRecoverSessions(m)   => self.handle_recover(m).await,
                    // Phase 0b: workspace ops (FPC #14 + #15)
                    ServerMsg::AgentWorkspaceList(m)     => self.handle_workspace_list(m).await,
                    ServerMsg::AgentWorkspaceRead(m)     => self.handle_workspace_read(m).await,
                    ServerMsg::AgentResetWorkspace(m)    => self.handle_reset_workspace(m).await,
                    // Phase 0b: working-memory ops (FPC #16)
                    ServerMsg::AgentWorkingSet(m)        => self.handle_working_set(m).await,
                    ServerMsg::AgentWorkingGet(m)        => self.handle_working_get(m).await,
                    ServerMsg::AgentWorkingClear(m)      => self.handle_working_clear(m).await,
                    ServerMsg::Ping(_)                   => {} // conn layer handles
                    ServerMsg::ServerShutdown(m)         => self.handle_server_shutdown(m).await,
                    ServerMsg::Unknown                   => tracing::warn!("router: dropped unknown msg"),
                },
                Some(change) = self.state_rx.recv() => self.handle_state_change(change).await,
                _ = shutdown.wait() => break,
            }
        }
        // Graceful stop: best-effort SIGTERM to each agent.
        for (id, tx) in self.agents.drain() {
            let _ = tx.send(AgentCmd::Stop { force: false }).await;
            tracing::info!(agent_id = %id, "router: shutdown requested stop");
        }
        Ok(())
    }
}

impl AgentRouter {
    async fn handle_start(&mut self, m: AgentStartMsg) {
        self.spawn_agent(
            m.agent_id,
            &m.config,
            m.launch_id,
            None, // fresh start, no session to resume
        )
        .await;
    }

    /// Core spawn path shared by `handle_start` (agent:start) and
    /// `handle_recover` (agent:recover-sessions). `resume_session = Some(sid)`
    /// causes the claude child to be spawned with `--resume <sid>`.
    async fn spawn_agent(
        &mut self,
        agent_id: String,
        config: &cocli_protocol::types::AgentConfig,
        launch_id: String,
        resume_session: Option<String>,
    ) {
        if self.agents.contains_key(&agent_id) {
            tracing::warn!(agent_id = %agent_id, "router: agent already running, ignoring spawn");
            return;
        }

        let (cmd_tx, cmd_rx) = mpsc::channel::<AgentCmd>(64);
        self.agents.insert(agent_id.clone(), cmd_tx.clone());

        // Drain any deliveries that arrived before this agent finished
        // starting (race: server's /messages handler can dispatch
        // agent:deliver before /agents/start's goroutine sends agent:start;
        // similarly the post-reconnect backlog flush precedes our spawn).
        let buffered = self.delivery_queue.drain(&agent_id);
        if !buffered.is_empty() {
            tracing::info!(
                agent_id = %agent_id,
                count = buffered.len(),
                "router: draining buffered delivers into starting agent"
            );
            for d in buffered {
                let _ = cmd_tx.send(AgentCmd::Deliver(d)).await;
            }
        }

        // Update the shared registry immediately on registration so
        // ServerConnActor's reconnect path sees this agent as running
        // even before the spawn task fires.
        if let Ok(mut reg) = self.running_registry.write() {
            reg.insert(agent_id.clone());
        }
        // Emit Spawned for observability.
        let _ = self
            .state_tx_template
            .send(AgentStateChange::Spawned {
                agent_id: agent_id.clone(),
            })
            .await;

        let actor = AgentActor::<Idle> {
            id: agent_id.clone(),
            mailbox: cmd_rx,
            outbound: self.outbound_tx.clone(),
            state_tx: self.state_tx_template.clone(),
            obs_tx: self.obs_tx.clone(),
            state: Idle,
        };
        let cfg = StartCfg {
            claude_binary: self.cfg.claude_binary.clone(),
            bridge_binary: self.cfg.bridge_binary.clone(),
            workspace_root: self.cfg.agent_workspace_root.clone(),
            server_url: self.cfg.server_url.clone(),
            // Phase 0a: reuse the machine API key as the per-agent bearer.
            auth_token: self.cfg.api_key.clone(),
            // Phase 0a: AgentStartMsg does not yet carry the channel —
            // the run-loop will not need it until activity emission ships.
            channel_id: uuid::Uuid::nil(),
            channel_name: String::new(),
            model: config.model.clone(),
            launch_id: launch_id.clone(),
            resume_session,
        };

        let outbound = self.outbound_tx.clone();
        let state_tx = self.state_tx_template.clone();
        let id = agent_id;

        tokio::spawn(async move {
            match actor.start(cfg).await {
                Ok(running) => {
                    // Surface "active" status to server.
                    let _ = outbound
                        .send(DaemonMsg::AgentStatus(AgentStatusMsg {
                            agent_id: id.clone(),
                            status: "active".to_string(),
                            launch_id: launch_id.clone(),
                            error_detail: String::new(),
                        }))
                        .await;
                    let _ = state_tx
                        .send(AgentStateChange::Running {
                            agent_id: id.clone(),
                            session_id: running.state.session_id.clone(),
                        })
                        .await;
                    // Drive the running actor: consume mailbox commands +
                    // claude stdout events; emit agent:activity /
                    // agent:turn / agent:session as events arrive. Returns
                    // when the actor is stopped (Stop cmd, mailbox close,
                    // or claude EOF).
                    running.run_loop(launch_id.clone()).await;
                    // Loop exit → fully stopped. Phase 0a: emit Stopped
                    // state-change with a best-effort "manual_stop" reason
                    // (Phase 0b will distinguish idle / crash / context_reset).
                    let _ = state_tx
                        .send(AgentStateChange::Stopped {
                            agent_id: id.clone(),
                            end_reason: "manual_stop".to_string(),
                        })
                        .await;
                }
                Err(e) => {
                    // Start failure: status="error", NO AgentSessionEndMsg.
                    let _ = outbound
                        .send(DaemonMsg::AgentStatus(AgentStatusMsg {
                            agent_id: id.clone(),
                            status: "error".to_string(),
                            launch_id: launch_id.clone(),
                            error_detail: e.to_string(),
                        }))
                        .await;
                    let _ = state_tx
                        .send(AgentStateChange::Stopped {
                            agent_id: id.clone(),
                            end_reason: "error".to_string(),
                        })
                        .await;
                }
            }
        });
    }

    async fn handle_stop(&mut self, m: AgentStopMsg) {
        if let Some(tx) = self.agents.get(&m.agent_id) {
            let _ = tx.send(AgentCmd::Stop { force: m.force }).await;
        } else {
            tracing::warn!(agent_id = %m.agent_id, "router: stop for unknown agent");
        }
    }

    async fn handle_deliver(&mut self, m: AgentDeliverMsg) {
        if let Some(tx) = self.agents.get(&m.agent_id) {
            let agent_id = m.agent_id.clone();
            let seq = m.seq;
            if tx.send(AgentCmd::Deliver(m)).await.is_err() {
                tracing::warn!(
                    agent_id = %agent_id,
                    seq,
                    "router: agent mailbox closed; delivery dropped"
                );
            }
        } else {
            // Agent not yet started (race: server's /messages handler
            // dispatched agent:deliver before /agents/start's goroutine
            // propagated agent:start, OR daemon reconnected and server is
            // flushing backlog before recover-sessions spawn).
            // Buffer in delivery_queue; drain on handle_start.
            tracing::info!(
                agent_id = %m.agent_id,
                seq = m.seq,
                "router: deliver buffered (agent not yet running)"
            );
            let aid = m.agent_id.clone();
            self.delivery_queue.enqueue(&aid, m);
        }
    }

    async fn handle_turn_cancel(&mut self, m: AgentTurnCancelMsg) {
        // FPC #12: forward to the agent actor; the actor SIGINTs the child.
        // Log at info so the e2e harness can verify propagation even when the
        // child is gone (no-op) or the actor hasn't yet spawned (drop).
        if let Some(tx) = self.agents.get(&m.agent_id) {
            tracing::info!(
                agent_id = %m.agent_id,
                "router: forwarding agent:turn:cancel to actor"
            );
            let _ = tx.send(AgentCmd::TurnCancel).await;
        } else {
            tracing::warn!(
                agent_id = %m.agent_id,
                "router: agent:turn:cancel for unknown agent (dropped)"
            );
        }
    }

    async fn handle_recover(&mut self, m: AgentRecoverSessionsMsg) {
        // FPC #8: for each session the server tells us was active on this
        // machine, spawn the agent with `--resume <sid>`. Phase 0a recipe
        // (vs Go agent_recovery.go:65-90):
        //   - log + return on spawn failure (no fallback to fresh start;
        //     no AgentRecoveryRecordMsg — that's a quota-probe path)
        //   - RecoveryGraceUntil + turn_count restoration are Phase 0b
        for sess in m.sessions {
            if self.agents.contains_key(&sess.agent_id) {
                tracing::debug!(
                    agent_id = %sess.agent_id,
                    "router: recover skip — agent already running"
                );
                continue;
            }
            tracing::info!(
                agent_id = %sess.agent_id,
                session = %sess.session_id,
                "router: recover-sessions → spawning with resume"
            );
            // Synthesize a "launch_id" tag for observability; not used by
            // the server side. Empty string is acceptable.
            let launch_id = String::new();
            self.spawn_agent(
                sess.agent_id.clone(),
                &sess.config,
                launch_id,
                Some(sess.session_id.clone()),
            )
            .await;
        }
    }

    async fn handle_state_change(&mut self, change: AgentStateChange) {
        match change {
            AgentStateChange::Spawned { agent_id } => {
                tracing::debug!(agent_id = %agent_id, "router: spawned");
            }
            AgentStateChange::Running {
                agent_id,
                session_id,
            } => {
                tracing::debug!(
                    agent_id = %agent_id,
                    session_id = %session_id,
                    "router: running"
                );
            }
            AgentStateChange::Stopping { agent_id } => {
                tracing::debug!(agent_id = %agent_id, "router: stopping");
            }
            AgentStateChange::Stopped {
                agent_id,
                end_reason,
            } => {
                self.agents.remove(&agent_id);
                self.delivery_queue.forget(&agent_id);
                if let Ok(mut reg) = self.running_registry.write() {
                    reg.remove(&agent_id);
                }
                // FPC #16: drop the CurrentWork anchor so a re-spawned agent
                // never inherits stale focus from the previous incarnation.
                self.working.clear(&agent_id);
                // Emit "inactive" status (Go parity: status enum is
                // "active" | "inactive" | "error", NOT "stopped").
                let _ = self
                    .outbound_tx
                    .send(DaemonMsg::AgentStatus(AgentStatusMsg {
                        agent_id: agent_id.clone(),
                        status: "inactive".to_string(),
                        launch_id: String::new(),
                        error_detail: String::new(),
                    }))
                    .await;
                tracing::info!(
                    agent_id = %agent_id,
                    end_reason = %end_reason,
                    "router: stopped"
                );
            }
        }
    }
}

// ============================================================================
// Phase 0b — stub handlers
// ============================================================================
//
// These methods are added by the pre-flight commit so that subagents implementing
// FPC #14 / #15 / #16 only need to fill in the bodies; the protocol-crate enum
// variants + router dispatch arms are settled (and won't merge-conflict between
// the parallel subagent commits).
//
// FPC #14 + #15: agent workspace ops — owns the agent_workspace_dir(agent_id)
//                helper plus list/read/reset handlers.
// FPC #16:       working-memory anchor (per-agent CurrentWork in router state)
//                + set/get/clear handlers.

impl AgentRouter {
    // FPC #14 — workspace list
    async fn handle_workspace_list(
        &mut self,
        m: cocli_protocol::server_msg::AgentWorkspaceListMsg,
    ) {
        use cocli_protocol::daemon_msg::AgentWorkspaceFileTreeMsg;
        use cocli_protocol::types::FileTreeEntry;

        let work_dir =
            crate::workspace::agent_workspace_dir(&self.cfg.agent_workspace_root, &m.agent_id);

        let dir_path_for_reply = m.dir_path.clone();
        let files: Vec<FileTreeEntry> =
            match crate::workspace::resolve_within(&work_dir, &m.dir_path) {
                Ok(target) => match std::fs::read_dir(target) {
                    Ok(entries) => entries
                        .filter_map(|e| e.ok())
                        .map(|e| {
                            let meta = e.metadata().ok();
                            let is_dir = meta.as_ref().map(|md| md.is_dir()).unwrap_or(false);
                            let size = if is_dir {
                                0
                            } else {
                                meta.as_ref().map(|md| md.len() as i64).unwrap_or(0)
                            };
                            FileTreeEntry {
                                name: e.file_name().to_string_lossy().into_owned(),
                                is_dir,
                                size,
                            }
                        })
                        .collect(),
                    Err(e) => {
                        tracing::warn!(
                            agent_id = %m.agent_id,
                            dir_path = %m.dir_path,
                            error = %e,
                            "router: workspace:list — read_dir failed; returning empty"
                        );
                        Vec::new()
                    }
                },
                Err(e) => {
                    tracing::warn!(
                        agent_id = %m.agent_id,
                        dir_path = %m.dir_path,
                        error = %e,
                        "router: workspace:list — resolve_within rejected; returning empty"
                    );
                    Vec::new()
                }
            };

        let reply = AgentWorkspaceFileTreeMsg {
            agent_id: m.agent_id,
            request_id: m.request_id,
            dir_path: dir_path_for_reply,
            files,
        };
        if let Err(e) = self
            .outbound_tx
            .send(cocli_protocol::DaemonMsg::AgentWorkspaceFileTree(reply))
            .await
        {
            tracing::warn!(error = %e, "router: workspace:list — outbound send failed");
        }
    }

    // FPC #14 — workspace read
    async fn handle_workspace_read(
        &mut self,
        m: cocli_protocol::server_msg::AgentWorkspaceReadMsg,
    ) {
        use base64::{engine::general_purpose::STANDARD, Engine as _};
        use cocli_protocol::daemon_msg::AgentWorkspaceFileContentMsg;

        const MAX_FILE_BYTES: usize = 1024 * 1024; // 1 MiB — Go parity

        let work_dir =
            crate::workspace::agent_workspace_dir(&self.cfg.agent_workspace_root, &m.agent_id);

        let (content, binary): (String, bool) =
            match crate::workspace::resolve_within(&work_dir, &m.path) {
                Err(e) => {
                    tracing::warn!(
                        agent_id = %m.agent_id,
                        path = %m.path,
                        error = %e,
                        "router: workspace:read — resolve_within rejected"
                    );
                    // Go parity: surface "access denied" / "error: ..." inline.
                    if e.kind() == std::io::ErrorKind::PermissionDenied {
                        ("access denied".to_string(), false)
                    } else {
                        (format!("error: {e}"), false)
                    }
                }
                Ok(target) => match std::fs::read(target) {
                    Err(e) => {
                        tracing::warn!(
                            agent_id = %m.agent_id,
                            path = %m.path,
                            error = %e,
                            "router: workspace:read — read failed"
                        );
                        (format!("error: {e}"), false)
                    }
                    Ok(data) if data.len() > MAX_FILE_BYTES => {
                        // Go parity: refuse files > 1 MiB.
                        ("file too large (>1MB)".to_string(), true)
                    }
                    Ok(data) => {
                        // Go parity: NUL-byte check on the first 512 bytes for
                        // binary detection (matches `bytes.Contains(data[:512],
                        // []byte{0})`). Rust extension: if NUL-free but still
                        // not valid UTF-8, base64-encode as a safety fallback.
                        let check_len = data.len().min(512);
                        let has_nul = data[..check_len].contains(&0);
                        if has_nul {
                            (STANDARD.encode(&data), true)
                        } else {
                            match String::from_utf8(data) {
                                Ok(s) => (s, false),
                                Err(e) => {
                                    // Not valid UTF-8 but had no NULs in the
                                    // first 512 bytes — fall back to base64.
                                    (STANDARD.encode(e.into_bytes()), true)
                                }
                            }
                        }
                    }
                },
            };

        let reply = AgentWorkspaceFileContentMsg {
            agent_id: m.agent_id,
            request_id: m.request_id,
            content,
            binary,
        };
        if let Err(e) = self
            .outbound_tx
            .send(cocli_protocol::DaemonMsg::AgentWorkspaceFileContent(reply))
            .await
        {
            tracing::warn!(error = %e, "router: workspace:read — outbound send failed");
        }
    }

    // FPC #15 — reset workspace
    async fn handle_reset_workspace(
        &mut self,
        m: cocli_protocol::server_msg::AgentResetWorkspaceMsg,
    ) {
        let work_dir =
            crate::workspace::agent_workspace_dir(&self.cfg.agent_workspace_root, &m.agent_id);

        if !work_dir.exists() {
            tracing::warn!(
                agent_id = %m.agent_id,
                work_dir = %work_dir.display(),
                "router: reset-workspace — workspace dir does not exist; nothing to clear"
            );
            return;
        }

        match crate::workspace::clear_dir_contents(&work_dir) {
            Ok(()) => {
                tracing::info!(
                    agent_id = %m.agent_id,
                    work_dir = %work_dir.display(),
                    "router: reset-workspace — cleared all entries"
                );
            }
            Err(e) => {
                tracing::warn!(
                    agent_id = %m.agent_id,
                    work_dir = %work_dir.display(),
                    error = %e,
                    "router: reset-workspace — clear_dir_contents failed"
                );
            }
        }
    }

    // FPC #16 — working memory set.
    //
    // Mirrors Go `dispatcher.go` AgentWorkingSetMsg branch + AgentManager.SetAgentWorkingState.
    // Phase 0a: server pre-trims/clamps the strings, so we just store and reply.
    async fn handle_working_set(&mut self, m: cocli_protocol::server_msg::AgentWorkingSetMsg) {
        use cocli_protocol::types::WorkingStatePayload;
        let incoming = WorkingStatePayload {
            task_id: m.task_id,
            task_number: m.task_number,
            channel_name: m.channel_name,
            summary: m.summary,
            next_step_hint: m.next_step_hint,
            // started_at / last_updated_at are filled by the store.
            started_at: String::new(),
            last_updated_at: String::new(),
        };
        let stored = self.working.set(&m.agent_id, incoming);
        tracing::info!(
            agent_id = %m.agent_id,
            request_id = %m.request_id,
            summary = %stored.summary,
            "router: working:set stored"
        );
        let resp = cocli_protocol::daemon_msg::AgentWorkingResultMsg {
            agent_id: m.agent_id,
            request_id: m.request_id,
            op: "set".to_string(),
            state: Some(stored),
            error: String::new(),
            error_code: String::new(),
        };
        if let Err(e) = self
            .outbound_tx
            .send(DaemonMsg::AgentWorkingResult(resp))
            .await
        {
            tracing::warn!(error = %e, "router: working:set reply send failed");
        }
    }

    // FPC #16 — working memory get.
    async fn handle_working_get(&mut self, m: cocli_protocol::server_msg::AgentWorkingGetMsg) {
        let state = self.working.get(&m.agent_id);
        tracing::info!(
            agent_id = %m.agent_id,
            request_id = %m.request_id,
            has_state = state.is_some(),
            "router: working:get"
        );
        let resp = cocli_protocol::daemon_msg::AgentWorkingResultMsg {
            agent_id: m.agent_id,
            request_id: m.request_id,
            op: "get".to_string(),
            state,
            error: String::new(),
            error_code: String::new(),
        };
        if let Err(e) = self
            .outbound_tx
            .send(DaemonMsg::AgentWorkingResult(resp))
            .await
        {
            tracing::warn!(error = %e, "router: working:get reply send failed");
        }
    }

    // FPC #16 — working memory clear (idempotent).
    async fn handle_working_clear(&mut self, m: cocli_protocol::server_msg::AgentWorkingClearMsg) {
        self.working.clear(&m.agent_id);
        tracing::info!(
            agent_id = %m.agent_id,
            request_id = %m.request_id,
            "router: working:clear"
        );
        let resp = cocli_protocol::daemon_msg::AgentWorkingResultMsg {
            agent_id: m.agent_id,
            request_id: m.request_id,
            op: "clear".to_string(),
            state: None,
            error: String::new(),
            error_code: String::new(),
        };
        if let Err(e) = self
            .outbound_tx
            .send(DaemonMsg::AgentWorkingResult(resp))
            .await
        {
            tracing::warn!(error = %e, "router: working:clear reply send failed");
        }
    }

    // FPC #22 — server:shutdown handler.
    //
    // Server is going away (deploy / restart). Per spec §2.3 FPC #22:
    //   - pause outbound delivery (handled implicitly: WS close → send Err
    //     → ServerConnActor reconnect loop holds outbound msgs in the
    //     mpsc buffer; on reconnect they flush)
    //   - wait for in-flight ack (Phase 0a fire-and-forget; lost acks
    //     are re-tried by server's delivery queue)
    //   - close WS (server initiates close frame; ServerConnActor sees
    //     `Close` frame, exits session, reconnect loop with backoff)
    //   - reconnect when server comes back (existing ServerConnActor logic)
    //
    // The KEY invariant: agents stay alive (claude children keep running),
    // the daemon process does not exit, and on reconnect the hot-path
    // branch in on_connect re-announces them via agent:status{active}
    // (FPC #9, already verified).
    //
    // Earlier code did `break` here, which exited the router loop and
    // drained every agent — defeating the whole point. Now we just log
    // and continue.
    async fn handle_server_shutdown(&mut self, m: cocli_protocol::server_msg::ServerShutdownMsg) {
        tracing::info!(
            reason = %m.reason,
            agents = self.agents.len(),
            "router: server announced shutdown; keeping agents alive, awaiting WS reconnect"
        );
        // Phase 0b polish: nothing else to do here. The conn layer will
        // see the close frame next and drive the reconnect.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_cfg() -> Arc<DaemonConfig> {
        Arc::new(DaemonConfig {
            server_url: "http://localhost:8080".to_string(),
            machine_id: "m1".to_string(),
            api_key: "k1".to_string(),
            claude_binary: std::path::PathBuf::from("/bin/false"),
            bridge_binary: std::path::PathBuf::from("/bin/false"),
            agent_workspace_root: std::path::PathBuf::from("/tmp/agent-test"),
        })
    }

    #[test]
    fn snapshot_is_empty_initially() {
        let (_inbound_tx, inbound_rx) = mpsc::channel(1);
        let (outbound_tx, _outbound_rx) = mpsc::channel(1);
        let (state_tx, state_rx) = mpsc::channel(1);
        let (obs_tx, _obs_rx) = broadcast::channel(1);
        let reg = Arc::new(RwLock::new(HashSet::new()));
        let r = AgentRouter::new(
            make_cfg(),
            inbound_rx,
            outbound_tx,
            state_rx,
            state_tx,
            obs_tx,
            reg,
        );
        assert!(r.running_agents_snapshot().is_empty());
    }
}
