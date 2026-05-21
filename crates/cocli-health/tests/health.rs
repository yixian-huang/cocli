//! Integration test for `HealthActor` idle detection.
//!
//! Sends a `Started` observation, waits beyond the (test-shrunk) idle
//! threshold, and asserts that an `AgentSessionIdle` `DaemonMsg` is emitted
//! on the outbound mpsc channel.

use std::time::Duration;

use tokio::sync::{broadcast, mpsc};
use uuid::Uuid;

use cocli_actor::{Actor, ShutdownSignal};
use cocli_agent::AgentObservationChanged;
use cocli_health::HealthActor;
use cocli_protocol::DaemonMsg;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn idle_after_threshold_emits_msg() {
    let (obs_tx, obs_rx) = broadcast::channel(16);
    let (out_tx, mut out_rx) = mpsc::channel::<DaemonMsg>(16);

    let mut health = HealthActor::new(obs_rx, out_tx, Duration::from_millis(200));
    health.tick_interval = Duration::from_millis(50);

    let shutdown = ShutdownSignal::new();
    let token = shutdown.subscribe();
    let handle = tokio::spawn(health.run(token));

    // Spawn an agent observation so the actor starts tracking it.
    obs_tx
        .send(AgentObservationChanged::Started {
            agent_id: "a1".into(),
            session_id: "s1".into(),
            channel_id: Uuid::nil(),
            channel_name: "c1".into(),
        })
        .expect("broadcast send");

    // Wait long enough for: (1) actor to consume the obs, (2) idle threshold
    // (200ms) to elapse, (3) at least one tick (50ms) after that.
    tokio::time::sleep(Duration::from_millis(500)).await;

    let msg = out_rx
        .try_recv()
        .expect("expected AgentSessionIdle on outbound");
    match msg {
        DaemonMsg::AgentSessionIdle(idle) => {
            assert_eq!(idle.agent_id, "a1");
            assert_eq!(idle.session_id, "s1");
            assert_eq!(idle.channel_name, "c1");
            assert_eq!(idle.active_sessions, 1);
        }
        other => panic!("expected AgentSessionIdle, got {:?}", other),
    }

    // No second idle msg should fire for the same already_idle agent.
    assert!(
        out_rx.try_recv().is_err(),
        "duplicate idle msg emitted despite already_idle guard"
    );

    shutdown.fire();
    let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;
}
