//! cocli — local-first multi-agent platform.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use cocli_api::{EchoRuntimeService, RuntimeService};
use cocli_server::{LocalRuntimeConfig, LocalRuntimeService, Server, ServerConfig};

mod portable;

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
        /// Write a versioned portable bundle directory instead of a legacy SQLite snapshot.
        #[arg(long)]
        portable: bool,
    },
    /// Restore and migrate a SQLite snapshot while cocli is stopped.
    Restore {
        /// SQLite snapshot or portable bundle to validate, migrate, and install.
        #[arg(long)]
        input: PathBuf,
    },
    /// Validate a portable backup bundle without changing local state.
    Preflight {
        /// Portable bundle directory to validate.
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
        Command::Backup { output, portable } => {
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
            if portable {
                let manifest = portable::create_bundle(&store, &output).await?;
                println!("portable backup written: {}", output.display());
                println!(
                    "{}",
                    serde_json::to_string_pretty(&manifest)
                        .context("failed to print portable backup manifest")?
                );
            } else {
                store
                    .export_snapshot(&output)
                    .await
                    .context("failed to export local state")?;
                println!("backup written: {}", output.display());
            }
            store.close().await;
            Ok(())
        }
        Command::Restore { input } if input.is_dir() => {
            let restored = portable::restore_bundle(data_dir, &input).await?;
            println!("portable state restored: {}", data_dir.display());
            println!(
                "{}",
                serde_json::to_string_pretty(&restored)
                    .context("failed to print portable restore result")?
            );
            Ok(())
        }
        Command::Restore { input } => restore_snapshot(data_dir, &input).await,
        Command::Preflight { input } => {
            let preflight = portable::preflight_bundle(&input).await?;
            println!(
                "{}",
                serde_json::to_string_pretty(&preflight)
                    .context("failed to print portable preflight result")?
            );
            Ok(())
        }
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
    let previous = install_staged_database(data_dir, &staged, "pre-restore").await?;
    println!("state restored: {}", target.display());
    if let Some(previous) = previous {
        println!("previous state preserved: {}", previous.display());
    }
    Ok(())
}

async fn install_staged_database(
    data_dir: &std::path::Path,
    staged: &std::path::Path,
    backup_prefix: &str,
) -> Result<Option<PathBuf>> {
    let target = data_dir.join("cocli.sqlite3");
    let previous = if target.exists() {
        let backup_dir = data_dir.join("backups");
        tokio::fs::create_dir_all(&backup_dir)
            .await
            .context("failed to create restore safety-backup directory")?;
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        Some(backup_dir.join(format!("{backup_prefix}-{nonce}.sqlite3")))
    } else {
        None
    };

    if let Err(error) = atomic_install_database(staged, &target, previous.as_deref()).await {
        let _ = tokio::fs::remove_file(staged).await;
        return Err(error).context("failed to install restored state");
    }
    Ok(previous)
}

#[cfg(unix)]
async fn atomic_install_database(
    staged: &std::path::Path,
    target: &std::path::Path,
    previous: Option<&std::path::Path>,
) -> Result<()> {
    if let Some(previous) = previous {
        tokio::fs::copy(target, previous)
            .await
            .context("failed to preserve current state before restore")?;
        let backup = tokio::fs::OpenOptions::new()
            .read(true)
            .open(previous)
            .await
            .context("failed to open restore safety backup")?;
        backup
            .sync_all()
            .await
            .context("failed to sync restore safety backup")?;
    }

    let staged_file = tokio::fs::OpenOptions::new()
        .read(true)
        .open(staged)
        .await
        .context("failed to open staged restore state")?;
    staged_file
        .sync_all()
        .await
        .context("failed to sync staged restore state")?;
    tokio::fs::rename(staged, target)
        .await
        .context("failed to atomically replace current state")
}

#[cfg(windows)]
async fn atomic_install_database(
    staged: &std::path::Path,
    target: &std::path::Path,
    previous: Option<&std::path::Path>,
) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;

    let staged = staged.to_owned();
    let target = target.to_owned();
    let previous = previous.map(std::path::Path::to_owned);
    tokio::task::spawn_blocking(move || -> Result<()> {
        if let Some(previous) = previous {
            let wide = |path: &std::path::Path| {
                path.as_os_str()
                    .encode_wide()
                    .chain(std::iter::once(0))
                    .collect::<Vec<_>>()
            };
            let staged = wide(&staged);
            let target = wide(&target);
            let previous = wide(&previous);
            // SAFETY: all paths are NUL-terminated and remain alive for the call. The final two
            // reserved pointers are required to be null by ReplaceFileW.
            let replaced = unsafe {
                windows_sys::Win32::Storage::FileSystem::ReplaceFileW(
                    target.as_ptr(),
                    staged.as_ptr(),
                    previous.as_ptr(),
                    windows_sys::Win32::Storage::FileSystem::REPLACEFILE_WRITE_THROUGH,
                    std::ptr::null(),
                    std::ptr::null(),
                )
            };
            if replaced == 0 {
                return Err(std::io::Error::last_os_error())
                    .context("failed to atomically replace current state");
            }
        } else {
            std::fs::rename(&staged, &target)
                .context("failed to atomically install restored state")?;
        }
        Ok(())
    })
    .await
    .context("atomic restore installation task failed")?
}

#[cfg(not(any(unix, windows)))]
async fn atomic_install_database(
    staged: &std::path::Path,
    target: &std::path::Path,
    previous: Option<&std::path::Path>,
) -> Result<()> {
    if previous.is_some() {
        anyhow::bail!("atomic database replacement is unsupported on this platform");
    }
    tokio::fs::rename(staged, target)
        .await
        .context("failed to atomically install restored state")
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

    #[tokio::test]
    async fn failed_install_leaves_current_state_in_place() {
        let temp = tempfile::tempdir().expect("temp dir");
        let data_dir = temp.path().join("target");
        tokio::fs::create_dir_all(&data_dir)
            .await
            .expect("target dir");
        let target = data_dir.join("cocli.sqlite3");
        let current = b"current installation state";
        tokio::fs::write(&target, current)
            .await
            .expect("current state");

        let error = install_staged_database(
            &data_dir,
            &data_dir.join("missing-staged.sqlite3"),
            "pre-failed-restore",
        )
        .await
        .expect_err("missing staged state must fail");

        assert!(error
            .to_string()
            .contains("failed to install restored state"));
        assert_eq!(
            tokio::fs::read(&target)
                .await
                .expect("current state remains"),
            current
        );
    }
}
