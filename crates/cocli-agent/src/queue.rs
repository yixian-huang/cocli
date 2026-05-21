//! Per-agent in-memory delivery queue.
//!
//! Used by `AgentRouter` to buffer `AgentDeliverMsg`s that arrive while
//! the target `AgentActor` is still in `Starting` (race window between
//! `agent:start` arrival and the actor's `Running` transition).
//!
//! Phase 0a: no persistence, no retry, no cap. The server's own delivery
//! queue handles offline agents end-to-end; this is purely a per-process
//! buffer for the local race window.

use std::collections::{HashMap, VecDeque};

use cocli_protocol::AgentDeliverMsg;

#[derive(Default)]
pub struct DeliveryQueue {
    per_agent: HashMap<String, VecDeque<AgentDeliverMsg>>,
}

impl DeliveryQueue {
    pub fn new() -> Self {
        Self {
            per_agent: HashMap::new(),
        }
    }

    /// Append `msg` to the tail of `agent_id`'s queue.
    pub fn enqueue(&mut self, agent_id: &str, msg: AgentDeliverMsg) {
        self.per_agent
            .entry(agent_id.to_string())
            .or_default()
            .push_back(msg);
    }

    /// Remove and return every queued message for `agent_id` in arrival
    /// order. Returns empty Vec when nothing is queued.
    pub fn drain(&mut self, agent_id: &str) -> Vec<AgentDeliverMsg> {
        self.per_agent
            .remove(agent_id)
            .map(|q| q.into_iter().collect())
            .unwrap_or_default()
    }

    /// Drop the queue for `agent_id` without surfacing pending msgs —
    /// called on `Stopped` so we don't hold memory for departed agents.
    pub fn forget(&mut self, agent_id: &str) {
        self.per_agent.remove(agent_id);
    }

    /// Test-helper: pending count for an agent (0 when none queued).
    #[cfg(test)]
    pub fn len(&self, agent_id: &str) -> usize {
        self.per_agent.get(agent_id).map(|q| q.len()).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cocli_protocol::types::DeliveryMessage;

    fn make_deliver(agent_id: &str, seq: i64) -> AgentDeliverMsg {
        AgentDeliverMsg {
            agent_id: agent_id.to_string(),
            seq,
            attempt: 1,
            message: DeliveryMessage::default(),
            ..Default::default()
        }
    }

    #[test]
    fn enqueue_then_drain_preserves_fifo() {
        let mut q = DeliveryQueue::new();
        q.enqueue("a1", make_deliver("a1", 1));
        q.enqueue("a1", make_deliver("a1", 2));
        q.enqueue("a1", make_deliver("a1", 3));
        let drained = q.drain("a1");
        assert_eq!(drained.len(), 3);
        assert_eq!(drained[0].seq, 1);
        assert_eq!(drained[1].seq, 2);
        assert_eq!(drained[2].seq, 3);
        // Second drain returns empty.
        assert!(q.drain("a1").is_empty());
    }

    #[test]
    fn forget_clears_pending() {
        let mut q = DeliveryQueue::new();
        q.enqueue("a1", make_deliver("a1", 1));
        q.enqueue("a1", make_deliver("a1", 2));
        assert_eq!(q.len("a1"), 2);
        q.forget("a1");
        assert_eq!(q.len("a1"), 0);
        assert!(q.drain("a1").is_empty());
    }

    #[test]
    fn drain_unknown_agent_returns_empty() {
        let mut q = DeliveryQueue::new();
        assert!(q.drain("nobody").is_empty());
    }

    #[test]
    fn per_agent_isolation() {
        let mut q = DeliveryQueue::new();
        q.enqueue("a1", make_deliver("a1", 1));
        q.enqueue("a2", make_deliver("a2", 1));
        q.enqueue("a2", make_deliver("a2", 2));
        assert_eq!(q.len("a1"), 1);
        assert_eq!(q.len("a2"), 2);
        let drained_a2 = q.drain("a2");
        assert_eq!(drained_a2.len(), 2);
        assert_eq!(q.len("a1"), 1); // a1 untouched
    }
}
