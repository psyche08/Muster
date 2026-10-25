//! Agent sessions: the list, a chat, its watch, sending and answering.

use super::*;

pub(super) fn capture_composer(
    state: &mut State,
    ctx: &Ctx,
    draft: app::ChatDraft,
    durable: bool,
) -> bool {
    let Some(chat) = &mut state.chat else {
        return false;
    };
    // The key is issued by Rust for the current conversation, never a path
    // supplied by the shell. Reject stale snapshots after rapid navigation.
    if draft.key != chat.draft.key || draft.revision.is_empty() {
        return false;
    }
    if draft.text.len() > 1024 * 1024 {
        state.toast("Draft is too large to save (1 MiB limit)", true);
        return false;
    }
    let draft = crate::drafts::reconcile(&ctx.cache, draft);
    chat.draft = draft.clone();
    if durable {
        if let Err(error) = ctx.cache.put_durable(draft.key.clone(), &draft) {
            state.toast(format!("Draft could not be saved: {error}"), true);
            return false;
        }
    } else {
        ctx.cache.remember(draft.key.clone(), &draft);
    }
    true
}

pub(super) fn cache_key(state: &State, kind: &str, device: &str, id: &str) -> String {
    crate::cache::Cache::key(&[
        state
            .network
            .as_ref()
            .map(|n| n.network_id.as_str())
            .unwrap_or("local"),
        kind,
        device,
        id,
    ])
}

pub(crate) fn flush_chat_cache(state: &mut State, ctx: &Ctx) {
    if let Some(chat) = &mut state.chat
        && chat.cache_dirty
    {
        cache_chat(ctx, chat);
        chat.cache_dirty = false;
    }
}

pub(super) fn cache_chat(ctx: &Ctx, chat: &Chat) {
    ctx.cache.put(
        chat.cache_key.clone(),
        &v1::AgentMessagePage {
            session: chat.session.id.clone(),
            messages: chat.messages.clone(),
            has_more: chat.has_earlier,
            ..Default::default()
        },
    );
}

/// Apply the same visibility rule to cached catalogs and live peer responses.
/// Ended sessions remain history; only explicitly archived sessions are hidden.
/// Sessions listed per device, newest first: the list refreshes every few
/// seconds while visible, and a phone does not need a device's whole history.
const AGENT_LIST_LIMIT: u32 = 200;

pub(super) fn visible_agents(mut sessions: Vec<v1::AgentSession>) -> Vec<v1::AgentSession> {
    sessions.retain(|session| !session.archived);
    sessions
}

pub(super) fn open_agents(state: &mut State, ctx: &Ctx) {
    let filter = if state.agents.device_id.is_empty() {
        "*".to_string()
    } else {
        state.agents.device_id.clone()
    };
    state.go(Screen::Agents);
    state.agents.device_id = filter.clone();
    state.agents_errors.clear();
    state
        .agents
        .sessions
        .retain(|s| filter == "*" || s.device_id == filter);
    state.agents_epoch += 1;
    let epoch = state.agents_epoch;
    // An offline Mac is neither asked nor shown (its cache stays on disk);
    // it is listed again once presence says it is back.
    let devices: Vec<_> = state
        .reachable_service_devices()
        .into_iter()
        .filter(|m| filter == "*" || m.device_id == filter)
        .map(|m| (m.device_id, m.name, m.is_self))
        .collect();
    state
        .agents
        .sessions
        .retain(|s| devices.iter().any(|(id, _, _)| id == &s.device_id));
    state.agents_pending = devices.len();
    // A refresh every few seconds must not flip the view: loading shows
    // only while there is nothing to list yet.
    state.agents.loading = !devices.is_empty() && state.agents.sessions.is_empty();
    for (device, name, is_self) in devices {
        if !state.agents.sessions.iter().any(|s| s.device_id == device)
            && let Some(page) = ctx
                .cache
                .get::<v1::AgentSessionPage>(&cache_key(state, "agents", &device, ""))
        {
            state.agents.sessions.extend(visible_agents(page.sessions));
        }
        // Network state can arrive before DeviceInfo. Route an explicitly
        // local member through the local socket even before self_id is loaded.
        let self_id = if is_self {
            device.clone()
        } else {
            state.self_id.clone()
        };
        let backend = ctx.backend.clone();
        spawn(ctx, async move {
            let result = async {
                let client = tokio::time::timeout(
                    Duration::from_secs(3),
                    backend.client_for(&device, &self_id),
                )
                .await
                .map_err(|_| Error::Unavailable("device connection timed out".into()))??;
                // Connection establishment and the read RPC have separate budgets.
                // A recovering relay must not consume the catalog's entire deadline.
                let response = tokio::time::timeout(
                    Duration::from_secs(3),
                    unary(
                        &client,
                        Req::AgentList(v1::AgentListRequest {
                            limit: AGENT_LIST_LIMIT,
                            ..Default::default()
                        }),
                    ),
                )
                .await
                .map_err(|_| Error::Unavailable("session list response timed out".into()))??;
                match response {
                    Resp::AgentSessions(mut page) => {
                        for session in &mut page.sessions {
                            session.device_id = device.clone();
                            session.device_name = name.clone();
                        }
                        Ok(page.sessions)
                    }
                    _ => Err(unexpected()),
                }
            }
            .await
            .map_err(|error: Error| Error::Unavailable(format!("{name}: {error}")));
            Msg::Agents {
                epoch,
                device,
                result,
            }
        });
    }
}

pub(super) fn open_agent(state: &mut State, ctx: &Ctx, session_id: String) {
    let device = state.target();
    let session = state
        .agents
        .sessions
        .iter()
        .find(|s| s.id == session_id && (s.device_id.is_empty() || s.device_id == device))
        .cloned()
        .unwrap_or_else(|| v1::AgentSession {
            id: session_id.clone(),
            ..Default::default()
        });
    show_chat(state, ctx, device, session);
}

/// The chat screen on `session` of `device`, with a fresh watch.
pub(super) fn show_chat(state: &mut State, ctx: &Ctx, device: String, session: v1::AgentSession) {
    flush_chat_cache(state, ctx);
    let session_id = session.id.clone();
    // Desktop chats keep one draft across hosts; Agent sessions are host-local.
    let draft_host = if session.mode == "chat" { "" } else { &device };
    let draft_key = cache_key(state, "composer-draft", draft_host, &session_id);
    let draft = ctx
        .cache
        .get::<app::ChatDraft>(&draft_key)
        .unwrap_or(app::ChatDraft {
            key: draft_key,
            ..Default::default()
        });
    let key = cache_key(state, "messages", &device, &session_id);
    let cached = ctx
        .cache
        .get::<v1::AgentMessagePage>(&key)
        .unwrap_or_default();
    super::terminals::detach_session_terminal(state);
    state.chat = Some(Chat {
        content_revision: crate::state::next_content_revision(),
        cache_dirty: false,
        cache_key: key,
        device: device.clone(),
        session,
        messages: cached.messages,
        history_start: None,
        interactions: Vec::new(),
        has_earlier: cached.has_more,
        sending: false,
        responding_interaction: String::new(),
        response_interaction: String::new(),
        response_error: String::new(),
        response_unknown: false,
        pending_draft_id: String::new(),
        sent_draft_id: String::new(),
        unconfirmed: None,
        draft,
        loading: true,
        load_error: String::new(),
        watch: None,
        options: None,
        options_busy: false,
        options_error: String::new(),
        completing: false,
        completion: None,
        completion_error: String::new(),
    });
    state
        .chat
        .as_mut()
        .expect("opened chat")
        .reconcile_history_start();
    state.files = None;
    state.file = None;
    state.git = None;
    state.go(Screen::Chat);
    super::admitted::settle(state);
    watch_agent(state, ctx, device, session_id);
    super::send_recovery::restore(state, ctx);
}

/// A fresh tail repairs holes in stream delivery without discarding older
/// pages. Its order is authoritative; IDs, never repeated text, deduplicate.
pub(super) fn merge_chat_tail(current: &mut Vec<v1::AgentMessage>, tail: Vec<v1::AgentMessage>) {
    if tail.is_empty() {
        return;
    }
    let ids: std::collections::HashSet<_> = tail.iter().map(|m| m.id.as_str()).collect();
    let prefix = current
        .iter()
        .position(|m| ids.contains(m.id.as_str()))
        .unwrap_or(0);
    // No overlap means an unknown gap. Show the authoritative recent tail;
    // earlier-page navigation can then recover the gap instead of hiding it.
    current.truncate(prefix);
    current.extend(tail);
}

/// Drop replayed messages through the snapshot's last queued overlap. Without
/// overlap their order is unknown, so reconnect/catch up instead of appending
/// potentially older messages after the authoritative tail.
pub(super) fn reconcile_queued(
    queued: &mut std::collections::VecDeque<v1::event::Ev>,
    messages: &[v1::AgentMessage],
) -> bool {
    let ids: std::collections::HashSet<_> = messages.iter().map(|m| m.id.as_str()).collect();
    let mut has_messages = false;
    let mut through = None;
    for (i, event) in queued.iter().enumerate() {
        if let v1::event::Ev::AgentEvent(v1::AgentEvent {
            ev: Some(v1::agent_event::Ev::Message(m)),
        }) = event
        {
            has_messages = true;
            if ids.contains(m.id.as_str()) {
                through = Some(i);
            }
        }
    }
    if has_messages && through.is_none() {
        return false;
    }
    if let Some(through) = through {
        let mut i = 0;
        queued.retain(|event| {
            let keep = i > through
                || !matches!(
                    event,
                    v1::event::Ev::AgentEvent(v1::AgentEvent {
                        ev: Some(v1::agent_event::Ev::Message(_)),
                    })
                );
            i += 1;
            keep
        });
    }
    true
}

/// Reuse a validated cache for small deltas. A large offline gap must not
/// delay today's tail behind an unbounded forward crawl; fetch the recent tail
/// once, mark the gap explicitly, and leave older messages to pagination.
pub(super) struct ChatCatchUp {
    pub messages: Vec<v1::AgentMessage>,
    pub reset: bool,
    latest: bool,
}
impl ChatCatchUp {
    pub fn new(messages: Vec<v1::AgentMessage>) -> Self {
        Self {
            messages,
            reset: false,
            latest: false,
        }
    }
    pub fn request(&self, session: &str) -> v1::AgentMessagesRequest {
        muster_proto::history::request(session, &self.messages)
    }
    pub fn apply(&mut self, page: v1::AgentMessagePage) -> Result<Option<bool>, Error> {
        if page.has_newer {
            if self.latest || self.messages.is_empty() {
                return Err(Error::Unavailable(
                    "recent history was not a complete tail".into(),
                ));
            }
            self.latest = true;
            self.reset = true;
            self.messages.clear();
            return Ok(None);
        }
        if page.reset {
            self.reset = true;
            self.messages.clear();
        }
        merge_chat_tail(&mut self.messages, page.messages);
        Ok(Some(page.has_more))
    }
}

/// Streams a session's events into the open chat until its watch ends.
pub(super) fn watch_agent(state: &mut State, ctx: &Ctx, device: String, session_id: String) {
    flush_chat_cache(state, ctx);
    let (watch, token) = state.new_watch();
    match &mut state.chat {
        Some(chat) if chat.device == device && chat.session.id == session_id => {
            chat.watch = Some(watch);
        }
        _ => return,
    }
    let cache = ctx.cache.clone();
    let key = state.chat.as_ref().expect("chat watch").cache_key.clone();
    let self_id = state.self_id.clone();
    let backend = ctx.backend.clone();
    let tx = ctx.tx.clone();
    tokio::spawn(async move {
        let mut retry = 1;
        loop {
            let mut active_call = None;
            let attempt = async {
                let client = backend.client_for(&device, &self_id).await?;
                active_call = Some(
                    client
                        .start(Req::AgentWatch(v1::AgentWatchRequest {
                            session: session_id.clone(),
                        }))
                        .await?,
                );
                let call = active_call.as_mut().expect("started watch");
                // The first State comes after server subscription. Fetch history
                // only after that barrier, while subsequent events queue here.
                let first = match tokio::time::timeout(Duration::from_secs(3), call.events.recv())
                    .await
                    .map_err(|_| Error::Unavailable("session watch timed out".into()))?
                {
                    Some(event) => event,
                    None => {
                        return Err(Error::Unavailable(
                            "session watch ended before initial state".into(),
                        ));
                    }
                };
                let cached = cache.get::<v1::AgentMessagePage>(&key).unwrap_or_default();
                let mut catch_up = ChatCatchUp::new(cached.messages);
                let mut queued = std::collections::VecDeque::from([first]);
                let has_more = loop {
                    let request = catch_up.request(&session_id);
                    let snapshot = tokio::time::timeout(
                        Duration::from_secs(3),
                        unary(&client, Req::AgentMessages(request)),
                    );
                    tokio::pin!(snapshot);
                    let response = loop {
                        tokio::select! {
                            result=&mut snapshot => break result.map_err(|_|Error::Unavailable("session history timed out".into()))??,
                            event=call.events.recv() => match event {
                                Some(event) if queued.len()<2048 => queued.push_back(event),
                                _ => return Err(Error::Unavailable("session watch needs resynchronization".into())),
                            }
                        }
                    };
                    let Resp::AgentMessages(page) = response else {
                        return Err(unexpected());
                    };
                    if let Some(has_more) = catch_up.apply(page)? {
                        break has_more;
                    }
                };
                let ChatCatchUp {
                    messages, reset, ..
                } = catch_up;
                let reconciled = reconcile_queued(&mut queued, &messages);
                let _ = tx.send(loop_message(Msg::Chat {
                    token: token.clone(),
                    device: device.clone(),
                    session: session_id.clone(),
                    earlier: false,
                    reset,
                    result: Ok((messages, has_more)),
                }));
                if !reconciled {
                    return Err(Error::Unavailable(
                        "session history needs another catch-up".into(),
                    ));
                }
                retry = 1;
                let mut next = queued.pop_front();
                loop {
                    if let Some(v1::event::Ev::AgentEvent(v1::AgentEvent { ev: Some(event) })) =
                        next
                    {
                        let _ = tx.send(loop_message(Msg::AgentEvent {
                            token: token.clone(),
                            device: device.clone(),
                            session: session_id.clone(),
                            event,
                        }));
                    }
                    next = match queued.pop_front() {
                        Some(event) => Some(event),
                        None => call.events.recv().await,
                    };
                    if next.is_none() {
                        break;
                    }
                }
                Err::<(), Error>(Error::Unavailable("session watch ended".into()))
            };
            tokio::select! {
                _ = token.cancelled() => {},
                result = attempt => {
                    if let Err(error) = result {
                        log::warn!("session live updates interrupted; retrying in {retry}s: {error}");
                        let _ = tx.send(loop_message(Msg::Chat {
                            token: token.clone(), device: device.clone(), session: session_id.clone(),
                            earlier: false, reset: false, result: Err(error),
                        }));
                    }
                }
            }
            if let Some(call) = active_call {
                let _ = tokio::time::timeout(Duration::from_secs(1), call.cancel()).await;
            }
            tokio::select! {
                _ = token.cancelled() => return,
                _ = tokio::time::sleep(Duration::from_secs(retry)) => {}
            }
            retry = (retry * 2).min(8);
        }
    });
}

/// The open chat's device and session, or a toast.
pub(super) fn chat_target(state: &mut State) -> Option<(String, String)> {
    let target = state.chat_target();
    if target.is_none() {
        state.toast("open a session first", true);
    }
    target
}

pub(super) fn intent_open_agents(state: &mut State, ctx: &Ctx, o: app::OpenAgents) {
    state.agents.device_id = if o.device_id.is_empty() {
        state.target()
    } else {
        o.device_id
    };
    if state.agents.device_id != "*" {
        state.selected = state.agents.device_id.clone();
    }
    open_agents(state, ctx);
}

pub(super) fn related_agent(
    state: &State,
    device: &str,
    id: &str,
) -> Option<(String, v1::AgentSession)> {
    let chat = state.chat.as_ref()?;
    if !device.is_empty() && device != chat.device {
        return None;
    }
    let session = chat
        .session
        .subagents
        .iter()
        .chain(chat.session.subsessions.iter())
        .find(|s| s.id == id)
        .cloned()
        .or_else(|| {
            (!chat.session.parent_session_id.is_empty() && chat.session.parent_session_id == id)
                .then(|| {
                    state
                        .agents
                        .sessions
                        .iter()
                        .find(|s| s.id == id && s.device_id == chat.device)
                        .cloned()
                        .unwrap_or_else(|| v1::AgentSession {
                            id: id.into(),
                            read_only: true,
                            ..Default::default()
                        })
                })
        })?;
    Some((chat.device.clone(), session))
}

pub(super) fn intent_open_agent(state: &mut State, ctx: &Ctx, o: app::OpenAgent) {
    // Only Rust-projected relationships on the currently viewed device can
    // navigate outside the normal catalog. A child never selects another Mac.
    if let Some((device, session)) = related_agent(state, &o.device_id, &o.session) {
        show_chat(state, ctx, device, session);
        return;
    }
    if !o.device_id.is_empty() {
        if !state
            .agents
            .sessions
            .iter()
            .any(|s| s.id == o.session && s.device_id == o.device_id)
        {
            state.toast("session is no longer listed on this device", true);
            return;
        }
        state.selected = o.device_id;
    }
    open_agent(state, ctx, o.session);
}

pub(super) fn intent_agent_send(state: &mut State, ctx: &Ctx, s: app::AgentSend) {
    let op_id = state.op_id();
    let mut diagnostic = crate::send_diagnostics::Trace::new(
        ctx.shell.clone(),
        "send",
        state
            .chat
            .as_ref()
            .map(|c| c.device.as_str())
            .unwrap_or_default(),
        &Req::AgentSend(v1::AgentSendRequest {
            op_id: op_id.clone(),
            ..Default::default()
        }),
    );
    diagnostic.stage("validation");
    if state.chat.as_ref().is_some_and(|c| c.session.read_only) {
        state.toast(
            "Subagent transcripts are read-only; use the parent session",
            true,
        );
        diagnostic.rejection("read_only");
        return;
    }
    if let Err(error) = muster_proto::attachments::validate(&s.text, &s.attachments) {
        state.toast(error, true);
        diagnostic.rejection("invalid_input");
        return;
    }
    if state
        .chat
        .as_ref()
        .is_some_and(|c| c.session.mode == "chat" && !s.attachments.is_empty())
    {
        state.toast("Desktop chats take text only", true);
        diagnostic.rejection("attachments_unsupported");
        return;
    }
    let Some(chat) = &mut state.chat else {
        diagnostic.rejection("no_conversation");
        return;
    };
    if chat.sending {
        diagnostic.rejection("already_sending");
        return;
    }
    // A release names the refusal this conversation shows, nothing else.
    if !s.release_unconfirmed.is_empty()
        && chat.unconfirmed.as_ref().map(|u| &u.token) != Some(&s.release_unconfirmed)
    {
        diagnostic.rejection("stale_release");
        return;
    }
    let (device, session) = (chat.device.clone(), chat.session.id.clone());
    let tag = format!("{}:{}", op_id, s.draft_id);
    let request = v1::AgentSendRequest {
        attachments: s.attachments,
        op_id,
        session,
        text: s.text,
        release_unconfirmed: s.release_unconfirmed,
    };
    if let Err(error) = super::send_recovery::prepare(state, ctx, &device, &request, &tag) {
        state.toast(error.to_string(), true);
        return;
    }
    let chat = state.chat.as_mut().expect("checked");
    chat.sending = true;
    chat.pending_draft_id = tag.clone();
    if chat.draft.text == request.text {
        state.pending_drafts.insert(tag, chat.draft.clone());
    }
    // An explicit release means the user checked the conversation on the Mac
    // themselves; only that drops records this conversation never accounted for.
    if !request.release_unconfirmed.is_empty() {
        super::admitted::release(state);
    }
    super::admitted::admit(state, &request.op_id, &request.text);
    diagnostic.event("validated");
    op_request(state, ctx, "send", device, Req::AgentSend(request));
}

pub(super) fn intent_agent_respond(state: &mut State, ctx: &Ctx, r: app::AgentRespond) {
    if state.chat.as_ref().is_some_and(|c| c.session.read_only) {
        state.toast(
            "Subagent transcripts are read-only; use the parent session",
            true,
        );
        return;
    }
    let Some(chat) = &mut state.chat else { return };
    if !chat.responding_interaction.is_empty() || state.presence.is_some() {
        return;
    }
    if chat.response_unknown {
        return;
    }
    let Some(interaction) = chat.interactions.iter().find(|i| i.id == r.interaction) else {
        return;
    };
    chat.response_interaction = r.interaction.clone();
    if interaction.kind == "question" {
        if let Err(error) = muster_agent::questions::validate(interaction, &r.answers) {
            chat.response_error = error.to_string();
            return;
        }
    } else if !interaction.options.contains(&r.option) || !r.answers.is_empty() {
        chat.response_error = "This option is not offered".into();
        return;
    }
    chat.responding_interaction = r.interaction.clone();
    chat.response_error.clear();
    let session = chat.session.id.clone();
    let device = chat.device.clone();
    let op_id = state.op_id();
    op_request(
        state,
        ctx,
        "respond",
        device,
        Req::AgentRespond(v1::AgentRespondRequest {
            op_id,
            interaction: r.interaction,
            session,
            option: r.option,
            answer: r.answer,
            answers: r.answers,
        }),
    );
}

pub(super) fn intent_agent_interrupt(state: &mut State, ctx: &Ctx) {
    if state.chat.as_ref().is_some_and(|c| c.session.read_only) {
        state.toast(
            "Subagent transcripts are read-only; use the parent session",
            true,
        );
        return;
    }
    let Some(chat) = &state.chat else { return };
    let (device, session) = (chat.device.clone(), chat.session.id.clone());
    let op_id = state.op_id();
    op_request(
        state,
        ctx,
        "interrupt",
        device,
        Req::AgentInterrupt(v1::AgentInterruptRequest { op_id, session }),
    );
}

pub(super) fn intent_agent_load_earlier(state: &mut State, ctx: &Ctx, load: app::AgentLoadEarlier) {
    let Some(chat) = &mut state.chat else { return };
    if chat.loading {
        return;
    }
    if chat.reveal_history(load.all) && (!load.all || !chat.has_earlier) {
        return;
    }
    if !chat.has_earlier {
        return;
    }
    let Some(first) = chat.messages.first() else {
        return;
    };
    chat.loading = true;
    let (device, session, before) = (
        chat.device.clone(),
        chat.session.id.clone(),
        first.id.clone(),
    );
    let Some(watch) = &chat.watch else { return };
    let token = watch.token.clone();
    let self_id = state.self_id.clone();
    let backend = ctx.backend.clone();
    spawn(ctx, async move {
        let result = async {
            let client = backend.client_for(&device, &self_id).await?;
            let mut cursor = before;
            let mut earlier = Vec::new();
            loop {
                if token.is_cancelled() {
                    return Err(Error::Unavailable("history loading cancelled".into()));
                }
                let response = tokio::time::timeout(
                    Duration::from_secs(8),
                    unary(
                        &client,
                        Req::AgentMessages(v1::AgentMessagesRequest {
                            session: session.clone(),
                            tail: crate::state::CHAT_HISTORY_PAGE as u32,
                            before_id: cursor.clone(),
                            ..Default::default()
                        }),
                    ),
                )
                .await
                .map_err(|_| Error::Unavailable("history loading timed out".into()))??;
                let Resp::AgentMessages(page) = response else {
                    return Err(unexpected());
                };
                if page.reset {
                    return Err(Error::Unavailable(
                        "History changed while loading; reopen the conversation to resynchronize"
                            .into(),
                    ));
                }
                let next = page.messages.first().map(|m| m.id.clone());
                let mut messages = page.messages;
                messages.extend(earlier);
                earlier = messages;
                if !load.all || !page.has_more {
                    return Ok((earlier, page.has_more));
                }
                let Some(next) = next.filter(|next| next != &cursor) else {
                    return Err(Error::Unavailable("history cursor did not advance".into()));
                };
                cursor = next;
            }
        }
        .await;
        Msg::Chat {
            token,
            device,
            session,
            earlier: true,
            reset: false,
            result,
        }
    });
}

pub(super) fn msg_retry_agents(state: &mut State, ctx: &Ctx, epoch: u64) {
    if epoch == state.agents_epoch
        && state.foreground
        && matches!(state.screen, Screen::Home | Screen::Agents)
    {
        open_agents(state, ctx);
    }
}

pub(super) fn msg_agents(
    state: &mut State,
    ctx: &Ctx,
    epoch: u64,
    device: String,
    result: Result<Vec<v1::AgentSession>, Error>,
) {
    if epoch != state.agents_epoch {
        return;
    }
    state.agents_pending = state.agents_pending.saturating_sub(1);
    state.agents.loading = state.agents.loading && state.agents_pending > 0;
    // A late answer from a Mac that has gone offline since must not bring
    // its sessions back, nor overwrite its cache.
    match result {
        _ if state.offline(&device) => {}
        Ok(sessions) => {
            let sessions = visible_agents(sessions);
            ctx.cache.put(
                cache_key(state, "agents", &device, ""),
                &v1::AgentSessionPage {
                    sessions: sessions.clone(),
                },
            );
            state.agents.sessions.retain(|s| s.device_id != device);
            state.agents.sessions.extend(sessions);
            state.agents.sessions.sort_by(|a, b| {
                b.updated_unix
                    .cmp(&a.updated_unix)
                    .then_with(|| a.device_id.cmp(&b.device_id))
                    .then_with(|| a.id.cmp(&b.id))
            });
        }
        Err(error) => state.agents_errors.push(error.to_string()),
    }
    if state.agents_pending == 0 {
        state.agents.errors = std::mem::take(&mut state.agents_errors);
        let delay = if state.agents.errors.is_empty() {
            state.agents_retry = 1;
            // A successful list is a snapshot, not a subscription.
            // Refresh while visible; RetryAgents rejects old epochs
            // and stops in the background or on another screen.
            3
        } else {
            let delay = state.agents_retry;
            state.agents_retry = (delay * 2).min(8);
            delay
        };
        spawn(ctx, async move {
            tokio::time::sleep(Duration::from_secs(delay)).await;
            Msg::RetryAgents(epoch)
        });
    }
}

pub(super) fn msg_chat(
    state: &mut State,
    token: CancellationToken,
    device: String,
    session: String,
    earlier: bool,
    reset: bool,
    result: Result<(Vec<v1::AgentMessage>, bool), Error>,
) {
    if token.is_cancelled() {
        return;
    }
    let Some(chat) = &mut state.chat else { return };
    if chat.device != device || chat.session.id != session {
        return;
    }
    chat.loading = false;
    match result {
        Ok((messages, has_more)) => {
            chat.cache_dirty = true;
            chat.content_revision = crate::state::next_content_revision();
            chat.load_error.clear();
            if earlier {
                let messages_ids: std::collections::HashSet<_> =
                    messages.iter().map(|m| m.id.clone()).collect();
                let mut merged = messages;
                merged.extend(
                    chat.messages
                        .drain(..)
                        .filter(|m| !messages_ids.contains(&m.id)),
                );
                chat.messages = merged;
                chat.history_start = chat.messages.first().map(|m| m.id.clone());
                chat.has_earlier = has_more;
            } else {
                // Earlier pages may finish while catch-up is in flight.
                // Keep that prefix only when the server validated history.
                let prefix = if reset {
                    None
                } else {
                    messages
                        .first()
                        .and_then(|first| chat.messages.iter().position(|m| m.id == first.id))
                };
                if prefix.is_some_and(|i| i > 0) {
                    merge_chat_tail(&mut chat.messages, messages);
                } else {
                    chat.messages = messages;
                    chat.has_earlier = has_more;
                }
            }
            if reset {
                chat.history_start = None;
            }
            chat.reconcile_history_start();
        }
        Err(error) => {
            chat.load_error = if earlier {
                format!("{error}. Retry loading earlier messages.")
            } else {
                format!("{error}. Reconnecting…")
            };
        }
    }
    super::admitted::settle(state);
}

pub(super) fn msg_agent_event(
    state: &mut State,
    ctx: &Ctx,
    token: CancellationToken,
    device: String,
    session: String,
    event: v1::agent_event::Ev,
) {
    if token.is_cancelled() {
        return;
    }
    let foreground = state.foreground;
    let Some(chat) = &mut state.chat else { return };
    if chat.device != device || chat.session.id != session {
        return;
    }
    let mut notify = None;
    let mut messages_changed = false;
    match event {
        v1::agent_event::Ev::Message(m) => {
            // Desktop chat replies grow under the same id.
            if let Some(known) = chat.messages.iter_mut().find(|x| x.id == m.id) {
                if *known != m {
                    *known = m;
                    messages_changed = true;
                }
            } else {
                chat.messages.push(m);
                messages_changed = true;
            }
            if messages_changed {
                chat.reconcile_history_start();
                chat.content_revision = crate::state::next_content_revision();
            }
        }
        v1::agent_event::Ev::State(s) => {
            if s.state != "waiting_approval" {
                // Question lifetime is tied to a provider result, not
                // transient process/AX activity (including locked use).
                chat.interactions
                    .retain(|i| matches!(i.kind.as_str(), "question" | "queued_message"));
            }
            chat.session = s;
        }
        v1::agent_event::Ev::InteractionResolved(id) => {
            chat.interactions.retain(|i| i.id != id);
            if chat.responding_interaction == id && !id.starts_with("queued:") {
                chat.responding_interaction.clear();
            }
            if chat.response_interaction == id && !id.starts_with("queued:") {
                chat.response_unknown = false;
                chat.response_error.clear();
            }
        }
        v1::agent_event::Ev::Interaction(i) => {
            if let Some(known) = chat.interactions.iter_mut().find(|x| x.id == i.id) {
                // A partial provider reply updates the remaining questions
                // without resolving the entire interaction or its draft.
                if !foreground && known.optional && !i.optional {
                    notify = Some((
                        chat.session.title.clone(),
                        i.prompt.clone(),
                        session.clone(),
                    ));
                }
                *known = i;
            } else {
                if !foreground && !i.optional {
                    notify = Some((
                        chat.session.title.clone(),
                        i.prompt.clone(),
                        session.clone(),
                    ));
                }
                chat.interactions.push(i);
            }
        }
        v1::agent_event::Ev::ListChanged(page) => {
            if let Some(s) = page.sessions.into_iter().find(|s| s.id == session) {
                chat.session = s;
            }
        }
    }
    if messages_changed {
        chat.cache_dirty = true;
    }
    // A new message or queue row can account for a send this client made.
    super::admitted::settle(state);
    if let Some((title, body, session)) = notify {
        effect(
            state,
            ctx,
            EffectKind::Other,
            app::effect::Ef::Notify(app::Notify {
                title,
                body,
                session,
            }),
        );
    }
}

pub(super) fn msg_watch_ended(
    state: &mut State,
    what: &'static str,
    device: String,
    session: String,
    error: Option<String>,
) {
    match what {
        "agent" => {
            if let Some(chat) = &mut state.chat
                && chat.device == device
                && chat.session.id == session
            {
                chat.watch = None;
                if let Some(error) = error {
                    state.toast(format!("live updates stopped: {error}"), true);
                }
            }
        }
        _ => {
            if let Some(terminal) = &mut state.terminal
                && terminal.device == device
                && terminal.session == session
            {
                terminal.ended = true;
                terminal.status = error.unwrap_or_else(|| "detached".into());
            }
        }
    }
}
