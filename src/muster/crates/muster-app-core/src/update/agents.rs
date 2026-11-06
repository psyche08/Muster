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
            messages: chat.messages[chat
                .messages
                .len()
                .saturating_sub(crate::state::CHAT_HISTORY_PAGE)..]
                .to_vec(),
            has_more: chat.has_earlier || chat.messages.len() > crate::state::CHAT_HISTORY_PAGE,
            ..Default::default()
        },
    );
}

/// Apply the same visibility rule to cached catalogs and live peer responses.
/// Ended sessions remain history; only explicitly archived sessions are hidden.
/// Sessions listed per device, newest first: the list refreshes every few
/// seconds while visible, and a phone does not need a device's whole history.
const AGENT_LIST_LIMIT: u32 = muster_agent::catalog::PAGE_SIZE as u32;

pub(super) fn visible_agents(mut sessions: Vec<v1::AgentSession>) -> Vec<v1::AgentSession> {
    sessions.retain(|session| !session.archived);
    sessions
}

pub(super) fn open_agents(state: &mut State, ctx: &Ctx) {
    state.go(Screen::Agents);
    load_catalog(state, ctx);
}

pub(super) fn load_catalog(state: &mut State, ctx: &Ctx) {
    let filter = if state.agents.device_id.is_empty() {
        "*".to_string()
    } else {
        state.agents.device_id.clone()
    };
    state.agents.device_id = filter.clone();
    super::catalog::restart_search(state, ctx);
    state.agents_errors.clear();
    state
        .agents
        .sessions
        .retain(|s| filter == "*" || s.device_id == filter);
    state.catalog_watches.clear();
    state.agent_catalog_pending.clear();
    state.agent_catalog_pages.clear();
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
            restore_catalog_cache(state, &device, &name, page);
        }
        // Network state can arrive before DeviceInfo. Route an explicitly
        // local member through the local socket even before self_id is loaded.
        let self_id = if is_self {
            device.clone()
        } else {
            state.self_id.clone()
        };
        let visible_sessions = crate::catalog::visible_ids(
            &state.agents.sessions,
            &device,
            *state
                .agent_catalog_limits
                .get(crate::catalog::PROVIDER_LIMIT)
                .unwrap_or(&muster_agent::catalog::PAGE_SIZE),
        );
        let (watch, token) = state.new_watch();
        state.catalog_watches.insert(device.clone(), watch);
        let backend = ctx.backend.clone();
        let tx = ctx.tx.clone();
        tokio::spawn(async move {
            let mut retry = 1;
            loop {
                let mut active = None;
                let attempt = async {
                    let client = tokio::time::timeout(
                        Duration::from_secs(3),
                        backend.client_for(&device, &self_id),
                    )
                    .await
                    .map_err(|_| Error::Unavailable("catalog connection timed out".into()))??;
                    active = Some(
                        client
                            .start(Req::AgentWatch(v1::AgentWatchRequest {
                                diagnostic_id: String::new(),
                                session: String::new(),
                                catalog_only: true,
                                provider_pages: true,
                                limit: AGENT_LIST_LIMIT,
                                grouped: true,
                                visible_sessions: visible_sessions.clone(),
                            }))
                            .await?,
                    );
                    let call = active.as_mut().unwrap();
                    let mut first = true;
                    loop {
                        let event = if first {
                            // Same budget as a session watch: on a desktop it
                            // covers the daemon's connection to the member.
                            match tokio::time::timeout(WATCH_START, call.events.recv()).await {
                                Ok(event) => event,
                                Err(_) => {
                                    backend.discard(&device, &client);
                                    return Err(Error::Unavailable(
                                        "catalog snapshot timed out".into(),
                                    ));
                                }
                            }
                        } else {
                            call.events.recv().await
                        };
                        let Some(event) = event else { break };
                        if let v1::event::Ev::AgentEvent(v1::AgentEvent {
                            ev: Some(v1::agent_event::Ev::ListChanged(mut page)),
                        }) = event
                        {
                            first = false;
                            retry = 1;
                            if page.groups.is_empty() {
                                page.sessions.truncate(AGENT_LIST_LIMIT as usize);
                            }
                            for session in &mut page.sessions {
                                session.device_id = device.clone();
                                session.device_name = name.clone();
                            }
                            let _ = tx.send(loop_message(Msg::Agents {
                                epoch,
                                device: device.clone(),
                                result: Ok({
                                    for section in &mut page.sections {
                                        section.device_id = device.clone();
                                        section.device_name = name.clone();
                                    }
                                    page
                                }),
                            }));
                        }
                    }
                    // Older daemons may not stream catalogs. A bounded snapshot
                    // remains a compatibility fallback before reconnecting.
                    if let Ok(Ok(Resp::AgentSessions(mut page))) = tokio::time::timeout(
                        Duration::from_secs(3),
                        unary(
                            &client,
                            Req::AgentList(v1::AgentListRequest {
                                limit: AGENT_LIST_LIMIT,
                                grouped: true,
                                ..Default::default()
                            }),
                        ),
                    )
                    .await
                    {
                        for session in &mut page.sessions {
                            session.device_id = device.clone();
                            session.device_name = name.clone();
                        }
                        let _ = tx.send(loop_message(Msg::Agents {
                            epoch,
                            device: device.clone(),
                            result: Ok({
                                for section in &mut page.sections {
                                    section.device_id = device.clone();
                                    section.device_name = name.clone();
                                }
                                page
                            }),
                        }));
                        retry = 3;
                        return Ok(());
                    }
                    Err::<(), Error>(Error::Unavailable(
                        "catalog subscription ended; reconnecting".into(),
                    ))
                };
                let result = tokio::select! {
                    _ = token.cancelled() => None,
                    result = attempt => Some(result),
                };
                if let Some(call) = active.take() {
                    let _ = call.cancel().await;
                }
                let Some(result) = result else { break };
                if let Err(error) = result {
                    let _ = tx.send(loop_message(Msg::Agents {
                        epoch,
                        device: device.clone(),
                        result: Err(Error::Unavailable(format!("{name}: {error}"))),
                    }));
                }
                tokio::select! {
                    _ = token.cancelled() => break,
                    _ = tokio::time::sleep(Duration::from_secs(retry)) => {},
                }
                retry = (retry * 2).min(15);
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
        .chain(state.agents_search_sessions.iter())
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
    let entry_trace =
        crate::session_diagnostics::Trace::new(ctx.shell.clone(), &device, &session.id);
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
    // A new namespace avoids synchronously decoding legacy whole-session entries.
    let key = format!(
        "history-{}",
        cache_key(state, "recent-messages-v2", &device, &session_id)
    );
    super::terminals::detach_session_terminal(state);
    state.chat = Some(Chat {
        load_trace: None,
        content_revision: crate::state::next_content_revision(),
        cache_dirty: false,
        cache_key: key,
        device: device.clone(),
        session,
        messages: Vec::new(),
        history_start: None,
        history_browsing: false,
        history_load_all: false,
        timeline_index: Default::default(),
        interactions: Vec::new(),
        approval_refresh_attempted: false,
        approval_refresh_pending: false,
        has_earlier: false,
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
    watch_agent_traced(state, ctx, device, session_id, Some(entry_trace));
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

/// Drop replayed messages through the snapshot's last queued overlap only when
/// their final overlapping revisions agree. IDs establish order, not freshness:
/// a same-ID update can beat an older snapshot across independent streams.
pub(super) fn reconcile_queued(
    queued: &mut std::collections::VecDeque<v1::event::Ev>,
    messages: &[v1::AgentMessage],
) -> bool {
    let snapshot: std::collections::HashMap<_, _> =
        messages.iter().map(|m| (m.id.as_str(), m)).collect();
    let mut latest = std::collections::HashMap::new();
    let mut has_messages = false;
    let mut through = None;
    for (i, event) in queued.iter().enumerate() {
        if let v1::event::Ev::AgentEvent(v1::AgentEvent {
            ev: Some(v1::agent_event::Ev::Message(m)),
        }) = event
        {
            has_messages = true;
            if snapshot.contains_key(m.id.as_str()) {
                through = Some(i);
                latest.insert(m.id.as_str(), m);
            }
        }
    }
    if has_messages && through.is_none() {
        return false;
    }
    if latest
        .iter()
        .any(|(id, message)| snapshot.get(id) != Some(message))
    {
        // Keep all evidence for the bounded same-subscription retry. Comparing
        // only the last revision per ID permits ordinary incremental updates.
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

/// How long a watch may take to deliver its first event. On a desktop the
/// local daemon first connects to the member (up to `CONNECT_TIMEOUT`) and
/// only then forwards the call, so the budget covers both.
const WATCH_START: Duration =
    Duration::from_secs(muster_net::reconnect::CONNECT_TIMEOUT.as_secs() + 3);

/// A watch's first event. A stream that ends first reports its final
/// response (a forwarding daemon's "offline" or "unreachable"), not a
/// generic timeout. A silent link is forgotten so the retry reconnects.
pub(super) async fn first_watch_event(
    backend: &crate::backend::Backend,
    device: &str,
    client: &Client,
    call: &mut Option<muster_rpc::Call>,
    timed_out: &str,
) -> Result<v1::event::Ev, Error> {
    let events = &mut call.as_mut().expect("started watch").events;
    match tokio::time::timeout(WATCH_START, events.recv()).await {
        Ok(Some(event)) => Ok(event),
        Ok(None) => {
            let ended = call.take().expect("started watch");
            match tokio::time::timeout(Duration::from_secs(1), ended.finish()).await {
                Ok(Err(error)) => Err(error.into()),
                _ => Err(Error::Unavailable(
                    "session watch ended before initial state".into(),
                )),
            }
        }
        Err(_) => {
            backend.discard(device, client);
            Err(Error::Unavailable(timed_out.into()))
        }
    }
}

/// Streams a session's events into the open chat until its watch ends.
pub(super) fn watch_agent(state: &mut State, ctx: &Ctx, device: String, session_id: String) {
    watch_agent_traced(state, ctx, device, session_id, None);
}
fn watch_agent_traced(
    state: &mut State,
    ctx: &Ctx,
    device: String,
    session_id: String,
    entry: Option<crate::session_diagnostics::Trace>,
) {
    let mut trace = entry.unwrap_or_else(|| {
        crate::session_diagnostics::Trace::new(ctx.shell.clone(), &device, &session_id)
    });
    flush_chat_cache(state, ctx);
    let (watch, token) = state.new_watch();
    match &mut state.chat {
        Some(chat) if chat.device == device && chat.session.id == session_id => {
            chat.load_trace = Some(trace.clone());
            chat.watch = Some(watch);
            chat.approval_refresh_attempted = false;
            chat.approval_refresh_pending = false;
        }
        _ => return,
    }
    let cache = ctx.cache.clone();
    let key = state.chat.as_ref().expect("chat watch").cache_key.clone();
    let self_id = state.self_id.clone();
    let backend = ctx.backend.clone();
    let tx = ctx.tx.clone();
    let shell = ctx.shell.clone();
    {
        let (cache, key, token, device, session) = (
            cache.clone(),
            key.clone(),
            token.clone(),
            device.clone(),
            session_id.clone(),
        );
        spawn(ctx, async move {
            let page = cache.history(key).await.unwrap_or_default();
            Msg::ChatCache {
                token,
                device,
                session,
                page,
            }
        });
    }
    tokio::spawn(async move {
        let entry_trace_id = trace.id().to_string();
        let mut retry = 1;
        let mut attempt_index = 0u64;
        loop {
            attempt_index += 1;
            let request_id = format!("{}-{:x}", trace.id(), attempt_index);
            trace.record(
                "attempt_started",
                serde_json::json!({"attempt":attempt_index}),
            );
            let mut phase = "watch_acquire";
            let mut active_call = None;
            let attempt = async {
                let client = tokio::time::timeout(
                    Duration::from_secs(3),
                    backend.client_for_observed(
                        &device,
                        &self_id,
                        Some(trace.observer(format!("watch-{attempt_index}"))),
                    ),
                )
                .await
                .map_err(|_| Error::Unavailable("session connection timed out".into()))??;
                trace.record("watch_client_ready", serde_json::json!({"attempt":attempt_index,"route":if backend.is_embedded(){"embedded"}else{"socket_proxy"}}));
                phase = "watch_first_event";
                active_call = Some(
                    client
                        .start(Req::AgentWatch(v1::AgentWatchRequest {
                            session: session_id.clone(),
                            diagnostic_id: request_id.clone(),
                            ..Default::default()
                        }))
                        .await?,
                );
                // The first State comes after server subscription. Fetch history
                // only after that barrier, while subsequent events queue here.
                let first = first_watch_event(
                    &backend,
                    &device,
                    &client,
                    &mut active_call,
                    "session watch timed out",
                )
                .await?;
                trace.record(
                    "watch_first_event",
                    serde_json::json!({"attempt":attempt_index}),
                );
                let call = active_call.as_mut().expect("started watch");
                // A newly established subscription replays the server's current
                // pending interactions. Retire the old stream's display IDs
                // before that replay, not on transient activity changes.
                if let v1::event::Ev::AgentEvent(v1::AgentEvent {
                    ev: Some(v1::agent_event::Ev::State(initial)),
                }) = &first
                    && initial.id == session_id
                {
                    let _ = tx.send(loop_message(Msg::AgentStreamStarted {
                        token: token.clone(),
                        device: device.clone(),
                        session: session_id.clone(),
                        provider: initial.provider.clone(),
                    }));
                }
                let cached = cache.history(key.clone()).await.unwrap_or_default();
                let mut catch_up = ChatCatchUp::new(cached.messages);
                let mut queued = std::collections::VecDeque::new();
                let dispatch = |event, queued: &mut std::collections::VecDeque<_>| {
                    match event {
                        v1::event::Ev::AgentEvent(v1::AgentEvent { ev: Some(event) })
                            if !matches!(event, v1::agent_event::Ev::Message(_)) =>
                        {
                            // Metadata and controls must not wait behind transcript I/O.
                            let _ = tx.send(loop_message(Msg::AgentEvent {
                                token: token.clone(),
                                device: device.clone(),
                                session: session_id.clone(),
                                event,
                            }));
                        }
                        event => queued.push_back(event),
                    }
                };
                dispatch(first, &mut queued);
                // History owns a separate service stream from the live watch;
                // both reuse the underlying authenticated peer connection.
                phase = "history_acquire";
                trace.record(
                    "history_acquire_started",
                    serde_json::json!({"attempt":attempt_index}),
                );
                let history_client = tokio::time::timeout(
                    Duration::from_secs(3),
                    backend.client_for_observed(
                        &device,
                        &self_id,
                        Some(trace.observer(format!("history-open-{attempt_index}"))),
                    ),
                )
                .await
                .map_err(|_| Error::Unavailable("session history connection timed out".into()))??;
                trace.record(
                    "history_client_ready",
                    serde_json::json!({"attempt":attempt_index}),
                );
                let mut page_index = 0u64;
                let mut retried_overlap = false;
                let has_more = loop {
                    phase = "history_response";
                    page_index += 1;
                    let mut request = catch_up.request(&session_id);
                    request.diagnostic_id = format!("{request_id}-b-{page_index:x}");
                    trace.record("history_requested",serde_json::json!({"attempt":attempt_index,"page":page_index,"tail":request.tail,"cached_messages":catch_up.messages.len()}));
                    let snapshot_client = history_client.clone().with_timing_observer(Some(
                        trace.observer(format!("history-{attempt_index}-{page_index}")),
                    ));
                    let snapshot = tokio::time::timeout(
                        Duration::from_secs(15),
                        unary(&snapshot_client, Req::AgentMessages(request)),
                    );
                    tokio::pin!(snapshot);
                    let response = loop {
                        tokio::select! {
                            result=&mut snapshot => break result.map_err(|_|Error::Unavailable("session history timed out".into()))??,
                            event=call.events.recv() => match event {
                                Some(event) if queued.len()<2048 => dispatch(event, &mut queued),
                                _ => return Err(Error::Unavailable("session watch needs resynchronization".into())),
                            }
                        }
                    };
                    let Resp::AgentMessages(page) = response else {
                        return Err(unexpected());
                    };
                    trace.record("history_received",serde_json::json!({"attempt":attempt_index,"page":page_index,"messages":page.messages.len(),"reset":page.reset,"has_newer":page.has_newer}));
                    if let Some(has_more) = catch_up.apply(page)? {
                        if reconcile_queued(&mut queued, &catch_up.messages) {
                            break has_more;
                        }
                        // A newly appended event can beat an older snapshot
                        // across the independent streams. Keep its subscription
                        // and queue for one more snapshot so their overlap can
                        // establish order, instead of repeating the same race
                        // on a brand-new subscription during sustained output.
                        if retried_overlap {
                            return Err(Error::Unavailable(
                                "session history needs another catch-up".into(),
                            ));
                        }
                        trace.record(
                            "reconcile_retry",
                            serde_json::json!({"attempt":attempt_index,"page":page_index}),
                        );
                        retried_overlap = true;
                    }
                };
                drop(history_client);
                let ChatCatchUp {
                    messages, reset, ..
                } = catch_up;
                trace.record("history_reconciled",serde_json::json!({"attempt":attempt_index,"messages":messages.len(),"queued_events":queued.len()}));
                let _ = tx.send(loop_message(Msg::Chat {
                    token: token.clone(),
                    device: device.clone(),
                    session: session_id.clone(),
                    earlier: false,
                    reset,
                    result: Ok((messages, has_more)),
                }));
                phase = "live_watch";
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
                _ = token.cancelled() => { trace.record("cancelled",serde_json::json!({"attempt":attempt_index})); },
                result = attempt => {
                    if let Err(error) = result {
                        trace.record("attempt_failed",serde_json::json!({"attempt":attempt_index,"phase":phase,"retry_delay_ms":retry*1000,"error_category":match &error { Error::Invalid(_)=>"invalid",Error::Unauthorized(_)=>"unauthorized",Error::Rpc(_)=>"remote",Error::Busy(_)=>"busy",Error::Unavailable(_)=>"unavailable" }}));
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
            if token.is_cancelled() {
                return;
            }
            // Publish fresh readiness during backoff, not only after the next
            // snapshot. The old trace's native acknowledgements stay stale.
            let previous_trace_id = trace.id().to_string();
            trace = crate::session_diagnostics::Trace::new(shell.clone(), &device, &session_id);
            trace.record(
                "retry_of",
                serde_json::json!({
                    "entry_trace_id": entry_trace_id,
                    "previous_trace_id": previous_trace_id,
                }),
            );
            let _ = tx.send(loop_message(Msg::AgentLoadStarted {
                token: token.clone(),
                device: device.clone(),
                session: session_id.clone(),
                trace: trace.clone(),
            }));
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
    let next = if o.device_id.is_empty() {
        state.target()
    } else {
        o.device_id.clone()
    };
    if next != state.agents.device_id {
        state.agent_catalog_limits.clear();
        state.agent_catalog_pending.clear();
    }
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
            .chain(state.agents_search_sessions.iter())
            .any(|s| s.id == o.session && s.device_id == o.device_id)
        {
            state.toast("session is no longer listed on this device", true);
            return;
        }
        state.selected = o.device_id;
    }
    open_agent(state, ctx, o.session);
}

pub(super) fn intent_agent_send(state: &mut State, ctx: &Ctx, mut s: app::AgentSend) {
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
    if state.chat.as_ref().is_some_and(|c| c.sending) {
        diagnostic.rejection("already_sending");
        return;
    }
    match super::send_recovery::release_for_send(state, ctx, &s.release_unconfirmed) {
        Ok(token) => s.release_unconfirmed = token,
        Err(error) => {
            diagnostic.rejection("stale_or_busy_release");
            state.toast(error.to_string(), true);
            return;
        }
    }
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
    // Retry/ignore releases only the explicitly dismissed host latch.
    // Local pending presentation is separate from provider delivery evidence.
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
    if load.latest || load.newer {
        chat.advance_history(load.latest);
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
    chat.history_load_all = load.all;
    let (device, session, before) = (
        chat.device.clone(),
        chat.session.id.clone(),
        first.id.clone(),
    );
    let Some(watch) = &chat.watch else { return };
    let token = watch.token.clone();
    let self_id = state.self_id.clone();
    let backend = ctx.backend.clone();
    let cache = ctx.cache.clone();
    let cache_key = chat.cache_key.clone();
    spawn(ctx, async move {
        let result = async {
            let client = backend.client_for(&device, &self_id).await?;
            let mut cursor = before;
            let mut chunks = Vec::new();
            loop {
                if token.is_cancelled() {
                    return Err(Error::Unavailable("history loading cancelled".into()));
                }
                let key = format!(
                    "history-{}",
                    crate::cache::Cache::key(&["history-page-v2", &cache_key, &cursor])
                );
                let cached = cache.history(key.clone()).await.unwrap_or_default();
                let cached_messages = cached.messages;
                let mut request = muster_proto::history::request(&session, &cached_messages);
                request.tail = crate::state::CHAT_HISTORY_PAGE as u32;
                request.before_id = cursor.clone();
                let fetch = |request| async {
                    match tokio::time::timeout(
                        Duration::from_secs(8),
                        unary(&client, Req::AgentMessages(request)),
                    )
                    .await
                    .map_err(|_| Error::Unavailable("history loading timed out".into()))??
                    {
                        Resp::AgentMessages(page) => Ok(page),
                        _ => Err(unexpected()),
                    }
                };
                let mut page = fetch(request).await?;
                if !cached_messages.is_empty() {
                    if page.reset
                        || page.has_newer
                        || page.messages.iter().any(|m| m.id == cursor)
                        || page.messages.first().map(|m| &m.id)
                            != cached_messages.last().map(|m| &m.id)
                    {
                        // An older peer can ignore the upper bound on a proof.
                        // A changed generation also falls back to a plain page.
                        page = fetch(v1::AgentMessagesRequest {
                            session: session.clone(),
                            tail: crate::state::CHAT_HISTORY_PAGE as u32,
                            before_id: cursor.clone(),
                            ..Default::default()
                        })
                        .await?;
                    } else {
                        let mut validated = cached_messages;
                        merge_chat_tail(&mut validated, page.messages);
                        if validated.len() > crate::state::CHAT_HISTORY_PAGE {
                            validated.drain(..validated.len() - crate::state::CHAT_HISTORY_PAGE);
                            page.has_more = true;
                        }
                        page.messages = validated;
                    }
                }
                if page.reset {
                    return Err(Error::Unavailable(
                        "History changed while loading; reopen the conversation to resynchronize"
                            .into(),
                    ));
                }
                cache.put(key, &page);
                let next = page.messages.first().map(|m| m.id.clone());
                chunks.push(page.messages);
                if !load.all || !page.has_more {
                    return Ok((chunks.into_iter().rev().flatten().collect(), page.has_more));
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

fn restore_catalog_cache(
    state: &mut State,
    device: &str,
    name: &str,
    mut page: v1::AgentSessionPage,
) {
    // Old cache entries stored only rows. Do not render UUIDs as group names
    // while waiting for the first complete native snapshot after upgrade.
    if page.sections.is_empty() && page.sessions.iter().any(|s| !s.section_id.is_empty()) {
        return;
    }
    for session in &mut page.sessions {
        session.device_id = device.into();
        session.device_name = name.into();
    }
    state.agents.sessions.extend(visible_agents(page.sessions));
    state
        .agents
        .native_sections
        .retain(|s| s.device_id != device);
    state
        .agents
        .native_sections
        .extend(page.sections.into_iter().map(|mut s| {
            s.device_id = device.into();
            s.device_name = name.into();
            s
        }));
    state.agent_catalog_groups.retain(|g| g.device_id != device);
    state
        .agent_catalog_groups
        .extend(page.groups.into_iter().map(|mut g| {
            g.device_id = device.into();
            g
        }));
}

pub(super) fn msg_agents(
    state: &mut State,
    ctx: &Ctx,
    epoch: u64,
    device: String,
    result: Result<v1::AgentSessionPage, Error>,
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
        Ok(page) => {
            ctx.cache
                .put(cache_key(state, "agents", &device, ""), &page);
            state.agent_catalog_groups.retain(|g| g.device_id != device);
            state
                .agent_catalog_groups
                .extend(page.groups.into_iter().map(|mut g| {
                    g.device_id = device.clone();
                    g
                }));
            let sessions = visible_agents(page.sessions);
            for fresh in &sessions {
                for result in &mut state.agents_search_sessions {
                    crate::catalog::reconcile_session(result, fresh, true);
                }
                if let Some(chat) = &mut state.chat {
                    crate::catalog::reconcile_session(&mut chat.session, fresh, true);
                    for child in chat
                        .session
                        .subagents
                        .iter_mut()
                        .chain(chat.session.subsessions.iter_mut())
                    {
                        crate::catalog::reconcile_session(child, fresh, true);
                    }
                }
            }
            state
                .agents
                .native_sections
                .retain(|s| s.device_id != device);
            state
                .agents
                .native_sections
                .extend(page.sections.into_iter().map(|mut s| {
                    s.device_id = device.clone();
                    if s.device_name.is_empty() {
                        s.device_name = sessions
                            .first()
                            .map(|s| s.device_name.clone())
                            .unwrap_or_else(|| device.clone());
                    }
                    s
                }));
            // Catalog notices from older peers describe optional native metadata.
            // Keep them silent; transport and mutation failures have separate paths.

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
    }
}

pub(super) fn msg_chat_cache(
    state: &mut State,
    token: CancellationToken,
    device: &str,
    session: &str,
    page: v1::AgentMessagePage,
) {
    if token.is_cancelled() {
        return;
    }
    if let Some(chat) = &mut state.chat
        && chat.device == device
        && chat.session.id == session
        && chat.messages.is_empty()
        && !chat
            .load_trace
            .as_ref()
            .is_some_and(|trace| trace.history_ready())
    {
        chat.messages = page.messages;
        chat.has_earlier = page.has_more;
        chat.reconcile_history_start();
        chat.content_revision = crate::state::next_content_revision();
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
    let failure = result
        .as_ref()
        .err()
        .map(|error| chat_failure(state, &device, error));
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
                let first = if chat.history_load_all {
                    0
                } else {
                    messages
                        .len()
                        .saturating_sub(crate::state::CHAT_HISTORY_STEP)
                };
                let mut merged = messages;
                merged.extend(
                    chat.messages
                        .drain(..)
                        .filter(|m| !messages_ids.contains(&m.id)),
                );
                chat.messages = merged;
                chat.history_start = chat.messages.get(first).map(|m| m.id.clone());
                chat.history_browsing = true;
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
                chat.history_browsing = false;
            }
            chat.timeline_index.borrow_mut().invalidate(0);
            chat.reconcile_history_start();
            if !earlier && let Some(trace) = &chat.load_trace {
                trace.history_applied();
                trace.record(
                    "history_applied",
                    serde_json::json!({"messages":chat.messages.len(),"reset":reset}),
                );
            }
        }
        Err(_) => {
            let failure = failure.unwrap_or_default();
            chat.load_error = if earlier {
                format!("{failure}. Retry loading earlier messages.")
            } else {
                format!("{failure}. Reconnecting…")
            };
        }
    }
    super::admitted::settle(state);
}

/// A chat failure as shown: a member the daemon reports offline is named
/// as such, and a forwarding daemon's error names the device, not its id.
pub(super) fn chat_failure(state: &State, device: &str, error: &Error) -> String {
    let name = state
        .network
        .as_ref()
        .and_then(|n| n.members.iter().find(|m| m.device_id == device))
        .map(|m| m.name.as_str())
        .filter(|name| !name.is_empty());
    let text = error.to_string();
    match name {
        Some(name) if state.offline(device) => format!("{name} is offline"),
        Some(name) if !device.is_empty() => text.replace(device, name),
        _ => text,
    }
}

/// The open chat's member came back: a watch waiting out its retry delay
/// reconnects now instead.
pub(super) fn rewatch_returned_member(state: &mut State, ctx: &Ctx, returned: &[String]) {
    let waiting = state
        .chat
        .as_ref()
        .is_some_and(|chat| !chat.load_error.is_empty() && returned.contains(&chat.device));
    if waiting
        && state.on_path(Screen::Chat)
        && let Some((device, session)) = state.chat_target()
    {
        watch_agent(state, ctx, device, session);
    }
}

pub(super) fn msg_agent_load_started(
    state: &mut State,
    token: CancellationToken,
    device: &str,
    session: &str,
    trace: crate::session_diagnostics::Trace,
) {
    if !token.is_cancelled()
        && let Some(chat) = &mut state.chat
        && chat.device == device
        && chat.session.id == session
    {
        chat.load_trace = Some(trace);
    }
}

pub(super) fn msg_agent_stream_started(
    state: &mut State,
    token: CancellationToken,
    device: &str,
    session: &str,
    provider: &str,
) {
    if token.is_cancelled() {
        return;
    }
    let Some(chat) = &mut state.chat else { return };
    if chat.device != device || chat.session.id != session || chat.session.provider != provider {
        return;
    }
    // This drops stale display handles; it does not assert command completion
    // or reset an outstanding/unknown response receipt.
    chat.interactions.retain(|i| i.kind == "queued_message");
    chat.approval_refresh_attempted = false;
    chat.approval_refresh_pending = false;
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
            if let Some(index) = chat.messages.iter().position(|x| x.id == m.id) {
                if chat.messages[index] != m {
                    chat.timeline_index.borrow_mut().invalidate(index);
                    chat.messages[index] = m;
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
        v1::agent_event::Ev::State(mut s) => {
            if s.id != session
                || (!chat.session.provider.is_empty() && s.provider != chat.session.provider)
            {
                return;
            }
            s.device_id = device.clone();
            s.device_name = chat.session.device_name.clone();
            for child in s.subagents.iter_mut().chain(s.subsessions.iter_mut()) {
                child.device_id = device.clone();
                child.device_name = s.device_name.clone();
            }
            for listed in state
                .agents
                .sessions
                .iter_mut()
                .chain(state.agents_search_sessions.iter_mut())
            {
                if listed.device_id == device
                    && listed.provider == s.provider
                    && listed.id == session
                {
                    crate::catalog::reconcile_session(listed, &s, false);
                }
            }
            super::approval_cards::episode_boundary(chat, &s.state);
            if s.state != "waiting_approval" && s.provider != "claude" {
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
                super::approval_cards::episode_boundary(chat, &s.state);
                chat.session = s;
            }
        }
    }
    if messages_changed {
        chat.cache_dirty = true;
    }
    super::approval_cards::schedule(state, ctx, token);
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

#[cfg(test)]
mod catalog_cache_regressions {
    use super::*;
    #[tokio::test]
    async fn catalog_read_notices_are_silent_but_transport_errors_remain_visible() {
        let (socket, _peer) = tokio::io::duplex(1024);
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let dir = tempfile::tempdir().unwrap();
        let ctx = Ctx {
            shell: Arc::new(crate::send_diagnostics::tests::Capture::default()),
            cache: Arc::new(crate::cache::Cache::new(dir.path().into())),
            backend: Arc::new(crate::backend::Backend::Socket {
                path: dir.path().into(),
                local: muster_rpc::Client::new(socket),
            }),
            tx,
        };
        let mut state = State::new(false, Some("host".into()));
        let epoch = state.agents_epoch;
        for _ in 0..3 {
            msg_agents(
                &mut state,
                &ctx,
                epoch,
                "host".into(),
                Ok(v1::AgentSessionPage {
                    notice: "Native Pin, Section and Project metadata unavailable".into(),
                    sessions: vec![v1::AgentSession {
                        id: "session".into(),
                        provider: "claude".into(),
                        section_id: "native-id".into(),
                        pinned: true,
                        ..Default::default()
                    }],
                    sections: vec![v1::AgentSection {
                        id: "native-id".into(),
                        title: "Example group".into(),
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
            );
            assert!(state.agents.errors.is_empty());
            assert!(state.agents_errors.is_empty());
            assert_eq!(state.agents.native_sections[0].title, "Example group");
            assert!(state.agents.sessions[0].pinned);
        }
        msg_agents(
            &mut state,
            &ctx,
            epoch,
            "host".into(),
            Err(Error::Unavailable("device disconnected".into())),
        );
        assert!(
            state
                .agents
                .errors
                .iter()
                .any(|e| e.contains("device disconnected"))
        );
    }

    #[test]
    fn restored_sessions_keep_native_names_and_page_metadata_in_same_frame() {
        let mut state = State::new(false, Some("host".into()));
        restore_catalog_cache(
            &mut state,
            "host",
            "Example Mac",
            v1::AgentSessionPage {
                sessions: vec![v1::AgentSession {
                    id: "session".into(),
                    section_id: "native-id".into(),
                    ..Default::default()
                }],
                sections: vec![v1::AgentSection {
                    id: "native-id".into(),
                    title: "Example group".into(),
                    ..Default::default()
                }],
                groups: vec![v1::AgentCatalogGroup {
                    key: "native-id".into(),
                    count: 21,
                    next_cursor: "next".into(),
                    ..Default::default()
                }],
                ..Default::default()
            },
        );
        assert_eq!(state.agents.sessions[0].device_id, "host");
        assert_eq!(state.agents.native_sections[0].title, "Example group");
        assert_eq!(state.agents.native_sections[0].device_id, "host");
        assert_eq!(state.agent_catalog_groups[0].count, 21);
        assert_eq!(state.agent_catalog_groups[0].device_id, "host");
    }
}
