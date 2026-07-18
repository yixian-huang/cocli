//! Best-effort live event broadcast for the loopback cocli server.

use std::convert::Infallible;

use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::get;
use axum::Router;
use futures::stream::{self, Stream};
use serde_json::Value;
use tokio::sync::broadcast;

/// Process-local fan-out hub for transient execution events.
///
/// Slow or disconnected clients may miss events and must reload durable state
/// through the regular HTTP API.
#[derive(Clone, Debug)]
pub struct EventHub {
    sender: broadcast::Sender<Value>,
}

impl EventHub {
    /// Creates a hub with a bounded per-subscriber backlog.
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity.max(1));
        Self { sender }
    }

    /// Publishes an event to current subscribers.
    pub fn publish(&self, event: Value) {
        let _ = self.sender.send(event);
    }

    /// Subscribes to events published after this call.
    pub fn subscribe(&self) -> broadcast::Receiver<Value> {
        self.sender.subscribe()
    }
}

impl Default for EventHub {
    fn default() -> Self {
        Self::new(1_024)
    }
}

/// Builds the server-sent event route consumed by the local UI.
pub fn router(hub: EventHub) -> Router {
    Router::new()
        .route("/api/events", get(stream_events))
        .with_state(hub)
}

async fn stream_events(
    State(hub): State<EventHub>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let receiver = hub.subscribe();
    let stream = stream::unfold(receiver, |mut receiver| async move {
        loop {
            match receiver.recv().await {
                Ok(value) => {
                    let data = serde_json::to_string(&value)
                        .unwrap_or_else(|_| "{\"kind\":\"serialization_error\"}".to_owned());
                    return Some((Ok(Event::default().data(data)), receiver));
                }
                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                    tracing::warn!(skipped, "live event subscriber lagged");
                }
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::EventHub;

    #[tokio::test]
    async fn publishes_to_current_subscribers() {
        let hub = EventHub::new(4);
        let mut receiver = hub.subscribe();

        hub.publish(json!({ "kind": "text_delta" }));

        assert_eq!(
            receiver.recv().await.expect("event should be broadcast"),
            json!({ "kind": "text_delta" })
        );
    }
}
