use async_trait::async_trait;
use cocli_actor::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

struct CounterActor {
    counter: Arc<AtomicUsize>,
    name: &'static str,
}

#[async_trait]
impl Actor for CounterActor {
    fn name(&self) -> &'static str {
        self.name
    }
    async fn run(self, mut shutdown: ShutdownToken) -> ActorResult<()> {
        self.counter.fetch_add(1, Ordering::SeqCst);
        shutdown.wait().await;
        Ok(())
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn supervisor_spawns_and_shuts_down() {
    let counter = Arc::new(AtomicUsize::new(0));
    let mut sup = Supervisor::new();
    sup.spawn(CounterActor {
        counter: counter.clone(),
        name: "a1",
    });
    sup.spawn(CounterActor {
        counter: counter.clone(),
        name: "a2",
    });

    // Give actors a moment to spawn and increment the counter before shutdown.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // simulate ctrl-c
    sup.shutdown.fire();
    let results = sup.join_all().await;
    assert_eq!(counter.load(Ordering::SeqCst), 2);
    assert_eq!(results.len(), 2);
    for (_, res) in results {
        assert!(res.is_ok());
    }
}
