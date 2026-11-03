use super::*;

/// The owner's own LLM that looks at a failed native send (ADR 0116). Local
/// daemon only: the API key goes out once and never comes back.
pub(super) fn request(state: &mut State, ctx: &Ctx, request: v1::VisionRequest) {
    if state.embedded {
        return;
    }
    if state.vision.busy {
        // A refresh while a save or test runs is simply skipped.
        if request.action != v1::vision_request::Action::Get as i32 {
            state.toast("正在处理上一个请求，请稍候", true);
        }
        return;
    }
    state.vision.busy = true;
    let client = ctx.backend.local().clone();
    spawn(ctx, async move {
        let result = match unary(&client, Req::Vision(request)).await {
            Ok(Resp::Vision(status)) => Ok(status),
            Ok(_) => Err(unexpected()),
            Err(error) => Err(error),
        };
        Msg::Vision(result)
    });
}

pub(super) fn apply_result(state: &mut State, result: Result<v1::VisionStatus, Error>) {
    state.vision.busy = false;
    match result {
        Ok(status) => state.vision = status,
        Err(error) => state.vision.message = error.to_string(),
    }
}
