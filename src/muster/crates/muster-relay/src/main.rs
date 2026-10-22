use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use clap::Parser;
use muster_relay::{RelayOptions, RelayServer};

#[derive(Parser, Debug)]
#[command(
    version,
    about = "muster relay: rendezvous, forwarding, mailbox, reflection"
)]
struct Cli {
    /// Shared TCP/TLS and UDP/QUIC address (production)
    #[arg(long, default_value = "0.0.0.0:9443")]
    listen: SocketAddr,
    /// PEM full certificate chain for the relay DNS name
    #[arg(long, required_unless_present = "insecure_legacy")]
    cert: Option<PathBuf>,
    /// PEM private key matching --cert
    #[arg(long, required_unless_present = "insecure_legacy")]
    key: Option<PathBuf>,
    /// Explicit local/test compatibility mode: no TLS, never production
    #[arg(long, conflicts_with_all = ["cert", "key"])]
    insecure_legacy: bool,
    /// Plain UDP address, only with --insecure-legacy
    #[arg(long, requires = "insecure_legacy")]
    udp: Option<SocketAddr>,
    /// Validate certificate/key configuration and exit without opening sockets
    #[arg(long, conflicts_with = "insecure_legacy")]
    check_config: bool,
    /// Items kept per mailbox key
    #[arg(long, default_value_t = 512)]
    mailbox_items: usize,
    /// Drop registrations idle this long
    #[arg(long, default_value_t = 90)]
    idle_secs: u64,
    /// Registered devices at most; later registrations get "relay full"
    #[arg(long, default_value_t = 10_000)]
    max_clients: usize,
    /// Frames per second each registered device may send
    #[arg(long, default_value_t = 3000)]
    client_pps: u32,
    /// Forwarded bytes per second each registered device may send
    #[arg(long, default_value_t = 30_000_000)]
    client_bps: u64,
    /// Unauthenticated frames per second per source address
    #[arg(long, default_value_t = 30)]
    source_pps: u32,
    /// Distinct mailbox keys at most
    #[arg(long, default_value_t = 10_000)]
    mailbox_keys: usize,
    /// Program run as `<device id hex> <kind> <token hex>` to wake an
    /// offline device that registered a push token
    #[arg(long)]
    push_command: Option<PathBuf>,
    /// At most one push per device in this many seconds. iOS throttles
    /// background pushes past a few an hour, then delays or drops them.
    #[arg(long, default_value_t = 600)]
    push_interval_secs: u64,
    /// A device unregistered for fewer seconds is re-registering: senders
    /// that asked for presence get no ABSENT for it yet
    #[arg(long, default_value_t = 10)]
    absent_grace_secs: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();
    let cli = Cli::parse();
    let options = RelayOptions {
        mailbox_items: cli.mailbox_items,
        idle: Duration::from_secs(cli.idle_secs),
        max_clients: cli.max_clients,
        client_frames_per_sec: cli.client_pps,
        client_bytes_per_sec: cli.client_bps,
        source_frames_per_sec: cli.source_pps,
        mailbox_keys: cli.mailbox_keys,
        push_command: cli.push_command,
        push_interval: Duration::from_secs(cli.push_interval_secs),
        absent_grace: Duration::from_secs(cli.absent_grace_secs),
    };
    let server = if cli.insecure_legacy {
        let udp = cli.udp.unwrap_or(cli.listen);
        anyhow::ensure!(
            udp.ip().is_loopback(),
            "insecure legacy listeners must bind loopback addresses"
        );
        RelayServer::bind(udp, options).await?
    } else {
        let config = muster_net::relay_tls::server_config(
            cli.cert.as_deref().expect("clap requires certificate"),
            cli.key.as_deref().expect("clap requires key"),
        )?;
        if cli.check_config {
            log::info!("relay TLS certificate and key configuration valid");
            return Ok(());
        }
        RelayServer::bind_secure(cli.listen, config, options).await?
    };
    server.serve().await
}
