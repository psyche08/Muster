//! Local development daemon. Its parent owns stdin and closes it to stop.
use std::io::{BufRead, Write};
use std::sync::Arc;

use muster_core::SoftSigner;
use musterd::{Daemon, DaemonConfig, DefaultPlatform};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let mut config = DaemonConfig::new(directory.path());
    config.name = "Development desktop".into();
    config.bind = "127.0.0.1:0".parse()?;
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
