//! cocli — local-first multi-agent platform.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Parser;
use cocli_api::{EchoRuntimeService, RuntimeService};
use cocli_server::{LocalRuntimeConfig, LocalRuntimeService, Server, ServerConfig};

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

    /// Directory containing the built cocli web assets.
    #[arg(long, env = "COCLI_WEB_DIR")]
    web_dir: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    tracing_subscriber::fmt::init();

    let reap = cocli_reaper::reap_orphaned_agents()
        .context("failed to reconcile orphaned local agent processes")?;
    tracing::info!(
        scanned = reap.scanned,
        reaped = reap.reaped,
        cleaned_stale = reap.cleaned_stale,
        skipped_foreign = reap.skipped_foreign,
        skipped_unknown_ppid = reap.skipped_unknown_ppid,
        "reconciled local agent process ownership"
    );

    let data_dir = args.data_dir.map(Ok).unwrap_or_else(default_data_dir)?;
    let web_dir = args.web_dir.or_else(default_web_dir);
    let runtime: Arc<dyn RuntimeService> = if args.fake_runtime {
        Arc::new(EchoRuntimeService)
    } else {
        Arc::new(
            LocalRuntimeService::discover(LocalRuntimeConfig::new(
                data_dir.join("workspaces"),
                format!("http://{}", args.bind),
            ))
            .context("failed to discover local runtimes")?,
        )
    };
    let server = Server::bind(
        ServerConfig {
            bind: args.bind,
            data_dir,
            web_dir: web_dir.clone(),
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
    if let Some(web_dir) = web_dir {
        println!("web assets: {}", web_dir.display());
    }

    server.run().await.context("cocli local server stopped")
}

fn default_web_dir() -> Option<PathBuf> {
    std::env::current_dir()
        .ok()
        .map(|directory| directory.join("web").join("dist"))
        .filter(|directory| directory.is_dir())
}

fn default_data_dir() -> Result<PathBuf> {
    dirs::data_local_dir()
        .map(|path| path.join("cocli"))
        .context("could not determine the local data directory; pass --data-dir")
}
