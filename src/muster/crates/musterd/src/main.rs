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
    /// Offer terminals: "auto" (on macOS all installed iTerm2/Ghostty sources; a PTY shell elsewhere), "iterm2", "ghostty", "pty" or "off"
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
    // The AppKit panel bodies (the idle-compaction banner and the Locked Use
    // console cover) are no longer subcommands here. Run from
    // `Contents/MacOS`, each one registered with Launch Services as a second
    // running application under the app's bundle id, which is how the app's
    // windows went missing from the Accessibility API. They are the
    // `muster-panel` helper in `Contents/Helpers` now (ADR 0128).
    //
    // The daemon itself is still registered that way, and with the
    // registration comes a window-server connection whose event queue the
    // window server expects the main thread to read. A main thread parked in
    // `block_on` never reads it, so the window server flags the daemon
    // unresponsive and Activity Monitor shows it "Not Responding". Inside the
    // bundle the runtime moves to its own thread and the main thread serves
    // that queue until the runtime ends the process (ADR 0164).
    #[cfg(target_os = "macos")]
    if let Some(events) = muster_platform::macos::events::AppEvents::take() {
        let left_the_dock = events.leave_the_dock();
        std::thread::Builder::new()
            .name("musterd-runtime".into())
            // The main thread's size: the daemon's future was sized for it.
            .stack_size(8 << 20)
            .spawn(move || {
                let outcome = std::panic::catch_unwind(move || run_daemon(Some(left_the_dock)));
                let code = exit_code(outcome);
                let _ = std::io::Write::flush(&mut std::io::stdout());
                std::process::exit(code);
            })?;
        events.serve();
    }
    run_daemon(None)
}

/// `left_the_dock` is `None` when the main thread is not serving AppKit
/// events (outside an app bundle, or not macOS).
fn run_daemon(left_the_dock: Option<bool>) -> anyhow::Result<()> {
    tokio::runtime::Runtime::new()?.block_on(run(left_the_dock))
}

/// The status `main` returning `outcome` would have exited with: Rust prints
/// `Error: {e:?}` and exits 1 for an error, and exits 101 after a panic, whose
/// message the panic hook has already printed.
#[cfg(target_os = "macos")]
fn exit_code(outcome: std::thread::Result<anyhow::Result<()>>) -> i32 {
    match outcome {
        Ok(Ok(())) => 0,
        Ok(Err(e)) => {
            eprintln!("Error: {e:?}");
            1
        }
        Err(_) => 101,
    }
}

async fn run(left_the_dock: Option<bool>) -> anyhow::Result<()> {
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
    // The Dock tile went in `main`, before the runtime started: the
    // prohibited activation policy is recorded as `BackgroundOnly`, which has
    // no tile (ADR 0132's outcome, now through AppKit; ADR 0164). A refusal is
    // logged rather than fatal, as before.
    match left_the_dock {
        Some(true) => log::info!("main thread serving window-server events; out of the Dock"),
        Some(false) => log::warn!(
            "main thread serving window-server events; could not leave the Dock: \
             setActivationPolicy(prohibited) was refused"
        ),
        None => {}
    }
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
        "auto" => {
            let mut hosts = musterd::ghostty::auto().into_iter();
            let primary = hosts.next();
            config.additional_terminals = hosts.collect();
            primary
        }
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

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::exit_code;

    #[test]
    fn exits_as_main_returning_the_outcome_would() {
        assert_eq!(exit_code(Ok(Ok(()))), 0);
        assert_eq!(exit_code(Ok(Err(anyhow::anyhow!("no --dir")))), 1);
        assert_eq!(exit_code(Err(Box::new("a panic"))), 101);
    }
}
