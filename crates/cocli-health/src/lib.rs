//! `HealthActor` — idle detection for running agents.
//!
//! Subscribes to `AgentObservationChanged` events on a broadcast channel
//! (multi-consumer; `AgentRouter` also listens). Tracks per-agent
//! `last_stdout_at`; when `now - last_stdout_at > idle_threshold` emits
//! `DaemonMsg::AgentSessionIdle` to the outbound queue.
//!
//! Spec reference: §5.10 of `docs/superpowers/specs/2026-05-21-rust-daemon-phase0a.md`.
//!
//! Source-of-truth Go file: `daemon/agent/agent_health.go`.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tokio::sync::{broadcast, mpsc};
use uuid::Uuid;

use cocli_actor::{Actor, ActorResult, ShutdownToken};
use cocli_agent::AgentObservationChanged;
use cocli_protocol::{daemon_msg::AgentSessionIdleMsg, DaemonMsg};

#[derive(Debug)]
struct Observation {
    last_stdout_at: Instant,
    session_id: String,
    channel_id: Uuid,
    channel_name: String,
    turn_count: i32,
    already_idle: bool,
}

/// Idle-detection actor. Builds an in-memory observation table from the
/// `AgentObservationChanged` broadcast bus and ticks every `tick_interval` to
/// emit `AgentSessionIdle` for any agent that has crossed `idle_threshold`.
pub struct HealthActor {
    pub obs_rx: broadcast::Receiver<AgentObservationChanged>,
    pub outbound: mpsc::Sender<DaemonMsg>,
    pub idle_threshold: Duration,
    pub tick_interval: Duration,
    running: HashMap<String, Observation>,
}

impl HealthActor {
    /// Defaults: `tick_interval = 60s` (prod). Override via the public field
    /// for tests (the integration test uses `50ms`).
    pub fn new(
        obs_rx: broadcast::Receiver<AgentObservationChanged>,
        outbound: mpsc::Sender<DaemonMsg>,
        idle_threshold: Duration,
    ) -> Self {
        Self {
            obs_rx,
            outbound,
            idle_threshold,
            tick_interval: Duration::from_secs(60),
            running: HashMap::new(),
        }
    }
}

#[async_trait]
impl Actor for HealthActor {
    fn name(&self) -> &'static str {
        "health"
    }

    async fn run(mut self, mut shutdown: ShutdownToken) -> ActorResult<()> {
        let mut ticker = tokio::time::interval(self.tick_interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        tracing::debug!(
            tick = ?self.tick_interval,
            idle = ?self.idle_threshold,
            "health: started"
        );
        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    tracing::debug!(running = self.running.len(), "health: tick");
                    self.check_idle().await;
                }
                rec = self.obs_rx.recv() => match rec {
                    Ok(ev) => {
                        tracing::debug!(?ev, "health: obs");
                        self.apply(ev);
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(skipped = n, "health obs lagged");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                },
                _ = shutdown.wait() => break,
            }
        }
        Ok(())
    }
}

impl HealthActor {
    fn apply(&mut self, ev: AgentObservationChanged) {
        match ev {
            AgentObservationChanged::Started {
                agent_id,
                session_id,
                channel_id,
                channel_name,
            } => {
                self.running.insert(
                    agent_id,
                    Observation {
                        last_stdout_at: Instant::now(),
                        session_id,
                        channel_id,
                        channel_name,
                        turn_count: 0,
                        already_idle: false,
                    },
                );
            }
            AgentObservationChanged::StdoutSeen { agent_id } => {
                if let Some(o) = self.running.get_mut(&agent_id) {
                    o.last_stdout_at = Instant::now();
                    o.already_idle = false;
                }
            }
            AgentObservationChanged::TurnEnded {
                agent_id,
                turn_count,
            } => {
                if let Some(o) = self.running.get_mut(&agent_id) {
                    // turn_count is u64 from obs; AgentSessionIdleMsg is i32.
                    // Saturate on overflow (no practical concern, but keep
                    // clippy quiet under -D warnings).
                    o.turn_count = i32::try_from(turn_count).unwrap_or(i32::MAX);
                }
            }
            AgentObservationChanged::Stopped { agent_id } => {
                self.running.remove(&agent_id);
            }
        }
    }

    async fn check_idle(&mut self) {
        let now = Instant::now();
        let threshold = self.idle_threshold;
        let mut to_emit = Vec::new();
        for (id, o) in self.running.iter_mut() {
            let age = now.duration_since(o.last_stdout_at);
            tracing::debug!(
                agent_id = %id,
                age_ms = age.as_millis() as u64,
                already_idle = o.already_idle,
                "health: check_idle"
            );
            if !o.already_idle && age > threshold {
                o.already_idle = true;
                to_emit.push(AgentSessionIdleMsg {
                    agent_id: id.clone(),
                    channel_id: o.channel_id,
                    channel_name: o.channel_name.clone(),
                    session_id: o.session_id.clone(),
                    turn_count: o.turn_count,
                    total_cost_usd: 0.0,
                    cache_ttl_seconds: 0,
                    active_sessions: 1,
                });
            }
        }
        for m in to_emit {
            if let Err(e) = self.outbound.send(DaemonMsg::AgentSessionIdle(m)).await {
                tracing::warn!(error = %e, "failed to send AgentSessionIdle (outbound closed?)");
            }
        }
    }
}
