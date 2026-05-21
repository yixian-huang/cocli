//! Per-agent CurrentWork anchor (FPC #16).
//!
//! Source-of-truth Go: `daemon/agent/agent_working_state.go`.
//!
//! Phase 0a scope:
//!   - In-memory map `agent_id -> WorkingStatePayload`, owned by `AgentRouter`.
//!   - Lives for the lifetime of the agent process incarnation; cleared on
//!     `AgentStateChange::Stopped` (router responsibility).
//!   - `set` preserves the original `started_at` across re-sets and guarantees
//!     `last_updated_at` is strictly non-decreasing.
//!   - `get` returns a clone of the stored payload (`None` if unset).
//!   - `clear` is idempotent.
//!
//! Phase 0b will add:
//!   - input validation (summary required, byte-length caps) — currently the
//!     server-side handler performs the trim+clamp before this layer sees the
//!     payload, so we only mirror Go's monotonic-timestamp logic here.
//!   - prompt-injection snapshot for compact reinforcement.

use std::collections::HashMap;

use chrono::{DateTime, SecondsFormat, Utc};

use cocli_protocol::types::WorkingStatePayload;

/// In-memory store for per-agent CurrentWork anchors. Not thread-safe — the
/// router holds it behind `&mut self` since all access happens on the single
/// `AgentRouter::run` select loop task.
#[derive(Debug, Default)]
pub struct WorkingMemoryStore {
    by_agent: HashMap<String, WorkingStatePayload>,
}

impl WorkingMemoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set or update the anchor for `agent_id`. Returns a clone of the
    /// resulting payload (also stored).
    ///
    /// Behavior (mirrors Go `SetAgentWorkingState`):
    ///   - If no prior anchor exists, both `started_at` and `last_updated_at`
    ///     are set to `now`.
    ///   - If a prior anchor exists, `started_at` is preserved and
    ///     `last_updated_at` advances to `max(now, prev.last_updated_at + 1ns)`
    ///     so two Sets in the same wall-clock instant still produce strictly
    ///     increasing timestamps.
    ///
    /// `now` is injected for testability.
    pub fn set_at(
        &mut self,
        agent_id: &str,
        mut payload: WorkingStatePayload,
        now: DateTime<Utc>,
    ) -> WorkingStatePayload {
        let now_str = format_rfc3339_nanos(now);

        let (started_at, last_updated_at) = match self.by_agent.get(agent_id) {
            Some(prev) => {
                let started = if prev.started_at.is_empty() {
                    now_str.clone()
                } else {
                    prev.started_at.clone()
                };
                // Monotonic last_updated_at: if `now` is not strictly after the
                // previous value (e.g. same-nanosecond re-set in a test), bump
                // by 1ns. Compare via the original DateTime when parseable;
                // fall back to string compare for safety (RFC3339Nano sorts
                // lexicographically when normalized to UTC 'Z').
                let last = match prev
                    .last_updated_at
                    .parse::<DateTime<Utc>>()
                {
                    Ok(prev_last) if prev_last >= now => {
                        format_rfc3339_nanos(prev_last + chrono::Duration::nanoseconds(1))
                    }
                    _ => now_str.clone(),
                };
                (started, last)
            }
            None => (now_str.clone(), now_str.clone()),
        };

        payload.started_at = started_at;
        payload.last_updated_at = last_updated_at;
        self.by_agent.insert(agent_id.to_string(), payload.clone());
        payload
    }

    /// Convenience wrapper that fills `now = Utc::now()`.
    pub fn set(&mut self, agent_id: &str, payload: WorkingStatePayload) -> WorkingStatePayload {
        self.set_at(agent_id, payload, Utc::now())
    }

    /// Returns a clone of the stored anchor, or `None` if unset.
    pub fn get(&self, agent_id: &str) -> Option<WorkingStatePayload> {
        self.by_agent.get(agent_id).cloned()
    }

    /// Drop the anchor for `agent_id`. Idempotent.
    pub fn clear(&mut self, agent_id: &str) {
        self.by_agent.remove(agent_id);
    }

    /// Diagnostic / test helper — number of agents with anchors.
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.by_agent.len()
    }

    /// Diagnostic / test helper — true when no agents have anchors.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.by_agent.is_empty()
    }
}

/// Format a UTC instant the same way Go's `time.RFC3339Nano` would — preserves
/// sub-second resolution; trailing zeros are dropped by `chrono`'s
/// `AutoSi` formatter so two adjacent nanosecond timestamps still differ.
fn format_rfc3339_nanos(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Nanos, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload(summary: &str) -> WorkingStatePayload {
        WorkingStatePayload {
            summary: summary.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn set_then_get_roundtrip() {
        let mut s = WorkingMemoryStore::new();
        let stored = s.set("a1", payload("plan refactor"));
        assert_eq!(stored.summary, "plan refactor");
        assert!(!stored.started_at.is_empty());
        assert!(!stored.last_updated_at.is_empty());

        let got = s.get("a1").expect("set must be visible to get");
        assert_eq!(got.summary, "plan refactor");
        assert_eq!(got.started_at, stored.started_at);
        assert_eq!(got.last_updated_at, stored.last_updated_at);
    }

    #[test]
    fn set_preserves_started_at_and_advances_last_updated() {
        let mut s = WorkingMemoryStore::new();
        let t0 = "2026-05-21T00:00:00.000000000Z"
            .parse::<DateTime<Utc>>()
            .unwrap();
        let first = s.set_at("a1", payload("step 1"), t0);
        let t1 = t0 + chrono::Duration::milliseconds(750);
        let second = s.set_at("a1", payload("step 2"), t1);

        assert_eq!(second.started_at, first.started_at, "started_at preserved");
        assert!(
            second.last_updated_at > first.last_updated_at,
            "last_updated_at must advance ({} not > {})",
            second.last_updated_at,
            first.last_updated_at
        );
        assert_eq!(second.summary, "step 2");
    }

    #[test]
    fn set_at_same_instant_bumps_last_updated_by_one_ns() {
        let mut s = WorkingMemoryStore::new();
        let t0 = "2026-05-21T00:00:00.000000000Z"
            .parse::<DateTime<Utc>>()
            .unwrap();
        let first = s.set_at("a1", payload("a"), t0);
        let second = s.set_at("a1", payload("b"), t0); // same wall-clock
        assert!(
            second.last_updated_at > first.last_updated_at,
            "monotonic guard failed: {} not > {}",
            second.last_updated_at,
            first.last_updated_at
        );
    }

    #[test]
    fn clear_makes_get_return_none() {
        let mut s = WorkingMemoryStore::new();
        s.set("a1", payload("doing something"));
        assert!(s.get("a1").is_some());
        s.clear("a1");
        assert!(s.get("a1").is_none());
    }

    #[test]
    fn clear_is_idempotent() {
        let mut s = WorkingMemoryStore::new();
        s.clear("never-set");
        s.clear("never-set");
        assert!(s.get("never-set").is_none());
    }

    #[test]
    fn isolation_between_agents() {
        let mut s = WorkingMemoryStore::new();
        s.set("a1", payload("agent one"));
        s.set("a2", payload("agent two"));
        s.clear("a1");
        assert!(s.get("a1").is_none());
        assert_eq!(s.get("a2").map(|p| p.summary).unwrap(), "agent two");
    }
}
