//! Durable investigation and dismissal of an original send; no send dispatch here.
use super::*;
use crate::state::Watch;

fn key(state: &State, device: &str, session: &str) -> String {
    cache_key(state, "pending-send-v1", device, session)
}
fn load(ctx: &Ctx, key: &str) -> Option<app::PendingAgentSend> {
    ctx.cache
        .get::<app::PendingAgentSend>(key)
        .filter(|p| p.request.is_some())
}

fn dismissed_token(state: &State, ctx: &Ctx, device: &str, session: &str) -> String {
    ctx.cache
        .get::<app::PendingAgentSend>(&key(state, device, session))
        .map_or_else(String::new, |p| p.dismissed_token)
}

pub(super) fn is_dismissed(
    state: &State,
    ctx: &Ctx,
    device: &str,
    session: &str,
    token: &str,
) -> bool {
    !token.is_empty() && dismissed_token(state, ctx, device, session) == token
}

/// Hiding an unknown result is not an assertion of delivery. Persist together
/// with the request so the next normal send replaces both in one durable write.
fn dismiss(state: &mut State, ctx: &Ctx, token: &str) -> Result<(), Error> {
    let Some(chat) = &state.chat else {
        return Err(Error::Invalid("No conversation".into()));
    };
    if chat.sending || chat.unconfirmed.as_ref().map(|u| u.token.as_str()) != Some(token) {
        return Err(Error::Invalid("发送提示已更新，请使用当前提示".into()));
    }
    let notice_op = super::admitted::op_of(&chat.pending_draft_id).to_owned();
    let key = key(state, &chat.device, &chat.session.id);
    let mut pending = ctx
        .cache
        .get::<app::PendingAgentSend>(&key)
        .unwrap_or_default();
    pending.dismissed_token = token.to_owned();
    ctx.cache
        .put_durable(key, &pending)
        .map_err(|e| Error::Unavailable(format!("无法关闭发送提示：{e}")))?;
    state.chat.as_mut().expect("checked").unconfirmed = None;
    super::admitted::dismiss(state, token);
    if notice_op != token {
        super::admitted::dismiss(state, &notice_op);
    }
    Ok(())
}

/// Both the retry button and subsequent ordinary sends use the normal send
/// reducer. A dismissed token only authorizes that reducer's next explicit send.
pub(super) fn release_for_send(
    state: &mut State,
    ctx: &Ctx,
    explicit: &str,
) -> Result<String, Error> {
    let Some(chat) = &state.chat else {
        return Ok(String::new());
    };
    if chat.sending {
        return Err(Error::Busy("消息正在发送".into()));
    }
    let dismissed = dismissed_token(state, ctx, &chat.device, &chat.session.id);
    if explicit.is_empty() {
        return Ok(dismissed);
    }
    if chat.unconfirmed.as_ref().map(|u| u.token.as_str()) == Some(explicit) {
        dismiss(state, ctx, explicit)?;
    } else if dismissed != explicit {
        return Err(Error::Invalid("发送提示已更新，请使用当前提示".into()));
    }
    Ok(explicit.to_owned())
}

pub(super) fn prepare(
    state: &mut State,
    ctx: &Ctx,
    device: &str,
    request: &v1::AgentSendRequest,
    draft_id: &str,
) -> Result<(), Error> {
    let key = key(state, device, &request.session);
    if let Some(previous) = load(ctx, &key) {
        let original = previous.request.as_ref().expect("loaded");
        if request.release_unconfirmed != original.op_id {
            restore(state, ctx);
            return Err(Error::Busy(
                "上一条消息仍在确认中，请先检查结果；不会自动重发".into(),
            ));
        }
    }
    let pending = app::PendingAgentSend {
        request: Some(request.clone()),
        draft: state
            .chat
            .as_ref()
            .filter(|c| c.draft.text == request.text)
            .map(|c| c.draft.clone()),
        draft_id: draft_id.to_owned(),
        dismissed_token: String::new(),
    };
    ctx.cache
        .put_durable(key.clone(), &pending)
        .map_err(|e| Error::Unavailable(format!("无法保存待确认消息，尚未发送：{e}")))?;
    state.send_recoveries.remove(&key);
    Ok(())
}

pub(super) fn not_dispatched(
    state: &mut State,
    ctx: &Ctx,
    what: &str,
    device: &str,
    origin: &Option<(String, String)>,
) {
    if what != "send" {
        return;
    }
    outcome(
        state,
        ctx,
        device,
        origin,
        &Err(Error::Unavailable(
            "Authorization ended before dispatch".into(),
        )),
    );
    if let Some(chat) = state.chat.as_mut().filter(|c| {
        c.device == device
            && origin.as_ref().is_some_and(|(session, tag)| {
                &c.session.id == session && &c.pending_draft_id == tag
            })
    }) {
        chat.sending = false;
    }
}

/// A terminal response for this exact revision may settle the local record.
/// The record itself, absence, a timeout or a transport failure never proves delivery.
pub(super) fn outcome(
    state: &mut State,
    ctx: &Ctx,
    device: &str,
    origin: &Option<(String, String)>,
    result: &Result<v1::OpResult, Error>,
) {
    let Some((session, tag)) = origin else { return };
    let key = key(state, device, session);
    let Some(pending) = load(ctx, &key).filter(|p| &p.draft_id == tag) else {
        return;
    };
    let terminal = match result {
        Ok(r) => matches!(
            v1::op_result::Status::try_from(r.status),
            Ok(v1::op_result::Status::UserConfirmed
                | v1::op_result::Status::Applied
                | v1::op_result::Status::Duplicate
                | v1::op_result::Status::Rejected)
        ),
        // The operation driver returns Err only before dispatch or for an explicit
        // server rejection. Lost acknowledgements are converted to Unknown there.
        Err(_) => true,
    };
    if terminal {
        if let Err(e) = ctx
            .cache
            .put_durable(key.clone(), &app::PendingAgentSend::default())
        {
            state.toast(format!("发送结果已收到，本地恢复记录清理失败：{e}"), true);
        }
        state.send_recoveries.remove(&key);
    } else {
        start(state, ctx, device.to_owned(), key, pending);
    }
}

pub(super) fn restore(state: &mut State, ctx: &Ctx) {
    let Some(chat) = &state.chat else { return };
    let device = chat.device.clone();
    let key = key(state, &device, &chat.session.id);
    if let Some(pending) = load(ctx, &key) {
        // Submission used to keep only an in-memory draft. On restart preserve
        // the submitted revision if there is no later durable composer snapshot.
        if let Some(draft) = &pending.draft
            && ctx.cache.get::<app::ChatDraft>(&draft.key).is_none()
            && ctx.cache.put_durable(draft.key.clone(), draft).is_ok()
            && let Some(chat) = &mut state.chat
            && chat.draft.key == draft.key
            && chat.draft.text.is_empty()
            && chat.draft.revision.is_empty()
        {
            chat.draft = draft.clone();
        }
        let request = pending.request.as_ref().expect("loaded");
        let notice = if pending.dismissed_token == request.op_id {
            None
        } else {
            Some(app::UnconfirmedSend {
                token: request.op_id.clone(),
                message:
                    "消息结果待确认，正在只读查询；不会自动重发。若长期未确认，请先检查桌面会话。"
                        .into(),
            })
        };
        if let Some(chat) = &mut state.chat {
            chat.pending_draft_id = pending.draft_id.clone();
            chat.unconfirmed = notice;
        }
        if let Some(draft) = &pending.draft {
            state
                .pending_drafts
                .insert(pending.draft_id.clone(), draft.clone());
        }
        start(state, ctx, device, key, pending);
    }
}

fn start(
    state: &mut State,
    ctx: &Ctx,
    device: String,
    key: String,
    pending: app::PendingAgentSend,
) {
    if !state.foreground || state.send_recoveries.contains_key(&key) {
        return;
    }
    let cancel = CancellationToken::new();
    state.send_recoveries.insert(
        key.clone(),
        Watch {
            token: cancel.clone(),
        },
    );
    let backend = ctx.backend.clone();
    let route = state.self_id.clone();
    let tx = ctx.tx.clone();
    tokio::spawn(async move {
        let Some(request) = pending.request.as_ref() else {
            return;
        };
        let mut attempts = 0;
        loop {
            let query = async {
                let client = backend.client_for(&device, &route).await?;
                let Resp::DeviceInfo(info) =
                    unary(&client, Req::DeviceInfo(v1::DeviceInfoRequest {})).await?
                else {
                    return Err(unexpected());
                };
                if !info.services.iter().any(|s| s == "agent.send.status.v1") {
                    return Err(Error::Invalid("目标 Mac 需更新后才能查询发送结果".into()));
                }
                let Resp::OpResult(result) =
                    unary(&client, Req::AgentSendStatus(request.clone())).await?
                else {
                    return Err(unexpected());
                };
                Ok(result)
            };
            let result = tokio::select! {
                _ = cancel.cancelled() => return,
                result = tokio::time::timeout(Duration::from_secs(8), query) => result.unwrap_or_else(|_| Err(Error::Unavailable("发送结果查询超时".into()))),
            };
            let terminal = matches!(&result, Ok(r) if matches!(v1::op_result::Status::try_from(r.status), Ok(v1::op_result::Status::UserConfirmed | v1::op_result::Status::Applied | v1::op_result::Status::Rejected)));
            let stopped = matches!(&result, Err(Error::Unauthorized(_) | Error::Invalid(_)));
            if tx
                .send(loop_message(Msg::SendRecovered {
                    device: device.clone(),
                    key: key.clone(),
                    pending: pending.clone(),
                    result,
                }))
                .is_err()
            {
                return;
            }
            if terminal || stopped {
                return;
            }
            attempts += 1;
            tokio::select! {
                _ = cancel.cancelled() => return,
                _ = tokio::time::sleep(Duration::from_secs(if attempts < 6 { 2 } else { 15 })) => {}
            }
        }
    });
}

/// An explicit action on the original pending request; never an AgentSend call.
pub(super) fn resolve(state: &mut State, ctx: &Ctx, action: app::ResolveAgentSend) {
    let Some(chat) = &state.chat else { return };
    if chat.sending || chat.unconfirmed.as_ref().map(|u| &u.token) != Some(&action.token) {
        return;
    }
    if action.dismiss {
        if let Err(error) = dismiss(state, ctx, &action.token) {
            state.toast(error.to_string(), true);
        }
        return;
    }
    let device = chat.device.clone();
    let key = key(state, &device, &chat.session.id);
    let Some(pending) =
        load(ctx, &key).filter(|p| p.request.as_ref().is_some_and(|r| r.op_id == action.token))
    else {
        state.toast(
            "本客户端没有原始发送记录，无法安全确认；请使用发送消息的设备",
            true,
        );
        return;
    };
    state.send_recoveries.remove(&key);
    if !action.delivered {
        start(state, ctx, device, key, pending);
        return;
    }
    let backend = ctx.backend.clone();
    let route = state.self_id.clone();
    spawn(ctx, async move {
        let query = async {
            let client = backend.client_for(&device, &route).await?;
            let Resp::DeviceInfo(info) =
                unary(&client, Req::DeviceInfo(v1::DeviceInfoRequest {})).await?
            else {
                return Err(unexpected());
            };
            if !info.services.iter().any(|s| s == "agent.send.confirm.v1") {
                return Err(Error::Invalid(
                    "目标 Mac 需更新后才能手动确认；消息未重发".into(),
                ));
            }
            let Resp::OpResult(result) = unary(
                &client,
                Req::AgentConfirmDelivered(pending.request.clone().expect("loaded")),
            )
            .await?
            else {
                return Err(unexpected());
            };
            Ok(result)
        };
        let result = tokio::time::timeout(Duration::from_secs(8), query)
            .await
            .unwrap_or_else(|_| {
                Err(Error::Unavailable(
                    "确认结果未知，请重新核对；不会重发".into(),
                ))
            });
        Msg::SendRecovered {
            device,
            key,
            pending,
            result,
        }
    });
}

pub(super) fn recovered(
    state: &mut State,
    ctx: &Ctx,
    device: String,
    key: String,
    pending: app::PendingAgentSend,
    result: Result<v1::OpResult, Error>,
) {
    // A former request cannot settle a newer operation, even with the same draft text.
    let Some(current) = load(ctx, &key) else {
        return;
    };
    if current.request != pending.request
        || current.draft != pending.draft
        || current.draft_id != pending.draft_id
    {
        return;
    }
    let pending = current;
    let Some(request) = &pending.request else {
        return;
    };
    if key != self::key(state, &device, &request.session) {
        return;
    }
    match result {
        Ok(r)
            if matches!(
                v1::op_result::Status::try_from(r.status),
                Ok(v1::op_result::Status::UserConfirmed
                    | v1::op_result::Status::Applied
                    | v1::op_result::Status::Rejected)
            ) =>
        {
            if r.status == v1::op_result::Status::UserConfirmed as i32 {
                super::admitted::user_confirmed(state, &device, &request.session, &request.op_id);
                state.toast("已记录你的送达确认，没有重发消息", false);
            }
            if let Some(draft) = &pending.draft {
                state
                    .pending_drafts
                    .insert(pending.draft_id.clone(), draft.clone());
            }
            operations::msg_op_done(
                state,
                ctx,
                "send",
                device,
                None,
                Some((request.session.clone(), pending.draft_id)),
                Ok(r),
            );
        }
        result => {
            let dismissed = is_dismissed(state, ctx, &device, &request.session, &request.op_id);
            if !dismissed
                && let Some(chat) = state
                    .chat
                    .as_mut()
                    .filter(|c| c.device == device && c.session.id == request.session)
            {
                chat.unconfirmed = Some(app::UnconfirmedSend {
                    token: request.op_id.clone(),
                    message: match &result {
                        Err(e) => format!("消息结果待确认：{e}。不会自动重发。"),
                        _ => "消息结果待确认，正在只读查询；不会自动重发。".into(),
                    },
                });
            }
            if matches!(result, Err(Error::Unauthorized(_) | Error::Invalid(_))) {
                state.send_recoveries.remove(&key);
            } else {
                // A manual confirmation can lose its response. Resume read-only
                // investigation of the same request, never replay the mutation.
                start(state, ctx, device, key, pending);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Shell;
    impl crate::Shell for Shell {
        fn on_view(&self, _: &[u8]) {}
        fn on_effect(&self, _: u64, _: &[u8]) {}
        fn hw_public_key(&self) -> Option<Vec<u8>> {
            None
        }
        fn hw_sign(&self, _: &[u8]) -> Option<Vec<u8>> {
            None
        }
    }
    async fn fixture(path: &std::path::Path) -> (State, Ctx) {
        let shell = Arc::new(Shell);
        let config = crate::AppConfig {
            dir: path.join("backend"),
            mode: crate::Mode::Embedded {
                relay: None,
                name: "recovery".into(),
                kind: "phone".into(),
                bind: "127.0.0.1:0".parse().unwrap(),
                version: String::new(),
            },
            allow_loopback: true,
        };
        let backend = Arc::new(
            crate::backend::Backend::connect(&config, shell.clone())
                .await
                .unwrap(),
        );
        let (tx, _) = mpsc::unbounded_channel();
        let ctx = Ctx {
            shell,
            backend,
            tx,
            cache: Arc::new(crate::cache::Cache::new(path.join("cache"))),
        };
        let mut state = State::new(true, Some("phone".into()));
        state.foreground = false; // Exercise reducer/persistence without dialing fixtures.
        state.chat = Some(Chat {
            device: "mac".into(),
            session: v1::AgentSession {
                id: "session".into(),
                ..Default::default()
            },
            draft: app::ChatDraft {
                key: "draft".into(),
                revision: "1".into(),
                text: "original".into(),
            },
            ..Default::default()
        });
        (state, ctx)
    }
    fn request(op: &str) -> v1::AgentSendRequest {
        v1::AgentSendRequest {
            op_id: op.into(),
            session: "session".into(),
            text: "original".into(),
            ..Default::default()
        }
    }
    #[tokio::test]
    async fn ignore_survives_restart_and_unknown_callbacks_without_asserting_delivery() {
        let dir = tempfile::tempdir().unwrap();
        let (mut state, mut ctx) = fixture(dir.path()).await;
        super::super::admitted::admit(&mut state, "first", "original");
        prepare(&mut state, &ctx, "mac", &request("first"), "first:ui").unwrap();
        restore(&mut state, &ctx);
        let journal = key(&state, "mac", "session");
        let original = load(&ctx, &journal).unwrap();
        let newer = app::ChatDraft {
            key: "draft".into(),
            revision: "2".into(),
            text: "new edit".into(),
        };
        ctx.cache.put_durable(newer.key.clone(), &newer).unwrap();
        state.chat.as_mut().unwrap().draft = newer.clone();
        resolve(
            &mut state,
            &ctx,
            app::ResolveAgentSend {
                token: "first".into(),
                dismiss: true,
                ..Default::default()
            },
        );
        assert!(state.chat.as_ref().unwrap().unconfirmed.is_none());
        assert!(super::super::admitted::open(&state).is_empty());
        assert_eq!(load(&ctx, &journal).unwrap().request, original.request);
        assert_eq!(state.chat.as_ref().unwrap().draft, newer);
        ctx.cache = Arc::new(crate::cache::Cache::new(dir.path().join("cache")));
        restore(&mut state, &ctx);
        assert!(state.chat.as_ref().unwrap().unconfirmed.is_none());
        for result in [
            Ok(v1::OpResult {
                status: v1::op_result::Status::Unknown as i32,
                ..Default::default()
            }),
            Err(Error::Unavailable("offline".into())),
        ] {
            recovered(
                &mut state,
                &ctx,
                "mac".into(),
                journal.clone(),
                original.clone(),
                result,
            );
            assert!(state.chat.as_ref().unwrap().unconfirmed.is_none());
            assert!(load(&ctx, &journal).is_some());
        }
        // A real receipt still settles the original request, preserving newer input.
        recovered(
            &mut state,
            &ctx,
            "mac".into(),
            journal.clone(),
            original,
            Ok(v1::OpResult {
                result_id: "session".into(),
                ..Default::default()
            }),
        );
        assert!(load(&ctx, &journal).is_none());
        assert_eq!(state.chat.as_ref().unwrap().draft, newer);
        assert_eq!(release_for_send(&mut state, &ctx, "").unwrap(), "");
    }

    #[tokio::test]
    async fn dismissed_notice_stays_hidden_when_terminal_callback_clears_journal() {
        let dir = tempfile::tempdir().unwrap();
        let (mut state, ctx) = fixture(dir.path()).await;
        prepare(&mut state, &ctx, "mac", &request("first"), "first:ui").unwrap();
        restore(&mut state, &ctx);
        dismiss(&mut state, &ctx, "first").unwrap();
        super::super::operations::msg_op_done(
            &mut state,
            &ctx,
            "send",
            "mac".into(),
            None,
            Some(("session".into(), "first:ui".into())),
            Ok(v1::OpResult {
                status: v1::op_result::Status::Rejected as i32,
                unconfirmed: "first".into(),
                ..Default::default()
            }),
        );
        assert!(load(&ctx, &key(&state, "mac", "session")).is_none());
        assert!(state.chat.as_ref().unwrap().unconfirmed.is_none());
    }

    #[tokio::test]
    async fn retry_closes_notice_before_validation_and_does_not_accept_stale_tokens() {
        let dir = tempfile::tempdir().unwrap();
        let (mut state, ctx) = fixture(dir.path()).await;
        prepare(&mut state, &ctx, "mac", &request("first"), "first:ui").unwrap();
        restore(&mut state, &ctx);
        assert!(release_for_send(&mut state, &ctx, "stale").is_err());
        assert!(state.chat.as_ref().unwrap().unconfirmed.is_some());
        super::super::agents::intent_agent_send(
            &mut state,
            &ctx,
            app::AgentSend {
                text: String::new(),
                release_unconfirmed: "first".into(),
                ..Default::default()
            },
        );
        assert!(state.chat.as_ref().unwrap().unconfirmed.is_none());
        assert!(!state.chat.as_ref().unwrap().sending);
        assert_eq!(
            load(&ctx, &key(&state, "mac", "session")).unwrap().request,
            Some(request("first"))
        );
        assert_eq!(release_for_send(&mut state, &ctx, "").unwrap(), "first");
        state.chat.as_mut().unwrap().session.id = "another-session".into();
        assert_eq!(release_for_send(&mut state, &ctx, "").unwrap(), "");
        assert!(release_for_send(&mut state, &ctx, "first").is_err());
    }

    #[tokio::test]
    async fn ignore_host_only_refusal_preserves_draft_and_releases_only_on_explicit_send() {
        let dir = tempfile::tempdir().unwrap();
        let (mut state, mut ctx) = fixture(dir.path()).await;
        super::super::admitted::admit(&mut state, "blocked-attempt", "original");
        state.chat.as_mut().unwrap().pending_draft_id = "blocked-attempt:ui".into();
        state.chat.as_mut().unwrap().unconfirmed = Some(app::UnconfirmedSend {
            token: "host-only".into(),
            message: "unknown".into(),
        });
        resolve(
            &mut state,
            &ctx,
            app::ResolveAgentSend {
                token: "host-only".into(),
                dismiss: true,
                ..Default::default()
            },
        );
        assert!(state.chat.as_ref().unwrap().unconfirmed.is_none());
        assert!(super::super::admitted::open(&state).is_empty());
        assert_eq!(state.chat.as_ref().unwrap().draft.text, "original");
        ctx.cache = Arc::new(crate::cache::Cache::new(dir.path().join("cache")));
        restore(&mut state, &ctx);
        assert!(load(&ctx, &key(&state, "mac", "session")).is_none());
        assert_eq!(release_for_send(&mut state, &ctx, "").unwrap(), "host-only");
        let mut next = request("next");
        next.release_unconfirmed = "host-only".into();
        prepare(&mut state, &ctx, "mac", &next, "next:ui").unwrap();
        assert_eq!(release_for_send(&mut state, &ctx, "").unwrap(), "");
        restore(&mut state, &ctx);
        assert_eq!(
            state
                .chat
                .as_ref()
                .unwrap()
                .unconfirmed
                .as_ref()
                .unwrap()
                .token,
            "next"
        );
    }

    #[tokio::test]
    async fn retry_and_send_after_ignore_dispatch_normal_send_with_current_input_once() {
        struct Server {
            sends: std::sync::Mutex<Vec<v1::AgentSendRequest>>,
        }
        impl muster_rpc::Handler for Server {
            fn call(
                &self,
                _: muster_rpc::Context,
                req: Req,
                _: muster_rpc::EventSink,
            ) -> muster_rpc::HandlerFuture<'_> {
                Box::pin(async move {
                    match req {
                        Req::DeviceInfo(_) => Resp::DeviceInfo(v1::DeviceInfo {
                            services: vec!["agent.attachments".into()],
                            ..Default::default()
                        }),
                        Req::AgentSend(r) => {
                            self.sends.lock().unwrap().push(r);
                            Resp::OpResult(v1::OpResult {
                                result_id: "session".into(),
                                ..Default::default()
                            })
                        }
                        _ => panic!("unexpected RPC: {req:?}"),
                    }
                })
            }
        }
        for ignore in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let socket = dir.path().join("rpc.sock");
            let listener = tokio::net::UnixListener::bind(&socket).unwrap();
            let server = Arc::new(Server {
                sends: Default::default(),
            });
            let handler = server.clone();
            let accept = tokio::spawn(async move {
                while let Ok((stream, _)) = listener.accept().await {
                    tokio::spawn(muster_rpc::serve(
                        stream,
                        muster_rpc::Principal::Local,
                        handler.clone(),
                    ));
                }
            });
            let shell = Arc::new(Shell);
            let backend = Arc::new(
                crate::backend::Backend::connect(
                    &crate::AppConfig {
                        dir: dir.path().into(),
                        mode: crate::Mode::Socket { path: socket },
                        allow_loopback: true,
                    },
                    shell.clone(),
                )
                .await
                .unwrap(),
            );
            let (tx, mut rx) = mpsc::unbounded_channel();
            let ctx = Ctx {
                shell,
                backend,
                tx,
                cache: Arc::new(crate::cache::Cache::new(dir.path().join("cache"))),
            };
            let mut state = State::new(false, Some("mac".into()));
            state.foreground = false;
            state.chat = Some(Chat {
                device: "mac".into(),
                session: v1::AgentSession {
                    id: "session".into(),
                    ..Default::default()
                },
                ..Default::default()
            });
            prepare(&mut state, &ctx, "mac", &request("first"), "first:old-ui").unwrap();
            restore(&mut state, &ctx);
            if ignore {
                resolve(
                    &mut state,
                    &ctx,
                    app::ResolveAgentSend {
                        token: "first".into(),
                        dismiss: true,
                        ..Default::default()
                    },
                );
                assert!(server.sends.lock().unwrap().is_empty());
            }
            let attachment = v1::AgentAttachment {
                name: "sample.txt".into(),
                media_type: "text/plain".into(),
                data: b"sample".to_vec(),
            };
            super::super::agents::intent_agent_send(
                &mut state,
                &ctx,
                app::AgentSend {
                    text: "current composer".into(),
                    attachments: vec![attachment.clone()],
                    draft_id: "new-ui".into(),
                    release_unconfirmed: if ignore {
                        String::new()
                    } else {
                        "first".into()
                    },
                },
            );
            assert!(state.chat.as_ref().unwrap().unconfirmed.is_none());
            assert!(state.chat.as_ref().unwrap().sending);
            let pending = load(&ctx, &key(&state, "mac", "session")).unwrap();
            assert!(pending.dismissed_token.is_empty());
            assert!(pending.draft_id.ends_with(":new-ui"));
            // The ordinary busy guard prevents double clicking from dispatching twice.
            super::super::agents::intent_agent_send(
                &mut state,
                &ctx,
                app::AgentSend {
                    text: "duplicate click".into(),
                    ..Default::default()
                },
            );
            let Loop::Msg(message) = tokio::time::timeout(Duration::from_secs(5), rx.recv())
                .await
                .unwrap()
                .unwrap()
            else {
                panic!()
            };
            assert!(matches!(*message, Msg::OpDone { result: Ok(_), .. }));
            let sends = server.sends.lock().unwrap();
            assert_eq!(sends.len(), 1);
            assert_ne!(sends[0].op_id, "first");
            assert_eq!(sends[0].session, "session");
            assert_eq!(sends[0].text, "current composer");
            assert_eq!(sends[0].attachments, vec![attachment]);
            assert_eq!(sends[0].release_unconfirmed, "first");
            accept.abort();
        }
    }

    #[tokio::test]
    async fn manual_confirmation_settles_original_and_preserves_newer_composer() {
        let dir = tempfile::tempdir().unwrap();
        let (mut state, ctx) = fixture(dir.path()).await;
        super::super::admitted::admit(&mut state, "first", "original");
        prepare(&mut state, &ctx, "mac", &request("first"), "first:ui").unwrap();
        restore(&mut state, &ctx);
        let journal = key(&state, "mac", "session");
        let pending = load(&ctx, &journal).unwrap();
        let newer = app::ChatDraft {
            key: "draft".into(),
            revision: "2".into(),
            text: "next message".into(),
        };
        ctx.cache.put_durable(newer.key.clone(), &newer).unwrap();
        state.chat.as_mut().unwrap().draft = newer.clone();
        recovered(
            &mut state,
            &ctx,
            "mac".into(),
            journal.clone(),
            pending,
            Ok(v1::OpResult {
                status: v1::op_result::Status::UserConfirmed as i32,
                ..Default::default()
            }),
        );
        assert!(load(&ctx, &journal).is_none());
        assert_eq!(state.chat.as_ref().unwrap().draft, newer);
        assert!(state.chat.as_ref().unwrap().unconfirmed.is_none());
        assert!(state.admitted[&super::super::admitted::key("mac", "session")][0].user_confirmed);
    }

    #[tokio::test]
    async fn restored_worker_recovers_late_receipt_over_rpc_without_submitting() {
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        struct Server {
            confirmed: AtomicBool,
            queries: AtomicUsize,
        }
        impl muster_rpc::Handler for Server {
            fn call(
                &self,
                _: muster_rpc::Context,
                req: Req,
                _: muster_rpc::EventSink,
            ) -> muster_rpc::HandlerFuture<'_> {
                Box::pin(async move {
                    match req {
                        Req::DeviceInfo(_) => Resp::DeviceInfo(v1::DeviceInfo {
                            services: vec!["agent.send.status.v1".into()],
                            ..Default::default()
                        }),
                        Req::AgentSendStatus(r) => {
                            assert_eq!(r, request("original"));
                            self.queries.fetch_add(1, Ordering::SeqCst);
                            Resp::OpResult(v1::OpResult {
                                status: if self.confirmed.load(Ordering::SeqCst) {
                                    v1::op_result::Status::Applied
                                } else {
                                    v1::op_result::Status::Unknown
                                } as i32,
                                result_id: "session".into(),
                                ..Default::default()
                            })
                        }
                        _ => panic!("recovery attempted something other than a read"),
                    }
                })
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("rpc.sock");
        let listener = tokio::net::UnixListener::bind(&socket).unwrap();
        let server = Arc::new(Server {
            confirmed: false.into(),
            queries: 0.into(),
        });
        let handler = server.clone();
        let accept = tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                tokio::spawn(muster_rpc::serve(
                    stream,
                    muster_rpc::Principal::Local,
                    handler.clone(),
                ));
            }
        });
        let shell = Arc::new(Shell);
        let backend = Arc::new(
            crate::backend::Backend::connect(
                &crate::AppConfig {
                    dir: dir.path().into(),
                    mode: crate::Mode::Socket { path: socket },
                    allow_loopback: true,
                },
                shell.clone(),
            )
            .await
            .unwrap(),
        );
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut ctx = Ctx {
            shell,
            backend,
            tx,
            cache: Arc::new(crate::cache::Cache::new(dir.path().join("cache"))),
        };
        let mut state = State::new(false, Some("mac".into()));
        state.chat = Some(Chat {
            device: "mac".into(),
            session: v1::AgentSession {
                id: "session".into(),
                ..Default::default()
            },
            ..Default::default()
        });
        prepare(&mut state, &ctx, "mac", &request("original"), "original:ui").unwrap();
        // Recreate the cache as after a process exit. Only the durable request remains.
        ctx.cache = Arc::new(crate::cache::Cache::new(dir.path().join("cache")));
        restore(&mut state, &ctx);
        for expected in [
            v1::op_result::Status::Unknown,
            v1::op_result::Status::Applied,
        ] {
            let Loop::Msg(message) = tokio::time::timeout(Duration::from_secs(5), rx.recv())
                .await
                .unwrap()
                .unwrap()
            else {
                panic!()
            };
            let Msg::SendRecovered {
                device,
                key,
                pending,
                result,
            } = *message
            else {
                panic!()
            };
            assert_eq!(result.as_ref().unwrap().status, expected as i32);
            recovered(&mut state, &ctx, device, key, pending, result);
            server.confirmed.store(true, Ordering::SeqCst);
        }
        assert_eq!(server.queries.load(Ordering::SeqCst), 2);
        assert!(load(&ctx, &key(&state, "mac", "session")).is_none());
        assert!(state.send_recoveries.is_empty());
        accept.abort();
    }

    #[tokio::test]
    async fn manual_action_recovers_uncertain_ack_without_a_send_rpc() {
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        struct Server {
            confirmed: AtomicBool,
            queries: AtomicUsize,
        }
        impl muster_rpc::Handler for Server {
            fn call(
                &self,
                _: muster_rpc::Context,
                req: Req,
                _: muster_rpc::EventSink,
            ) -> muster_rpc::HandlerFuture<'_> {
                Box::pin(async move {
                    match req {
                        Req::DeviceInfo(_) => Resp::DeviceInfo(v1::DeviceInfo {
                            services: vec![
                                "agent.send.status.v1".into(),
                                "agent.send.confirm.v1".into(),
                            ],
                            ..Default::default()
                        }),
                        Req::AgentConfirmDelivered(r) => {
                            assert_eq!(r, request("original"));
                            assert!(
                                !self.confirmed.swap(true, Ordering::SeqCst),
                                "manual mutation must not be replayed"
                            );
                            Resp::Error(v1::Error {
                                code: v1::error::Code::Unavailable as i32,
                                message: "acknowledgement unavailable after persistence".into(),
                            })
                        }
                        Req::AgentSendStatus(r) => {
                            assert_eq!(r, request("original"));
                            self.queries.fetch_add(1, Ordering::SeqCst);
                            Resp::OpResult(v1::OpResult {
                                status: if self.confirmed.load(Ordering::SeqCst) {
                                    v1::op_result::Status::UserConfirmed
                                } else {
                                    v1::op_result::Status::Unknown
                                } as i32,
                                result_id: "session".into(),
                                ..Default::default()
                            })
                        }
                        _ => panic!("recovery attempted something other than a read"),
                    }
                })
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("rpc.sock");
        let listener = tokio::net::UnixListener::bind(&socket).unwrap();
        let server = Arc::new(Server {
            confirmed: false.into(),
            queries: 0.into(),
        });
        let handler = server.clone();
        let accept = tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                tokio::spawn(muster_rpc::serve(
                    stream,
                    muster_rpc::Principal::Local,
                    handler.clone(),
                ));
            }
        });
        let shell = Arc::new(Shell);
        let backend = Arc::new(
            crate::backend::Backend::connect(
                &crate::AppConfig {
                    dir: dir.path().into(),
                    mode: crate::Mode::Socket { path: socket },
                    allow_loopback: true,
                },
                shell.clone(),
            )
            .await
            .unwrap(),
        );
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut ctx = Ctx {
            shell,
            backend,
            tx,
            cache: Arc::new(crate::cache::Cache::new(dir.path().join("cache"))),
        };
        let mut state = State::new(false, Some("mac".into()));
        state.chat = Some(Chat {
            device: "mac".into(),
            session: v1::AgentSession {
                id: "session".into(),
                ..Default::default()
            },
            ..Default::default()
        });
        prepare(&mut state, &ctx, "mac", &request("original"), "original:ui").unwrap();
        // Recreate the cache as after a process exit. Only the durable request remains.
        ctx.cache = Arc::new(crate::cache::Cache::new(dir.path().join("cache")));
        restore(&mut state, &ctx);
        for expected in [
            Some(v1::op_result::Status::Unknown),
            None,
            Some(v1::op_result::Status::UserConfirmed),
        ] {
            let Loop::Msg(message) = tokio::time::timeout(Duration::from_secs(5), rx.recv())
                .await
                .unwrap()
                .unwrap()
            else {
                panic!()
            };
            let Msg::SendRecovered {
                device,
                key,
                pending,
                result,
            } = *message
            else {
                panic!()
            };
            match expected {
                Some(expected) => assert_eq!(result.as_ref().unwrap().status, expected as i32),
                None => assert!(result.is_err()),
            }
            recovered(&mut state, &ctx, device, key, pending, result);
            if expected == Some(v1::op_result::Status::Unknown) {
                resolve(
                    &mut state,
                    &ctx,
                    app::ResolveAgentSend {
                        token: "original".into(),
                        delivered: true,
                        ..Default::default()
                    },
                );
            }
        }
        assert_eq!(server.queries.load(Ordering::SeqCst), 2);
        assert!(server.confirmed.load(Ordering::SeqCst));
        assert!(load(&ctx, &key(&state, "mac", "session")).is_none());
        assert!(state.send_recoveries.is_empty());
        accept.abort();
    }

    #[tokio::test]
    async fn reopening_queries_original_without_replay_and_preserves_newer_draft_and_lease() {
        let dir = tempfile::tempdir().unwrap();
        let (mut state, mut ctx) = fixture(dir.path()).await;
        prepare(&mut state, &ctx, "mac", &request("first"), "first:ui-1").unwrap();
        let journal = key(&state, "mac", "session");
        let original = load(&ctx, &journal).unwrap();
        // Simulated process restart loses all in-memory state and loads only disk.
        ctx.cache = Arc::new(crate::cache::Cache::new(dir.path().join("cache")));
        state.pending_drafts.clear();
        state.chat.as_mut().unwrap().draft = app::ChatDraft {
            key: "draft".into(),
            ..Default::default()
        };
        restore(&mut state, &ctx);
        assert_eq!(state.chat.as_ref().unwrap().draft.text, "original");
        assert_eq!(
            state
                .chat
                .as_ref()
                .unwrap()
                .unconfirmed
                .as_ref()
                .unwrap()
                .token,
            "first"
        );
        assert!(prepare(&mut state, &ctx, "mac", &request("second"), "second:ui-1").is_err());
        let newer = app::ChatDraft {
            key: "draft".into(),
            revision: "2".into(),
            text: "new edit".into(),
        };
        ctx.cache.put_durable(newer.key.clone(), &newer).unwrap();
        state.chat.as_mut().unwrap().draft = newer.clone();
        state.locked = Some(("mac".into(), vec![2; 32]));
        recovered(
            &mut state,
            &ctx,
            "mac".into(),
            journal.clone(),
            original.clone(),
            Ok(v1::OpResult {
                status: v1::op_result::Status::Unknown as i32,
                ..Default::default()
            }),
        );
        assert_eq!(load(&ctx, &journal), Some(original.clone()));
        recovered(
            &mut state,
            &ctx,
            "mac".into(),
            journal.clone(),
            original,
            Ok(v1::OpResult {
                result_id: "session".into(),
                ..Default::default()
            }),
        );
        assert!(load(&ctx, &journal).is_none());
        assert_eq!(state.chat.as_ref().unwrap().draft, newer);
        assert_eq!(state.locked, Some(("mac".into(), vec![2; 32])));
        assert_eq!(state.chat.as_ref().unwrap().sent_draft_id, "ui-1");
    }
    #[tokio::test]
    async fn old_result_cannot_clear_a_new_operation_with_same_ui_revision() {
        let dir = tempfile::tempdir().unwrap();
        let (mut state, ctx) = fixture(dir.path()).await;
        prepare(&mut state, &ctx, "mac", &request("first"), "first:ui").unwrap();
        let journal = key(&state, "mac", "session");
        let old = load(&ctx, &journal).unwrap();
        let mut next = request("next");
        next.release_unconfirmed = "first".into();
        prepare(&mut state, &ctx, "mac", &next, "next:ui").unwrap();
        recovered(
            &mut state,
            &ctx,
            "mac".into(),
            journal.clone(),
            old,
            Ok(v1::OpResult::default()),
        );
        outcome(
            &mut state,
            &ctx,
            "mac",
            &Some(("session".into(), "first:ui".into())),
            &Ok(v1::OpResult::default()),
        );
        assert_eq!(load(&ctx, &journal).unwrap().request, Some(next));
    }
    #[tokio::test]
    async fn transport_error_retains_record_and_rejected_result_preserves_draft() {
        let dir = tempfile::tempdir().unwrap();
        let (mut state, ctx) = fixture(dir.path()).await;
        prepare(&mut state, &ctx, "mac", &request("first"), "first:ui").unwrap();
        let journal = key(&state, "mac", "session");
        let pending = load(&ctx, &journal).unwrap();
        restore(&mut state, &ctx);
        recovered(
            &mut state,
            &ctx,
            "mac".into(),
            journal.clone(),
            pending.clone(),
            Err(Error::Unavailable("disconnected".into())),
        );
        assert_eq!(load(&ctx, &journal), Some(pending.clone()));
        recovered(
            &mut state,
            &ctx,
            "mac".into(),
            journal.clone(),
            pending,
            Ok(v1::OpResult {
                status: v1::op_result::Status::Rejected as i32,
                message: "not sent".into(),
                ..Default::default()
            }),
        );
        assert!(load(&ctx, &journal).is_none());
        assert_eq!(state.chat.as_ref().unwrap().draft.text, "original");
    }
}
