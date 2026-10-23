//! Terminals: iTerm2 / Ghostty / PTY sessions on a device and in-app SSH.
//! A locked Mac shows a Ghostty terminal only inside a Locked Use terminal
//! session, which this phone keeps open while the terminal is shown.

use super::*;

pub(super) fn open_terminals(state: &mut State, ctx: &Ctx) {
    let filter = if state.terminals.device_id.is_empty() {
        "*".to_string()
    } else {
        state.terminals.device_id.clone()
    };
    state.go(Screen::Terminals);
    let devices: Vec<_> = state
        .reachable_service_devices()
        .into_iter()
        .filter(|m| filter == "*" || m.device_id == filter)
        .collect();
    state.terminals_epoch += 1;
    let epoch = state.terminals_epoch;
    state.terminals_pending = devices.len();
    state.terminals = app::TerminalsView {
        device_id: filter.clone(),
        sessions: Vec::new(),
        ssh_available: filter != "*" && state.embedded && !state.is_self(&filter),
        loading: !devices.is_empty(),
        errors: Vec::new(),
        can_create: false,
        sources: Vec::new(),
    };
    for member in devices {
        let device = member.device_id;
        let name = member.name;
        let self_id = if member.is_self {
            device.clone()
        } else {
            state.self_id.clone()
        };
        let backend = ctx.backend.clone();
        spawn(ctx, async move {
            let result = tokio::time::timeout(Duration::from_secs(3), async {
                let client = backend.client_for(&device, &self_id).await?;
                match unary(
                    &client,
                    Req::TerminalSessions(v1::TerminalSessionsRequest { watch: false }),
                )
                .await?
                {
                    Resp::TerminalSessions(mut list) => {
                        for session in &mut list.sessions {
                            session.device_id = device.clone();
                            session.device_name = name.clone();
                        }
                        Ok(list)
                    }
                    _ => Err(unexpected()),
                }
            })
            .await
            .unwrap_or_else(|_| Err(Error::Unavailable("connection timed out".into())))
            .map_err(|error| Error::Unavailable(format!("{name}: {error}")));
            Msg::Terminals {
                epoch,
                device,
                result,
            }
        });
    }
}

pub(super) fn open_terminal(state: &mut State, ctx: &Ctx, session: String) {
    state.detach_session_terminal();
    let device = state.target();
    let (watch, token) = state.new_watch();
    let meta = state
        .terminals
        .sessions
        .iter()
        .find(|s| s.id == session && (s.device_id.is_empty() || s.device_id == device))
        .cloned();
    state.terminal = Some(Terminal {
        device: device.clone(),
        session: session.clone(),
        kind: TerminalKind::Remote {
            _attach: Some(watch),
            lease: None,
            leased: false,
            input: Default::default(),
        },
        source: meta
            .as_ref()
            .map(|m| m.source.clone())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "iterm2".into()),
        cols: meta.as_ref().map(|m| m.cols).unwrap_or(80),
        rows: meta.as_ref().map(|m| m.rows).unwrap_or(24),
        lines: Vec::new(),
        cursor: (0, 0),
        ended: false,
        status: "attaching".into(),
        rev: 0,
        desktop_attach_command: String::new(),
        iterm2_attach_command: String::new(),
        native_sources: Vec::new(),
    });
    state.go(Screen::Terminal);
    attach(state, ctx, device, session, token);
}

/// Streams `session`'s frames until `token` ends it. A locked Mac that shows
/// this terminal only when unlocked (Ghostty) answers LockedUseRequired:
/// the terminal then asks for a Locked Use session.
fn attach(state: &State, ctx: &Ctx, device: String, session: String, token: CancellationToken) {
    let self_id = state.self_id.clone();
    let backend = ctx.backend.clone();
    let tx = ctx.tx.clone();
    tokio::spawn(async move {
        let outcome: Result<(), muster_rpc::Error> = async {
            let client = backend
                .client_for(&device, &self_id)
                .await
                .map_err(|e| muster_rpc::Error::Io(e.to_string()))?;
            let mut call = client
                .start(Req::TerminalAttach(v1::TerminalAttachRequest {
                    session: session.clone(),
                    since_rev: 0,
                }))
                .await?;
            loop {
                tokio::select! {
                    _ = token.cancelled() => { call.cancel().await; return Ok(()); }
                    ev = call.events.recv() => match ev {
                        Some(v1::event::Ev::TerminalFrame(frame)) => {
                            let _ = tx.send(loop_message(Msg::TerminalFrame { device: device.clone(), session: session.clone(), frame }));
                        }
                        Some(_) => {}
                        None => break,
                    }
                }
            }
            call.finish().await?;
            Ok(())
        }
        .await;
        if token.is_cancelled() {
            return;
        }
        let message = match outcome {
            Err(muster_rpc::Error::Remote {
                code: v1::error::Code::LockedUseRequired,
                ..
            }) => Msg::TerminalLocked { device, session },
            outcome => Msg::WatchEnded {
                what: "terminal",
                device,
                session,
                error: outcome.err().map(|e| Error::from(e).to_string()),
            },
        };
        let _ = tx.send(loop_message(message));
    });
}

/// How often an open terminal session is renewed. Each signed window lasts
/// 15 s, so a missed renewal still leaves time for the next.
const LEASE_RENEWAL: std::time::Duration = std::time::Duration::from_secs(8);

/// The locked Mac shows this terminal only when unlocked: open a Locked Use
/// terminal session (the phone's device key signs, no prompt), then attach
/// again. The session lives while this terminal is shown and renewed every
/// few seconds; leaving the terminal, the background or an error ends it.
pub(super) fn msg_terminal_locked(state: &mut State, ctx: &Ctx, device: String, session: String) {
    let phone = state.embedded
        && state
            .self_info
            .as_ref()
            .is_some_and(|info| info.kind == "phone");
    let foreground = state.foreground;
    let network = state.network.as_ref().map(|n| n.network_id.clone());
    let op_id = state.op_id();
    let (watch, token) = state.new_watch();
    let Some(terminal) = state
        .terminal
        .as_mut()
        .filter(|t| t.device == device && t.session == session)
    else {
        return;
    };
    let TerminalKind::Remote { lease, leased, .. } = &mut terminal.kind else {
        return;
    };
    let refused = if !phone {
        Some(
            "The Mac is locked: open this terminal from a phone, which keeps the Mac unlocked under its cover while you use it",
        )
    } else if !foreground {
        Some("The Mac is locked")
    } else if *leased {
        Some("The Mac is locked and its terminal session could not be kept open")
    } else {
        None
    };
    if let Some(why) = refused {
        terminal.ended = true;
        terminal.status = why.into();
        return;
    }
    *leased = true;
    *lease = Some(watch);
    terminal.status = "正在遮罩下解锁 Mac…".into();
    let self_id = state.self_id.clone();
    let backend = ctx.backend.clone();
    let tx = ctx.tx.clone();
    tokio::spawn(async move {
        let mut nonce = None;
        let result: Result<(), Error> = async {
            let client = backend.client_for(&device, &self_id).await?;
            let signer = backend
                .signer()
                .ok_or_else(|| Error::Unavailable("device signing key unavailable".into()))?;
            let network: [u8; 32] = hex::decode(network.ok_or_else(unexpected)?)
                .map_err(|_| unexpected())?
                .try_into()
                .map_err(|_| unexpected())?;
            let target = muster_core::DeviceId::from_hex(&device).map_err(|_| unexpected())?;
            let requester = muster_core::DeviceId::from_hex(&self_id).map_err(|_| unexpected())?;
            let operation = muster_proto::locked_use::terminal(&op_id, &session);
            let digest = muster_proto::locked_use::digest(&operation);
            // Rust checks every challenge, first or renewal, against this
            // network, Mac, phone, session and operation before signing.
            let sign = |bytes: &[u8]| -> Result<Vec<u8>, Error> {
                let c = muster_core::locked_use::Challenge::decode(bytes)
                    .map_err(|e| Error::Invalid(e.to_string()))?;
                let message = c
                    .signing_message_for(&network, target, requester, &op_id, &digest)
                    .map_err(|e| Error::Invalid(e.to_string()))?;
                signer
                    .sign(&message)
                    .map_err(|e| Error::Unauthorized(e.to_string()))
            };
            let Resp::LockedUseChallenge(first) = unary(
                &client,
                Req::LockedUsePrepare(v1::LockedUsePrepareRequest {
                    operation: Some(operation.clone()),
                }),
            )
            .await?
            else {
                return Err(unexpected());
            };
            nonce = Some(
                muster_core::locked_use::Challenge::decode(&first.challenge)
                    .map_err(|e| Error::Invalid(e.to_string()))?
                    .nonce()
                    .to_vec(),
            );
            let signature = sign(&first.challenge)?;
            let opened = tokio::select! {
                _ = token.cancelled() => return Ok(()),
                opened = unary(&client, Req::LockedUseExecute(v1::LockedUseExecuteRequest {
                    challenge: first.challenge,
                    signature,
                })) => opened?,
            };
            match opened {
                Resp::OpResult(r) if r.status == v1::op_result::Status::Applied as i32 => {}
                Resp::OpResult(r) => return Err(Error::Unavailable(r.message)),
                _ => return Err(unexpected()),
            }
            let _ = tx.send(loop_message(Msg::TerminalLeaseOpened {
                device: device.clone(),
                session: session.clone(),
            }));
            loop {
                tokio::select! {
                    _ = token.cancelled() => return Ok(()),
                    _ = tokio::time::sleep(LEASE_RENEWAL) => {}
                }
                let Resp::LockedUseChallenge(next) = unary(
                    &client,
                    Req::LockedUseRenew(v1::LockedUseRenewRequest {
                        nonce: nonce.clone().unwrap_or_default(),
                    }),
                )
                .await?
                else {
                    return Err(unexpected());
                };
                let signature = sign(&next.challenge)?;
                unary(
                    &client,
                    Req::LockedUseExtend(v1::LockedUseExtendRequest {
                        challenge: next.challenge,
                        signature,
                    }),
                )
                .await?;
            }
        }
        .await;
        // Relock now rather than when the signed window runs out.
        if let Some(nonce) = nonce
            && let Ok(client) = backend.client_for(&device, &self_id).await
        {
            let reason = if token.is_cancelled() {
                "user"
            } else {
                "expired"
            };
            let _ = tokio::time::timeout(
                std::time::Duration::from_secs(3),
                unary(
                    &client,
                    Req::LockedUseCancel(v1::LockedUseCancelRequest {
                        nonce,
                        reason: reason.into(),
                    }),
                ),
            )
            .await;
        }
        if let Err(error) = result
            && !token.is_cancelled()
        {
            let _ = tx.send(loop_message(Msg::TerminalLeaseEnded {
                device,
                session,
                error: error.to_string(),
            }));
        }
    });
}

pub(super) fn msg_terminal_lease_opened(
    state: &mut State,
    ctx: &Ctx,
    device: String,
    session: String,
) {
    let (watch, token) = state.new_watch();
    let Some(terminal) = state
        .terminal
        .as_mut()
        .filter(|t| t.device == device && t.session == session)
    else {
        return;
    };
    let TerminalKind::Remote { _attach, .. } = &mut terminal.kind else {
        return;
    };
    *_attach = Some(watch);
    terminal.ended = false;
    terminal.status = "Mac 在遮罩下保持解锁 · 离开终端或 10 分钟无输入后锁屏".into();
    attach(state, ctx, device, session, token);
}

pub(super) fn msg_terminal_lease_ended(
    state: &mut State,
    device: String,
    session: String,
    error: String,
) {
    if let Some(terminal) = state
        .terminal
        .as_mut()
        .filter(|t| t.device == device && t.session == session)
    {
        if let TerminalKind::Remote { lease, .. } = &mut terminal.kind {
            *lease = None;
        }
        terminal.ended = true;
        terminal.status = format!("The Mac locked again: {error}");
    }
}

/// The background ends a terminal session at once (the phone cannot keep
/// renewing it); the foreground opens the terminal again, and with it a new
/// session.
pub(super) fn background_terminal(state: &mut State) {
    if let Some(terminal) = state.terminal.as_mut()
        && let TerminalKind::Remote { input, _attach, .. } = &mut terminal.kind
    {
        input.stop();
        *_attach = None;
        terminal.ended = true;
        terminal.status = "Terminal detached while Muster was in the background".into();
    }
    if let Some(terminal) = state.terminal.as_mut()
        && let TerminalKind::Remote { lease, leased, .. } = &mut terminal.kind
        && lease.take().is_some()
    {
        *leased = false;
        terminal.ended = true;
        terminal.status = "The Mac locked again while Muster was in the background".into();
    }
}

pub(super) fn foreground_terminal(state: &mut State, ctx: &Ctx) {
    if state.screen == Screen::Chat && state.session_terminal.active {
        session_terminal_action(state, ctx, app::SessionTerminalAction { action: 0 });
        return;
    }
    if state.screen == Screen::Terminal
        && let Some(terminal) = &state.terminal
        && terminal.ended
        && matches!(terminal.kind, TerminalKind::Remote { .. })
    {
        let session = terminal.session.clone();
        state.selected = terminal.device.clone();
        open_terminal(state, ctx, session);
    }
}

pub(super) fn terminal_input(state: &mut State, ctx: &Ctx, bytes: Vec<u8>) {
    if !state.foreground
        || (state.screen != Screen::Terminal
            && !(state.screen == Screen::Chat
                && state.session_terminal.active
                && !state.session_terminal.busy))
    {
        return;
    }
    let Some(terminal) = &mut state.terminal else {
        return;
    };
    if terminal.ended || bytes.is_empty() {
        return;
    }
    match &mut terminal.kind {
        TerminalKind::Ssh { input, .. } => {
            let _ = input.send(SshCommand::Write(bytes));
        }
        TerminalKind::Remote { input, .. } => {
            if input.stopped {
                return;
            }
            if bytes.len() > muster_terminal::MAX_PASTE.saturating_sub(input.pending.len()) {
                input.stop();
                terminal.status =
                    "Input stopped: buffer full. Reopen this terminal before typing".into();
                state.toast(
                    "Terminal input buffer full; unsent input discarded. Reopen the terminal",
                    true,
                );
                return;
            }
            input.pending.extend(bytes);
            dispatch_terminal_input(state, ctx);
        }
    }
}

/// Called only by the reducer, before spawning I/O: intent order is byte order.
fn dispatch_terminal_input(state: &mut State, ctx: &Ctx) {
    let Some(terminal) = state.terminal.as_ref() else {
        return;
    };
    let TerminalKind::Remote { input, .. } = &terminal.kind else {
        return;
    };
    if terminal.ended || input.stopped || input.in_flight.is_some() || input.pending.is_empty() {
        return;
    }
    let op_id = state.op_id();
    let terminal = state.terminal.as_mut().unwrap();
    let TerminalKind::Remote { input, .. } = &mut terminal.kind else {
        return;
    };
    input.in_flight = Some(op_id.clone());
    let request = v1::TerminalInputRequest {
        op_id: op_id.clone(),
        session: terminal.session.clone(),
        bytes: std::mem::take(&mut input.pending),
    };
    let device = terminal.device.clone();
    let origin = Some((terminal.session.clone(), op_id));
    op_request_from(
        state,
        ctx,
        "input",
        device,
        Req::TerminalInput(request),
        origin,
    );
}

pub(super) fn msg_terminal_input_done(
    state: &mut State,
    ctx: &Ctx,
    device: String,
    origin: Option<(String, String)>,
    result: Result<v1::OpResult, Error>,
) {
    let Some((session, op_id)) = origin else {
        return;
    };
    let Some(terminal) = state
        .terminal
        .as_mut()
        .filter(|t| t.device == device && t.session == session)
    else {
        return;
    };
    let TerminalKind::Remote { input, .. } = &mut terminal.kind else {
        return;
    };
    if input.in_flight.as_deref() != Some(&op_id) || input.stopped {
        return;
    }
    input.in_flight = None;
    if matches!(&result, Ok(r) if matches!(v1::op_result::Status::try_from(r.status),
        Ok(v1::op_result::Status::Applied | v1::op_result::Status::Duplicate)))
    {
        dispatch_terminal_input(state, ctx);
    } else {
        let why = match result {
            Ok(r) => r.message,
            Err(e) => e.to_string(),
        };
        input.stop();
        terminal.status = format!("Input stopped: {why}. Reopen this terminal before typing");
        state.toast(
            "Terminal input was not confirmed; unsent input discarded. Reopen the terminal",
            true,
        );
    }
}

/// Trust on first use, remembered in the embedded daemon's store.
pub(super) struct Tofu {
    expected: Option<String>,
}

impl ssh::HostVerifier for Tofu {
    fn accept(&self, _key_type: &str, fingerprint: &str) -> bool {
        self.expected.as_deref().is_none_or(|e| e == fingerprint)
    }
}

pub(super) fn open_ssh(state: &mut State, ctx: &Ctx, o: app::OpenSsh) {
    state.detach_session_terminal();
    let device = state.target();
    if !state.embedded || state.is_self(&device) {
        state.toast(
            "in-app SSH is for reaching other devices from a phone",
            true,
        );
        return;
    }
    let port = if o.port == 0 { 22 } else { o.port as u16 };
    let cols = o.cols.clamp(2, 500) as u16;
    let rows = o.rows.clamp(1, 300) as u16;
    let user = if o.user.is_empty() {
        state
            .self_info
            .as_ref()
            .map(|i| i.name.clone())
            .unwrap_or_default()
    } else {
        o.user
    };
    let session = format!("ssh:{}:{port}", &device[..8.min(device.len())]);
    let (input_tx, mut input_rx) = mpsc::unbounded_channel::<SshCommand>();
    let (watch, token) = state.new_watch();
    let grid = Box::new(Grid::new(cols, rows));
    state.terminal = Some(Terminal {
        device: device.clone(),
        session: session.clone(),
        kind: TerminalKind::Ssh {
            grid,
            input: input_tx,
            _reader: Some(watch),
        },
        source: "ssh".into(),
        cols: cols as u32,
        rows: rows as u32,
        lines: Vec::new(),
        cursor: (0, 0),
        ended: false,
        status: "connecting".into(),
        rev: 0,
        desktop_attach_command: String::new(),
        iterm2_attach_command: String::new(),
        native_sources: Vec::new(),
    });
    state.go(Screen::Terminal);
    let backend = ctx.backend.clone();
    let tx = ctx.tx.clone();
    let Some(signer) = backend.signer() else {
        state.toast("no device key for SSH", true);
        return;
    };
    let store_key = format!("ssh.host.{device}:{port}");
    let expected = backend
        .daemon()
        .and_then(|d| d.store().kv_get(&store_key).ok().flatten())
        .map(|v| String::from_utf8_lossy(&v).into_owned());
    tokio::spawn(async move {
        let outcome: Result<(), Error> = async {
            let deadline = tokio::time::Instant::now() + muster_net::reconnect::CONNECT_TIMEOUT;
            let stream = backend.forward(&device, port).await?;
            let config = ssh::SshConfig {
                user,
                cols,
                rows,
                ..ssh::SshConfig::default()
            };
            let first_time = expected.is_none();
            let verifier = Arc::new(Tofu { expected });
            let mut ssh = tokio::time::timeout_at(deadline, ssh::connect(stream, &config, signer, verifier))
                .await
                .map_err(|_| Error::Unavailable("SSH connection timed out after 3 seconds".into()))?
                .map_err(|e| Error::Unavailable(e.to_string()))?;
            if first_time && let Some(daemon) = backend.daemon() {
                let _ = daemon.store().kv_set(&store_key, ssh.host_fingerprint.as_bytes());
            }
            let _ = tx.send(loop_message(Msg::SshConnected {
                session: session.clone(),
                key_type: ssh.host_key_type.clone(),
                fingerprint: ssh.host_fingerprint.clone(),
            }));
            loop {
                tokio::select! {
                    _ = token.cancelled() => { ssh.close().await; return Ok(()); }
                    cmd = input_rx.recv() => match cmd {
                        Some(SshCommand::Write(bytes)) => ssh.write(&bytes).await.map_err(|e| Error::Unavailable(e.to_string()))?,
                        Some(SshCommand::Resize(c, r)) => ssh.resize(c, r).await.map_err(|e| Error::Unavailable(e.to_string()))?,
                        Some(SshCommand::Close) | None => { ssh.close().await; return Ok(()); }
                    },
                    ev = ssh.next() => match ev {
                        Some(ssh::SshEvent::Output(bytes)) => { let _ = tx.send(loop_message(Msg::SshOutput { session: session.clone(), bytes })); }
                        Some(ssh::SshEvent::Exit(code)) => { let _ = tx.send(loop_message(Msg::SshEnded { session: session.clone(), status: format!("exited ({code})") })); }
                        Some(ssh::SshEvent::Closed) | None => { let _ = tx.send(loop_message(Msg::SshEnded { session: session.clone(), status: "closed".into() })); return Ok(()); }
                    }
                }
            }
        }
        .await;
        if let Err(error) = outcome {
            let _ = tx.send(loop_message(Msg::SshEnded {
                session,
                status: error.to_string(),
            }));
        }
    });
}

pub(super) fn apply_frame(terminal: &mut Terminal, frame: v1::TerminalFrame) {
    if frame.full {
        terminal.desktop_attach_command = frame.desktop_attach_command;
        terminal.iterm2_attach_command = frame.iterm2_attach_command;
        terminal.native_sources = frame.native_sources;
        terminal.lines = frame.lines;
        terminal.lines.sort_by_key(|l| l.index);
    } else {
        for line in frame.lines {
            match terminal
                .lines
                .binary_search_by_key(&line.index, |l| l.index)
            {
                Ok(i) => terminal.lines[i] = line,
                Err(i) => terminal.lines.insert(i, line),
            }
        }
    }
    if frame.cols > 0 {
        terminal.cols = frame.cols;
    }
    if frame.rows > 0 {
        terminal.rows = frame.rows;
        terminal.lines.retain(|l| l.index < frame.rows);
    }
    terminal.cursor = (frame.cursor_x, frame.cursor_y);
    terminal.rev = frame.rev;
    if frame.ended {
        terminal.ended = true;
        terminal.status = "ended".into();
    }
}

pub(super) fn intent_open_terminals(state: &mut State, ctx: &Ctx, r: app::OpenTerminals) {
    let filter = if r.device_id.is_empty() {
        if state.terminals.device_id.is_empty() {
            "*".into()
        } else {
            state.terminals.device_id.clone()
        }
    } else {
        r.device_id
    };
    if filter != "*"
        && !state
            .service_devices()
            .iter()
            .any(|m| m.device_id == filter)
    {
        state.toast("Choose a desktop or server for terminals", true);
        return;
    }
    state.terminals.device_id = filter.clone();
    if filter != "*" {
        state.selected = filter;
    }
    open_terminals(state, ctx);
}

pub(super) fn intent_open_terminal(state: &mut State, ctx: &Ctx, o: app::OpenTerminal) {
    if !o.device_id.is_empty() {
        if !state
            .terminals
            .sessions
            .iter()
            .any(|s| s.id == o.session && s.device_id == o.device_id)
        {
            state.toast("Terminal is no longer listed on this device", true);
            return;
        }
        state.selected = o.device_id;
    }
    open_terminal(state, ctx, o.session);
}

/// Clipboard text as input (`muster_terminal::paste_bytes`). Bracketed when
/// the program asked for it: known for our SSH grid; iTerm2 and remote PTY
/// sessions do not report the mode, so their pastes are plain.
pub(super) fn intent_terminal_paste(state: &mut State, ctx: &Ctx, p: app::TerminalPaste) {
    let Some(terminal) = &state.terminal else {
        return;
    };
    if p.text.len() > muster_terminal::MAX_PASTE {
        state.toast(
            format!(
                "Not pasted: {} KB is more than the {} KB a paste may send",
                p.text.len().div_ceil(1024),
                muster_terminal::MAX_PASTE / 1024
            ),
            true,
        );
        return;
    }
    let bracketed =
        matches!(&terminal.kind, TerminalKind::Ssh { grid, .. } if grid.bracketed_paste);
    terminal_input(state, ctx, muster_terminal::paste_bytes(&p.text, bracketed));
}

pub(super) fn intent_terminal_key(state: &mut State, ctx: &Ctx, k: app::TerminalKey) {
    if let Some(bytes) = muster_terminal::key_bytes(&k.key) {
        terminal_input(state, ctx, bytes);
    }
}

pub(super) fn intent_terminal_create(state: &mut State, ctx: &Ctx, c: app::TerminalCreate) {
    // Native attachment stays with the current terminal's device, even if
    // the global selected device changed while its conversation remained open.
    let (device, managed_session) = if c.attach_managed {
        let Some(terminal) = state
            .terminal
            .as_ref()
            .filter(|t| t.source == "tmux" && !t.ended)
        else {
            return;
        };
        if !terminal.native_sources.iter().any(|s| s.source == c.source) {
            return;
        }
        (terminal.device.clone(), terminal.session.clone())
    } else {
        if state.terminals.device_id == "*" {
            state.toast("Select a device before creating a terminal", true);
            return;
        }
        if !c.source.is_empty() && !state.terminals.sources.iter().any(|s| s.source == c.source) {
            return;
        }
        (state.terminals.device_id.clone(), String::new())
    };
    let self_id = state.self_id.clone();
    let backend = ctx.backend.clone();
    let op_id = state.op_id();
    state.busy += 1;
    spawn(ctx, async move {
        let result = async {
            let client = backend.client_for(&device, &self_id).await?;
            match unary(
                &client,
                Req::TerminalCreate(v1::TerminalCreateRequest {
                    profile: c.profile,
                    background: false,
                    source: c.source,
                    managed_session,
                    op_id,
                }),
            )
            .await?
            {
                Resp::TerminalSession(mut s) => {
                    s.device_id = device.clone();
                    Ok(s)
                }
                Resp::OpResult(r) => Err(Error::Unavailable(r.message)),
                _ => Err(unexpected()),
            }
        }
        .await;
        if c.attach_managed {
            Msg::Done {
                what: "native terminal",
                result: result.map(|_| None),
            }
        } else {
            Msg::TerminalCreated { device, result }
        }
    });
}

pub(super) fn intent_terminal_resize(state: &mut State, ctx: &Ctx, r: app::TerminalResize) {
    let Some(terminal) = &mut state.terminal else {
        return;
    };
    if !terminal.resizable() {
        // Somebody's iTerm2 window: the shell fits the grid it is
        // given; its viewport never reaches the device.
        return;
    }
    let (cols, rows) = (r.cols.clamp(2, 500), r.rows.clamp(1, 300));
    terminal.cols = cols;
    terminal.rows = rows;
    match &mut terminal.kind {
        TerminalKind::Ssh { grid, input, .. } => {
            grid.resize(cols as u16, rows as u16);
            let _ = input.send(SshCommand::Resize(cols as u16, rows as u16));
            let frame = grid.full_frame(&terminal.session);
            apply_frame(terminal, frame);
        }
        TerminalKind::Remote { .. } => {
            let (device, session) = (terminal.device.clone(), terminal.session.clone());
            let self_id = state.self_id.clone();
            let backend = ctx.backend.clone();
            spawn(ctx, async move {
                let result = async {
                    let client = backend.client_for(&device, &self_id).await?;
                    unary(
                        &client,
                        Req::TerminalResize(v1::TerminalResizeRequest {
                            session,
                            cols,
                            rows,
                        }),
                    )
                    .await?;
                    Ok(None)
                }
                .await;
                Msg::Done {
                    what: "resize",
                    result,
                }
            });
        }
    }
}

pub(super) fn intent_close_terminal(state: &mut State) {
    if let Some(terminal) = state.terminal.take()
        && let TerminalKind::Ssh { input, .. } = &terminal.kind
    {
        let _ = input.send(SshCommand::Close);
    }
    if state.screen == Screen::Terminal {
        state.back();
    }
}

pub(super) fn msg_terminals(
    state: &mut State,
    epoch: u64,
    device: String,
    result: Result<v1::TerminalSessionList, Error>,
) {
    if state.terminals_epoch != epoch
        || (state.terminals.device_id != "*" && state.terminals.device_id != device)
    {
        return;
    }
    state.terminals_pending = state.terminals_pending.saturating_sub(1);
    state.terminals.loading = state.terminals_pending > 0;
    // A late answer from a member that has gone offline since.
    if state.offline(&device) {
        return;
    }
    match result {
        Ok(list) => {
            if state.terminals.device_id != "*" {
                state.terminals.can_create = !list.sources.is_empty();
                state.terminals.sources = list.sources;
            }
            state.terminals.errors.extend(list.errors);
            state.terminals.sessions.retain(|s| s.device_id != device);
            state.terminals.sessions.extend(list.sessions);
            state.terminals.sessions.sort_by(|a, b| {
                (&a.device_name, &a.title, &a.id).cmp(&(&b.device_name, &b.title, &b.id))
            });
        }
        Err(error) => state.terminals.errors.push(error.to_string()),
    }
}

pub(super) fn msg_terminal_created(
    state: &mut State,
    ctx: &Ctx,
    device: String,
    result: Result<v1::TerminalSession, Error>,
) {
    state.busy = state.busy.saturating_sub(1);
    match result {
        Ok(session) => {
            state.terminals.sessions.push(session.clone());
            state.selected = if state.is_self(&device) {
                String::new()
            } else {
                device
            };
            open_terminal(state, ctx, session.id);
        }
        Err(error) => state.toast(format!("new terminal: {error}"), true),
    }
}

pub(super) fn msg_terminal_frame(
    state: &mut State,
    device: String,
    session: String,
    frame: v1::TerminalFrame,
) {
    if let Some(terminal) = &mut state.terminal
        && terminal.device == device
        && terminal.session == session
    {
        if terminal.status == "attaching" {
            terminal.status = "attached".into();
        }
        apply_frame(terminal, frame);
    }
}

pub(super) fn msg_ssh_connected(
    state: &mut State,
    session: String,
    key_type: String,
    fingerprint: String,
) {
    if let Some(terminal) = &mut state.terminal
        && terminal.session == session
    {
        terminal.status = format!("connected ({key_type} {fingerprint})");
    }
}

pub(super) fn msg_ssh_output(state: &mut State, session: String, bytes: Vec<u8>) {
    if let Some(terminal) = &mut state.terminal
        && terminal.session == session
        && let TerminalKind::Ssh { grid, input, .. } = &mut terminal.kind
    {
        let replies = grid.feed(&bytes);
        if !replies.is_empty() {
            let _ = input.send(SshCommand::Write(replies));
        }
        if let Some(frame) = grid.frame(&session) {
            apply_frame(terminal, frame);
        }
    }
}

pub(super) fn msg_ssh_ended(state: &mut State, session: String, status: String) {
    if let Some(terminal) = &mut state.terminal
        && terminal.session == session
    {
        terminal.ended = true;
        terminal.status = status;
    }
}

/// Leaving a content tab detaches the stream, never kills the tmux shell.
pub(super) fn detach_session_terminal(state: &mut State) {
    state.detach_session_terminal();
}

pub(super) fn session_terminal_action(
    state: &mut State,
    ctx: &Ctx,
    action: app::SessionTerminalAction,
) {
    if action.action == 3 {
        detach_session_terminal(state);
        return;
    }
    if state.screen != Screen::Chat
        || state.session_terminal.busy
        || !(0..=2).contains(&action.action)
    {
        return;
    }
    let Some(chat) = &state.chat else {
        return;
    };
    if (action.action != 1 && chat.session.cwd.is_empty()) || chat.session.mode == "chat" {
        state.session_terminal.error = "This session has no working directory".into();
        return;
    }
    let (device, agent) = (chat.device.clone(), chat.session.id.clone());
    // Terminal actions never retarget a global selected-device value.
    let self_id = state.self_id.clone();
    let op_id = state.op_id();
    state.session_terminal.epoch += 1;
    let epoch = state.session_terminal.epoch;
    state.session_terminal.busy = true;
    state.session_terminal.error.clear();
    let backend = ctx.backend.clone();
    spawn(ctx, async move {
        let result = async {
            let client = backend.client_for(&device, &self_id).await?;
            let response = unary(
                &client,
                Req::AgentTerminal(v1::AgentTerminalRequest {
                    session: agent.clone(),
                    op_id,
                    action: action.action,
                }),
            )
            .await?;
            match response {
                Resp::OpResult(r) if matches!(r.status, 0 | 1) => Ok(r.result_id),
                Resp::OpResult(r) => Err(Error::Unavailable(r.message)),
                _ => Err(unexpected()),
            }
        }
        .await;
        Msg::SessionTerminalReady {
            epoch,
            device,
            agent,
            result,
        }
    });
}

pub(super) fn session_terminal_ready(
    state: &mut State,
    ctx: &Ctx,
    epoch: u64,
    device: String,
    agent: String,
    result: Result<String, Error>,
) {
    if epoch != state.session_terminal.epoch
        || state.screen != Screen::Chat
        || state.chat_target() != Some((device.clone(), agent))
    {
        return;
    }
    state.session_terminal.busy = false;
    match result {
        Err(error) => state.session_terminal.error = error.to_string(),
        Ok(session) if session.is_empty() => {
            state.terminal = None;
            state.session_terminal.active = false;
        }
        Ok(session) => {
            let (watch, token) = state.new_watch();
            state.terminal = Some(Terminal {
                device: device.clone(),
                session: session.clone(),
                kind: TerminalKind::Remote {
                    _attach: Some(watch),
                    lease: None,
                    leased: false,
                    input: Default::default(),
                },
                source: "tmux".into(),
                cols: 80,
                rows: 24,
                lines: Vec::new(),
                cursor: (0, 0),
                ended: false,
                status: "attaching".into(),
                rev: 0,
                desktop_attach_command: String::new(),
                iterm2_attach_command: String::new(),
                native_sources: Vec::new(),
            });
            state.session_terminal.active = true;
            attach(state, ctx, device, session, token);
        }
    }
}
