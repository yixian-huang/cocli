//! Local server assembly for cocli.

mod runtime;
mod skills;

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use axum::extract::State;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use cocli_api::{
    reconcile_skill_state, router_with_live_events, LiveEvent, LiveEventSink, RuntimeService,
    SqliteRuntimeBridgeTokenProvider, SqliteRuntimeHistorySink, SqliteRuntimeKnowledgeProvider,
};
use cocli_store::{Store, StoreError};
use cocli_ws::EventHub;
use tokio::net::TcpListener;

pub use runtime::{LocalRuntimeConfig, LocalRuntimeService, RuntimeSetupError};

/// Configuration for one local cocli server.
#[derive(Clone, Debug)]
pub struct ServerConfig {
    /// Loopback address to bind.
    pub bind: SocketAddr,
    /// Directory containing the SQLite database.
    pub data_dir: PathBuf,
    /// Optional Vite build directory for the local web workspace.
    pub web_dir: Option<PathBuf>,
}

/// A bound server ready to accept requests.
#[derive(Debug)]
pub struct Server {
    listener: TcpListener,
    app: Router,
    data_dir: PathBuf,
}

struct HubLiveEventSink {
    hub: EventHub,
}

#[derive(Clone)]
struct BackupState {
    store: Store,
    data_dir: PathBuf,
}

#[async_trait]
impl LiveEventSink for HubLiveEventSink {
    async fn emit(&self, event: LiveEvent) {
        match serde_json::to_value(event) {
            Ok(value) => self.hub.publish(value),
            Err(error) => tracing::warn!(%error, "failed to serialize live event"),
        }
    }
}

impl Server {
    /// Creates the data directory, opens SQLite, and binds the HTTP listener.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError`] when filesystem, database, or socket setup fails.
    pub async fn bind(
        config: ServerConfig,
        runtime: Arc<dyn RuntimeService>,
    ) -> Result<Self, ServerError> {
        if !config.bind.ip().is_loopback() {
            return Err(ServerError::NonLoopbackBind(config.bind));
        }
        tokio::fs::create_dir_all(&config.data_dir).await?;
        let listener = TcpListener::bind(config.bind).await?;
        let store = Store::open(database_path(&config.data_dir)).await?;
        store
            .close_stale_agent_sessions("process_restart", chrono::Utc::now())
            .await?;
        runtime.set_history_sink(Arc::new(SqliteRuntimeHistorySink::new(store.clone())));
        runtime
            .set_knowledge_provider(Arc::new(SqliteRuntimeKnowledgeProvider::new(store.clone())));
        runtime.set_bridge_token_provider(Arc::new(SqliteRuntimeBridgeTokenProvider::new(
            store.clone(),
        )));
        let event_hub = EventHub::default();
        let live_events: Arc<dyn LiveEventSink> = Arc::new(HubLiveEventSink {
            hub: event_hub.clone(),
        });
        runtime.set_live_event_sink(Arc::clone(&live_events));
        reconcile_skill_state(&store, &runtime)
            .await
            .map_err(ServerError::Runtime)?;
        let backup_router = Router::new()
            .route("/api/backups/state", get(download_state_backup))
            .with_state(BackupState {
                store: store.clone(),
                data_dir: config.data_dir.clone(),
            });
        Ok(Self {
            listener,
            app: router_with_live_events(store, runtime, live_events)
                .merge(cocli_ws::router(event_hub))
                .merge(backup_router)
                .merge(cocli_web::router(config.web_dir)),
            data_dir: config.data_dir,
        })
    }

    /// Returns the actual bound address, including an OS-assigned port.
    ///
    /// # Errors
    ///
    /// Returns [`std::io::Error`] if the socket address cannot be read.
    pub fn local_addr(&self) -> Result<SocketAddr, std::io::Error> {
        self.listener.local_addr()
    }

    /// Returns the directory containing local state.
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// Serves requests until shutdown.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError`] when the HTTP server exits with an error.
    pub async fn run(self) -> Result<(), ServerError> {
        axum::serve(self.listener, self.app).await?;
        Ok(())
    }
}

async fn download_state_backup(State(state): State<BackupState>) -> Response {
    let backup_dir = state.data_dir.join("backups");
    let snapshot_path = backup_dir.join(format!("download-{}.sqlite3", uuid::Uuid::new_v4()));
    let snapshot = async {
        tokio::fs::create_dir_all(&backup_dir)
            .await
            .map_err(|error| error.to_string())?;
        state
            .store
            .export_snapshot(&snapshot_path)
            .await
            .map_err(|error| error.to_string())?;
        tokio::fs::read(&snapshot_path)
            .await
            .map_err(|error| error.to_string())
    }
    .await;
    let _ = tokio::fs::remove_file(&snapshot_path).await;

    match snapshot {
        Ok(bytes) => {
            let filename = format!(
                "attachment; filename=\"cocli-state-{}.sqlite3\"",
                chrono::Utc::now().format("%Y%m%d-%H%M%S")
            );
            let mut response = bytes.into_response();
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/vnd.sqlite3"),
            );
            if let Ok(value) = HeaderValue::from_str(&filename) {
                response
                    .headers_mut()
                    .insert(header::CONTENT_DISPOSITION, value);
            }
            response
        }
        Err(error) => {
            tracing::error!(%error, "failed to export application state backup");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to export application state backup",
            )
                .into_response()
        }
    }
}

fn database_path(data_dir: &Path) -> PathBuf {
    data_dir.join("cocli.sqlite3")
}

/// Errors emitted while binding or running the local server.
#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    /// The local single-user server may only listen on a loopback interface.
    #[error("refusing non-loopback bind address {0}; cocli local only supports loopback")]
    NonLoopbackBind(SocketAddr),
    /// Filesystem, socket, or HTTP serving failed.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// SQLite initialization failed.
    #[error(transparent)]
    Store(#[from] StoreError),
    /// Runtime-backed startup reconciliation failed.
    #[error(transparent)]
    Runtime(#[from] cocli_api::RuntimeError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use cocli_api::EchoRuntimeService;
    use std::net::{IpAddr, Ipv4Addr};

    #[tokio::test]
    async fn rejects_non_loopback_bind_addresses() {
        let temp = tempfile::tempdir().expect("temp dir");
        let bind = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0);
        let error = Server::bind(
            ServerConfig {
                bind,
                data_dir: temp.path().join("data"),
                web_dir: None,
            },
            Arc::new(EchoRuntimeService),
        )
        .await
        .expect_err("non-loopback bind must be rejected");

        assert!(matches!(error, ServerError::NonLoopbackBind(address) if address == bind));
        assert!(
            !temp.path().join("data").exists(),
            "validation should happen before local state is created"
        );
    }

    #[tokio::test]
    async fn downloads_a_sqlite_application_state_snapshot() {
        let temp = tempfile::tempdir().expect("temp dir");
        let store = Store::open(temp.path().join("cocli.sqlite3"))
            .await
            .expect("store should open");
        store
            .create_channel("portable")
            .await
            .expect("channel should be created");

        let response = download_state_backup(State(BackupState {
            store,
            data_dir: temp.path().to_path_buf(),
        }))
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE),
            Some(&HeaderValue::from_static("application/vnd.sqlite3"))
        );
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("backup body should load");
        assert!(bytes.starts_with(b"SQLite format 3"));
    }
}
