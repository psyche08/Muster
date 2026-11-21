//! Local development daemon. Its parent owns stdin and closes it to stop.
use std::io::{BufRead, Write};
use std::net::{Ipv4Addr,SocketAddr};
use std::sync::Arc;

use muster_core::SoftSigner;
use musterd::{Daemon, DaemonConfig, DefaultPlatform};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let mut config = DaemonConfig::new(directory.path());
    config.name = "Development desktop".into();
    // A two-device acceptance run may bind its temporary node to one
    // explicitly selected private interface. Default fixtures stay on loopback.
    let ip = match std::env::args().skip(1).collect::<Vec<_>>().as_slice() {
        [] => Ipv4Addr::LOCALHOST,
        [flag, value] if flag == "--paired-interface" => {
            let ip: Ipv4Addr = value.parse()?;
            anyhow::ensure!(ip.is_private(), "select a private LAN interface");
            ip
        }
        _ => anyhow::bail!("usage: local-dev [--paired-interface PRIVATE_IPV4]"),
    };
    config.bind = SocketAddr::from((ip, 0));
    config.agent = None;
    config.cloud = false;
    let socket = config.socket_path();
    let daemon = Daemon::start_with(
        config,
        Arc::new(DefaultPlatform),
        Some(Arc::new(SoftSigner::generate())),
    )
    .await?;
    println!("{}", socket.display());
    std::io::stdout().flush()?;
    let (closed, parent) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
        let _ = std::io::stdin().lock().read_line(&mut String::new());
        let _ = closed.send(());
    });
    tokio::select! {
        _ = parent => {}
        _ = tokio::signal::ctrl_c() => {}
    }
    daemon.shutdown().await;
    Ok(())
}
