use std::collections::HashMap;
use std::time::{Duration, Instant};

const PROBE_BACKOFF_SCHEDULE: [Duration; 5] = [
    Duration::from_secs(5 * 60),
    Duration::from_secs(10 * 60),
    Duration::from_secs(30 * 60),
    Duration::from_secs(60 * 60),
    Duration::from_secs(120 * 60),
];

pub fn probe_backoff_for(attempt: u32) -> Duration {
    PROBE_BACKOFF_SCHEDULE
        .get(attempt as usize)
        .copied()
        .unwrap_or(PROBE_BACKOFF_SCHEDULE[PROBE_BACKOFF_SCHEDULE.len() - 1])
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeResult {
    Recovered,
    StillLimited,
    Error,
    NoState,
}

impl ProbeResult {
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::Recovered => "recovered",
            Self::StillLimited => "still_limited",
            Self::Error => "error",
            Self::NoState => "no_state",
        }
    }
}

#[derive(Debug, Clone)]
struct RecoveryEntry {
    provider: String,
    stop_reason: String,
    attempt_count: u32,
    expected_recovery_at: Option<Instant>,
    last_probe_at: Option<Instant>,
    probe_in_flight: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DueProbe {
    pub agent_id: String,
    pub provider: String,
    pub stop_reason: String,
    pub attempt_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryStatus {
    pub provider: String,
    pub stop_reason: String,
    pub attempt_count: u32,
}

#[derive(Debug, Default)]
pub struct RecoveryStore {
    entries: HashMap<String, RecoveryEntry>,
}

impl RecoveryStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(
        &mut self,
        agent_id: impl Into<String>,
        provider: impl Into<String>,
        stop_reason: impl Into<String>,
    ) {
        self.register_with_expected_recovery_at(agent_id, provider, stop_reason, None);
    }

    pub fn register_with_expected_recovery_at(
        &mut self,
        agent_id: impl Into<String>,
        provider: impl Into<String>,
        stop_reason: impl Into<String>,
        expected_recovery_at: Option<Instant>,
    ) {
        let agent_id = agent_id.into();
        if agent_id.trim().is_empty() {
            return;
        }
        let prior = self.entries.get(&agent_id);
        let prior_attempts = prior.map(|entry| entry.attempt_count).unwrap_or_default();
        let last_probe_at = prior.and_then(|entry| entry.last_probe_at);
        let probe_in_flight = prior.map(|entry| entry.probe_in_flight).unwrap_or_default();
        self.entries.insert(
            agent_id,
            RecoveryEntry {
                provider: provider.into(),
                stop_reason: stop_reason.into(),
                attempt_count: prior_attempts,
                expected_recovery_at,
                last_probe_at,
                probe_in_flight,
            },
        );
    }

    pub fn probe_now(&mut self, agent_id: &str) -> (ProbeResult, String) {
        let Some(entry) = self.entries.get_mut(agent_id) else {
            return (ProbeResult::NoState, String::new());
        };
        if entry.probe_in_flight {
            return (
                ProbeResult::StillLimited,
                format!(
                    "recovery probe already in flight for provider={} stopReason={}",
                    entry.provider, entry.stop_reason
                ),
            );
        }
        entry.attempt_count = entry.attempt_count.saturating_add(1);
        entry.last_probe_at = Some(Instant::now());
        (
            ProbeResult::StillLimited,
            format!(
                "recovery probe pending for provider={} stopReason={}",
                entry.provider, entry.stop_reason
            ),
        )
    }

    pub fn clear(&mut self, agent_id: &str) {
        self.entries.remove(agent_id);
    }

    pub fn contains(&self, agent_id: &str) -> bool {
        self.entries.contains_key(agent_id)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn status(&self, agent_id: &str) -> Option<RecoveryStatus> {
        self.entries.get(agent_id).map(|entry| RecoveryStatus {
            provider: entry.provider.clone(),
            stop_reason: entry.stop_reason.clone(),
            attempt_count: entry.attempt_count,
        })
    }

    pub fn due_probes(&mut self, now: Instant) -> Vec<DueProbe> {
        let mut out = Vec::new();
        for (agent_id, entry) in self.entries.iter_mut() {
            if entry.probe_in_flight {
                continue;
            }
            if let Some(expected) = entry.expected_recovery_at {
                if expected > now {
                    continue;
                }
            } else if let Some(last_probe_at) = entry.last_probe_at {
                if now.duration_since(last_probe_at) < probe_backoff_for(entry.attempt_count) {
                    continue;
                }
            }

            entry.probe_in_flight = true;
            out.push(DueProbe {
                agent_id: agent_id.clone(),
                provider: entry.provider.clone(),
                stop_reason: entry.stop_reason.clone(),
                attempt_count: entry.attempt_count,
            });
        }
        out
    }

    pub fn complete_probe(&mut self, agent_id: &str, now: Instant, result: ProbeResult) {
        if result == ProbeResult::Recovered {
            self.entries.remove(agent_id);
            return;
        }

        let Some(entry) = self.entries.get_mut(agent_id) else {
            return;
        };
        entry.probe_in_flight = false;
        if result != ProbeResult::NoState {
            entry.attempt_count = entry.attempt_count.saturating_add(1);
            entry.expected_recovery_at = None;
            entry.last_probe_at = Some(now);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn probe_now_returns_no_state_for_unknown_agent() {
        let mut store = RecoveryStore::new();

        let (result, detail) = store.probe_now("ghost-agent");

        assert_eq!(result, ProbeResult::NoState);
        assert!(detail.is_empty());
    }

    #[test]
    fn probe_now_returns_still_limited_for_registered_agent() {
        let mut store = RecoveryStore::new();
        store.register("agent-1", "gemini", "stopped_by_quota");

        let (result, detail) = store.probe_now("agent-1");

        assert_eq!(result, ProbeResult::StillLimited);
        assert!(detail.contains("gemini"));
        assert!(detail.contains("stopped_by_quota"));
    }

    #[test]
    fn register_preserves_attempt_count_for_same_agent() {
        let mut store = RecoveryStore::new();
        store.register("agent-1", "gemini", "first");
        let _ = store.probe_now("agent-1");
        store.register("agent-1", "gemini", "second");
        let _ = store.probe_now("agent-1");

        assert_eq!(store.entries["agent-1"].attempt_count, 2);
    }

    #[test]
    fn clear_removes_recovery_entry() {
        let mut store = RecoveryStore::new();
        store.register("agent-1", "gemini", "stopped_by_quota");

        store.clear("agent-1");

        let (result, detail) = store.probe_now("agent-1");
        assert_eq!(result, ProbeResult::NoState);
        assert!(detail.is_empty());
    }

    #[test]
    fn status_returns_provider_reason_and_attempt_count() {
        let mut store = RecoveryStore::new();
        store.register("agent-1", "gemini", "stopped_by_quota");
        let _ = store.probe_now("agent-1");

        let status = store.status("agent-1").expect("recovery status");

        assert_eq!(status.provider, "gemini");
        assert_eq!(status.stop_reason, "stopped_by_quota");
        assert_eq!(status.attempt_count, 1);
    }

    #[test]
    fn probe_backoff_for_matches_go_ladder() {
        assert_eq!(probe_backoff_for(0), Duration::from_secs(5 * 60));
        assert_eq!(probe_backoff_for(1), Duration::from_secs(10 * 60));
        assert_eq!(probe_backoff_for(2), Duration::from_secs(30 * 60));
        assert_eq!(probe_backoff_for(3), Duration::from_secs(60 * 60));
        assert_eq!(probe_backoff_for(4), Duration::from_secs(120 * 60));
        assert_eq!(probe_backoff_for(99), Duration::from_secs(120 * 60));
    }

    #[test]
    fn due_probes_respect_expected_recovery_at() {
        let mut store = RecoveryStore::new();
        let now = Instant::now();
        store.register_with_expected_recovery_at(
            "agent-1",
            "gemini",
            "stopped_by_quota",
            Some(now + Duration::from_secs(10 * 60)),
        );

        assert!(store.due_probes(now).is_empty());
        let due = store.due_probes(now + Duration::from_secs(10 * 60));
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].agent_id, "agent-1");
        assert_eq!(due[0].attempt_count, 0);
        assert!(
            store
                .due_probes(now + Duration::from_secs(20 * 60))
                .is_empty(),
            "in-flight probe should suppress duplicate due entries"
        );
    }

    #[test]
    fn failed_probe_completion_advances_backoff_window() {
        let mut store = RecoveryStore::new();
        let now = Instant::now();
        store.register("agent-1", "gemini", "stopped_by_quota");
        let due = store.due_probes(now);
        assert_eq!(due.len(), 1);

        store.complete_probe("agent-1", now, ProbeResult::StillLimited);

        assert!(store
            .due_probes(now + Duration::from_secs(9 * 60))
            .is_empty());
        let due = store.due_probes(now + Duration::from_secs(10 * 60));
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].attempt_count, 1);
    }

    #[test]
    fn recovered_probe_completion_clears_entry() {
        let mut store = RecoveryStore::new();
        let now = Instant::now();
        store.register("agent-1", "gemini", "stopped_by_quota");
        let due = store.due_probes(now);
        assert_eq!(due.len(), 1);

        store.complete_probe("agent-1", now, ProbeResult::Recovered);

        assert!(store
            .due_probes(now + Duration::from_secs(60 * 60))
            .is_empty());
        let (result, detail) = store.probe_now("agent-1");
        assert_eq!(result, ProbeResult::NoState);
        assert!(detail.is_empty());
    }

    #[test]
    fn failed_expected_time_probe_switches_to_backoff_schedule() {
        let mut store = RecoveryStore::new();
        let now = Instant::now();
        store.register_with_expected_recovery_at(
            "agent-1",
            "gemini",
            "stopped_by_quota",
            Some(now),
        );
        assert_eq!(store.due_probes(now).len(), 1);

        store.complete_probe("agent-1", now, ProbeResult::Error);

        assert!(store
            .due_probes(now + Duration::from_secs(9 * 60))
            .is_empty());
        assert_eq!(
            store.due_probes(now + Duration::from_secs(10 * 60)).len(),
            1
        );
    }
}
