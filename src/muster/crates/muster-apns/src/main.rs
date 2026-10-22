use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use muster_apns::{DEFAULT_SOCKET, Environment, Http2, Http2Options, Pusher, Transport, serve};
use tokio::net::UnixListener;

#[derive(Parser, Debug)]
#[command(
    version,
    about = "muster APNs provider: silent wake-ups for the relay's push hook"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Answer the relay's push hook (muster-apns-push) on a local socket
    Serve {
        #[command(flatten)]
        apns: ApnsArgs,
        /// Unix socket the hook connects to
        #[arg(long, default_value = DEFAULT_SOCKET)]
        socket: PathBuf,
    },
    /// Send one push to each environment and print APNs's answers. The
    /// default all-zero token expects BadDeviceToken once the certificate is
    /// accepted; without --cert, MissingProviderToken proves reachability.
    Probe {
        #[command(flatten)]
        apns: ApnsArgs,
        /// Device token, hex
        #[arg(
            long,
            default_value = "0000000000000000000000000000000000000000000000000000000000000000"
        )]
        token: String,
    },
}

#[derive(Args, Debug)]
struct ApnsArgs {
    /// PEM certificate "Apple Push Services: <topic>"
    #[arg(long, requires = "key")]
    cert: Option<PathBuf>,
    /// PEM private key matching --cert
    #[arg(long, requires = "cert")]
    key: Option<PathBuf>,
    /// The app's bundle id
    #[arg(long, default_value = "dev.linsheng.muster")]
    topic: String,
    /// A wake-up not delivered within this many seconds is dropped
    #[arg(long, default_value_t = 600)]
    ttl_secs: u64,
    #[arg(long, default_value = "https://api.push.apple.com:2197")]
    production: String,
    #[arg(long, default_value = "https://api.sandbox.push.apple.com:2197")]
    sandbox: String,
}

impl ApnsArgs {
    fn transport(&self) -> Result<Http2> {
        let identity_pem = match (&self.key, &self.cert) {
            (Some(key), Some(cert)) => {
                let mut pem =
                    std::fs::read(key).with_context(|| format!("read {}", key.display()))?;
                pem.push(b'\n');
                pem.extend(
                    std::fs::read(cert).with_context(|| format!("read {}", cert.display()))?,
                );
                Some(pem)
            }
            _ => None,
        };
        Http2::new(Http2Options {
            identity_pem,
            topic: self.topic.clone(),
            ttl: Duration::from_secs(self.ttl_secs),
            production: self.production.clone(),
            sandbox: self.sandbox.clone(),
        })
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();
    match Cli::parse().command {
        Command::Serve { apns, socket } => {
            anyhow::ensure!(apns.cert.is_some(), "serve needs --cert and --key");
            let pusher = Arc::new(Pusher::new(apns.transport()?));
            // A socket file left by a previous run would refuse the bind.
            let _ = std::fs::remove_file(&socket);
            let listener = UnixListener::bind(&socket)
                .with_context(|| format!("bind {}", socket.display()))?;
            // The hook runs as another user in this socket's group.
            std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o660))?;
            log::info!(
                "muster-apns: topic {}, listening on {}",
                apns.topic,
                socket.display()
            );
            serve(listener, pusher).await
        }
        Command::Probe { apns, token } => {
            let transport = apns.transport()?;
            let token = hex::decode(&token).context("token must be hex")?;
            for env in [Environment::Production, Environment::Sandbox] {
                match transport.push(env, &token).await {
                    Ok(outcome) => println!("{}: {outcome:?}", env.name()),
                    Err(error) => println!("{}: {error:#}", env.name()),
                }
            }
            Ok(())
        }
    }
}
