//! Open the host's returned child in the same timeline as a native sub-session.
use super::*;
use muster_agent::side_chat::{MAX_QUESTION, available, ended, is_child};
use v1::agent_side_question_request::Action;

pub(super) fn action(
    state: &mut State,
    ctx: &Ctx,
    action: Action,
    question: &str,
    draft_id: &str,
    draft: Option<app::ChatDraft>,
) {
    let op_id = state.op_id();
    let Some(chat) = state.chat.as_mut() else {
        return;
    };
    let permitted = match action {
        Action::Create => available(&chat.session),
        Action::Ask | Action::End => {
            is_child(&chat.session.id) && !ended(&chat.session) && !chat.session.read_only
        }
    };
    if !permitted {
        state.toast("Side chat is unavailable for this session", true);
        return;
    }
    if chat.side_request.is_some() {
        return;
    }
    if question.len() > MAX_QUESTION || (action == Action::Ask && question.trim().is_empty()) {
        state.toast("Enter a side question of up to 8 KiB", true);
        return;
    }
    let origin = chat.session.id.clone();
    let device = chat.device.clone();
    let token = op_id.clone();
    chat.side_request = Some(token.clone());
    let self_id = chat_route(state, &device);
    let backend = ctx.backend.clone();
    let draft_id = draft_id.to_owned();
    let request = v1::AgentSideQuestionRequest {
        session: origin.clone(),
        action: action as i32,
        question: question.into(),
        op_id,
    };
    spawn(ctx, async move {
        let result = tokio::time::timeout(Duration::from_secs(30), async {
            let client = backend.client_for(&device, &self_id).await?;
            let Resp::DeviceInfo(info) =
                unary(&client, Req::DeviceInfo(v1::DeviceInfoRequest {})).await?
            else {
                return Err(unexpected());
            };
            if !info.services.iter().any(|s| s == "agent.side_chat.v1") {
                return Err(Error::Unavailable(
                    "Update Muster on the host to use side chat".into(),
                ));
            }
            match unary(&client, Req::AgentSideQuestion(request)).await? {
                Resp::AgentSideAnswer(reply) => reply
                    .session
                    .filter(|s| {
                        is_child(&s.id)
                            && if action == Action::Create {
                                s.parent_session_id == origin
                                    && muster_agent::side_chat::parent(&s.id)
                                        == Some(origin.as_str())
                            } else {
                                s.id == origin
                            }
                    })
                    .map(|s| *s)
                    .ok_or_else(unexpected),
                Resp::OpResult(op) => Err(Error::Unavailable(op.message)),
                _ => Err(unexpected()),
            }
        })
        .await
        .unwrap_or_else(|_| {
            Err(Error::Unavailable(
                "Side chat status is unknown; refresh before retrying".into(),
            ))
        });
        Msg::SideAnswer {
            token,
            device,
            origin,
            draft,
            draft_id,
            result,
        }
    });
}

#[allow(clippy::too_many_arguments)]
pub(super) fn answer(
    state: &mut State,
    ctx: &Ctx,
    token: &str,
    device: String,
    origin: String,
    draft_id: String,
    draft: Option<app::ChatDraft>,
    result: Result<v1::AgentSession, Error>,
) {
    let cleared = if result.is_ok() {
        draft.and_then(|d| {
            crate::drafts::clear_sent(&ctx.cache, &d, v1::op_result::Status::Applied as i32)
                .ok()
                .flatten()
        })
    } else {
        None
    };
    let Some(chat) = state.chat.as_mut().filter(|c| {
        c.device == device && c.session.id == origin && c.side_request.as_deref() == Some(token)
    }) else {
        return;
    };
    chat.side_request = None;
    match result {
        Ok(mut s) => {
            if let Some(d) = cleared
                && d.revision == chat.draft.revision
            {
                chat.draft = d;
            }
            chat.sent_draft_id = draft_id;
            s.device_id = device.clone();
            if s.id != origin {
                if state.screen == app::Screen::Chat {
                    show_chat(state, ctx, device, s);
                }
            } else if ended(&s) {
                // The watch can already have announced a later completion or End.
                // Never replace its state with the stale admission of a running turn.
                chat.session = s;
            }
        }
        Err(e) => state.toast(e.to_string(), true),
    }
}
