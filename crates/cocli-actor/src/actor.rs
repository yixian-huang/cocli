use crate::error::ActorResult;
use async_trait::async_trait;
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct ShutdownSignal {
    rx: broadcast::Sender<()>,
}

impl Default for ShutdownSignal {
    fn default() -> Self {
        Self::new()
    }
}

impl ShutdownSignal {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(1);
        Self { rx: tx }
    }
    pub fn fire(&self) {
        let _ = self.rx.send(());
    }
    pub fn subscribe(&self) -> ShutdownToken {
        ShutdownToken {
            rx: self.rx.subscribe(),
        }
    }
}

pub struct ShutdownToken {
    rx: broadcast::Receiver<()>,
}

impl ShutdownToken {
    pub async fn wait(&mut self) {
        let _ = self.rx.recv().await;
    }
}

#[async_trait]
pub trait Actor: Send + 'static {
    fn name(&self) -> &'static str;
    async fn run(self, shutdown: ShutdownToken) -> ActorResult<()>;
}
