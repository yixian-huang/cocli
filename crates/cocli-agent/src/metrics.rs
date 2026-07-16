use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Default)]
pub struct AgentMetrics {
    delivery_queue_buffered_total: AtomicU64,
    delivery_queue_updated_total: AtomicU64,
    delivery_queue_rejected_total: AtomicU64,
    delivery_queue_flush_sent_total: AtomicU64,
    delivery_queue_rebuffered_total: AtomicU64,
    delivery_queue_depth: AtomicU64,
    turn_exit_queued_total: AtomicU64,
    turn_exit_duplicate_total: AtomicU64,
    turn_exit_respawn_total: AtomicU64,
    turn_exit_respawn_failure_total: AtomicU64,
    turn_exit_pending_depth: AtomicU64,
    recovery_probe_scheduled_total: AtomicU64,
    recovery_probe_recovered_total: AtomicU64,
    recovery_probe_error_total: AtomicU64,
    recovery_tracked_agents: AtomicU64,
    local_session_started_total: AtomicU64,
    local_session_reused_total: AtomicU64,
    local_session_stopped_total: AtomicU64,
    local_turn_started_total: AtomicU64,
    local_turn_completed_total: AtomicU64,
    local_turn_failed_total: AtomicU64,
    local_turn_cancelled_total: AtomicU64,
    local_turn_timed_out_total: AtomicU64,
    local_active_sessions: AtomicU64,
    local_active_turns: AtomicU64,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct AgentMetricsSnapshot {
    pub counters: BTreeMap<String, i64>,
    pub gauges: BTreeMap<String, f64>,
}

impl AgentMetrics {
    pub fn inc_delivery_queue_buffered(&self) {
        self.delivery_queue_buffered_total
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_delivery_queue_updated(&self) {
        self.delivery_queue_updated_total
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_delivery_queue_rejected(&self) {
        self.delivery_queue_rejected_total
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn add_delivery_queue_flush_sent(&self, count: usize) {
        self.delivery_queue_flush_sent_total
            .fetch_add(count as u64, Ordering::Relaxed);
    }

    pub fn add_delivery_queue_rebuffered(&self, count: usize) {
        self.delivery_queue_rebuffered_total
            .fetch_add(count as u64, Ordering::Relaxed);
    }

    pub fn set_delivery_queue_depth(&self, depth: usize) {
        self.delivery_queue_depth
            .store(depth as u64, Ordering::Relaxed);
    }

    pub fn inc_turn_exit_queued(&self) {
        self.turn_exit_queued_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_turn_exit_duplicate(&self) {
        self.turn_exit_duplicate_total
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_turn_exit_respawn(&self) {
        self.turn_exit_respawn_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_turn_exit_respawn_failure(&self) {
        self.turn_exit_respawn_failure_total
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn set_turn_exit_pending_depth(&self, depth: usize) {
        self.turn_exit_pending_depth
            .store(depth as u64, Ordering::Relaxed);
    }

    pub fn inc_recovery_probe_scheduled(&self) {
        self.recovery_probe_scheduled_total
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_recovery_probe_recovered(&self) {
        self.recovery_probe_recovered_total
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_recovery_probe_error(&self) {
        self.recovery_probe_error_total
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn set_recovery_tracked_agents(&self, count: usize) {
        self.recovery_tracked_agents
            .store(count as u64, Ordering::Relaxed);
    }

    pub fn inc_local_session_started(&self) {
        self.local_session_started_total
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_local_session_reused(&self) {
        self.local_session_reused_total
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_local_session_stopped(&self) {
        self.local_session_stopped_total
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_local_turn_started(&self) {
        self.local_turn_started_total
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_local_turn_completed(&self) {
        self.local_turn_completed_total
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_local_turn_failed(&self) {
        self.local_turn_failed_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_local_turn_cancelled(&self) {
        self.local_turn_cancelled_total
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_local_turn_timed_out(&self) {
        self.local_turn_timed_out_total
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_local_active_sessions(&self) {
        self.local_active_sessions.fetch_add(1, Ordering::Relaxed);
    }

    pub fn dec_local_active_sessions(&self) {
        saturating_decrement(&self.local_active_sessions);
    }

    pub fn inc_local_active_turns(&self) {
        self.local_active_turns.fetch_add(1, Ordering::Relaxed);
    }

    pub fn dec_local_active_turns(&self) {
        saturating_decrement(&self.local_active_turns);
    }

    pub fn snapshot(&self) -> AgentMetricsSnapshot {
        let mut counters = BTreeMap::new();
        counters.insert(
            "agent_delivery_queue_buffered_total".to_string(),
            saturating_i64(self.delivery_queue_buffered_total.load(Ordering::Relaxed)),
        );
        counters.insert(
            "agent_delivery_queue_updated_total".to_string(),
            saturating_i64(self.delivery_queue_updated_total.load(Ordering::Relaxed)),
        );
        counters.insert(
            "agent_delivery_queue_rejected_total".to_string(),
            saturating_i64(self.delivery_queue_rejected_total.load(Ordering::Relaxed)),
        );
        counters.insert(
            "agent_delivery_queue_flush_sent_total".to_string(),
            saturating_i64(self.delivery_queue_flush_sent_total.load(Ordering::Relaxed)),
        );
        counters.insert(
            "agent_delivery_queue_rebuffered_total".to_string(),
            saturating_i64(self.delivery_queue_rebuffered_total.load(Ordering::Relaxed)),
        );
        counters.insert(
            "agent_turn_exit_queued_total".to_string(),
            saturating_i64(self.turn_exit_queued_total.load(Ordering::Relaxed)),
        );
        counters.insert(
            "agent_turn_exit_duplicate_total".to_string(),
            saturating_i64(self.turn_exit_duplicate_total.load(Ordering::Relaxed)),
        );
        counters.insert(
            "agent_turn_exit_respawn_total".to_string(),
            saturating_i64(self.turn_exit_respawn_total.load(Ordering::Relaxed)),
        );
        counters.insert(
            "agent_turn_exit_respawn_failure_total".to_string(),
            saturating_i64(self.turn_exit_respawn_failure_total.load(Ordering::Relaxed)),
        );
        counters.insert(
            "agent_recovery_probe_scheduled_total".to_string(),
            saturating_i64(self.recovery_probe_scheduled_total.load(Ordering::Relaxed)),
        );
        counters.insert(
            "agent_recovery_probe_recovered_total".to_string(),
            saturating_i64(self.recovery_probe_recovered_total.load(Ordering::Relaxed)),
        );
        counters.insert(
            "agent_recovery_probe_error_total".to_string(),
            saturating_i64(self.recovery_probe_error_total.load(Ordering::Relaxed)),
        );
        counters.insert(
            "local_agent_session_started_total".to_string(),
            saturating_i64(self.local_session_started_total.load(Ordering::Relaxed)),
        );
        counters.insert(
            "local_agent_session_reused_total".to_string(),
            saturating_i64(self.local_session_reused_total.load(Ordering::Relaxed)),
        );
        counters.insert(
            "local_agent_session_stopped_total".to_string(),
            saturating_i64(self.local_session_stopped_total.load(Ordering::Relaxed)),
        );
        counters.insert(
            "local_agent_turn_started_total".to_string(),
            saturating_i64(self.local_turn_started_total.load(Ordering::Relaxed)),
        );
        counters.insert(
            "local_agent_turn_completed_total".to_string(),
            saturating_i64(self.local_turn_completed_total.load(Ordering::Relaxed)),
        );
        counters.insert(
            "local_agent_turn_failed_total".to_string(),
            saturating_i64(self.local_turn_failed_total.load(Ordering::Relaxed)),
        );
        counters.insert(
            "local_agent_turn_cancelled_total".to_string(),
            saturating_i64(self.local_turn_cancelled_total.load(Ordering::Relaxed)),
        );
        counters.insert(
            "local_agent_turn_timed_out_total".to_string(),
            saturating_i64(self.local_turn_timed_out_total.load(Ordering::Relaxed)),
        );

        let mut gauges = BTreeMap::new();
        gauges.insert(
            "agent_delivery_queue_depth".to_string(),
            self.delivery_queue_depth.load(Ordering::Relaxed) as f64,
        );
        gauges.insert(
            "agent_turn_exit_pending_depth".to_string(),
            self.turn_exit_pending_depth.load(Ordering::Relaxed) as f64,
        );
        gauges.insert(
            "agent_recovery_tracked_agents".to_string(),
            self.recovery_tracked_agents.load(Ordering::Relaxed) as f64,
        );
        gauges.insert(
            "local_agent_active_sessions".to_string(),
            self.local_active_sessions.load(Ordering::Relaxed) as f64,
        );
        gauges.insert(
            "local_agent_active_turns".to_string(),
            self.local_active_turns.load(Ordering::Relaxed) as f64,
        );

        AgentMetricsSnapshot { counters, gauges }
    }
}

fn saturating_i64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn saturating_decrement(value: &AtomicU64) {
    let _ = value.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
        Some(current.saturating_sub(1))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_exposes_queue_turn_exit_and_recovery_metrics() {
        let metrics = AgentMetrics::default();
        metrics.inc_delivery_queue_buffered();
        metrics.inc_delivery_queue_updated();
        metrics.inc_delivery_queue_rejected();
        metrics.add_delivery_queue_flush_sent(3);
        metrics.add_delivery_queue_rebuffered(2);
        metrics.set_delivery_queue_depth(5);
        metrics.inc_turn_exit_queued();
        metrics.inc_turn_exit_duplicate();
        metrics.inc_turn_exit_respawn();
        metrics.inc_turn_exit_respawn_failure();
        metrics.set_turn_exit_pending_depth(7);
        metrics.inc_recovery_probe_scheduled();
        metrics.inc_recovery_probe_recovered();
        metrics.inc_recovery_probe_error();
        metrics.set_recovery_tracked_agents(11);
        metrics.inc_local_session_started();
        metrics.inc_local_session_reused();
        metrics.inc_local_session_stopped();
        metrics.inc_local_turn_started();
        metrics.inc_local_turn_completed();
        metrics.inc_local_turn_failed();
        metrics.inc_local_turn_cancelled();
        metrics.inc_local_turn_timed_out();
        metrics.inc_local_active_sessions();
        metrics.inc_local_active_turns();

        let snap = metrics.snapshot();

        assert_eq!(snap.counters["agent_delivery_queue_buffered_total"], 1);
        assert_eq!(snap.counters["agent_delivery_queue_updated_total"], 1);
        assert_eq!(snap.counters["agent_delivery_queue_rejected_total"], 1);
        assert_eq!(snap.counters["agent_delivery_queue_flush_sent_total"], 3);
        assert_eq!(snap.counters["agent_delivery_queue_rebuffered_total"], 2);
        assert_eq!(snap.gauges["agent_delivery_queue_depth"], 5.0);
        assert_eq!(snap.counters["agent_turn_exit_queued_total"], 1);
        assert_eq!(snap.counters["agent_turn_exit_duplicate_total"], 1);
        assert_eq!(snap.counters["agent_turn_exit_respawn_total"], 1);
        assert_eq!(snap.counters["agent_turn_exit_respawn_failure_total"], 1);
        assert_eq!(snap.gauges["agent_turn_exit_pending_depth"], 7.0);
        assert_eq!(snap.counters["agent_recovery_probe_scheduled_total"], 1);
        assert_eq!(snap.counters["agent_recovery_probe_recovered_total"], 1);
        assert_eq!(snap.counters["agent_recovery_probe_error_total"], 1);
        assert_eq!(snap.gauges["agent_recovery_tracked_agents"], 11.0);
        assert_eq!(snap.counters["local_agent_session_started_total"], 1);
        assert_eq!(snap.counters["local_agent_session_reused_total"], 1);
        assert_eq!(snap.counters["local_agent_session_stopped_total"], 1);
        assert_eq!(snap.counters["local_agent_turn_started_total"], 1);
        assert_eq!(snap.counters["local_agent_turn_completed_total"], 1);
        assert_eq!(snap.counters["local_agent_turn_failed_total"], 1);
        assert_eq!(snap.counters["local_agent_turn_cancelled_total"], 1);
        assert_eq!(snap.counters["local_agent_turn_timed_out_total"], 1);
        assert_eq!(snap.gauges["local_agent_active_sessions"], 1.0);
        assert_eq!(snap.gauges["local_agent_active_turns"], 1.0);

        metrics.dec_local_active_sessions();
        metrics.dec_local_active_sessions();
        metrics.dec_local_active_turns();
        metrics.dec_local_active_turns();
        let snap = metrics.snapshot();
        assert_eq!(snap.gauges["local_agent_active_sessions"], 0.0);
        assert_eq!(snap.gauges["local_agent_active_turns"], 0.0);
    }
}
