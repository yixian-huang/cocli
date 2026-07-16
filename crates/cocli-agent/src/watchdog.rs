use std::collections::HashMap;

pub const AUTO_RETRY_MAX: i32 = 3;
pub const WATCHDOG_MAX_RETRIES: i32 = 5;
pub const WATCHDOG_TOTAL_MAX_RETRIES: i32 = AUTO_RETRY_MAX + WATCHDOG_MAX_RETRIES;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchdogEvent {
    pub agent_id: String,
    pub agent_name: String,
    pub action: String,
    pub detail: String,
    pub attempt: i32,
    pub max_retries: i32,
}

#[derive(Debug, Clone)]
struct WatchdogEntry {
    agent_name: String,
    retry_count: i32,
    pending: Option<WatchdogEvent>,
}

#[derive(Debug, Default)]
pub struct WatchdogStore {
    entries: HashMap<String, WatchdogEntry>,
}

impl WatchdogStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_down(&mut self, agent_id: impl Into<String>, agent_name: impl Into<String>) {
        let agent_id = agent_id.into();
        if agent_id.trim().is_empty() {
            return;
        }
        let agent_name = agent_name.into();
        let entry = self
            .entries
            .entry(agent_id)
            .or_insert_with(|| WatchdogEntry {
                agent_name: agent_name.clone(),
                // Cloud watchdog starts only after A1 auto-recovery has
                // exhausted maxAutoRetries. Local runtime recovery follows
                // the same boundary.
                retry_count: AUTO_RETRY_MAX,
                pending: None,
            });
        entry.agent_name = agent_name;
    }

    pub fn clear(&mut self, agent_id: &str) {
        self.entries.remove(agent_id);
    }

    pub fn tracked_agent_ids(&self) -> Vec<String> {
        self.entries.keys().cloned().collect()
    }

    pub fn plan_restart(
        &mut self,
        agent_id: &str,
        agent_name: impl Into<String>,
    ) -> Option<WatchdogEvent> {
        let entry = self.entries.get_mut(agent_id)?;
        if let Some(pending) = &entry.pending {
            return Some(pending.clone());
        }

        let agent_name = agent_name.into();
        if !agent_name.is_empty() {
            entry.agent_name = agent_name;
        }

        if entry.retry_count >= WATCHDOG_TOTAL_MAX_RETRIES {
            return Some(WatchdogEvent {
                agent_id: agent_id.to_string(),
                agent_name: entry.agent_name.clone(),
                action: "max_retries_exceeded".to_string(),
                detail: "agent exhausted all recovery attempts - manual intervention required"
                    .to_string(),
                attempt: WATCHDOG_TOTAL_MAX_RETRIES,
                max_retries: WATCHDOG_TOTAL_MAX_RETRIES,
            });
        }

        entry.retry_count = entry.retry_count.saturating_add(1);
        let event = WatchdogEvent {
            agent_id: agent_id.to_string(),
            agent_name: entry.agent_name.clone(),
            action: "auto_restart".to_string(),
            detail: "watchdog is attempting to restart the agent".to_string(),
            attempt: entry.retry_count,
            max_retries: WATCHDOG_TOTAL_MAX_RETRIES,
        };
        entry.pending = Some(event.clone());
        Some(event)
    }

    pub fn mark_recovered(&mut self, agent_id: &str) -> Option<WatchdogEvent> {
        let mut entry = self.entries.remove(agent_id)?;
        let pending = entry.pending.take()?;
        Some(WatchdogEvent {
            action: "recovered".to_string(),
            detail: "agent successfully restarted by watchdog".to_string(),
            ..pending
        })
    }

    pub fn mark_restart_failed(
        &mut self,
        agent_id: &str,
        detail: impl Into<String>,
    ) -> Option<WatchdogEvent> {
        let entry = self.entries.get_mut(agent_id)?;
        let pending = entry.pending.take()?;
        Some(WatchdogEvent {
            action: "restart_failed".to_string(),
            detail: detail.into(),
            ..pending
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watchdog_store_auto_restart_then_recovered() {
        let mut store = WatchdogStore::new();
        store.register_down("agent-1", "Builder");

        let event = store
            .plan_restart("agent-1", "Builder")
            .expect("auto restart");
        assert_eq!(event.action, "auto_restart");
        assert_eq!(event.attempt, AUTO_RETRY_MAX + 1);
        assert_eq!(event.max_retries, WATCHDOG_TOTAL_MAX_RETRIES);

        let recovered = store.mark_recovered("agent-1").expect("pending recovery");
        assert_eq!(recovered.action, "recovered");
        assert!(store.plan_restart("agent-1", "Builder").is_none());
    }

    #[test]
    fn watchdog_store_restart_failed_preserves_retry_count() {
        let mut store = WatchdogStore::new();
        store.register_down("agent-1", "Builder");

        let first = store
            .plan_restart("agent-1", "Builder")
            .expect("first restart");
        let failed = store
            .mark_restart_failed("agent-1", "spawn failed")
            .expect("pending restart failure");

        assert_eq!(failed.action, "restart_failed");
        assert_eq!(failed.attempt, first.attempt);
        assert_eq!(failed.detail, "spawn failed");

        let second = store
            .plan_restart("agent-1", "Builder")
            .expect("second restart");
        assert_eq!(second.attempt, first.attempt + 1);
    }

    #[test]
    fn watchdog_store_max_retries_exceeded_after_combined_limit() {
        let mut store = WatchdogStore::new();
        store.register_down("agent-1", "Builder");

        for _ in AUTO_RETRY_MAX..WATCHDOG_TOTAL_MAX_RETRIES {
            let _ = store.plan_restart("agent-1", "Builder");
            let _ = store.mark_restart_failed("agent-1", "still down");
        }

        let event = store
            .plan_restart("agent-1", "Builder")
            .expect("max retries event");
        assert_eq!(event.action, "max_retries_exceeded");
        assert_eq!(event.attempt, WATCHDOG_TOTAL_MAX_RETRIES);
    }
}
