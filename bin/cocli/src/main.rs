//! cocli — local-first multi-agent platform.
//!
//! M0 ships only a `--version` flag. Real bootstrap (DB init, axum serve,
//! browser open) lands in M0.0.1.

use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "cocli", version, about = "Local-first multi-agent platform")]
struct Args {
    /// Path to config (default: $XDG_CONFIG_HOME/cocli/config.toml)
    #[arg(long, env = "COCLI_CONFIG")]
    config: Option<std::path::PathBuf>,
}

fn main() -> anyhow::Result<()> {
    let _args = Args::parse();
    tracing_subscriber::fmt::init();
    println!(
        "cocli {} — M0 bootstrap stub. Real implementation lands in M0.0.1.",
        env!("CARGO_PKG_VERSION")
    );
    Ok(())
}
