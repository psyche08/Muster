//! Remote operations that need a presence proof or Locked Use.

use super::*;

/// Desktop loading needs the same signed Locked Use continuation as send,
/// but returns a typed catalog/session/options response after confirmed relock.
pub(super) async fn protected_chat_call(
    tx: tokio::sync::mpsc::UnboundedSender<Loop>,
    epoch: u64,
    device: String,
    provider: String,
    request: Req,
) -> Result<Resp, Error> {
    let (reply, result) = tokio::sync::oneshot::channel();
    tx.send(loop_message(Msg::ChatControl {
        epoch,
        device,
        provider,
        request,
        reply,
    }))
    .map_err(|_| Error::Unavailable("Chat loading stopped".into()))?;
    tokio::time::timeout(Duration::from_secs(120), result)
        .await
        .map_err(|_| Error::Unavailable("Chat loading timed out".into()))?
        .map_err(|_| Error::Unavailable("Chat loading cancelled".into()))?
}

fn fail_chat_call(state: &mut State, origin: &Option<(String, String)>, why: &str) {
    if let Some((_, id)) = origin
        && let Some(reply) = state.chat_calls.remove(id)
    {
        let _ = reply.send(Err(Error::Unavailable(why.into())));
    }
}

/// An op_id mutation on `device`; the result comes back as OpDone.
pub(super) fn op_request(
    state: &mut State,
    ctx: &Ctx,
    what: &'static str,
    device: String,
    req: Req,
) {
    let origin = state
        .chat
        .as_ref()
        .filter(|c| c.device == device)
        .and_then(|c| match what {
            "respond" => Some((c.session.id.clone(), c.responding_interaction.clone())),
            "send" => Some((c.session.id.clone(), c.pending_draft_id.clone())),
            _ => None,
        });
    op_request_from(state, ctx, what, device, req, origin);
}

pub(super) fn op_request_from(
    state: &mut State,
    ctx: &Ctx,
    what: &'static str,
    device: String,
    req: Req,
    response_origin: Option<(String, String)>,
) {
    let self_id = state.self_id.clone();
    let network_id = state.network.as_ref().map(|n| n.network_id.clone());
    let backend = ctx.backend.clone();
    let provider = if what == "terminal_result" {
        super::code_runs::provider(state, &device, &response_origin)
    } else if what == "chat" {
        response_origin
            .as_ref()
            .map(|(provider, _)| provider.clone())
            .unwrap_or_default()
    } else {
        state
            .chat
            .as_ref()
            .filter(|c| c.device == device)
            .map(|c| c.session.provider.clone())
            .unwrap_or_default()
    };
    let route = if what == "chat" {
        chat_route(state, &device)
    } else {
        self_id.clone()
    };
    let remote_native_send = device != route
        && matches!(&req, Req::AgentSend(r) if !muster_agent::cloud::is_cloud(&r.session) && !muster_agent::orca::is_orca(&r.session));
    let mut diagnostic =
        crate::send_diagnostics::Trace::new(ctx.shell.clone(), what, &device, &req);
    spawn(ctx, async move {
        let locked_nonce = if let Req::LockedUseExecute(r) = &req {
            muster_core::locked_use::Challenge::decode(&r.challenge)
                .ok()
                .map(|c| c.nonce().to_vec())
        } else {
            None
        };
        let result = async {
            diagnostic.stage("connect");
            let client = backend.client_for(&device, &route).await?;
            if (what == "chat" || remote_native_send) && !matches!(&req, Req::LockedUseExecute(_)) {
                let Resp::DeviceInfo(info) = diagnostic
                    .call(
                        &client,
                        Req::DeviceInfo(v1::DeviceInfoRequest {}),
                        "capability_check",
                    )
                    .await?
                else {
                    return Err(unexpected());
                };
                if !info.services.iter().any(|s| s == "agent.chat.locked.v1") {
                    return Err(Error::Unavailable(
                        "Update Muster on the target Mac to load or send Chat through Locked Use"
                            .into(),
                    ));
                }
            }
            if let Req::AgentSend(send) = &req
                && !send.attachments.is_empty()
            {
                let Resp::DeviceInfo(info) = diagnostic
                    .call(
                        &client,
                        Req::DeviceInfo(v1::DeviceInfoRequest {}),
                        "capability_check",
                    )
                    .await?
                else {
                    return Err(unexpected());
                };
                if !info.services.iter().any(|s| s == "agent.attachments") {
                    return Err(Error::Invalid(
                        "Update Muster on the target Mac before sending attachments".into(),
                    ));
                }
            }
            if matches!(&req, Req::AgentRespond(r) if !r.answers.is_empty()) {
                let Resp::DeviceInfo(info) = diagnostic
                    .call(
                        &client,
                        Req::DeviceInfo(v1::DeviceInfoRequest {}),
                        "capability_check",
                    )
                    .await?
                else {
                    return Err(unexpected());
                };
                if !info.services.iter().any(|s| s == "agent.questions.v1") {
                    return Err(Error::Invalid(
                        "Update Muster on the target Mac to answer question cards".into(),
                    ));
                }
            }
            let mut response = diagnostic
                .call(
                    &client,
                    req.clone(),
                    if locked_nonce.is_some() {
                        "locked_execute"
                    } else {
                        "dispatch"
                    },
                )
                .await;
            if matches!(
                &response,
                Err(muster_rpc::Error::Remote {
                    code: v1::error::Code::LockedUseRequired,
                    ..
                })
            ) {
                // A terminal's input outside its terminal session: the Mac
                // locked again (the terminal reopens one).
                let mut operation = muster_proto::locked_use::operation(&req).ok_or_else(|| {
                    Error::Unavailable("The Mac is locked: open the terminal again".into())
                })?;
                if muster_core::apps::app(&provider).is_none() {
                    return Err(Error::Invalid(
                        "Cannot determine this operation's provider for Locked Use".into(),
                    ));
                }
                operation.provider_queue = provider;
                match diagnostic
                    .call(
                        &client,
                        Req::LockedUsePrepare(v1::LockedUsePrepareRequest {
                            operation: Some(operation.clone()),
                        }),
                        "locked_prepare",
                    )
                    .await
                {
                    Err(muster_rpc::Error::Remote {
                        code: v1::error::Code::Conflict,
                        ..
                    }) => {
                        // Another request from this device may have issued the
                        // queue challenge after our initial locked response.
                        // That initial response proves this op never started.
                        response = diagnostic
                            .call(&client, req.clone(), "locked_conflict_retry")
                            .await;
                    }
                    Ok(Resp::LockedUseChallenge(reply)) => {
                        let c = muster_core::locked_use::Challenge::decode(&reply.challenge)
                            .map_err(|e| Error::Invalid(e.to_string()))?;
                        let network: [u8; 32] = hex::decode(network_id.ok_or_else(unexpected)?)
                            .map_err(|_| unexpected())?
                            .try_into()
                            .map_err(|_| unexpected())?;
                        let target =
                            muster_core::DeviceId::from_hex(&device).map_err(|_| unexpected())?;
                        let requester =
                            muster_core::DeviceId::from_hex(&self_id).map_err(|_| unexpected())?;
                        let message = c
                            .signing_message_for(
                                &network,
                                target,
                                requester,
                                muster_proto::locked_use::turn(&operation)
                                    .ok_or_else(unexpected)?,
                                &muster_proto::locked_use::digest(&operation),
                            )
                            .map_err(|e| Error::Invalid(e.to_string()))?;
                        diagnostic.stage("authentication");
                        return Ok(Msg::PresenceNeeded {
                            response_origin: response_origin.clone(),
                            what,
                            device: device.clone(),
                            call_id: client.next_id(),
                            request: Box::new(Req::LockedUseExecute(v1::LockedUseExecuteRequest {
                                challenge: reply.challenge,
                                signature: Vec::new(),
                            })),
                            message,
                        });
                    }
                    Ok(_) => return Err(unexpected()),
                    Err(error) => return Err(error.into()),
                }
            }
            if what == "chat" {
                let id = response_origin.as_ref().ok_or_else(unexpected)?.1.clone();
                let result = match response {
                    Ok(Resp::OpResult(r)) => Err(Error::Unavailable(if r.message.is_empty() {
                        "Chat operation did not complete".into()
                    } else {
                        r.message
                    })),
                    Ok(response) => Ok(response),
                    Err(error) => Err(error.into()),
                };
                return Ok(Msg::ChatControlReply {
                    device: device.clone(),
                    id,
                    nonce: locked_nonce.clone(),
                    result,
                });
            }
            let result = match response {
                Ok(Resp::OpResult(r)) => r,
                Ok(Resp::Ack(_)) => v1::OpResult::default(),
                Ok(_) => return Err(unexpected()),
                // Once dispatched, a lost reply is not proof of rejection.
                // Keep the card blocked until provider completion is observed.
                // Explicit server rejection is definitive. RPC cancellation
                // timeouts can occur after delivery and remain ambiguous.
                Err(
                    error @ muster_rpc::Error::Remote {
                        code:
                            v1::error::Code::Invalid
                            | v1::error::Code::Unauthorized
                            | v1::error::Code::NotFound
                            | v1::error::Code::Conflict
                            | v1::error::Code::Busy,
                        ..
                    },
                ) => {
                    return Err(error.into());
                }
                Err(error) if matches!(what, "respond" | "send" | "terminal_result") => {
                    v1::OpResult {
                        status: v1::op_result::Status::Unknown as i32,
                        message: format!("Submission acknowledgement unavailable: {error}"),
                        ..Default::default()
                    }
                }
                Err(error) => return Err(error.into()),
            };
            Ok(Msg::OpDone {
                what,
                device: device.clone(),
                locked_nonce: locked_nonce.clone(),
                response_origin: response_origin.clone(),
                result: Ok(result),
            })
        }
        .await;
        if let Err(error) = &result {
            diagnostic.failure(error);
        } else if let Ok(Msg::OpDone {
            result: Ok(outcome),
            ..
        }) = &result
        {
            diagnostic.event(match v1::op_result::Status::try_from(outcome.status) {
                Ok(v1::op_result::Status::Applied) => "completed_applied",
                Ok(v1::op_result::Status::Duplicate) => "completed_duplicate",
                Ok(v1::op_result::Status::Rejected) => "completed_rejected",
                _ => "completed_unknown",
            });
        }
        result.unwrap_or_else(|error| {
            if what == "chat" {
                Msg::ChatControlReply {
                    device,
                    id: response_origin.map(|(_, id)| id).unwrap_or_default(),
                    nonce: locked_nonce,
                    result: Err(error),
                }
            } else {
                Msg::OpDone {
                    what,
                    device,
                    locked_nonce,
                    response_origin,
                    result: Err(error),
                }
            }
        })
    });
}

/// A send was applied: clear the draft, and follow a new desktop chat to
/// the conversation its first message became.
pub(super) fn sent(state: &mut State, ctx: &Ctx, device: &str, landed: &str) {
    use muster_agent::desktop_chat as dc;
    let Some(chat) = &mut state.chat else { return };
    if chat.device != device {
        return;
    }
    if chat.session.id == landed {
        let tag = std::mem::take(&mut chat.pending_draft_id);
        chat.sent_draft_id = tag
            .split_once(':')
            .map(|(_, draft)| draft)
            .unwrap_or(&tag)
            .to_owned();
        if let Some(app) = dc::app_of(landed).filter(|_| dc::is_new_session(landed)) {
            let text = format!(
                "Sent. The conversation appears in the chat list once {} names it",
                app.name()
            );
            state.toast(text, false);
        }
        return;
    }
    if dc::is_new_session(&chat.session.id)
        && dc::app_of(landed).is_some()
        && dc::app_of(landed) == dc::app_of(&chat.session.id)
    {
        let mut session = chat.session.clone();
        let placeholder = std::mem::replace(&mut session.id, landed.to_string());
        session.title.clear();
        let device = device.to_string();
        super::admitted::rekey(state, &device, &placeholder, landed);
        show_chat(state, ctx, device, session);
    }
}

pub(super) fn cancel_locked(state: &mut State, ctx: &Ctx, reason: CancelReason) {
    state.chat_call_epoch += 1;
    for (_, reply) in state.chat_calls.drain() {
        let _ = reply.send(Err(Error::Unavailable(format!(
            "Chat loading cancelled: {}",
            reason.as_str()
        ))));
    }
    let Some((device, nonce)) = state.locked.take() else {
        return;
    };
    state.manual_busy.remove(&device);
    super::code_runs::cancelled(state, &device);
    if let Some(p) = state.presence.take() {
        let what = match p.action_id.split('-').next() {
            Some("send") => "send",
            Some("respond") => "respond",
            Some("terminal_result") => "terminal_result",
            _ => "other",
        };
        let mut diagnostic =
            crate::send_diagnostics::Trace::new(ctx.shell.clone(), what, &p.device, &p.request);
        diagnostic.stage("authentication_cancel");
        diagnostic.rejection(reason.as_str());
        state.effects.remove(&p.effect_id);
        if what == "send" {
            super::send_recovery::outcome(
                state,
                ctx,
                &p.device,
                &p.response_origin,
                &Err(Error::Unavailable(
                    "Authentication cancelled before dispatch".into(),
                )),
            );
        }
    }
    if let Some(chat) = state.chat.as_mut() {
        chat.sending = false;
        chat.responding_interaction.clear();
        chat.response_error = "Authentication or Locked Use cancelled; your answer was kept".into();
    }
    cancel_challenge(state, ctx, device, nonce, reason);
    super::send_recovery::restore(state, ctx);
}

pub(super) fn cancel_challenge(
    state: &State,
    ctx: &Ctx,
    device: String,
    nonce: Vec<u8>,
    reason: CancelReason,
) {
    log::info!(
        "Locked Use: withdrawing the challenge ({})",
        reason.as_str()
    );
    let backend = ctx.backend.clone();
    let self_id = state.self_id.clone();
    spawn(ctx, async move {
        let result = async {
            let client = backend.client_for(&device, &self_id).await?;
            unary(
                &client,
                Req::LockedUseCancel(v1::LockedUseCancelRequest {
                    nonce: nonce.clone(),
                    reason: reason.as_str().into(),
                }),
            )
            .await?;
            Ok(v1::OpResult {
                status: v1::op_result::Status::Rejected as i32,
                message: "Locked Use cancelled".into(),
                ..Default::default()
            })
        }
        .await;
        Msg::OpDone {
            what: "locked",
            device,
            locked_nonce: Some(nonce),
            response_origin: None,
            result,
        }
    });
}

pub(super) fn msg_op_done(
    state: &mut State,
    ctx: &Ctx,
    what: &'static str,
    device: String,
    locked_nonce: Option<Vec<u8>>,
    response_origin: Option<(String, String)>,
    result: Result<v1::OpResult, Error>,
) {
    let mut draft_saved = true;
    // A late success clears only the exact submitted revision, even
    // after navigation. Failure/unknown and subsequent edits survive.
    if what == "input" {
        super::terminals::msg_terminal_input_done(state, ctx, device, response_origin, result);
        return;
    }
    if what == "send"
        && let Some((_, tag)) = &response_origin
        && let Ok(result) = &result
        && result.status != v1::op_result::Status::Unknown as i32
        && let Some(submitted) = state.pending_drafts.get(tag).cloned()
    {
        match crate::drafts::clear_sent(&ctx.cache, &submitted, result.status) {
            Ok(Some(cleared)) => {
                if let Some(chat) = state.chat.as_mut().filter(|c| c.draft.key == cleared.key) {
                    chat.draft = cleared;
                }
            }
            Ok(None) => {}
            Err(error) => {
                draft_saved = false;
                state.toast(format!("消息已发送，草稿清理暂未写入磁盘：{error}"), true);
            }
        }
        if draft_saved {
            state.pending_drafts.remove(tag);
        }
    }
    if what == "send" && draft_saved {
        super::send_recovery::outcome(state, ctx, &device, &response_origin, &result);
    }
    let same_origin = state.chat.as_ref().is_some_and(|c| {
        c.device == device
            && response_origin.as_ref().is_some_and(|(session, tag)| {
                &c.session.id == session && (what != "send" || &c.pending_draft_id == tag)
            })
    });
    let locked_finished =
        locked_nonce.is_none() || state.finish_locked(&device, locked_nonce.as_deref());
    if what == "terminal_result" {
        super::code_runs::delivery(state, &device, response_origin, result);
        return;
    }
    // The lease can end before a read-only receipt arrives. An authenticated
    // result for the exact pending draft remains relevant, but cannot close a
    // newer lease or affect a different session/revision.
    if !locked_finished && !(what == "send" && same_origin) {
        return;
    }
    if what == "manual" {
        state.manual_busy.remove(&device);
        match result {
            Ok(r) if r.status == v1::op_result::Status::Applied as i32 => {}
            Ok(r) => state.toast(format!("Locked Use: {}", r.message), true),
            Err(e) => state.toast(format!("Locked Use: {e}"), true),
        }
        return;
    }
    // A send refused behind an unconfirmed earlier one: the conversation
    // offers its release instead of a toast.
    let unconfirmed = match &result {
        Ok(r) if what == "send" && same_origin && !r.unconfirmed.is_empty() => {
            Some(app::UnconfirmedSend {
                token: r.unconfirmed.clone(),
                message: r.message.clone(),
            })
        }
        _ => None,
    };
    if let Some(chat) = &mut state.chat {
        if what == "send" && same_origin {
            chat.sending = false;
            chat.unconfirmed = unconfirmed.clone();
        }
        if what == "respond"
            && let Some((session, interaction)) = &response_origin
            && chat.device == device
            && &chat.session.id == session
            && &chat.responding_interaction == interaction
        {
            chat.responding_interaction.clear();
            match &result {
                Ok(r)
                    if matches!(
                        v1::op_result::Status::try_from(r.status),
                        Ok(v1::op_result::Status::Applied | v1::op_result::Status::Duplicate)
                    ) =>
                {
                    chat.interactions.retain(|i| &i.id != interaction);
                    chat.response_error.clear();
                    chat.response_unknown = false;
                }
                Ok(r) => {
                    chat.response_unknown = r.status == v1::op_result::Status::Unknown as i32;
                    chat.response_error = r.message.clone();
                }
                Err(error) => {
                    chat.response_error = error.to_string();
                }
            }
        }
    }
    // Answer errors belong to their card. A provider acknowledgement can
    // arrive before or after this RPC: never leave a global error toast
    // behind, or resurrect an error after InteractionResolved cleared it.
    if what == "respond" && response_origin.is_some() {
        if let Ok(r) = &result
            && matches!(
                v1::op_result::Status::try_from(r.status),
                Ok(v1::op_result::Status::Applied | v1::op_result::Status::Duplicate)
            )
            && !r.message.is_empty()
        {
            state.toast(r.message.clone(), false);
        }
        return;
    }
    // What the host said about this send. It never removes the record: only
    // the conversation's own messages or queue can account for the text.
    if what == "send"
        && same_origin
        && let Some((_, tag)) = &response_origin
    {
        let (unknown, error) = match &result {
            Ok(r) => match v1::op_result::Status::try_from(r.status) {
                Ok(
                    v1::op_result::Status::Applied
                    | v1::op_result::Status::Duplicate
                    | v1::op_result::Status::UserConfirmed,
                ) => (false, String::new()),
                Ok(v1::op_result::Status::Rejected) => (false, r.message.clone()),
                _ => (true, r.message.clone()),
            },
            Err(error) => (false, error.to_string()),
        };
        let op = super::admitted::op_of(tag).to_owned();
        super::admitted::outcome(state, &op, unknown, &error);
    }
    match result {
        Ok(r) => {
            let status =
                v1::op_result::Status::try_from(r.status).unwrap_or(v1::op_result::Status::Unknown);
            match status {
                v1::op_result::Status::Applied
                | v1::op_result::Status::Duplicate
                | v1::op_result::Status::UserConfirmed => {
                    if what == "send" && same_origin {
                        sent(state, ctx, &device, &r.result_id);
                    }
                }
                v1::op_result::Status::Rejected if unconfirmed.is_none() => {
                    state.toast(format!("{what}: {}", r.message), true)
                }
                v1::op_result::Status::Rejected => {}
                v1::op_result::Status::Unknown => state.toast(
                    if r.message.is_empty() {
                        format!("{what}: outcome unknown, check before retrying")
                    } else {
                        format!("{what}: {}", r.message)
                    },
                    true,
                ),
            }
        }
        Err(Error::Busy(why))
            if what == "send"
                && same_origin
                && state
                    .chat
                    .as_ref()
                    .is_some_and(|c| c.device == device && c.session.mode == "chat") =>
        {
            // Another Mac shows the same conversation: continue there.
            let session = state.chat.as_ref().expect("checked").session.clone();
            state.toast(format!("{why}; picking another Mac"), false);
            repick_chat_host(
                state,
                ctx,
                &device,
                ChatRetry::Move {
                    session: Box::new(session),
                    from: device.clone(),
                },
            );
        }
        Err(error) => state.toast(format!("{what}: {error}"), true),
    }
}

pub(super) fn msg_locked_expired(state: &mut State, ctx: &Ctx, nonce: Vec<u8>) {
    if state.locked.as_ref().is_some_and(|(_, n)| *n == nonce) {
        cancel_locked(state, ctx, CancelReason::Expired);
    }
}

// The arguments are Msg::PresenceNeeded's fields.
#[allow(clippy::too_many_arguments)]
pub(super) fn msg_presence_needed(
    state: &mut State,
    ctx: &Ctx,
    response_origin: Option<(String, String)>,
    what: &'static str,
    device: String,
    call_id: u64,
    request: Box<Req>,
    message: Vec<u8>,
) {
    let mut auth_diagnostic =
        crate::send_diagnostics::Trace::new(ctx.shell.clone(), what, &device, &request);
    auth_diagnostic.stage("authentication");
    if what == "chat"
        && response_origin.as_ref().is_none_or(|(_, id)| {
            state
                .chat_calls
                .get(id)
                .is_none_or(|reply| reply.is_closed())
        })
    {
        if let Req::LockedUseExecute(r) = &*request
            && let Ok(c) = muster_core::locked_use::Challenge::decode(&r.challenge)
        {
            cancel_challenge(state, ctx, device, c.nonce().to_vec(), CancelReason::User);
        }
        fail_chat_call(state, &response_origin, "Chat loading cancelled");
        return;
    }
    if what == "browser" && (state.presence.is_some() || !state.foreground) {
        state.browser.busy = false;
        state.browser.result = "Authentication request cancelled".into();
        return;
    }
    let locked = if let Req::LockedUseExecute(r) = &*request {
        match muster_core::locked_use::Challenge::decode(&r.challenge) {
            Ok(c) => Some((device.clone(), c.nonce().to_vec())),
            Err(e) => {
                if what == "manual" {
                    state.manual_busy.remove(&device);
                }
                if what == "chat" {
                    fail_chat_call(state, &response_origin, "Invalid Locked Use challenge");
                }
                if what == "terminal_result" {
                    super::code_runs::cancelled(state, &device);
                }
                super::send_recovery::not_dispatched(state, ctx, what, &device, &response_origin);
                state.toast(e.to_string(), true);
                return;
            }
        }
    } else {
        None
    };
    if let Some((_, nonce)) = &locked {
        if state.locked.is_some() || state.presence.is_some() || !state.foreground {
            if what == "manual" {
                state.manual_busy.remove(&device);
            }
            let reason = if state.foreground {
                CancelReason::Busy
            } else {
                CancelReason::Background
            };
            if what == "chat" {
                fail_chat_call(
                    state,
                    &response_origin,
                    "Another authorization is active or the app is in the background",
                );
            }
            if what == "terminal_result" {
                super::code_runs::cancelled(state, &device);
            }
            auth_diagnostic.event(if state.foreground {
                "authentication_busy"
            } else {
                "authentication_background"
            });
            super::send_recovery::not_dispatched(state, ctx, what, &device, &response_origin);
            cancel_challenge(state, ctx, device, nonce.clone(), reason);
            return;
        }
        let nonce = nonce.clone();
        spawn(ctx, async move {
            tokio::time::sleep(std::time::Duration::from_secs(15)).await;
            Msg::LockedExpired(nonce)
        });
        state.locked = locked;
    }
    // Rust already validated the challenge against the selected target,
    // network and complete pending operation. Only foreground phones
    // use their paired hardware identity; admin/browser requests retain
    // the Authenticate effect and presence key.
    if state.embedded
        && state
            .self_info
            .as_ref()
            .is_some_and(|info| info.kind == "phone")
        && let Req::LockedUseExecute(mut request) = *request.clone()
    {
        let mut diagnostic = crate::send_diagnostics::Trace::new(
            ctx.shell.clone(),
            what,
            &device,
            &Req::LockedUseExecute(request.clone()),
        );
        diagnostic.stage("authentication_sign");
        let signature = ctx
            .backend
            .signer()
            .ok_or_else(|| Error::Unavailable("device signing key unavailable".into()))
            .and_then(|key| {
                key.sign(&message)
                    .map_err(|e| Error::Unauthorized(e.to_string()))
            });
        match signature {
            Ok(signature) => {
                diagnostic.event("authenticated");
                request.signature = signature;
                op_request_from(
                    state,
                    ctx,
                    what,
                    device,
                    Req::LockedUseExecute(request),
                    response_origin,
                );
            }
            Err(error) => {
                diagnostic.failure(&error);
                super::send_recovery::not_dispatched(state, ctx, what, &device, &response_origin);
                cancel_locked(state, ctx, CancelReason::Authentication);
                state.toast(error.to_string(), true);
            }
        }
        return;
    }
    let action_id = format!("{what}-{call_id}");
    let reason = if what == "manual" {
        "Keep this Mac available under a privacy cover until you end Locked Use from this device, then lock it"
    } else if state.locked.is_some() {
        "Temporarily unlock the Mac under a privacy cover for this provider's queued operations from this device, then lock it when the queue finishes"
    } else {
        match what {
            "browser" => {
                "Authorize this browser operation on the selected device. Its screen stays locked."
            }
            "policy" => "Change the device's policy",
            "remove" => "Remove a device",
            "scopes" => "Change a device's permissions",
            "decide" => "Approve a device",
            _ => "Administer the network",
        }
    };
    let effect_id = effect(
        state,
        ctx,
        EffectKind::Authenticate,
        app::effect::Ef::Authenticate(app::Authenticate {
            reason: reason.into(),
            message,
        }),
    );
    state.presence = Some(PendingPresence {
        response_origin,
        action_id,
        call_id,
        device,
        request: *request,
        effect_id,
    });
}

#[cfg(test)]
mod diagnostic_tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    struct Reply {
        response: Resp,
        calls: AtomicUsize,
    }
    impl muster_rpc::Handler for Reply {
        fn call(
            &self,
            _: muster_rpc::Context,
            _: Req,
            _: muster_rpc::EventSink,
        ) -> muster_rpc::HandlerFuture<'_> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { self.response.clone() })
        }
    }
    #[tokio::test]
    async fn send_diagnostics_preserve_remote_codes_unknown_results_and_never_replay() {
        for response in [
            Resp::Error(v1::Error {
                code: v1::error::Code::Conflict as i32,
                message: "private draft".into(),
            }),
            Resp::Error(v1::Error {
                code: v1::error::Code::Timeout as i32,
                message: "private endpoint".into(),
            }),
            Resp::OpResult(v1::OpResult {
                status: v1::op_result::Status::Unknown as i32,
                message: "private receipt text".into(),
                ..Default::default()
            }),
            Resp::OpResult(v1::OpResult {
                status: v1::op_result::Status::Applied as i32,
                result_id: "private-session".into(),
                ..Default::default()
            }),
        ] {
            let (a, b) = tokio::io::duplex(65536);
            let handler = Arc::new(Reply {
                response: response.clone(),
                calls: AtomicUsize::new(0),
            });
            tokio::spawn(muster_rpc::serve(
                b,
                muster_rpc::Principal::Local,
                handler.clone(),
            ));
            let sink = Arc::new(crate::send_diagnostics::tests::Capture::default());
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let dir = tempfile::tempdir().unwrap();
            let ctx = Ctx {
                shell: sink.clone(),
                cache: Arc::new(crate::cache::Cache::new(dir.path().into())),
                backend: Arc::new(crate::backend::Backend::Socket {
                    path: dir.path().into(),
                    local: muster_rpc::Client::new(a),
                }),
                tx,
            };
            let mut state = State::new(false, Some("self".into()));
            op_request_from(
                &mut state,
                &ctx,
                "send",
                "self".into(),
                Req::AgentSend(v1::AgentSendRequest {
                    op_id: "app-fixture-1".into(),
                    text: "private body".into(),
                    ..Default::default()
                }),
                None,
            );
            let reply = tokio::time::timeout(Duration::from_secs(2), rx.recv())
                .await
                .unwrap()
                .unwrap();
            let Loop::Msg(msg) = reply else {
                panic!("expected result")
            };
            let Msg::OpDone { result, .. } = *msg else {
                panic!("expected send result")
            };
            let rows = sink.0.lock().unwrap();
            match response {
                Resp::Error(ref e) => {
                    assert!(
                        rows.iter()
                            .any(|r| r["phase"] == "dispatch" && r["remote_error_code"] == e.code)
                    );
                    if e.code == v1::error::Code::Timeout as i32 {
                        assert_eq!(
                            result.unwrap().status,
                            v1::op_result::Status::Unknown as i32
                        );
                        assert!(rows.iter().any(|r| r["event"] == "completed_unknown"));
                    } else {
                        assert!(result.is_err());
                    }
                }
                Resp::OpResult(ref r) => {
                    assert_eq!(result.unwrap().status, r.status);
                    assert!(
                        rows.iter()
                            .any(|r| r["event"] == "unknown" || r["event"] == "applied")
                    );
                }
                _ => unreachable!(),
            }
            assert!(!serde_json::to_string(&*rows).unwrap().contains("private"));
            assert_eq!(handler.calls.load(Ordering::SeqCst), 1);
        }
    }
}
