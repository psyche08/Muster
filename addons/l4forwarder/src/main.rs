//! l4forwarder: forwards TCP and UDP ports as its configuration file lists.

use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use l4forwarder::Config;
use tokio::io::AsyncReadExt;

#[derive(Parser, Debug)]
#[command(name = "l4forwarder", about = "TCP/UDP port forwarder")]
struct Args {
    /// The JSON configuration file (see config.example.json)
    #[arg(long)]
    config: PathBuf,
    /// Check the configuration and exit
    #[arg(long)]
    check: bool,
    /// Exit when standard input closes: the supervisor (musterd) is gone,
    /// even if it could not stop this process itself (ADR 0107)
    #[arg(long)]
    supervised: bool,
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
        _ = stdin_closed(), if args.supervised => {}
    }
    Ok(())
}

/// Completes when standard input reaches its end or fails.
async fn stdin_closed() {
    let mut stdin = tokio::io::stdin();
    let mut buf = [0u8; 64];
    while matches!(stdin.read(&mut buf).await, Ok(n) if n > 0) {}
}
