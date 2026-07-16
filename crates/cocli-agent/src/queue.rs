//! Per-agent in-memory delivery queue.
//!
//! Used by `AgentRouter` to buffer `AgentDeliverMsg`s that arrive while
//! the target `AgentActor` is still in `Starting` (race window between
//! `agent:start` arrival and the actor's `Running` transition).
//!
//! This queue is deliberately bounded to the actor mailbox capacity. It only
//! covers local start/backpressure races; durable retry belongs in the local
//! SQLite delivery layer.

use std::collections::{HashMap, VecDeque};

use cocli_protocol::AgentDeliverMsg;

/// Matches the per-agent actor mailbox capacity in `AgentRouter`.
pub const MAX_PENDING_PER_AGENT: usize = 64;

#[derive(Default)]
pub struct DeliveryQueue {
    per_agent: HashMap<String, VecDeque<AgentDeliverMsg>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnqueueResult {
    Queued,
    Updated,
    RejectedFull,
}

impl DeliveryQueue {
    pub fn new() -> Self {
        Self {
            per_agent: HashMap::new(),
        }
    }

    /// Append a delivery, or replace a retry for the same channel-local seq.
    pub fn enqueue(&mut self, agent_id: &str, msg: AgentDeliverMsg) -> EnqueueResult {
        let queue = self.per_agent.entry(agent_id.to_string()).or_default();

        if msg.seq > 0 {
            if let Some(pending) = queue.iter_mut().find(|pending| {
                pending.seq == msg.seq && pending.message.channel_id == msg.message.channel_id
            }) {
                *pending = msg;
                return EnqueueResult::Updated;
            }
        }

        if queue.len() >= MAX_PENDING_PER_AGENT {
            return EnqueueResult::RejectedFull;
        }

        queue.push_back(msg);
        EnqueueResult::Queued
    }

    /// Remove and return every queued message for `agent_id` in arrival
    /// order. Returns empty Vec when nothing is queued.
    pub fn drain(&mut self, agent_id: &str) -> Vec<AgentDeliverMsg> {
        self.per_agent
            .remove(agent_id)
            .map(|q| q.into_iter().collect())
            .unwrap_or_default()
    }

    /// Put an interrupted flush back at the head without changing order.
    pub fn prepend(&mut self, agent_id: &str, messages: Vec<AgentDeliverMsg>) {
        if messages.is_empty() {
            return;
        }

        let queue = self.per_agent.entry(agent_id.to_string()).or_default();
        for message in messages.into_iter().rev() {
            queue.push_front(message);
        }
        queue.truncate(MAX_PENDING_PER_AGENT);
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
        assert_eq!(
            q.enqueue("a1", make_deliver("a1", 1)),
            EnqueueResult::Queued
        );
        assert_eq!(
            q.enqueue("a1", make_deliver("a1", 2)),
            EnqueueResult::Queued
        );
        assert_eq!(
            q.enqueue("a1", make_deliver("a1", 3)),
            EnqueueResult::Queued
        );
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
        assert_eq!(
            q.enqueue("a1", make_deliver("a1", 1)),
            EnqueueResult::Queued
        );
        assert_eq!(
            q.enqueue("a1", make_deliver("a1", 2)),
            EnqueueResult::Queued
        );
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
        assert_eq!(
            q.enqueue("a1", make_deliver("a1", 1)),
            EnqueueResult::Queued
        );
        assert_eq!(
            q.enqueue("a2", make_deliver("a2", 1)),
            EnqueueResult::Queued
        );
        assert_eq!(
            q.enqueue("a2", make_deliver("a2", 2)),
            EnqueueResult::Queued
        );
        assert_eq!(q.len("a1"), 1);
        assert_eq!(q.len("a2"), 2);
        let drained_a2 = q.drain("a2");
        assert_eq!(drained_a2.len(), 2);
        assert_eq!(q.len("a1"), 1); // a1 untouched
    }

    #[test]
    fn enqueue_retry_updates_existing_delivery_instead_of_duplicating() {
        let mut q = DeliveryQueue::new();
        let channel_id = uuid::Uuid::new_v4();
        let mut first = make_deliver("a1", 7);
        first.message.channel_id = channel_id;
        let mut retry = first.clone();
        retry.attempt = 3;

        assert_eq!(q.enqueue("a1", first), EnqueueResult::Queued);
        assert_eq!(q.enqueue("a1", retry), EnqueueResult::Updated);

        let drained = q.drain("a1");
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].attempt, 3);
    }

    #[test]
    fn enqueue_rejects_new_delivery_when_agent_queue_is_full() {
        let mut q = DeliveryQueue::new();
        for seq in 1..=MAX_PENDING_PER_AGENT as i64 {
            assert_eq!(
                q.enqueue("a1", make_deliver("a1", seq)),
                EnqueueResult::Queued
            );
        }

        assert_eq!(
            q.enqueue("a1", make_deliver("a1", 9_999)),
            EnqueueResult::RejectedFull
        );
        assert_eq!(q.len("a1"), MAX_PENDING_PER_AGENT);
    }

    #[test]
    fn prepend_restores_interrupted_flush_in_original_order() {
        let mut q = DeliveryQueue::new();
        assert_eq!(
            q.enqueue("a1", make_deliver("a1", 3)),
            EnqueueResult::Queued
        );

        q.prepend("a1", vec![make_deliver("a1", 1), make_deliver("a1", 2)]);

        let seqs: Vec<i64> = q
            .drain("a1")
            .into_iter()
            .map(|message| message.seq)
            .collect();
        assert_eq!(seqs, vec![1, 2, 3]);
    }
}
