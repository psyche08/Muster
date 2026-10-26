use super::*;
pub(super) fn request(state: &mut State, ctx: &Ctx, request: v1::MulticaRequest) {
    if state.embedded || state.multica.busy {
        return;
    }
    state.multica.busy = true;
    let client = ctx.backend.local().clone();
    spawn(ctx, async move {
        let result = async {
            match unary(&client, Req::Multica(request)).await? {
                Resp::Multica(status) => Ok(status),
                _ => Err(unexpected()),
            }
        }
        .await;
        Msg::Multica(result)
    });
}
