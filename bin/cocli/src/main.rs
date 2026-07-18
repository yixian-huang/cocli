//! cocli — local-first multi-agent platform.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use cocli_api::{EchoRuntimeService, RuntimeService};
use cocli_server::{LocalRuntimeConfig, LocalRuntimeService, Server, ServerConfig};

#[derive(Parser, Debug)]
#[command(name = "cocli", version, about = "Local-first multi-agent platform")]
struct Args {
    #[command(subcommand)]
    command: Option<Command>,

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

#[derive(Debug, Subcommand)]
enum Command {
    /// Export a transactionally consistent SQLite state snapshot.
    Backup {
        /// New destination file. The command refuses to overwrite it.
        #[arg(long)]
        output: PathBuf,
    },
    /// Restore and migrate a SQLite snapshot while cocli is stopped.
    Restore {
        /// SQLite snapshot to validate, migrate, and install.
        #[arg(long)]
        input: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    tracing_subscriber::fmt::init();

    let data_dir = args
        .data_dir
        .clone()
        .map(Ok)
        .unwrap_or_else(default_data_dir)?;
    if let Some(command) = args.command {
        return run_state_command(command, &data_dir).await;
    }

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

async fn run_state_command(command: Command, data_dir: &std::path::Path) -> Result<()> {
    match command {
        Command::Backup { output } => {
            if output.exists() {
                anyhow::bail!("backup destination already exists: {}", output.display());
            }
            if let Some(parent) = output.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .context("failed to create backup destination directory")?;
            }
            let store = cocli_store::Store::open(data_dir.join("cocli.sqlite3"))
                .await
                .context("failed to open local state")?;
            store
                .export_snapshot(&output)
                .await
                .context("failed to export local state")?;
            store.close().await;
            println!("backup written: {}", output.display());
            Ok(())
        }
        Command::Restore { input } => restore_snapshot(data_dir, &input).await,
    }
}

async fn restore_snapshot(data_dir: &std::path::Path, input: &std::path::Path) -> Result<()> {
    if !input.is_file() {
        anyhow::bail!("restore snapshot does not exist: {}", input.display());
    }
    tokio::fs::create_dir_all(data_dir)
        .await
        .context("failed to create cocli data directory")?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let staged = data_dir.join(format!(".restore-{nonce}.sqlite3"));
    tokio::fs::copy(input, &staged)
        .await
        .context("failed to stage restore snapshot")?;
    let validation = cocli_store::Store::open(&staged).await;
    let store = match validation {
        Ok(store) => store,
        Err(error) => {
            let _ = tokio::fs::remove_file(&staged).await;
            return Err(error).context("restore snapshot is not a valid migratable cocli database");
        }
    };
    store.close().await;

    let target = data_dir.join("cocli.sqlite3");
    let backup_dir = data_dir.join("backups");
    tokio::fs::create_dir_all(&backup_dir)
        .await
        .context("failed to create restore safety-backup directory")?;
    let previous = backup_dir.join(format!("pre-restore-{nonce}.sqlite3"));
    let had_previous = target.exists();
    if had_previous {
        tokio::fs::rename(&target, &previous)
            .await
            .context("failed to preserve current state before restore")?;
    }
    if let Err(error) = tokio::fs::rename(&staged, &target).await {
        if had_previous {
            let _ = tokio::fs::rename(&previous, &target).await;
        }
        let _ = tokio::fs::remove_file(&staged).await;
        return Err(error).context("failed to install restored state");
    }
    println!("state restored: {}", target.display());
    if had_previous {
        println!("previous state preserved: {}", previous.display());
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn restore_validates_migrates_and_preserves_previous_state() {
        let temp = tempfile::tempdir().expect("temp dir");
        let source_dir = temp.path().join("source");
        let target_dir = temp.path().join("target");
        tokio::fs::create_dir_all(&source_dir)
            .await
            .expect("source dir");
        tokio::fs::create_dir_all(&target_dir)
            .await
            .expect("target dir");

        let source = cocli_store::Store::open(source_dir.join("cocli.sqlite3"))
            .await
            .expect("source store");
        source
            .create_channel("restored")
            .await
            .expect("source channel");
        let snapshot = temp.path().join("snapshot.sqlite3");
        source.export_snapshot(&snapshot).await.expect("snapshot");
        source.close().await;

        let current = cocli_store::Store::open(target_dir.join("cocli.sqlite3"))
            .await
            .expect("current store");
        current
            .create_channel("previous")
            .await
            .expect("current channel");
        current.close().await;

        restore_snapshot(&target_dir, &snapshot)
            .await
            .expect("restore should succeed");
        let restored = cocli_store::Store::open(target_dir.join("cocli.sqlite3"))
            .await
            .expect("restored store");
        assert_eq!(
            restored.list_channels().await.expect("channels")[0].name,
            "restored"
        );
        restored.close().await;
        assert_eq!(
            std::fs::read_dir(target_dir.join("backups"))
                .expect("backup directory")
                .count(),
            1
        );
    }
}
