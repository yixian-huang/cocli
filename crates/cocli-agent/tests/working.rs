//! Integration tests for FPC #16 working memory (set/get/clear).
//!
//! Exercises the per-agent `WorkingMemoryStore` semantics directly. A full
//! router roundtrip with real outbound assertions lives in the FPC bash
//! harness (`daemon-rs/tests/fpc/16_working_memory.sh`); these unit-style
//! tests pin the in-memory contract (started_at preserved, last_updated_at
//! strictly monotonic, clear idempotent).

use chrono::{DateTime, Duration, Utc};
use cocli_agent::working::WorkingMemoryStore;
use cocli_protocol::types::WorkingStatePayload;

fn payload(summary: &str) -> WorkingStatePayload {
    WorkingStatePayload {
        summary: summary.to_string(),
        ..Default::default()
    }
}

fn fixed_t(s: &str) -> DateTime<Utc> {
    s.parse::<DateTime<Utc>>().expect("parse RFC3339 fixture")
}

#[test]
fn set_then_get_roundtrip() {
    let mut s = WorkingMemoryStore::new();
    let stored = s.set("agent-A", payload("draft refactor plan"));
    assert_eq!(stored.summary, "draft refactor plan");
    assert!(!stored.started_at.is_empty());
    assert!(!stored.last_updated_at.is_empty());

    let got = s.get("agent-A").expect("get must return set state");
    assert_eq!(got.summary, "draft refactor plan");
    assert_eq!(got.started_at, stored.started_at);
    assert_eq!(got.last_updated_at, stored.last_updated_at);
}

#[test]
fn set_preserves_started_at() {
    let mut s = WorkingMemoryStore::new();
    let t0 = fixed_t("2026-05-21T10:00:00.000000000Z");
    let first = s.set_at("agent-A", payload("phase 1"), t0);

    // A few seconds later, the agent updates the anchor.
    let t1 = t0 + Duration::seconds(5);
    let second = s.set_at("agent-A", payload("phase 2 — adjusted"), t1);

    assert_eq!(
        second.started_at, first.started_at,
        "started_at must persist across re-sets"
    );
    assert!(
        second.last_updated_at > first.last_updated_at,
        "last_updated_at must move forward: {} not > {}",
        second.last_updated_at,
        first.last_updated_at
    );
    assert_eq!(second.summary, "phase 2 — adjusted");

    let got = s.get("agent-A").unwrap();
    assert_eq!(got.started_at, first.started_at);
    assert_eq!(got.summary, "phase 2 — adjusted");
}

#[test]
fn clear_makes_get_return_none() {
    let mut s = WorkingMemoryStore::new();
    s.set("agent-A", payload("about to clear"));
    assert!(s.get("agent-A").is_some());
    s.clear("agent-A");
    assert!(s.get("agent-A").is_none());
}

#[test]
fn clear_is_idempotent() {
    let mut s = WorkingMemoryStore::new();
    // No set first — clearing an unknown agent must not panic / leak state.
    s.clear("never-set-agent");
    s.clear("never-set-agent");
    assert!(s.get("never-set-agent").is_none());
}

#[test]
fn agents_are_isolated() {
    let mut s = WorkingMemoryStore::new();
    s.set("agent-A", payload("A's focus"));
    s.set("agent-B", payload("B's focus"));

    s.clear("agent-A");
    assert!(s.get("agent-A").is_none(), "A cleared");
    assert_eq!(
        s.get("agent-B").map(|p| p.summary).unwrap(),
        "B's focus",
        "B unaffected"
    );
}

#[test]
fn task_anchor_fields_roundtrip() {
    let mut s = WorkingMemoryStore::new();
    let task_id = uuid::Uuid::new_v4();
    let p = WorkingStatePayload {
        task_id: Some(task_id),
        task_number: 42,
        channel_name: "fpc16-test".to_string(),
        summary: "wire daemon-rs working memory".to_string(),
        next_step_hint: "run FPC harness".to_string(),
        ..Default::default()
    };
    let stored = s.set("agent-A", p);
    assert_eq!(stored.task_id, Some(task_id));
    assert_eq!(stored.task_number, 42);
    assert_eq!(stored.channel_name, "fpc16-test");
    assert_eq!(stored.next_step_hint, "run FPC harness");

    let got = s.get("agent-A").unwrap();
    assert_eq!(got.task_id, Some(task_id));
    assert_eq!(got.task_number, 42);
}
