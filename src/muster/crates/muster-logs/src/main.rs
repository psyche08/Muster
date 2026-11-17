//! muster-logs: the log collector (ADR 0228), behind nginx as
//! muster-logs.linsheng.dev:8443 (ADR 0229).
//!
//!   muster-logs serve --plain --root /var/lib/muster-logs   (127.0.0.1:9180, behind nginx)
//!   muster-logs serve --listen 0.0.0.0:9444 --cert fullchain.pem --key privkey.pem --root …
//!   muster-logs check-config --cert fullchain.pem --key privkey.pem

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use anyhow::{Result, bail};
use clap::{Parser, Subcommand};
use muster_logs::server::{self, Collector, Options};

#[derive(Parser)]
#[command(name = "muster-logs", about = "Muster log collector")]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Take uploads until stopped.
    Serve {
        #[arg(long, default_value = "127.0.0.1:9180")]
        listen: SocketAddr,
        #[arg(long, required_unless_present = "plain")]
        cert: Option<PathBuf>,
        #[arg(long, required_unless_present = "plain")]
        key: Option<PathBuf>,
        /// Plain HTTP for a TLS proxy on this host, which names the client
        /// in `X-Real-IP`; loopback addresses only.
        #[arg(long, conflicts_with_all = ["cert", "key"])]
        plain: bool,
        #[arg(long, default_value = "/var/lib/muster-logs")]
        root: PathBuf,
        #[arg(long, default_value_t = 30)]
        retention_days: u64,
        /// Compressed MiB one device may add per day.
        #[arg(long, default_value_t = 128)]
        device_daily_mib: u64,
    },
    /// Check that the certificate chain and key load and match.
    CheckConfig {
        #[arg(long)]
        cert: PathBuf,
        #[arg(long)]
        key: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    match Args::parse().command {
        Command::CheckConfig { cert, key } => {
            server::tls_config(&cert, &key)?;
            println!("ok");
            Ok(())
        }
        Command::Serve {
            listen,
            cert,
            key,
            plain,
            root,
            retention_days,
            device_daily_mib,
        } => {
            let tls = match (cert, key) {
                (Some(cert), Some(key)) => Some(Arc::new(server::tls_config(&cert, &key)?)),
                _ if plain && listen.ip().is_loopback() => None,
                _ => bail!("--plain listens on a loopback address only"),
            };
            std::fs::create_dir_all(&root)?;
            let mut options = Options::new(root);
            options.retention_days = retention_days.max(1);
            options.device_daily_bytes = device_daily_mib.max(1) << 20;
            let collector = Arc::new(Collector::new(options));
            let sweeper = collector.clone();
            tokio::spawn(async move {
                loop {
                    let collector = sweeper.clone();
                    match tokio::task::spawn_blocking(move || collector.sweep(SystemTime::now()))
                        .await
                    {
                        Ok(Ok(0)) => {}
                        Ok(Ok(removed)) => {
                            log::info!("retention: removed {removed} day directories")
                        }
                        Ok(Err(error)) => log::warn!("retention: {error}"),
                        Err(error) => log::warn!("retention task: {error}"),
                    }
                    tokio::time::sleep(Duration::from_secs(3600)).await;
                }
            });
            let listener = tokio::net::TcpListener::bind(listen).await?;
            log::info!(
                "muster-logs on {listen} ({}), root {}",
                if tls.is_some() {
                    "https"
                } else {
                    "http, behind a proxy"
                },
                collector.options().root.display()
            );
            server::serve(listener, tls, collector).await
        }
    }
}
