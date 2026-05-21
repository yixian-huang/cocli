use crate::{actor::*, error::ActorResult};
use std::time::Duration;
use tokio::task::JoinSet;

pub struct Supervisor {
    pub shutdown: ShutdownSignal,
    join_set: JoinSet<(String, ActorResult<()>)>,
}

impl Default for Supervisor {
    fn default() -> Self {
        Self::new()
    }
}

impl Supervisor {
    pub fn new() -> Self {
        Self {
            shutdown: ShutdownSignal::new(),
            join_set: JoinSet::new(),
        }
    }

    pub fn spawn<A: Actor>(&mut self, actor: A) {
        let name = actor.name().to_string();
        let token = self.shutdown.subscribe();
        self.join_set.spawn(async move {
            let res = actor.run(token).await;
            (name, res)
        });
    }

    /// Block until ctrl-c, then broadcast shutdown to all actors.
    pub async fn await_shutdown_signal(&self) {
        let _ = tokio::signal::ctrl_c().await;
        tracing::info!("ctrl-c received, broadcasting shutdown");
        self.shutdown.fire();
    }

    /// Wait for all actors to complete (10s grace, then abort).
    pub async fn join_all(mut self) -> Vec<(String, ActorResult<()>)> {
        let mut results = Vec::new();
        let deadline = tokio::time::sleep(Duration::from_secs(10));
        tokio::pin!(deadline);
        while !self.join_set.is_empty() {
            tokio::select! {
                Some(res) = self.join_set.join_next() => match res {
                    Ok(pair) => results.push(pair),
                    Err(e) => tracing::warn!(error=%e, "actor join error"),
                },
                _ = &mut deadline => {
                    tracing::warn!("shutdown 10s timeout, aborting remaining actors");
                    self.join_set.abort_all();
                    while let Some(res) = self.join_set.join_next().await {
                        if let Ok(pair) = res { results.push(pair); }
                    }
                    break;
                }
            }
        }
        results
    }
}
