//! muster: talk to the local musterd.

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use muster_proto::v1::{self, request::Req, response::Resp};
use muster_rpc::Client;

#[derive(Parser, Debug)]
#[command(name = "muster", about = "Muster command line")]
struct Args {
    /// musterd socket (default: musterd.sock in $MUSTER_DIR or the platform's
    /// state directory, e.g. ~/Library/Application Support/Muster on macOS)
    #[arg(long)]
    socket: Option<PathBuf>,
    /// Address a member device (name, fingerprint or id prefix) instead of this one
    #[arg(long)]
    target: Option<String>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Inspect a native question form through a signed, covered Locked Use lease.
    Inspect {
        session: String,
    },
    /// Call any shared daemon RPC from a JSON file (or - for stdin); emits JSON lines.
    Rpc {
        #[arg(long)]
        request: PathBuf,
        #[arg(long, default_value_t = 30)]
        timeout: u64,
    },
    /// Print the authoritative shared RPC schema without connecting to a daemon.
    Schema,

    /// This device
    Info,
    /// Network, members, link state
    Status,
    /// Atomically change the signed virtual address plan; restart desktop daemons afterwards.
    Readdress {
        cidr: String,
        /// Complete desktop/server assignments, for example Mac-A=192.168.13.11
        #[arg(long = "address", required = true)]
        addresses: Vec<String>,
    },
    /// Create a network on this device
    Create {
        name: String,
        #[arg(long, default_value = "")]
        cidr: String,
    },
    /// Print an invite (what the QR carries)
    Invite {
        #[arg(long, default_value_t = 0)]
        ttl: u32,
    },
    /// Join a network by invite text
    Join {
        invite: String,
        #[arg(long, default_value = "")]
        name: String,
        #[arg(long, default_value = "")]
        kind: String,
    },
    /// Joiners waiting for approval
    Pending,
    /// Approve a pending joiner (optionally with scopes)
    Approve {
        device: String,
        #[arg(long = "scope")]
        scopes: Vec<String>,
    },
    Reject {
        device: String,
        #[arg(long, default_value = "")]
        reason: String,
    },
    Remove {
        device: String,
    },
    /// Replace a member's scopes (and role). Modules: chats, agents and
    /// terminals are device-level switches; chats:<provider> (chatgpt,
    /// claude) and agents:<provider> (codex, claude, catdesk) count only under their
    /// switch. Also locked.control, browser.view, browser.control,
    /// net:<proto>/<port>, network.admin
    Scopes {
        device: String,
        scopes: Vec<String>,
        #[arg(long, default_value = "")]
        role: String,
    },
    /// Inspect the desktop's registered public Presence candidate.
    Presence,
    /// Approve/revoke a member's desktop Presence key as the network admin.
    EnrollPresence {
        device: String,
        #[arg(long, conflicts_with = "revoke")]
        grant_locked_control: bool,
        #[arg(long)]
        revoke: bool,
    },
    /// Show or change the local policy
    Policy {
        #[arg(long = "open-port")]
        open_ports: Vec<String>,
        #[arg(long)]
        allow_locked_control: Option<bool>,
        #[arg(long)]
        offer_agent: Option<bool>,
        #[arg(long)]
        offer_terminal: Option<bool>,
        /// Offer this machine as an external task scheduler's runtime
        /// (`muster acp`). Off until set: it admits work this machine's owner
        /// did not start.
        #[arg(long)]
        offer_tasks: Option<bool>,
        /// Let the Accessibility controller drive this app (bundle id, e.g. com.catpaw.cowork)
        #[arg(long = "ax-allow")]
        ax_allow: Vec<String>,
        /// Stop the Accessibility controller from driving this app (bundle id)
        #[arg(long = "ax-remove")]
        ax_remove: Vec<String>,
        /// Lend this machine's browser to members for this one origin
        /// ("https://example.test"): they read through it with
        /// `browser.view` and act with `browser.control`.
        #[arg(long)]
        web_origin: Option<String>,
        /// Close the web window
        #[arg(long, conflicts_with = "web_origin")]
        close_web: bool,
    },
    /// A loopback port here standing for a port on a member's loopback. The
    /// port stays open until this command ends.
    Forward {
        /// Member: device id, fingerprint prefix or name
        device: String,
        /// The port on that member's loopback
        port: u16,
        /// The port to listen on here; 0 asks the operating system for one
        #[arg(long, default_value_t = 0)]
        local_port: u16,
    },
    /// A loopback port here standing for a member's web window onto one
    /// origin, read with that member's browser. Open it in a browser here;
    /// the port stays open until this command ends.
    Window {
        /// Member: device id, fingerprint prefix or name
        device: String,
        /// The origin of the member's window ("https://example.test")
        origin: String,
        /// The port to listen on here; 0 asks the operating system for one
        #[arg(long, default_value_t = 0)]
        local_port: u16,
    },
    /// Agent Halo browser. Remote locked operations can be reviewed and authenticated in Muster.
    Browser {
        request: String,
        /// Open the exact request for review and hardware authentication in Muster (macOS).
        #[arg(long)]
        approve_in_app: bool,
    },
    /// Agent sessions
    Agents {
        #[arg(long, default_value = "")]
        provider: String,
    },
    /// Last messages of a session
    Messages {
        session: String,
        #[arg(long, default_value_t = 20)]
        tail: u32,
    },
    Send {
        session: String,
        text: String,
    },
    Respond {
        interaction: String,
        option: String,
        #[arg(default_value = "")]
        answer: String,
    },
    Interrupt {
        session: String,
    },
    /// Desktop Chat mode: the conversations the Claude or ChatGPT app lists
    /// (provider claude | codex); --all requests cached history, rebuilding when due
    Chats {
        provider: String,
        #[arg(long)]
        all: bool,
    },
    /// Show a desktop chat (a session id from `chats`) in its app, or a new
    /// one when omitted; then `send` / `messages` work on the printed id
    ChatOpen {
        provider: String,
        #[arg(default_value = "")]
        chat: String,
    },
    /// A desktop chat's model and effort choices (opens the pickers on the Mac)
    ChatOptions {
        session: String,
    },
    /// Choose a desktop chat's model and/or effort by name as offered
    ChatSet {
        session: String,
        #[arg(long, default_value = "")]
        model: String,
        #[arg(long, default_value = "")]
        effort: String,
    },
    /// Follow agent events (all sessions, or one)
    AgentWatch {
        #[arg(default_value = "")]
        session: String,
    },
    /// Terminal sessions
    Terminals,
    TermHistory {
        session: String,
        #[arg(default_value_t = 40)]
        lines: u32,
    },
    TermInput {
        session: String,
        text: String,
    },
    /// Point this daemon at a relay (host:port); remembered
    Relay {
        relay: String,
    },
    /// Ask macOS for Accessibility (drives the Claude desktop app); prints the state
    Accessibility,
    /// A platform diagnostic: ax-dump [pid|App], ax-sessions [pid|App], ax-chat [Claude|ChatGPT], ax-chat-mode "<Claude|ChatGPT> <chat|code>", procs <suffix>, locked-use, lock, lock-field, unlock-probe, unlock-key-probe
    Debug {
        what: String,
        #[arg(default_value = "")]
        arg: String,
    },
    /// Resize a terminal session (cols × rows)
    TermResize {
        session: String,
        cols: u32,
        rows: u32,
    },
    /// Follow device and network state
    Watch,
}

fn print_chat_options(o: &v1::AgentChatOptions) {
    println!("{}", o.session);
    for m in &o.models {
        println!(
            "  model  {} {} {}",
            if m.selected { "*" } else { " " },
            m.name,
            m.detail
        );
    }
    println!(
        "  effort {}",
        if o.effort.is_empty() { "-" } else { &o.effort }
    );
    for e in &o.efforts {
        println!("         {} {}", if e.selected { "*" } else { " " }, e.name);
    }
    if !o.note.is_empty() {
        println!("  note   {}", o.note);
    }
}

fn socket_path(args: &Args) -> PathBuf {
    args.socket
        .clone()
        .or_else(|| muster_paths::state_dir().map(|d| muster_paths::socket_path(&d)))
        .unwrap_or_else(|| PathBuf::from("musterd.sock"))
}

fn op_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("cli-{nanos}-{}", std::process::id())
}

/// Holds a loopback port here for a member's `port`, or for its web window
/// onto `window` when that is set, until the call ends.
async fn forward(
    client: &Client,
    device: &str,
    local_port: u16,
    port: u32,
    window: String,
) -> anyhow::Result<()> {
    let device = resolve_device(client, device).await?;
    let what = if window.is_empty() {
        format!("127.0.0.1:{port}")
    } else {
        format!("its window onto {window}")
    };
    let mut call = client
        .start(Req::ForwardOpen(v1::ForwardOpenRequest {
            device: device.clone(),
            port,
            local_port: u32::from(local_port),
            window,
        }))
        .await?;
    while let Some(ev) = call.events.recv().await {
        if let v1::event::Ev::ForwardOpened(opened) = ev {
            println!(
                "127.0.0.1:{} stands for {what} on {}",
                opened.local_port,
                &device[..12]
            );
            break;
        }
    }
    // The port lives as long as this call: until the daemon stops or this
    // command is interrupted. A member's refusal is per connection.
    call.finish().await?;
    Ok(())
}

async fn resolve_device(client: &Client, text: &str) -> anyhow::Result<String> {
    if text.len() == 64 && text.chars().all(|c| c.is_ascii_hexdigit()) {
        return Ok(text.to_ascii_lowercase());
    }
    let mut candidates = Vec::new();
    if let Resp::NetworkStatus(status) = client
        .call(Req::NetworkStatus(v1::NetworkStatusRequest {}))
        .await?
    {
        for m in status.members {
            if m.device_id.starts_with(text) || m.fingerprint.starts_with(text) || m.name == text {
                candidates.push(m.device_id);
            }
        }
    }
    if let Resp::PendingJoins(pending) = client
        .call(Req::NetworkPending(v1::NetworkPendingRequest {}))
        .await?
    {
        for p in pending.joins {
            if p.device_id.starts_with(text) || p.fingerprint.starts_with(text) || p.name == text {
                candidates.push(p.device_id);
            }
        }
    }
    candidates.sort();
    candidates.dedup();
    match candidates.len() {
        1 => Ok(candidates.remove(0)),
        0 => anyhow::bail!("no device matches {text:?}"),
        _ => anyhow::bail!("{text:?} is ambiguous"),
    }
}

fn print_status(s: &v1::NetworkStatus) {
    if !s.has_network {
        println!("no network (self {})", &s.self_id[..16]);
        println!("relay: {}", s.relay);
        return;
    }
    println!(
        "network {} ({}) vnet {}",
        s.name,
        &s.network_id[..16],
        s.vnet_cidr
    );
    println!(
        "relay: {}  reflected: {}  pending joins: {}",
        s.relay, s.reflected_addr, s.pending_joins
    );
    println!(
        "{:<18} {:<8} {:<7} {:<15} {:<9} {:<7} scopes",
        "device", "kind", "role", "vip", "link", "rtt"
    );
    for m in &s.members {
        let link = if m.is_self {
            "self".to_string()
        } else if m.connected {
            m.path.clone()
        } else {
            "-".into()
        };
        println!(
            "{:<18} {:<8} {:<7} {:<15} {:<9} {:<7} {}",
            format!(
                "{} {}",
                m.name,
                &m.fingerprint[..8.min(m.fingerprint.len())]
            ),
            m.kind,
            m.role,
            m.vip,
            link,
            if m.rtt_ms > 0 {
                format!("{}ms", m.rtt_ms)
            } else {
                String::new()
            },
            m.scopes.join(",")
        );
    }
}

fn print_op(resp: Resp) -> anyhow::Result<()> {
    match resp {
        Resp::OpResult(r) => {
            let status =
                v1::op_result::Status::try_from(r.status).unwrap_or(v1::op_result::Status::Unknown);
            match status {
                v1::op_result::Status::Applied => println!("ok {}", r.result_id),
                v1::op_result::Status::Duplicate => println!("already applied {}", r.result_id),
                v1::op_result::Status::Rejected => anyhow::bail!("rejected: {}", r.message),
                v1::op_result::Status::Unknown => anyhow::bail!("outcome unknown: {}", r.message),
            }
        }
        Resp::Ack(_) => println!("ok"),
        other => println!("{other:?}"),
    }
    Ok(())
}

async fn json_rpc(client: &Client, path: &std::path::Path, timeout: u64) -> anyhow::Result<()> {
    use std::io::Read;
    anyhow::ensure!(
        (1..=3600).contains(&timeout),
        "timeout must be 1..3600 seconds"
    );
    let mut input: Box<dyn Read> = if path == std::path::Path::new("-") {
        Box::new(std::io::stdin())
    } else {
        Box::new(std::fs::File::open(path)?)
    };
    let mut bytes = Vec::new();
    input
        .by_ref()
        .take(8 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    anyhow::ensure!(bytes.len() <= 8 * 1024 * 1024, "JSON request exceeds 8 MiB");
    let request: Req = serde_json::from_slice(&bytes)?;
    let mut call = client.start(request).await?;
    let deadline = tokio::time::sleep(std::time::Duration::from_secs(timeout));
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            event = call.events.recv() => match event {
                Some(event) => println!("{}", serde_json::json!({"event": event})),
                None => break,
            },
            _ = &mut deadline => {
                call.cancel().await;
                anyhow::bail!("RPC timed out and was cancelled; do not automatically retry a mutation");
            }
            _ = tokio::signal::ctrl_c() => {
                call.cancel().await;
                anyhow::bail!("RPC cancelled; a mutation may already have taken effect");
            }
        }
    }
    match call.finish().await {
        Ok(response) => {
            println!("{}", serde_json::json!({"response": response}));
            if let Resp::OpResult(result) = response {
                anyhow::ensure!(
                    matches!(result.status, 0 | 1),
                    "operation rejected or unconfirmed: {}",
                    result.message
                );
            }
            Ok(())
        }
        Err(error) => {
            println!("{}", serde_json::json!({"error": error.to_string()}));
            Err(error.into())
        }
    }
}

/// How `muster acp` was invoked.
///
/// An external scheduler launches this command with the launch arguments of
/// whichever protocol family the runtime profile names — `--no-auto-update
/// agent --always-approve stdio`, say — appended to whatever the profile
/// itself set. Those are the scheduler's, not Muster's, so this reads the
/// options it owns and ignores the rest instead of refusing to start.
struct AcpArgs {
    agent: String,
    report_version: Option<String>,
    auth_methods: Vec<String>,
    socket: Option<PathBuf>,
    print_version: bool,
    /// Name prefixes whose variables this process passes to each session's
    /// provider child. The scheduler injects its task variables into this
    /// process; the child is spawned by musterd and inherits the daemon's
    /// environment instead, so without forwarding the agent has no
    /// credential to read its own issue or post its result.
    forward_prefixes: Vec<String>,
    /// Also give the child this process's PATH, so a CLI reachable here is
    /// reachable there. Off by default: it replaces the daemon's PATH for
    /// that child.
    forward_path: bool,
}

/// The variables to hand a session, read from this process's environment.
fn forwarded_env(args: &AcpArgs) -> std::collections::HashMap<String, String> {
    let mut env: std::collections::HashMap<String, String> = std::env::vars()
        .filter(|(name, _)| {
            args.forward_prefixes
                .iter()
                .any(|prefix| name.starts_with(prefix))
        })
        .collect();
    if args.forward_path {
        if let Ok(path) = std::env::var("PATH") {
            env.insert("PATH".to_owned(), path);
        }
    }
    env
}

fn parse_acp_args(argv: &[String]) -> AcpArgs {
    let mut parsed = AcpArgs {
        agent: "claude".to_owned(),
        report_version: None,
        auth_methods: Vec::new(),
        socket: None,
        print_version: false,
        forward_prefixes: Vec::new(),
        forward_path: false,
    };
    let mut rest = argv.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--version" | "-V" => parsed.print_version = true,
            "--forward-env" => {
                if let Some(value) = rest.next() {
                    parsed.forward_prefixes.push(value.clone());
                }
            }
            "--forward-path" => parsed.forward_path = true,
            "--agent" => {
                if let Some(value) = rest.next() {
                    parsed.agent = value.clone();
                }
            }
            "--report-version" => parsed.report_version = rest.next().cloned(),
            "--auth-method" => {
                if let Some(value) = rest.next() {
                    parsed.auth_methods.push(value.clone());
                }
            }
            "--socket" => parsed.socket = rest.next().map(PathBuf::from),
            _ => {}
        }
    }
    if parsed.auth_methods.is_empty() {
        parsed.auth_methods.push("cached_token".to_owned());
    }
    parsed
}

/// Does this device offer itself as an external scheduler's task runtime?
///
/// The daemon owns the answer (`LocalPolicy::offer_tasks`); this only reads
/// it. A reply that is not a policy is treated as a refusal rather than
/// guessed at.
async fn offers_tasks(client: &muster_rpc::Client) -> anyhow::Result<bool> {
    match client.call(Req::PolicyGet(v1::PolicyGetRequest {})).await? {
        Resp::Policy(policy) => Ok(policy.offer_tasks),
        other => anyhow::bail!("unexpected reply to policy.get: {other:?}"),
    }
}

/// Serve ACP on stdio with every turn running in `musterd` (ADR 0009).
async fn serve_acp(argv: &[String]) -> anyhow::Result<()> {
    let parsed = parse_acp_args(argv);
    let identity = muster_acp::Identity {
        version: parsed
            .report_version
            .clone()
            .unwrap_or_else(|| muster_acp::VERSION.to_owned()),
        auth_methods: parsed.auth_methods.clone(),
    };
    let path = parsed
        .socket
        .clone()
        .or_else(|| muster_paths::state_dir().map(|d| muster_paths::socket_path(&d)))
        .unwrap_or_else(|| PathBuf::from("musterd.sock"));
    let stream = tokio::net::UnixStream::connect(&path)
        .await
        .map_err(|e| anyhow::anyhow!("musterd at {}: {e}", path.display()))?;
    let client = muster_rpc::Client::new(stream);
    // Offering this machine as a scheduler's runtime is off until its owner
    // turns it on, so the probe and the server both ask first and both refuse
    // together: a version a scheduler can read is a runtime it will register
    // and then send work to. An unreachable or unreadable policy refuses too —
    // not knowing whether the owner allowed this is not permission.
    if !offers_tasks(&client).await? {
        anyhow::bail!(
            "this device does not offer itself as a task runtime; turn it on with \
             `muster policy --offer-tasks true`"
        );
    }
    if parsed.print_version {
        // The probe reads stdout, so nothing else may be printed here.
        println!("{}", identity.version);
        return Ok(());
    }
    let host = std::sync::Arc::new(muster_acp::RpcHost::new(
        client,
        &parsed.agent,
        forwarded_env(&parsed),
    ));
    muster_acp::serve(tokio::io::stdin(), tokio::io::stdout(), host, identity).await
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let argv: Vec<String> = std::env::args().collect();
    // Handled before clap: see AcpArgs for why this subcommand must accept
    // arguments it does not own.
    if argv.get(1).map(String::as_str) == Some("acp") {
        return serve_acp(&argv[2..]).await;
    }
    let args = Args::parse();
    if matches!(&args.command, Command::Schema) {
        print!("{}", include_str!("../../muster-proto/proto/muster.proto"));
        return Ok(());
    }
    if let Command::Browser {
        request,
        approve_in_app,
    } = &args.command
    {
        let typed: muster_browser::Request = serde_json::from_str(request)?;
        typed.validate()?;
        if args.target.is_none() && !approve_in_app {
            let result = muster_browser::call(&muster_browser::socket_path()?, &typed).await?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            return Ok(());
        }
    }
    let path = socket_path(&args);
    let stream = tokio::net::UnixStream::connect(&path)
        .await
        .map_err(|e| anyhow::anyhow!("musterd at {}: {e}", path.display()))?;
    let local = Client::new(stream);
    let client = match &args.target {
        Some(target) => local.with_target(&resolve_device(&local, target).await?),
        None => local.clone(),
    };
    match args.command {
        Command::Schema => unreachable!(),
        Command::Rpc { request, timeout } => json_rpc(&client, &request, timeout).await?,
        Command::Inspect { session } => {
            let target = args
                .target
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("inspect requires --target"))?;
            let target = resolve_device(&local, target).await?;
            let arg = serde_json::json!({"target": target, "session": session, "op_id": op_id()})
                .to_string();
            match local
                .call(Req::DeviceDebug(v1::DeviceDebugRequest {
                    what: "locked-inspect".into(),
                    arg,
                }))
                .await?
            {
                Resp::DebugText(text) => println!("{}", text.text),
                other => anyhow::bail!("inspection did not return a diagnostic: {other:?}"),
            }
        }
        Command::Info => {
            if let Resp::DeviceInfo(info) = client
                .call(Req::DeviceInfo(v1::DeviceInfoRequest {}))
                .await?
            {
                println!("{} ({})", info.name, info.kind);
                println!("id: {}", info.device_id);
                println!("fingerprint: {}", info.fingerprint);
                println!(
                    "version {} on {}; services: {}",
                    info.version,
                    info.platform,
                    info.services.join(", ")
                );
                if let Some(state) = info.state {
                    println!(
                        "accessibility: {}",
                        if state.accessibility.is_empty() {
                            "unknown"
                        } else {
                            &state.accessibility
                        }
                    );
                    println!(
                        "locked: {}  console: {}  vip: {}  vnet: {}  relay: {}  uptime: {}s",
                        state.locked,
                        state.on_console,
                        state.vip,
                        state.vnet_up,
                        state.relay,
                        state.uptime_secs
                    );
                }
            }
        }
        Command::Status => {
            if let Resp::NetworkStatus(s) = client
                .call(Req::NetworkStatus(v1::NetworkStatusRequest {}))
                .await?
            {
                print_status(&s);
            }
        }
        Command::Create { name, cidr } => {
            if let Resp::NetworkStatus(s) = client
                .call(Req::NetworkCreate(v1::NetworkCreateRequest {
                    name,
                    vnet_cidr: cidr,
                }))
                .await?
            {
                print_status(&s);
            }
        }
        Command::Invite { ttl } => {
            if let Resp::InviteText(i) = client
                .call(Req::NetworkInvite(v1::NetworkInviteRequest {
                    ttl_secs: ttl,
                }))
                .await?
            {
                println!("{}", i.text);
                eprintln!("expires at unix {}", i.expires_unix);
            }
        }
        Command::Join { invite, name, kind } => {
            let mut call = client
                .start(Req::NetworkJoin(v1::NetworkJoinRequest {
                    invite_text: invite,
                    device_name: name,
                    device_kind: kind,
                }))
                .await?;
            while let Some(ev) = call.events.recv().await {
                if let v1::event::Ev::JoinProgress(p) = ev {
                    let phase = v1::join_progress::Phase::try_from(p.phase)
                        .unwrap_or(v1::join_progress::Phase::Failed);
                    eprintln!("{phase:?}: {}", p.message);
                }
            }
            if let Resp::NetworkStatus(s) = call.finish().await? {
                print_status(&s);
            }
        }
        Command::Pending => {
            if let Resp::PendingJoins(p) = client
                .call(Req::NetworkPending(v1::NetworkPendingRequest {}))
                .await?
            {
                if p.joins.is_empty() {
                    println!("no pending joins");
                }
                for j in p.joins {
                    println!(
                        "{} {} ({}) fingerprint {}",
                        &j.device_id[..16],
                        j.name,
                        j.kind,
                        j.fingerprint
                    );
                }
            }
        }
        Command::Approve { device, scopes } => {
            let device_id = resolve_device(&client, &device).await?;
            print_op(
                client
                    .call(Req::NetworkDecide(v1::NetworkDecideRequest {
                        device_id,
                        approve: true,
                        scopes,
                        reason: String::new(),
                    }))
                    .await?,
            )?;
        }
        Command::Reject { device, reason } => {
            let device_id = resolve_device(&client, &device).await?;
            print_op(
                client
                    .call(Req::NetworkDecide(v1::NetworkDecideRequest {
                        device_id,
                        approve: false,
                        scopes: Vec::new(),
                        reason,
                    }))
                    .await?,
            )?;
        }
        Command::Remove { device } => {
            let device_id = resolve_device(&client, &device).await?;
            if let Resp::NetworkStatus(s) = client
                .call(Req::NetworkRemove(v1::NetworkRemoveRequest { device_id }))
                .await?
            {
                print_status(&s);
            }
        }
        Command::Scopes {
            device,
            scopes,
            role,
        } => {
            let device_id = resolve_device(&client, &device).await?;
            if let Resp::NetworkStatus(s) = client
                .call(Req::NetworkSetScopes(v1::NetworkSetScopesRequest {
                    device_id,
                    scopes,
                    role,
                }))
                .await?
            {
                print_status(&s);
            }
        }
        Command::Presence => {
            let Resp::PresenceInfo(info) =
                client.call(Req::DevicePresence(Default::default())).await?
            else {
                anyhow::bail!("unexpected Presence reply");
            };
            println!(
                "candidate_registered={} enrolled={}",
                !info.public_key.is_empty(),
                info.enrolled
            );
        }
        Command::EnrollPresence {
            device,
            grant_locked_control,
            revoke,
        } => {
            let device_id = resolve_device(&client, &device).await?;
            let Resp::PresenceInfo(info) = client
                .call(Req::NetworkPresence(v1::NetworkPresenceRequest {
                    device_id,
                    grant_locked_control,
                    revoke,
                }))
                .await?
            else {
                anyhow::bail!("unexpected Presence reply");
            };
            println!(
                "Presence {}",
                if revoke {
                    "revoked"
                } else if info.enrolled {
                    "enrolled"
                } else {
                    "not enrolled"
                }
            );
        }
        Command::Policy {
            open_ports,
            allow_locked_control,
            offer_agent,
            offer_terminal,
            offer_tasks,
            ax_allow,
            ax_remove,
            web_origin,
            close_web,
        } => {
            let Resp::Policy(mut policy) =
                client.call(Req::PolicyGet(v1::PolicyGetRequest {})).await?
            else {
                anyhow::bail!("unexpected reply");
            };
            let change = !open_ports.is_empty()
                || allow_locked_control.is_some()
                || offer_agent.is_some()
                || offer_terminal.is_some()
                || offer_tasks.is_some()
                || !ax_allow.is_empty()
                || !ax_remove.is_empty()
                || web_origin.is_some()
                || close_web;
            if change {
                for bundle in ax_allow {
                    if !policy.ax_allowlist.contains(&bundle) {
                        policy.ax_allowlist.push(bundle);
                    }
                }
                policy.ax_allowlist.retain(|b| !ax_remove.contains(b));
                if !open_ports.is_empty() {
                    policy.open_ports = open_ports;
                }
                if let Some(v) = allow_locked_control {
                    policy.allow_locked_control = v;
                }
                if let Some(v) = offer_agent {
                    policy.offer_agent = v;
                }
                if let Some(v) = offer_terminal {
                    policy.offer_terminal = v;
                }
                if let Some(v) = offer_tasks {
                    policy.offer_tasks = v;
                }
                if close_web {
                    policy.web_window = None;
                } else if let Some(origin) = web_origin {
                    policy.web_window = Some(v1::WebWindow { origin });
                }
                if let Resp::Policy(p) = client
                    .call(Req::PolicySet(v1::PolicySetRequest {
                        policy: Some(policy.clone()),
                    }))
                    .await?
                {
                    policy = p;
                }
            }
            println!("open ports: {}", policy.open_ports.join(", "));
            println!("allow locked control: {}", policy.allow_locked_control);
            println!(
                "offer agent: {}  offer terminal: {}  offer tasks: {}",
                policy.offer_agent, policy.offer_terminal, policy.offer_tasks
            );
            println!("ax allowlist: {}", policy.ax_allowlist.join(", "));
            match &policy.web_window {
                None => println!("web window: off"),
                Some(window) => {
                    println!("web window: {} through this machine's browser", window.origin);
                    println!(
                        "  members read with browser.view and act with browser.control{}",
                        if policy.allow_locked_control {
                            ", also while the screen is locked"
                        } else {
                            "; not while the screen is locked"
                        }
                    );
                    if !policy.offer_agent {
                        println!("  closed to members while offer agent is off");
                    }
                }
            }
        }
        Command::Forward {
            device,
            port,
            local_port,
        } => {
            forward(&client, &device, local_port, u32::from(port), String::new()).await?;
        }
        Command::Window {
            device,
            origin,
            local_port,
        } => {
            forward(&client, &device, local_port, 0, origin).await?;
        }
        Command::Browser {
            request,
            approve_in_app,
        } => {
            if approve_in_app {
                let target = args
                    .target
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("--approve-in-app requires --target"))?;
                let target = resolve_device(&local, target).await?;
                let mut url = url::Url::parse("muster://browser")?;
                url.query_pairs_mut()
                    .append_pair("device", &target)
                    .append_pair("request", &request);
                #[cfg(target_os = "macos")]
                {
                    anyhow::ensure!(
                        std::process::Command::new("/usr/bin/open")
                            .args(["-b", "dev.linsheng.muster", url.as_str()])
                            .status()?
                            .success(),
                        "could not open Muster"
                    );
                    println!(
                        "Review the browser request in Muster and choose Run & authenticate. The result appears there."
                    );
                }
                #[cfg(not(target_os = "macos"))]
                anyhow::bail!(
                    "Use the Muster app on your phone or Mac to authenticate browser operations while locked"
                );
            } else {
                let response = client
                    .call(Req::BrowserCall(v1::BrowserCallRequest {
                        op_id: op_id(),
                        json: request,
                    }))
                    .await?;
                match response {
                    Resp::OpResult(r)
                        if matches!(
                            v1::op_result::Status::try_from(r.status),
                            Ok(v1::op_result::Status::Applied | v1::op_result::Status::Duplicate)
                        ) =>
                    {
                        println!("{}", r.result_id)
                    }
                    other => print_op(other)?,
                }
            }
        }
        Command::Agents { provider } => {
            if let Resp::AgentSessions(page) = client
                .call(Req::AgentList(v1::AgentListRequest { limit: 0, provider }))
                .await?
            {
                for s in page.sessions {
                    println!(
                        "{} {:<6} process={:<7} activity={:<16} {:<9} {}  [{}]",
                        &s.id[..8.min(s.id.len())],
                        s.provider,
                        if s.process_state.is_empty() {
                            "unknown"
                        } else {
                            &s.process_state
                        },
                        s.state,
                        s.controller,
                        s.title,
                        s.cwd
                    );
                }
            }
        }
        Command::Messages { session, tail } => {
            if let Resp::AgentMessages(page) = client
                .call(Req::AgentMessages(v1::AgentMessagesRequest {
                    session,
                    tail,
                    before_id: String::new(),
                    ..Default::default()
                }))
                .await?
            {
                for m in page.messages {
                    if let Some(u) = &m.usage {
                        println!(
                            "[turn_summary] end_unix={} model={} input={} output={} cache_read={} cache_hit={:.1}% tokens_known={} cost_usd={:.6} cost_known={} source={} pricing_date={} cache_hit_known={} requests={} first_hit={:.1}% first_hit_known={}",
                            m.ts_unix,
                            u.model,
                            u.input_tokens,
                            u.output_tokens,
                            u.cache_read_tokens,
                            u.cache_hit_percent,
                            u.tokens_known,
                            u.cost_usd,
                            u.cost_known,
                            u.cost_source,
                            u.pricing_date,
                            u.cache_hit_known,
                            u.request_count,
                            u.first_request_cache_hit_percent,
                            u.first_request_cache_hit_known
                        );
                        if let Some(expiry) = &u.cache_expiry {
                            println!(
                                "[cache_expiry] {} from_unix={} until_unix={} observed_gap_seconds={} {}",
                                expiry.label,
                                expiry.from_unix,
                                expiry.until_unix,
                                expiry.observed_gap_seconds,
                                expiry.detail
                            );
                        }
                        for change in &u.changes {
                            println!(
                                "[file_change] {} {} +{} -{} counts_known={} diff_bytes={} truncated={}",
                                change.kind,
                                change.path,
                                change.additions,
                                change.deletions,
                                change.counts_known,
                                change.diff.len(),
                                change.truncated
                            );
                        }
                    } else {
                        println!("[{}] {}: {}", m.kind, m.role, m.text);
                    }
                }
            }
        }
        Command::Chats { provider, all } => {
            if let Resp::AgentChats(catalog) = client
                .call(Req::AgentChats(v1::AgentChatsRequest {
                    provider,
                    full_history: all,
                    ..Default::default()
                }))
                .await?
            {
                if !catalog.app_running {
                    println!("the app is not running");
                } else if !catalog.chat_mode {
                    println!("the app is not in its Chat mode; `chat-open` switches it");
                }
                if !catalog.busy.is_empty() {
                    println!("in use by {} operation now", catalog.busy);
                }
                if catalog.agent_working {
                    println!("one of its agent sessions is running");
                }
                for chat in catalog.chats {
                    println!(
                        "{} {} {}",
                        if chat.current { "*" } else { " " },
                        chat.id,
                        chat.title
                    );
                }
                if all {
                    println!("history complete: {}", catalog.history_complete);
                    if !catalog.history_warning.is_empty() {
                        println!("{}", catalog.history_warning);
                    }
                }
                println!("new: {}", catalog.new_session);
            }
        }
        Command::ChatOpen { provider, chat } => {
            if let Resp::AgentSession(s) = client
                .call(Req::AgentChatOpen(v1::AgentChatOpenRequest {
                    provider,
                    chat,
                    ..Default::default()
                }))
                .await?
            {
                println!("{} {} activity={} {}", s.id, s.provider, s.state, s.title);
            }
        }
        Command::ChatOptions { session } => {
            if let Resp::AgentChatOptions(o) = client
                .call(Req::AgentChatOptions(v1::AgentChatOptionsRequest {
                    session,
                    ..Default::default()
                }))
                .await?
            {
                print_chat_options(&o);
            }
        }
        Command::ChatSet {
            session,
            model,
            effort,
        } => {
            if let Resp::AgentChatOptions(o) = client
                .call(Req::AgentChatSetOptions(v1::AgentChatSetOptionsRequest {
                    session,
                    model,
                    effort,
                    ..Default::default()
                }))
                .await?
            {
                print_chat_options(&o);
            }
        }
        Command::Send { session, text } => print_op(
            client
                .call(Req::AgentSend(v1::AgentSendRequest {
                    attachments: Vec::new(),
                    op_id: op_id(),
                    session,
                    text,
                    release_unconfirmed: String::new(),
                }))
                .await?,
        )?,
        Command::Respond {
            interaction,
            option,
            answer,
        } => print_op(
            client
                .call(Req::AgentRespond(v1::AgentRespondRequest {
                    answers: Vec::new(),
                    op_id: op_id(),
                    interaction,
                    option,
                    answer,
                    session: String::new(),
                }))
                .await?,
        )?,
        Command::Interrupt { session } => print_op(
            client
                .call(Req::AgentInterrupt(v1::AgentInterruptRequest {
                    op_id: op_id(),
                    session,
                }))
                .await?,
        )?,
        Command::AgentWatch { session } => {
            let mut call = client
                .start(Req::AgentWatch(v1::AgentWatchRequest { session }))
                .await?;
            while let Some(ev) = call.events.recv().await {
                if let v1::event::Ev::AgentEvent(v1::AgentEvent { ev: Some(ev) }) = ev {
                    match ev {
                        v1::agent_event::Ev::InteractionResolved(id) => println!("resolved {id}"),
                        v1::agent_event::Ev::ListChanged(page) => {
                            println!("list: {} sessions", page.sessions.len())
                        }
                        v1::agent_event::Ev::Message(m) => {
                            println!("[{}] {}: {}", m.kind, m.role, m.text)
                        }
                        v1::agent_event::Ev::State(s) => println!(
                            "state: {} process={} activity={} ({}){}",
                            &s.id[..8.min(s.id.len())],
                            if s.process_state.is_empty() {
                                "unknown"
                            } else {
                                &s.process_state
                            },
                            s.state,
                            s.controller,
                            if s.composer_suggestion.is_empty() {
                                String::new()
                            } else {
                                format!(" suggestion={:?}", s.composer_suggestion)
                            }
                        ),
                        v1::agent_event::Ev::Interaction(i) => println!(
                            "interaction {} [{}]: {} options {:?}",
                            i.id, i.kind, i.prompt, i.options
                        ),
                    }
                }
            }
        }
        Command::Terminals => {
            if let Resp::TerminalSessions(list) = client
                .call(Req::TerminalSessions(v1::TerminalSessionsRequest {
                    watch: false,
                }))
                .await?
            {
                for s in list.sessions {
                    println!("{} {}x{} {} [{}]", s.id, s.cols, s.rows, s.title, s.source);
                }
            }
        }
        Command::TermHistory { session, lines } => {
            if let Resp::TerminalLines(l) = client
                .call(Req::TerminalHistory(v1::TerminalHistoryRequest {
                    session,
                    lines,
                }))
                .await?
            {
                for line in l.lines {
                    println!("{}", line.text);
                }
            }
        }
        Command::TermInput { session, text } => print_op(
            client
                .call(Req::TerminalInput(v1::TerminalInputRequest {
                    op_id: op_id(),
                    session,
                    bytes: text.into_bytes(),
                }))
                .await?,
        )?,
        Command::Readdress { cidr, addresses } => {
            let mut resolved = std::collections::HashMap::new();
            for assignment in addresses {
                let (device, address) = assignment
                    .split_once('=')
                    .ok_or_else(|| anyhow::anyhow!("expected DEVICE=IP"))?;
                let id = resolve_device(&client, device).await?;
                anyhow::ensure!(
                    resolved.insert(id, address.to_owned()).is_none(),
                    "duplicate device"
                );
            }
            match client
                .call(Req::NetworkReaddress(v1::NetworkReaddressRequest {
                    vnet_cidr: cidr,
                    addresses: resolved,
                }))
                .await?
            {
                Resp::NetworkStatus(s) => {
                    print_status(&s);
                    println!("Restart desktop daemons to apply the virtual interfaces.");
                }
                other => anyhow::bail!("unexpected response {other:?}"),
            }
        }
        Command::Relay { relay } => match client
            .call(Req::DeviceSetRelay(v1::DeviceSetRelayRequest { relay }))
            .await?
        {
            Resp::DeviceInfo(info) => println!(
                "relay {} ({})",
                info.relay,
                info.state.map(|s| s.relay).unwrap_or_default()
            ),
            other => anyhow::bail!("unexpected response {other:?}"),
        },
        Command::Debug { what, arg } => match client
            .call(Req::DeviceDebug(v1::DeviceDebugRequest { what, arg }))
            .await?
        {
            Resp::DebugText(t) => println!("{}", t.text),
            other => anyhow::bail!("unexpected response {other:?}"),
        },
        Command::Accessibility => match client
            .call(Req::DeviceRequestAccessibility(
                v1::DeviceRequestAccessibilityRequest {},
            ))
            .await?
        {
            Resp::DeviceInfo(info) => println!(
                "accessibility: {}",
                info.state.map(|s| s.accessibility).unwrap_or_default()
            ),
            other => anyhow::bail!("unexpected response {other:?}"),
        },
        Command::TermResize {
            session,
            cols,
            rows,
        } => {
            client
                .call(Req::TerminalResize(v1::TerminalResizeRequest {
                    session,
                    cols,
                    rows,
                }))
                .await?;
            println!("resized to {cols}x{rows}");
        }
        Command::Watch => {
            let mut call = client
                .start(Req::DeviceWatch(v1::DeviceWatchRequest {}))
                .await?;
            while let Some(ev) = call.events.recv().await {
                match ev {
                    v1::event::Ev::DeviceState(s) => println!(
                        "device: locked={} vip={} vnet={} relay={} up={}s",
                        s.locked, s.vip, s.vnet_up, s.relay, s.uptime_secs
                    ),
                    v1::event::Ev::NetworkStatus(s) => print_status(&s),
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rpc_json_rejects_unknown_fields_instead_of_dropping_mutation_parameters() {
        let request: Req = serde_json::from_str(r#"{"agent_respond":{"session":"s","interaction":"q","op_id":"one","answers":[{"question_id":"q1","option_ids":["a"]}]}}"#).unwrap();
        assert!(matches!(request, Req::AgentRespond(_)));
        assert!(serde_json::from_str::<Req>(r#"{"agent_respond":{"sessoin":"s"}}"#).is_err());
        assert!(
            serde_json::from_str::<Req>(
                r#"{"agent_inspect":{"session":"s","op_id":"one","answers":[]}}"#
            )
            .is_err()
        );
        assert!(serde_json::from_str::<Req>(r#"{"unknown_method":{}}"#).is_err());
    }
}
