//! l4forwarder: forwards TCP and UDP ports as its configuration file lists.

use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use l4forwarder::Config;

#[derive(Parser, Debug)]
#[command(name = "l4forwarder", about = "TCP/UDP port forwarder")]
struct Args {
    /// The JSON configuration file (see config.example.json)
    #[arg(long)]
    config: PathBuf,
    /// Check the configuration and exit
    #[arg(long)]
    check: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args = Args::parse();
    let text = std::fs::read_to_string(&args.config)
        .with_context(|| format!("read {}", args.config.display()))?;
    let config =
        Config::parse(&text).map_err(|e| anyhow::anyhow!("{}: {e}", args.config.display()))?;
    if args.check {
        for rule in &config.rules {
            println!("{rule}");
        }
        return Ok(());
    }
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    tokio::select! {
        _ = l4forwarder::run(config) => {}
        _ = tokio::signal::ctrl_c() => {}
        _ = terminate.recv() => {}
    }
    Ok(())
}
