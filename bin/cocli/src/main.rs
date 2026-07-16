//! cocli — local-first multi-agent platform.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Parser;
use cocli_api::{EchoRuntimeService, NoRuntimeService, RuntimeService};
use cocli_server::{Server, ServerConfig};

#[derive(Parser, Debug)]
#[command(name = "cocli", version, about = "Local-first multi-agent platform")]
struct Args {
    /// Address for the local HTTP server.
    #[arg(long, env = "COCLI_BIND", default_value = "127.0.0.1:8090")]
    bind: SocketAddr,

    /// Directory for SQLite and other local state.
    #[arg(long, env = "COCLI_DATA_DIR")]
    data_dir: Option<PathBuf>,

    /// Enable the deterministic fake runtime for local-loop development.
    #[arg(long, env = "COCLI_FAKE_RUNTIME")]
    fake_runtime: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    tracing_subscriber::fmt::init();

    let data_dir = args.data_dir.map(Ok).unwrap_or_else(default_data_dir)?;
    let runtime: Arc<dyn RuntimeService> = if args.fake_runtime {
        Arc::new(EchoRuntimeService)
    } else {
        Arc::new(NoRuntimeService)
    };
    let server = Server::bind(
        ServerConfig {
            bind: args.bind,
            data_dir,
        },
        runtime,
    )
    .await
    .context("failed to initialize cocli local server")?;
    let local_addr = server
        .local_addr()
        .context("failed to read local server address")?;

    println!("cocli listening on http://{local_addr}");
    println!("data directory: {}", server.data_dir().display());

    server.run().await.context("cocli local server stopped")
}

fn default_data_dir() -> Result<PathBuf> {
    dirs::data_local_dir()
        .map(|path| path.join("cocli"))
        .context("could not determine the local data directory; pass --data-dir")
}
