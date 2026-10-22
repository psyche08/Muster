//! Creation uses the same device route and authenticated RPC as every agent action.
use super::*;
pub(super) fn create(state: &mut State, ctx: &Ctx, input: app::OrcaCreate) {
    if state.agents.creating {
        return;
    }
    if !state
        .reachable_service_devices()
        .iter()
        .any(|d| d.device_id == input.device_id)
        || !matches!(input.agent.as_str(), "codex" | "claude")
        || input.directory.trim().is_empty()
    {
        state.toast("Choose an available device, CLI agent and directory", true);
        return;
    }
    state.agents.creating = true;
    let self_id = state.self_id.clone();
    let backend = ctx.backend.clone();
    let op_id = state.op_id();
    let network = state.network.as_ref().map(|n| n.network_id.clone());
    spawn(ctx, async move {
        let device = input.device_id;
        let result = async {
            let client = backend.client_for(&device, &self_id).await?;
            let Resp::DeviceInfo(info) =
                unary(&client, Req::DeviceInfo(v1::DeviceInfoRequest {})).await?
            else {
                return Err(unexpected());
            };
            if !info.services.iter().any(|s| s == "agent.orca.v1") {
                return Err(Error::Unavailable(
                    "Update Muster on this device to use its built-in CLI session manager".into(),
                ));
            }
            let Resp::OpResult(op) = unary(
                &client,
                Req::OrcaCreate(v1::OrcaCreateRequest {
                    op_id,
                    directory: input.directory,
                    agent: input.agent,
                    // A session opened from the UI has no external task
                    // behind it, so there is no per-task environment to pass.
                    env: Default::default(),
                }),
            )
            .await?
            else {
                return Err(unexpected());
            };
            if !matches!(
                op.status(),
                v1::op_result::Status::Applied | v1::op_result::Status::Duplicate
            ) {
                return Err(Error::Unavailable(if op.message.is_empty() {
                    "Creation was not confirmed; refresh before creating again".into()
                } else {
                    op.message
                }));
            }
            let Resp::AgentSessions(page) = unary(
                &client,
                Req::AgentList(v1::AgentListRequest {
                    provider: "orca".into(),
                    limit: 1000,
                }),
            )
            .await?
            else {
                return Err(unexpected());
            };
            let mut session = page
                .sessions
                .into_iter()
                .find(|s| s.id == op.result_id)
                .ok_or_else(|| {
                    Error::Unavailable(
                        "Session created; refresh the session list to open it".into(),
                    )
                })?;
            session.device_id = device.clone();
            Ok(session)
        }
        .await;
        Msg::OrcaCreated {
            network,
            device,
            result,
        }
    });
}
