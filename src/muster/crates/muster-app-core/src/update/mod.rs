//! The reducer: intents and task results change the state; tasks do the
//! I/O and report back as messages. Nothing here touches the shell except
//! through effects.
//!
//! `intent` and `msg` below name every variant, so the compiler still checks
//! that each is handled; the handlers live with their domain: `network`,
//! `operations` (presence and Locked Use), `agents`, `desktop_chats`,
//! `files`, `terminals`, and `shell` (effect results and lifecycle).

use std::sync::Arc;
use std::time::Duration;

use muster_core::locked_use::CancelReason;
use muster_proto::Message as _;
use muster_proto::app::{self, Screen, intent::It};
use muster_proto::v1::{self, request::Req, response::Resp};
use muster_rpc::Client;
use muster_terminal::grid::Grid;
use muster_terminal::ssh;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::chat_hosts;
use crate::state::{
    Chat, ChatRetry, EffectKind, PendingPresence, SshCommand, State, Terminal, TerminalKind,
};
use crate::{Ctx, Error, Loop};

pub(crate) mod admitted;
mod agents;
pub(crate) mod code_runs;
mod desktop_chats;
mod files;
pub(crate) mod manual_locked_use;
mod multica;
mod network;
mod operations;
mod orca;
mod send_recovery;
mod shell;
mod terminals;
#[cfg(test)]
mod tests;

pub(crate) use agents::flush_chat_cache;
use agents::*;
use desktop_chats::*;
use files::*;
use network::*;
use operations::*;
use shell::*;
pub use shell::{effect_result, lifecycle};
use terminals::*;

/// What tasks report back.
pub enum Msg {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    UpdateChecked(Result<Option<crate::desktop_update::Candidate>, String>),
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    UpdateDownloaded(Result<crate::desktop_update::Prepared, String>),
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    UpdateApplied(Result<(), String>),
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    UpdateLease(Result<crate::desktop_update::Lease, String>),
    BrowserStatus {
        device: String,
        result: Result<v1::BrowserStatus, Error>,
    },
    BrowserResult {
        device: String,
        result: Result<v1::OpResult, Error>,
    },
    ManualDevice(String, Result<v1::DeviceInfo, Error>),
    ManualDone(String, Result<(), Error>),
    Info(Result<v1::DeviceInfo, Error>),
    Network(v1::NetworkStatus),
    DeviceState(v1::DeviceState),
    Pending(Vec<v1::PendingJoin>),
    Policy(Result<v1::Policy, Error>),
    Invite(Result<v1::InviteText, Error>),
    JoinProgress(String, String),
    JoinDone(Result<v1::NetworkStatus, Error>),
    Done {
        what: &'static str,
        result: Result<Option<v1::NetworkStatus>, Error>,
    },
    OrcaCreated {
        network: Option<String>,
        device: String,
        result: Result<v1::AgentSession, Error>,
    },
    SendRecovered {
        device: String,
        key: String,
        pending: app::PendingAgentSend,
        result: Result<v1::OpResult, Error>,
    },
    OpDone {
        what: &'static str,
        device: String,
        locked_nonce: Option<Vec<u8>>,
        response_origin: Option<(String, String)>,
        result: Result<v1::OpResult, Error>,
    },
    ChatControl {
        epoch: u64,
        device: String,
        provider: String,
        request: Req,
        reply: tokio::sync::oneshot::Sender<Result<Resp, Error>>,
    },
    ChatControlReply {
        device: String,
        id: String,
        nonce: Option<Vec<u8>>,
        result: Result<Resp, Error>,
    },
    LockedExpired(Vec<u8>),
    /// A background call with nothing to apply.
    Ignored,
    PresenceNeeded {
        response_origin: Option<(String, String)>,
        what: &'static str,
        device: String,
        call_id: u64,
        request: Box<Req>,
        message: Vec<u8>,
    },
    Multica(Result<v1::MulticaStatus, Error>),
    MulticaProgress(v1::MulticaInstallation),
    MulticaPage {
        generation: u64,
        result: Result<v1::MulticaPage, Error>,
    },
    DiagnosticsUploaded {
        name: String,
        result: Result<String, Error>,
    },
    Agents {
        epoch: u64,
        device: String,
        result: Result<Vec<v1::AgentSession>, Error>,
    },
    Chat {
        token: CancellationToken,
        device: String,
        session: String,
        earlier: bool,
        reset: bool,
        result: Result<(Vec<v1::AgentMessage>, bool), Error>,
    },
    DesktopChats {
        load: u64,
        result: Result<PickedHost, Error>,
    },
    DesktopChatOpened {
        view: u64,
        device: String,
        /// What to retry on another Mac if this one was busy.
        retry: Option<ChatRetry>,
        result: Result<v1::AgentSession, Error>,
    },
    Completion {
        device: String,
        session: String,
        turn: u64,
        result: Result<String, Error>,
    },
    ChatOptions {
        device: String,
        session: String,
        result: Result<v1::AgentChatOptions, Error>,
    },
    AgentEvent {
        token: CancellationToken,
        device: String,
        session: String,
        event: v1::agent_event::Ev,
    },
    WatchEnded {
        what: &'static str,
        device: String,
        session: String,
        error: Option<String>,
    },
    Terminals {
        epoch: u64,
        device: String,
        result: Result<v1::TerminalSessionList, Error>,
    },
    CodeTick,
    CodeStarted {
        prepared: Box<code_runs::Prepared>,
        auto_return: bool,
        result: Result<String, Error>,
    },
    CodeRuns {
        device: String,
        session: String,
        provider: String,
        result: Result<Vec<v1::TerminalRun>, Error>,
    },
    SessionTerminalReady {
        epoch: u64,
        device: String,
        agent: String,
        result: Result<String, Error>,
    },
    TerminalCreated {
        device: String,
        op_id: String,
        attach_managed: bool,
        result: Result<v1::TerminalSession, Error>,
    },
    TerminalFrame {
        device: String,
        session: String,
        frame: v1::TerminalFrame,
    },
    /// The locked Mac shows this terminal only inside a terminal session.
    TerminalLocked {
        device: String,
        session: String,
    },
    TerminalLeaseOpened {
        device: String,
        session: String,
    },
    TerminalLeaseEnded {
        device: String,
        session: String,
        error: String,
    },
    SshConnected {
        session: String,
        key_type: String,
        fingerprint: String,
    },
    SshOutput {
        session: String,
        bytes: Vec<u8>,
    },
    SshEnded {
        session: String,
        status: String,
    },
    Files {
        device: String,
        session: String,
        result: Result<v1::FileList, Error>,
    },
    File {
        device: String,
        session: String,
        path: String,
        result: Result<v1::FileContent, Error>,
    },
    Git {
        device: String,
        session: String,
        result: Result<v1::GitLog, Error>,
    },
}

// ------------------------------------------------------------- helpers

fn loop_message(msg: Msg) -> Loop {
    Loop::Msg(Box::new(msg))
}

fn spawn<F>(ctx: &Ctx, fut: F)
where
    F: std::future::Future<Output = Msg> + Send + 'static,
{
    let tx = ctx.tx.clone();
    tokio::spawn(async move {
        let msg = fut.await;
        let _ = tx.send(loop_message(msg));
    });
}

async fn unary(client: &Client, req: Req) -> Result<Resp, Error> {
    Ok(client.call(req).await?)
}

fn unexpected() -> Error {
    Error::Rpc("unexpected reply".into())
}

fn effect(state: &mut State, ctx: &Ctx, kind: EffectKind, ef: app::effect::Ef) -> u64 {
    let id = state.next_effect;
    state.next_effect += 1;
    state.effects.insert(id, kind);
    ctx.shell
        .on_effect(id, &app::Effect { ef: Some(ef) }.encode_to_vec());
    id
}

pub fn init(state: &mut State, ctx: &Ctx) {
    activate_network(state, ctx);
    load_basics(state, ctx);
    multica::request(state, ctx, v1::MulticaRequest::default());
}

// ------------------------------------------------------------- intents

pub fn intent(state: &mut State, ctx: &Ctx, intent: app::Intent) {
    let Some(mut it) = intent.it else { return };
    if let It::ComposerAction(snapshot) = it {
        let Some(action) = snapshot.action.and_then(|a| a.it) else {
            return;
        };
        if matches!(action, It::ComposerAction(_)) {
            return;
        }
        let durable = crate::drafts::saves_before(&action);
        if durable || matches!(action, It::AgentSend(_)) {
            if let Some(draft) = snapshot.draft {
                if !capture_composer(state, ctx, draft, durable) {
                    return;
                }
            }
        }
        it = action;
    }
    if crate::drafts::saves_before(&it) {
        state.chat_call_epoch += 1;
        if !state.chat_calls.is_empty() {
            cancel_locked(state, ctx, CancelReason::User);
        }
    }
    match it {
        It::ComposerAction(_) => unreachable!("nested composer action rejected"),
        It::OpenBrowser(open) => network::intent_open_browser(state, ctx, open),
        It::BrowserSetup(r) => browser_setup(state, ctx, r.install),
        It::BrowserRun(r) => network::intent_browser_run(state, ctx, r),
        It::MulticaBrowse(request) => multica::browse(state, ctx, request),
        It::Multica(request) => multica::request(state, ctx, request),
        It::Refresh(_) => intent_refresh(state, ctx),
        It::Navigate(n) => intent_navigate(state, ctx, n),
        It::UploadDiagnostics(u) => network::intent_upload_diagnostics(state, ctx, u),
        It::Dismiss(d) => state.toasts.retain(|t| t.id != d.toast_id),
        It::CreateNetwork(c) => network::intent_create_network(state, ctx, c),
        It::ShowInvite(i) => network::intent_show_invite(state, ctx, i),
        It::JoinNetwork(j) => network::intent_join_network(state, ctx, j),
        It::DecideJoin(d) => network::intent_decide_join(state, ctx, d),
        It::RemoveDevice(r) => network::intent_remove_device(state, ctx, r),
        It::SetScopes(s) => network::intent_set_scopes(state, ctx, s),
        It::SetPolicy(p) => network::intent_set_policy(state, ctx, p),
        It::SelectDevice(s) => network::intent_select_device(state, s),
        It::SetDeviceLockedUse(r) => manual_locked_use::set(state, ctx, r),
        It::CancelLockedUse(_) => cancel_locked(state, ctx, CancelReason::User),
        It::OrcaCreate(o) => orca::create(state, ctx, o),
        It::OpenAgents(o) => agents::intent_open_agents(state, ctx, o),
        It::OpenAgent(o) => agents::intent_open_agent(state, ctx, o),
        It::OpenDesktopChats(o) => open_desktop_chats(state, ctx, o.provider),
        It::DesktopChatOpen(o) => desktop_chat_open(state, ctx, o.chat, o.desktop, false),
        It::ChatOptionsLoad(_) => chat_options(state, ctx, None),
        It::ComposerComplete(_) => desktop_chats::complete(state, ctx),
        It::ChatSetOptions(o) => desktop_chats::intent_chat_set_options(state, ctx, o),
        It::AgentSend(s) => agents::intent_agent_send(state, ctx, s),
        It::ResolveAgentSend(r) => send_recovery::resolve(state, ctx, r),
        It::AgentRespond(r) => agents::intent_agent_respond(state, ctx, r),
        It::AgentInterrupt(_) => agents::intent_agent_interrupt(state, ctx),
        It::AgentLoadEarlier(load) => agents::intent_agent_load_earlier(state, ctx, load),
        It::OpenTerminals(r) => terminals::intent_open_terminals(state, ctx, r),
        It::OpenTerminal(o) => terminals::intent_open_terminal(state, ctx, o),
        It::TerminalKey(k) => terminals::intent_terminal_key(state, ctx, k),
        It::TerminalText(t) => terminal_input(state, ctx, t.text.into_bytes()),
        It::TerminalPaste(p) => terminals::intent_terminal_paste(state, ctx, p),
        It::CodeAction(a) => code_runs::action(state, ctx, a),
        It::SessionTerminalAction(a) => terminals::session_terminal_action(state, ctx, a),
        It::TerminalCreate(c) => terminals::intent_terminal_create(state, ctx, c),
        It::TerminalResize(r) => terminals::intent_terminal_resize(state, ctx, r),
        It::OpenFiles(o) => open_files(state, ctx, o.path),
        It::OpenFile(o) => open_file(state, ctx, o.path),
        It::OpenChange(o) => open_change(state, o),
        It::OpenFileLink(o) => files::intent_open_file_link(state, ctx, o),
        It::OpenGit(o) => open_git(state, ctx, o.diff_commit),
        It::SetCover(c) => network::intent_set_cover(state, ctx, c),
        It::DesktopUpdateAction(action) => {
            network::intent_desktop_update_action(state, ctx, action)
        }
        It::DesktopSetupAction(action) => network::intent_desktop_setup_action(state, ctx, action),
        It::RequestAccessibility(_) => network::intent_request_accessibility(state, ctx),
        It::SetRelay(r) => network::intent_set_relay(state, ctx, r),
        It::SetPushToken(t) => network::intent_set_push_token(state, ctx, t),
        It::OpenSsh(o) => open_ssh(state, ctx, o),
        It::CloseTerminal(_) => terminals::intent_close_terminal(state),
    }
}

fn intent_refresh(state: &mut State, ctx: &Ctx) {
    // An explicit refresh is the user's word: members found offline get one
    // more round of bounded attempts.
    if ctx.backend.is_embedded() {
        ctx.backend.refresh_presence();
    } else {
        let local = ctx.backend.local();
        spawn(ctx, async move {
            match unary(&local, Req::NetworkRefresh(v1::NetworkRefreshRequest {})).await {
                Ok(Resp::NetworkStatus(status)) => Msg::Network(status),
                // An older daemon: the regular reload still runs.
                _ => Msg::Ignored,
            }
        });
    }
    load_basics(state, ctx);
    match state.screen {
        Screen::Home | Screen::Agents => open_agents(state, ctx),
        Screen::Terminals => open_terminals(state, ctx),
        _ if state.workspace() == Screen::DesktopChats => load_desktop_chats(state, ctx),
        _ => {}
    }
}

fn intent_navigate(state: &mut State, ctx: &Ctx, n: app::Navigate) {
    if n.back {
        if n.back_count > 0 {
            let path = state.navigation_path();
            if n.expected_path != path || n.back_count as usize >= path.len() {
                return;
            }
        }
        for _ in 0..n.back_count.max(1) {
            state.back();
        }
        shell::release_hidden(state, ctx);
        if matches!(state.screen, Screen::Home | Screen::Agents) {
            open_agents(state, ctx);
        }
        if state.screen == Screen::DesktopChats {
            load_desktop_chats(state, ctx);
        }
    } else if let Ok(screen) = Screen::try_from(n.screen) {
        if screen == Screen::Invite && !state.can_invite() {
            state.toast("Only an admin device can invite", true);
            return;
        }
        if screen == Screen::Multica && !state.multica.installed {
            return;
        }
        if screen == Screen::Multica {
            multica::browse(state, ctx, state.multica_query.clone());
        }
        if screen == Screen::Settings || screen == Screen::Multica {
            multica::request(state, ctx, v1::MulticaRequest::default());
        }
        if screen == Screen::DesktopChats {
            open_desktop_chats(state, ctx, String::new());
        } else {
            state.go(screen);
        }
    }
}

// ------------------------------------------------------------- messages

pub fn msg(state: &mut State, ctx: &Ctx, msg: Msg) {
    match msg {
        Msg::CodeTick => code_runs::poll(state, ctx),
        Msg::CodeStarted {
            prepared,
            auto_return,
            result,
        } => code_runs::started(state, ctx, *prepared, auto_return, result),
        Msg::CodeRuns {
            device,
            session,
            provider,
            result,
        } => code_runs::received(state, ctx, device, session, provider, result),
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        Msg::UpdateChecked(result) => network::msg_update_checked(state, result),
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        Msg::UpdateDownloaded(result) => network::msg_update_downloaded(state, result),
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        Msg::UpdateLease(result) => network::msg_update_lease(state, ctx, result),
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        Msg::UpdateApplied(result) => network::msg_update_applied(state, ctx, result),
        Msg::Info(result) => network::msg_info(state, ctx, result),
        Msg::Network(status) => network::msg_network(state, ctx, status),
        Msg::DeviceState(s) => network::msg_device_state(state, ctx, s),
        Msg::Pending(p) => network::msg_pending(state, ctx, p),
        Msg::BrowserStatus { device, result } => network::msg_browser_status(state, device, result),
        Msg::BrowserResult { device, result } => network::msg_browser_result(state, device, result),
        Msg::Policy(Ok(policy)) => state.policy = Some(policy),
        Msg::Policy(Err(error)) => log::debug!("policy: {error}"),
        Msg::Invite(result) => network::msg_invite(state, result),
        Msg::JoinProgress(phase, message) => state.join = app::JoinView { phase, message },
        Msg::JoinDone(result) => network::msg_join_done(state, ctx, result),
        Msg::ManualDevice(id, result) => match result {
            Ok(info) => {
                state.manual_devices.insert(id, info);
            }
            Err(_) => {
                state.manual_devices.remove(&id);
            }
        },
        Msg::ManualDone(id, result) => {
            state.manual_busy.remove(&id);
            if let Err(error) = result {
                state.toast(format!("Locked Use: {error}"), true);
            }
        }
        Msg::Done { what, result } => network::msg_done(state, ctx, what, result),
        Msg::SendRecovered {
            device,
            key,
            pending,
            result,
        } => send_recovery::recovered(state, ctx, device, key, pending, result),
        Msg::OpDone {
            what,
            device,
            locked_nonce,
            response_origin,
            result,
        } => operations::msg_op_done(
            state,
            ctx,
            what,
            device,
            locked_nonce,
            response_origin,
            result,
        ),
        Msg::LockedExpired(nonce) => operations::msg_locked_expired(state, ctx, nonce),
        Msg::Ignored => {}
        Msg::ChatControl {
            epoch,
            device,
            provider,
            mut request,
            reply,
        } => {
            if epoch != state.chat_call_epoch || !state.foreground || reply.is_closed() {
                let _ = reply.send(Err(Error::Unavailable("Chat loading cancelled".into())));
            } else {
                let id = state.op_id();
                match &mut request {
                    Req::AgentChats(r) => r.op_id = id.clone(),
                    Req::AgentChatOpen(r) => r.op_id = id.clone(),
                    Req::AgentChatOptions(r) => r.op_id = id.clone(),
                    Req::AgentComplete(r) => r.op_id = id.clone(),
                    Req::AgentChatSetOptions(r) => r.op_id = id.clone(),
                    _ => {
                        let _ = reply.send(Err(unexpected()));
                        return;
                    }
                }
                state.chat_calls.retain(|_, reply| !reply.is_closed());
                state.chat_calls.insert(id.clone(), reply);
                op_request_from(state, ctx, "chat", device, request, Some((provider, id)));
            }
        }
        Msg::ChatControlReply {
            device,
            id,
            nonce,
            result,
        } => {
            let current = nonce.is_none() || state.finish_locked(&device, nonce.as_deref());
            if let Some(reply) = state.chat_calls.remove(&id) {
                let _ = reply.send(if current {
                    result
                } else {
                    Err(Error::Unavailable("Chat loading cancelled".into()))
                });
            }
        }
        Msg::PresenceNeeded {
            response_origin,
            what,
            device,
            call_id,
            request,
            message,
        } => operations::msg_presence_needed(
            state,
            ctx,
            response_origin,
            what,
            device,
            call_id,
            request,
            message,
        ),
        Msg::MulticaPage { generation, result } => multica::apply_page(state, generation, result),
        Msg::Multica(result) => multica::apply_result(state, result),
        Msg::MulticaProgress(progress) => {
            if state.multica.busy {
                state.multica.installation = Some(progress);
            }
        }
        Msg::DiagnosticsUploaded { name, result } => {
            network::msg_diagnostics_uploaded(state, name, result)
        }
        Msg::Agents {
            epoch,
            device,
            result,
        } => agents::msg_agents(state, ctx, epoch, device, result),
        Msg::Chat {
            token,
            device,
            session,
            earlier,
            reset,
            result,
        } => agents::msg_chat(state, token, device, session, earlier, reset, result),
        Msg::Completion {
            device,
            session,
            turn,
            result,
        } => desktop_chats::msg_completion(state, device, session, turn, result),
        Msg::ChatOptions {
            device,
            session,
            result,
        } => desktop_chats::msg_chat_options(state, device, session, result),
        Msg::DesktopChats { load, result } => {
            desktop_chats::msg_desktop_chats(state, ctx, load, result)
        }
        Msg::DesktopChatOpened {
            view,
            device,
            retry,
            result,
        } => desktop_chats::msg_desktop_chat_opened(state, ctx, view, device, retry, result),
        Msg::AgentEvent {
            token,
            device,
            session,
            event,
        } => agents::msg_agent_event(state, ctx, token, device, session, event),
        Msg::WatchEnded {
            what,
            device,
            session,
            error,
        } => agents::msg_watch_ended(state, what, device, session, error),
        Msg::Terminals {
            epoch,
            device,
            result,
        } => terminals::msg_terminals(state, epoch, device, result),
        Msg::OrcaCreated {
            network,
            device,
            result,
        } => {
            if state.network.as_ref().map(|n| n.network_id.clone()) != network {
                return;
            }
            state.agents.creating = false;
            match result {
                Ok(session) => agents::show_chat(state, ctx, device, session),
                Err(error) => state.toast(error.to_string(), true),
            }
        }
        Msg::SessionTerminalReady {
            epoch,
            device,
            agent,
            result,
        } => terminals::session_terminal_ready(state, ctx, epoch, device, agent, result),
        Msg::TerminalCreated {
            device,
            op_id,
            attach_managed,
            result,
        } => terminals::msg_terminal_created(state, ctx, device, op_id, attach_managed, result),
        Msg::TerminalFrame {
            device,
            session,
            frame,
        } => terminals::msg_terminal_frame(state, device, session, frame),
        Msg::TerminalLocked { device, session } => {
            terminals::msg_terminal_locked(state, ctx, device, session)
        }
        Msg::TerminalLeaseOpened { device, session } => {
            terminals::msg_terminal_lease_opened(state, ctx, device, session)
        }
        Msg::TerminalLeaseEnded {
            device,
            session,
            error,
        } => terminals::msg_terminal_lease_ended(state, device, session, error),
        Msg::SshConnected {
            session,
            key_type,
            fingerprint,
        } => terminals::msg_ssh_connected(state, session, key_type, fingerprint),
        Msg::SshOutput { session, bytes } => terminals::msg_ssh_output(state, session, bytes),
        Msg::Files {
            device,
            session,
            result,
        } => files::msg_files(state, device, session, result),
        Msg::File {
            device,
            session,
            path,
            result,
        } => files::msg_file(state, device, session, path, result),
        Msg::Git {
            device,
            session,
            result,
        } => files::msg_git(state, device, session, result),
        Msg::SshEnded { session, status } => terminals::msg_ssh_ended(state, session, status),
    }
}
