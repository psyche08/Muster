//! Desktop Chat mode: picking a Mac, the catalog, opening a chat, options.

use super::*;

/// The Chat tab: `provider`'s conversations through a Mac the core picks.
/// The Mac already picked is kept while it can still show them.
pub(super) fn open_desktop_chats(state: &mut State, ctx: &Ctx, provider: String) {
    use muster_agent::desktop_chat::ChatApp;
    let provider = if provider.is_empty() {
        if state.desktop_chats.provider.is_empty() {
            ChatApp::ALL[0].provider().as_str().to_string()
        } else {
            state.desktop_chats.provider.clone()
        }
    } else {
        provider
    };
    let Some(app) = ChatApp::from_provider(&provider) else {
        state.toast("Choose Claude or ChatGPT", true);
        return;
    };
    let provider = app.provider().as_str().to_string();
    if state.desktop_chats.provider != provider || state.desktop_chats.catalog.is_none() {
        state.desktop_chats_view += 1;
        state.desktop_chat_retry = None;
        state.desktop_chats = ctx
            .cache
            .get::<app::DesktopChatsView>(&cache_key(state, "chat-list-v2", "", &provider))
            .unwrap_or_else(|| app::DesktopChatsView {
                provider,
                ..Default::default()
            });
        state.desktop_chats.opening.clear();
    }
    state.desktop_chats.error.clear();
    state.go(Screen::DesktopChats);
    load_desktop_chats(state, ctx);
}

pub(super) fn chat_route(state: &State, device: &str) -> String {
    // Route an explicitly local member through the local socket.
    if state
        .service_devices()
        .iter()
        .any(|m| m.device_id == device && m.is_self)
    {
        device.to_string()
    } else {
        state.self_id.clone()
    }
}

/// The Mac the chat list comes from, and its list.
pub(crate) struct PickedHost {
    device: String,
    name: String,
    catalog: v1::AgentChatCatalog,
    /// The Mac used before and why it was left.
    left: Option<(String, String)>,
}

/// One Mac's chat list, if it offers desktop chats.
pub(super) async fn chat_catalog_of(
    backend: Arc<crate::backend::Backend>,
    host: &chat_hosts::Candidate,
    provider: &str,
) -> Result<v1::AgentChatCatalog, Error> {
    tokio::time::timeout(Duration::from_secs(6), async {
        let client = backend.client_for(&host.device, &host.route).await?;
        let Resp::DeviceInfo(info) =
            unary(&client, Req::DeviceInfo(v1::DeviceInfoRequest {})).await?
        else {
            return Err(unexpected());
        };
        if !info.services.iter().any(|s| s == "agent.chat") {
            return Err(Error::Invalid(
                "does not offer desktop chats (update Muster there)".into(),
            ));
        }
        if provider == "codex" && !info.services.iter().any(|s| s == "agent.chat.catalog.v1") {
            return Err(Error::Invalid(
                "update Muster on this Mac for the typed ChatGPT catalog".into(),
            ));
        }
        if !info.services.iter().any(|s| s == "agent.chat.locked.v1") {
            return Err(Error::Unavailable(
                "update Muster on this Mac to load Chat through Locked Use".into(),
            ));
        }
        match unary(
            &client,
            Req::AgentChats(v1::AgentChatsRequest {
                provider: provider.into(),
                ..Default::default()
            }),
        )
        .await?
        {
            Resp::AgentChats(catalog) => Ok(catalog),
            _ => Err(unexpected()),
        }
    })
    .await
    .unwrap_or_else(|_| Err(Error::Unavailable("did not answer in time".into())))
}

/// Keeps the picked Mac while it can show the chats, else asks every
/// reachable Mac and picks one (`chat_hosts::choose`).
pub(super) async fn pick_chat_host(
    backend: Arc<crate::backend::Backend>,
    app: muster_agent::desktop_chat::ChatApp,
    hosts: Vec<chat_hosts::Candidate>,
    sticky: Option<String>,
) -> Result<PickedHost, Error> {
    let provider = app.provider().as_str();
    let mut left = None;
    if let Some(host) = sticky.and_then(|d| hosts.iter().find(|h| h.device == d)) {
        let why = match chat_catalog_of(backend.clone(), host, provider).await {
            Ok(catalog) => match chat_hosts::unusable(app, &catalog) {
                None => {
                    return Ok(PickedHost {
                        device: host.device.clone(),
                        name: host.name.clone(),
                        catalog,
                        left: None,
                    });
                }
                Some(why) => why,
            },
            Err(error) => error.to_string(),
        };
        left = Some((host.name.clone(), why));
    }
    let mut probes = tokio::task::JoinSet::new();
    for (i, host) in hosts.iter().enumerate() {
        let (backend, host, provider) = (backend.clone(), host.clone(), provider.to_string());
        probes.spawn(async move {
            (
                i,
                chat_catalog_of(backend, &host, &provider)
                    .await
                    .map_err(|e| e.to_string()),
            )
        });
    }
    let mut answers: Vec<Option<Result<v1::AgentChatCatalog, String>>> = vec![None; hosts.len()];
    while let Some(done) = probes.join_next().await {
        if let Ok((i, answer)) = done {
            answers[i] = Some(answer);
        }
    }
    let answers: Vec<(chat_hosts::Candidate, Result<v1::AgentChatCatalog, String>)> = hosts
        .into_iter()
        .zip(answers)
        .map(|(host, answer)| (host, answer.unwrap_or_else(|| Err("no answer".into()))))
        .collect();
    let i = chat_hosts::choose(app, &answers).map_err(Error::Unavailable)?;
    let (host, answer) = answers.into_iter().nth(i).expect("chosen index");
    Ok(PickedHost {
        left: left.filter(|(name, _)| *name != host.name),
        device: host.device,
        name: host.name,
        catalog: answer.expect("a usable answer"),
    })
}

pub(super) fn load_desktop_chats(state: &mut State, ctx: &Ctx) {
    use muster_agent::desktop_chat::ChatApp;
    let Some(app) = ChatApp::from_provider(&state.desktop_chats.provider) else {
        return;
    };
    let hosts: Vec<chat_hosts::Candidate> = state
        .service_devices()
        .into_iter()
        .filter(|m| m.is_self || m.connected)
        .map(|m| chat_hosts::Candidate {
            route: chat_route(state, &m.device_id),
            device: m.device_id,
            name: m.name,
            rtt_ms: m.rtt_ms,
            is_self: m.is_self,
        })
        .collect();
    let view = &mut state.desktop_chats;
    view.loading = true;
    let sticky = (!view.device_id.is_empty()).then(|| view.device_id.clone());
    state.desktop_chats_load += 1;
    let load = state.desktop_chats_load;
    let backend = ctx.backend.clone();
    let tx = ctx.tx.clone();
    let epoch = state.chat_call_epoch;
    spawn(ctx, async move {
        let result = async {
            let mut picked = pick_chat_host(backend.clone(), app, hosts, sticky).await?;
            // The probe already returned the complete local SQLite projection.
            if app == ChatApp::ChatGpt {
                return Ok(picked);
            }
            let response = tokio::time::timeout(
                Duration::from_secs(120),
                protected_chat_call(
                    tx,
                    epoch,
                    picked.device.clone(),
                    app.provider().as_str().into(),
                    Req::AgentChats(v1::AgentChatsRequest {
                        provider: app.provider().as_str().into(),
                        full_history: true,
                        ..Default::default()
                    }),
                ),
            )
            .await
            .map_err(|_| {
                Error::Unavailable(
                    "Chat history list timed out; cached conversations are kept".into(),
                )
            })??;
            let Resp::AgentChats(catalog) = response else {
                return Err(unexpected());
            };
            picked.catalog = catalog;
            Ok(picked)
        }
        .await;
        Msg::DesktopChats { load, result }
    });
}

/// Picks another Mac because `device` is busy, then does `retry` there.
pub(super) fn repick_chat_host(state: &mut State, ctx: &Ctx, device: &str, retry: ChatRetry) {
    if state.desktop_chats.device_id == device {
        state.desktop_chats.device_id.clear();
        state.desktop_chats.device_name.clear();
    }
    state.desktop_chat_retry = Some(retry);
    load_desktop_chats(state, ctx);
}

/// Continues the open desktop chat through the Mac just picked, after
/// `from` turned out busy: a new chat needs nothing there, a listed one is
/// opened there first. Nothing is resent; the draft stays in the composer.
pub(super) fn move_chat(state: &mut State, ctx: &Ctx, mut session: v1::AgentSession, from: String) {
    use muster_agent::desktop_chat as dc;
    let device = state.desktop_chats.device_id.clone();
    // Only while that chat is still the one shown.
    if !state
        .chat
        .as_ref()
        .is_some_and(|c| c.device == from && c.session.id == session.id)
    {
        return;
    }
    if device.is_empty() || device == from {
        state.toast("No other Mac can take this chat now", true);
        return;
    }
    let name = state.desktop_chats.device_name.clone();
    state.toast(
        format!("This chat now goes through {name}; send again"),
        false,
    );
    if dc::is_new_session(&session.id) {
        session.device_id = device.clone();
        session.device_name = name;
        show_chat(state, ctx, device, session);
    } else {
        desktop_chat_open(state, ctx, session.id, false, true);
    }
}

/// Opens a listed conversation on the desktop (it must show it to be read),
/// or a new one. A new chat needs no desktop change until its first send,
/// which also works through Locked Use; `desktop` shows it there now. A Mac
/// that turns out busy is replaced by another once, and the open retried.
pub(super) fn desktop_chat_open(
    state: &mut State,
    ctx: &Ctx,
    chat: String,
    desktop: bool,
    retried: bool,
) {
    use muster_agent::desktop_chat as dc;
    let view = &mut state.desktop_chats;
    if !view.opening.is_empty() {
        return;
    }
    let Some(app) = dc::ChatApp::from_provider(&view.provider) else {
        return;
    };
    if view.device_id.is_empty() {
        // Still picking a Mac: open once one is picked.
        if view.loading && !retried {
            state.desktop_chat_retry = Some(ChatRetry::Open { chat, desktop });
        }
        return;
    }
    let device = view.device_id.clone();
    let new = chat.is_empty() || dc::is_new_session(&chat);
    if new && !desktop {
        let session = v1::AgentSession {
            id: dc::new_session_id(app),
            provider: view.provider.clone(),
            title: format!("New {} chat", app.name()),
            state: "unknown".into(),
            mode: "chat".into(),
            device_id: device.clone(),
            device_name: view.device_name.clone(),
            ..Default::default()
        };
        show_chat(state, ctx, device, session);
        return;
    }
    let chat = if new { String::new() } else { chat };
    if !new && dc::app_of(&chat) != Some(app) {
        state.toast("This conversation belongs to another app", true);
        return;
    }
    view.opening = if new { "new".into() } else { chat.clone() };
    view.error.clear();
    let (provider, name) = (view.provider.clone(), view.device_name.clone());
    let view_epoch = state.desktop_chats_view;
    let tx = ctx.tx.clone();
    let epoch = state.chat_call_epoch;
    let retry = (!retried).then(|| ChatRetry::Open {
        chat: chat.clone(),
        desktop,
    });
    spawn(ctx, async move {
        let result = tokio::time::timeout(Duration::from_secs(120), async {
            match protected_chat_call(
                tx,
                epoch,
                device.clone(),
                provider.clone(),
                Req::AgentChatOpen(v1::AgentChatOpenRequest {
                    provider,
                    chat,
                    ..Default::default()
                }),
            )
            .await?
            {
                Resp::AgentSession(mut session) => {
                    session.device_id = device.clone();
                    session.device_name = name;
                    Ok(session)
                }
                _ => Err(unexpected()),
            }
        })
        .await
        .unwrap_or_else(|_| {
            Err(Error::Unavailable(
                "opening the conversation timed out".into(),
            ))
        });
        Msg::DesktopChatOpened {
            view: view_epoch,
            device,
            retry,
            result,
        }
    });
}

/// Reads (or first chooses) the open native session's model and effort on the
/// Mac. One request at a time; the answer is the app's own list afterwards.
pub(super) fn chat_options(state: &mut State, ctx: &Ctx, set: Option<(String, String)>) {
    let Some(chat) = &mut state.chat else { return };
    if matches!(chat.session.mode.as_str(), "cloud" | "orca")
        || chat.session.read_only
        || chat.options_busy
    {
        return;
    }
    let service = if chat.session.mode == "chat" {
        "agent.chat.options"
    } else {
        "agent.options.v1"
    };
    chat.options_busy = true;
    chat.options_error.clear();
    let (device, session, name) = (
        chat.device.clone(),
        chat.session.id.clone(),
        chat.session.device_name.clone(),
    );
    let provider = chat.session.provider.clone();
    let tx = ctx.tx.clone();
    let epoch = state.chat_call_epoch;
    let self_id = chat_route(state, &device);
    let backend = ctx.backend.clone();
    spawn(ctx, async move {
        let result = tokio::time::timeout(Duration::from_secs(120), async {
            let client = backend.client_for(&device, &self_id).await?;
            let Resp::DeviceInfo(info) =
                unary(&client, Req::DeviceInfo(v1::DeviceInfoRequest {})).await?
            else {
                return Err(unexpected());
            };
            if !info.services.iter().any(|s| s == service) {
                return Err(Error::Invalid(format!(
                    "update Muster on {name} to choose models and effort for this session"
                )));
            }
            let req = match set {
                None => Req::AgentChatOptions(v1::AgentChatOptionsRequest {
                    session: session.clone(),
                    ..Default::default()
                }),
                Some((model, effort)) => Req::AgentChatSetOptions(v1::AgentChatSetOptionsRequest {
                    session: session.clone(),
                    model,
                    effort,
                    ..Default::default()
                }),
            };
            match protected_chat_call(tx, epoch, device.clone(), provider, req).await? {
                Resp::AgentChatOptions(options) if options.session == session => Ok(options),
                _ => Err(unexpected()),
            }
        })
        .await
        .unwrap_or_else(|_| Err(Error::Unavailable("the Mac did not answer in time".into())));
        Msg::ChatOptions {
            device,
            session,
            result,
        }
    });
}

pub(super) fn intent_chat_set_options(state: &mut State, ctx: &Ctx, o: app::ChatSetOptions) {
    if o.model.is_empty() && o.effort.is_empty() {
        return;
    }
    chat_options(state, ctx, Some((o.model, o.effort)))
}

pub(super) fn complete(state: &mut State, ctx: &Ctx) {
    let Some(chat) = &mut state.chat else { return };
    if !matches!(chat.session.provider.as_str(), "claude" | "codex")
        || chat.session.mode != "code"
        || chat.session.read_only
        || chat.completing
        || chat.sending
        || matches!(chat.session.state.as_str(), "running" | "waiting_approval")
    {
        return;
    }
    chat.completing = true;
    chat.completion = None;
    chat.completion_error.clear();
    let (device, session, turn, provider) = (
        chat.device.clone(),
        chat.session.id.clone(),
        chat.content_revision,
        chat.session.provider.clone(),
    );
    let self_id = chat_route(state, &device);
    let backend = ctx.backend.clone();
    let tx = ctx.tx.clone();
    let epoch = state.chat_call_epoch;
    spawn(ctx, async move {
        let result = tokio::time::timeout(Duration::from_secs(120), async {
            let client = backend.client_for(&device, &self_id).await?;
            let Resp::DeviceInfo(info) =
                unary(&client, Req::DeviceInfo(v1::DeviceInfoRequest {})).await?
            else {
                return Err(unexpected());
            };
            if !info.services.iter().any(|s| s == "agent.completion.v1") {
                return Err(Error::Unavailable(
                    "Update Muster on the Mac to read native suggestions".into(),
                ));
            }
            let request = Req::AgentComplete(v1::AgentCompleteRequest {
                session: session.clone(),
                ..Default::default()
            });
            match protected_chat_call(tx, epoch, device.clone(), provider, request).await? {
                Resp::AgentCompletion(reply) if reply.session == session => Ok(reply.text),
                _ => Err(unexpected()),
            }
        })
        .await
        .unwrap_or_else(|_| Err(Error::Unavailable("The Mac did not answer in time".into())));
        Msg::Completion {
            device,
            session,
            turn,
            result,
        }
    });
}

pub(super) fn msg_completion(
    state: &mut State,
    device: String,
    session: String,
    turn: u64,
    result: Result<String, Error>,
) {
    let Some(chat) = &mut state.chat else { return };
    if chat.device != device || chat.session.id != session {
        return;
    }
    chat.completing = false;
    match result {
        Ok(text) if !text.is_empty() && chat.content_revision == turn => {
            chat.completion = Some((turn, text));
        }
        Ok(_) => chat.completion_error = "The app has no suggestion for this turn".into(),
        Err(error) => chat.completion_error = error.to_string(),
    }
}

pub(super) fn msg_chat_options(
    state: &mut State,
    device: String,
    session: String,
    result: Result<v1::AgentChatOptions, Error>,
) {
    let Some(chat) = &mut state.chat else { return };
    if chat.device != device || chat.session.id != session {
        return;
    }
    chat.options_busy = false;
    match result {
        Ok(options) => {
            chat.options = Some(options);
            chat.options_error.clear();
        }
        Err(error) => {
            // A model can change the effort menu. Do not leave stale checkmarks
            // visible after a partial/unknown write or a failed refresh.
            chat.options = None;
            chat.options_error = error.to_string();
        }
    }
}

pub(super) fn msg_desktop_chats(
    state: &mut State,
    ctx: &Ctx,
    load: u64,
    result: Result<PickedHost, Error>,
) {
    if load != state.desktop_chats_load {
        return;
    }
    let view = &mut state.desktop_chats;
    view.loading = false;
    match result {
        Ok(picked) => {
            view.device_id = picked.device;
            view.device_name = picked.name;
            view.error = picked.catalog.history_warning.clone();
            view.catalog = Some(picked.catalog);
            ctx.cache.put(
                crate::cache::Cache::key(&[
                    state
                        .network
                        .as_ref()
                        .map(|n| n.network_id.as_str())
                        .unwrap_or("local"),
                    "chat-list-v2",
                    "",
                    &view.provider,
                ]),
                view,
            );
            if let Some((name, why)) = picked.left {
                let text = format!("{name}: {why}; chats now through {}", view.device_name);
                state.toast(text, false);
            }
            match state.desktop_chat_retry.take() {
                Some(ChatRetry::Open { chat, desktop }) => {
                    desktop_chat_open(state, ctx, chat, desktop, true)
                }
                Some(ChatRetry::Move { session, from }) => move_chat(state, ctx, *session, from),
                None => {}
            }
        }
        Err(error) => {
            view.device_id.clear();
            view.device_name.clear();
            view.error = error.to_string();
            if state.desktop_chat_retry.take().is_some() {
                state.toast(error.to_string(), true);
            }
        }
    }
}

pub(super) fn msg_desktop_chat_opened(
    state: &mut State,
    ctx: &Ctx,
    view: u64,
    device: String,
    retry: Option<ChatRetry>,
    result: Result<v1::AgentSession, Error>,
) {
    if view != state.desktop_chats_view {
        return;
    }
    state.desktop_chats.opening.clear();
    match result {
        // Only follow into the conversation while the Chat tab is
        // still shown (its list, or a chat beside it on a Mac).
        Ok(session)
            if state.workspace() == Screen::DesktopChats
                && matches!(state.screen, Screen::DesktopChats | Screen::Chat) =>
        {
            show_chat(state, ctx, device, session)
        }
        Ok(_) => {}
        Err(Error::Busy(why)) if retry.is_some() => {
            state.toast(format!("{why}; trying another Mac"), false);
            repick_chat_host(state, ctx, &device, retry.expect("checked"));
        }
        Err(error) => {
            let text = error.to_string();
            state.desktop_chats.error = text.clone();
            state.toast(text, true);
        }
    }
}
