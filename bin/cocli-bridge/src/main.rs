use std::io::{BufReader, BufWriter};
use std::path::PathBuf;

use clap::Parser;
use cocli_bridge::{load_action_config, run_action, run_stdio, BridgeConfig, HttpToolBackend};

#[derive(Debug, Parser)]
#[command(name = "cocli-bridge", version, trailing_var_arg = true)]
struct Args {
    #[arg(long, env = "BRIDGE_AGENT_ID")]
    agent_id: Option<String>,

    #[arg(
        long,
        env = "BRIDGE_SERVER_URL",
        default_value = "http://127.0.0.1:8090"
    )]
    server_url: String,

    #[arg(long, env = "BRIDGE_SCOPED_TOKEN", default_value = "")]
    auth_token: String,

    #[arg()]
    command: Vec<String>,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("cocli-bridge: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    if args.command.first().map(String::as_str) == Some("action") {
        let config_path = std::env::var_os("COCLI_ACTION_CONFIG")
            .map(PathBuf::from)
            .ok_or("COCLI_ACTION_CONFIG is required for action mode")?;
        let backend = HttpToolBackend::new(load_action_config(&config_path)?)?;
        let result = run_action(&backend, &args.command[1..]);
        println!("{}", serde_json::to_string(&result)?);
        if result["ok"] != true {
            std::process::exit(1);
        }
        return Ok(());
    }

    let agent_id = args
        .agent_id
        .or_else(|| std::env::var("CHATRS_AGENT_ID").ok())
        .ok_or("--agent-id, BRIDGE_AGENT_ID, or CHATRS_AGENT_ID is required")?;
    let backend = HttpToolBackend::new(BridgeConfig {
        agent_id,
        server_url: args.server_url,
        auth_token: args.auth_token,
    })?;
    run_stdio(
        &backend,
        BufReader::new(std::io::stdin()),
        BufWriter::new(std::io::stdout()),
    )?;
    Ok(())
}
