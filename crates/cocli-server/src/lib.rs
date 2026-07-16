//! Local server assembly for cocli.

mod runtime;
mod skills;

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::Router;
use cocli_api::{router, RuntimeService, SqliteRuntimeHistorySink, SqliteRuntimeKnowledgeProvider};
use cocli_store::{Store, StoreError};
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
        tokio::fs::create_dir_all(&config.data_dir).await?;
        let store = Store::open(database_path(&config.data_dir)).await?;
        runtime.set_history_sink(Arc::new(SqliteRuntimeHistorySink::new(store.clone())));
        runtime
            .set_knowledge_provider(Arc::new(SqliteRuntimeKnowledgeProvider::new(store.clone())));
        let listener = TcpListener::bind(config.bind).await?;
        Ok(Self {
            listener,
            app: router(store, runtime).merge(cocli_web::router(config.web_dir)),
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

fn database_path(data_dir: &Path) -> PathBuf {
    data_dir.join("cocli.sqlite3")
}

/// Errors emitted while binding or running the local server.
#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    /// Filesystem, socket, or HTTP serving failed.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// SQLite initialization failed.
    #[error(transparent)]
    Store(#[from] StoreError),
}
