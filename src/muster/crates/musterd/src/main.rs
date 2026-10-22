//! musterd: run until told to stop.

use std::path::PathBuf;

use clap::Parser;
use musterd::{Daemon, DaemonConfig, parse_kind};

#[derive(Parser, Debug)]
#[command(name = "musterd", about = "Muster device daemon")]
struct Args {
    /// State directory (store + socket). Default: $MUSTER_DIR, else the
    /// platform's per-user location (macOS: ~/Library/Application Support/Muster)
    #[arg(long)]
    dir: Option<PathBuf>,
    /// Log to a file in the platform's log directory (macOS:
    /// ~/Library/Logs/Muster/musterd.log) instead of stderr; for launchd
    #[arg(long)]
    log: bool,
    /// Unix socket path (default <dir>/musterd.sock)
    #[arg(long)]
    socket: Option<PathBuf>,
    /// UDP bind address for the link
    #[arg(long, default_value = "0.0.0.0:0")]
    bind: std::net::SocketAddr,
    /// Relay: host:port (QUIC + TLS; a loopback address is a plain-UDP test
    /// relay); remembered once given
    #[arg(long)]
    relay: Option<String>,
    /// Device name (remembered once given)
    #[arg(long, default_value = "")]
    name: String,
    /// desktop | phone | server
    #[arg(long, default_value = "desktop")]
    kind: String,
    /// Bring up the vnet TUN (needs root on Linux)
    #[arg(long, default_value_t = cfg!(target_os = "macos"))]
    vnet: bool,
    /// Disable the desktop virtual interface (macOS enables it by default).
    #[arg(long)]
    no_vnet: bool,
    #[arg(long)]
    no_agent: bool,
    /// Keep the agent service to local sessions: no listing of the
    /// providers' cloud sessions with the credentials their CLIs keep here.
    #[arg(long)]
    no_cloud: bool,
    /// Offer terminals: "auto" (on macOS iTerm2, or Ghostty when only it is
    /// installed; a PTY shell elsewhere), "iterm2", "ghostty", "pty" or "off"
    #[arg(long, default_value = "auto")]
    terminal: String,
    /// Keep this file in step with the roster (sshd AuthorizedKeysFile)
    #[arg(long)]
    authorized_keys: Option<PathBuf>,
    /// Accept loopback candidates (two daemons on one host)
    #[arg(long)]
    loopback: bool,
}

fn main() -> anyhow::Result<()> {
    #[cfg(target_os = "macos")]
    {
        let args: Vec<String> = std::env::args().collect();
        if args.get(1).is_some_and(|a| a == "--locked-use-watchdog") {
            return muster_platform::macos::locked_use::watchdog(&args[2..])
                .map_err(anyhow::Error::msg);
        }
    }
    tokio::runtime::Runtime::new()?.block_on(run())
}
async fn run() -> anyhow::Result<()> {
    let args = Args::parse();
    let mut logger =
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"));
    if args.log {
        let dir = muster_paths::log_dir()
            .ok_or_else(|| anyhow::anyhow!("no log directory: set HOME or MUSTER_DIR"))?;
        std::fs::create_dir_all(&dir)?;
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("musterd.log"))?;
        logger.target(env_logger::Target::Pipe(Box::new(file)));
    }
    logger.init();
    let dir = args
        .dir
        .or_else(muster_paths::state_dir)
        .ok_or_else(|| anyhow::anyhow!("no --dir and no HOME"))?;
    let mut config = DaemonConfig::new(dir);
    config.socket = args.socket;
    config.bind = args.bind;
    config.relay = args.relay;
    config.name = args.name;
    config.kind = parse_kind(&args.kind)
        .ok_or_else(|| anyhow::anyhow!("--kind must be desktop, phone or server"))?;
    config.vnet = args.vnet && !args.no_vnet;
    config.allow_loopback = args.loopback;
    if args.no_agent {
        config.agent = None;
    }
    // Codex composer suggestions: one short-lived app-server per completed
    // turn of a watched session.
    if let Some(agent) = config.agent.as_mut() {
        agent.predictor = Some(std::sync::Arc::new(
            muster_agent::prediction::AppServer::default(),
        ));
    }
    config.cloud = !args.no_cloud;
    config.terminal = match args.terminal.as_str() {
        "off" => None,
        "iterm2" => Some(muster_terminal::TerminalKind::Iterm2(
            muster_terminal::TerminalConfig::default(),
        )),
        "pty" => Some(muster_terminal::TerminalKind::Pty(
            muster_terminal::pty::PtyConfig::default(),
        )),
        #[cfg(target_os = "macos")]
        "ghostty" => Some(musterd::ghostty::ghostty()),
        #[cfg(target_os = "macos")]
        "auto" => Some(musterd::ghostty::auto()),
        #[cfg(not(target_os = "macos"))]
        "auto" => Some(muster_terminal::TerminalKind::Pty(
            muster_terminal::pty::PtyConfig::default(),
        )),
        other => anyhow::bail!("--terminal {other:?}: auto, iterm2, ghostty, pty or off"),
    };
    config.authorized_keys = args.authorized_keys;
    let daemon = Daemon::start(config).await?;
    let ctrl_c = tokio::signal::ctrl_c();
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        tokio::select! {
            _ = ctrl_c => {}
            _ = term.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        ctrl_c.await?;
    }
    log::info!("musterd stopping");
    daemon.shutdown().await;
    Ok(())
}
