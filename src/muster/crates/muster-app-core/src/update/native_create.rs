//! A new desktop (Code mode) session in the provider's own app on a Mac,
//! started by its first message (ADR 0150). It is one ledgered operation on
//! that Mac, run through Locked Use when the Mac is locked, so it goes
//! through `op_request_from` and settles as OpDone("native_create").
use super::*;

/// The desktop apps a session can be created in, in the order offered.
pub(crate) const APPS: [&str; 2] = ["claude", "codex"];

/// The members a desktop session can be created on.
pub(crate) fn hosts(state: &State) -> Vec<v1::MemberView> {
    state
        .reachable_service_devices()
        .into_iter()
        .filter(|m| m.kind == "desktop")
        .collect()
}

pub(super) fn create(state: &mut State, ctx: &Ctx, input: app::NativeCreate) {
    if state.agents.native_creating {
        return;
    }
    let directory = input.directory.trim().to_owned();
    if !hosts(state).iter().any(|m| m.device_id == input.device_id)
        || !APPS.contains(&input.provider.as_str())
        || !directory.starts_with('/')
        || input.prompt.trim().is_empty()
    {
        state.toast(
            "Choose an available Mac, an app, an absolute directory and a first message",
            true,
        );
        return;
    }
    state.agents.native_creating = true;
    // Kept until the Mac answers: a refusal leaves it to correct and resubmit.
    state.agents.native_create_draft = Some(app::NativeCreate {
        directory: directory.clone(),
        ..input.clone()
    });
    let request = v1::AgentNativeCreateRequest {
        op_id: state.op_id(),
        provider: input.provider,
        directory,
        prompt: input.prompt,
    };
    super::operations::op_request_from(
        state,
        ctx,
        "native_create",
        input.device_id,
        Req::AgentNativeCreate(request),
        None,
    );
}

/// Authorization ended before the creation was sent: nothing was created.
pub(super) fn not_dispatched(state: &mut State, what: &str) {
    if what == "native_create" {
        state.agents.native_creating = false;
    }
}

/// The Mac's answer. Only an applied creation names its session; an unknown
/// one may still have reached the app, so its input is not offered again.
pub(super) fn done(
    state: &mut State,
    ctx: &Ctx,
    device: String,
    result: Result<v1::OpResult, Error>,
) {
    state.agents.native_creating = false;
    let provider = state
        .agents
        .native_create_draft
        .as_ref()
        .map(|d| d.provider.clone())
        .unwrap_or_default();
    use v1::op_result::Status;
    let status = |r: &v1::OpResult| Status::try_from(r.status).unwrap_or(Status::Unknown);
    match result {
        Ok(r)
            if matches!(
                status(&r),
                Status::Applied | Status::Duplicate | Status::UserConfirmed
            ) && !r.result_id.is_empty() =>
        {
            state.agents.native_create_draft = None;
            open(state, ctx, device, provider, r.result_id);
        }
        Ok(r) if status(&r) == Status::Rejected => {
            state.toast(format!("Desktop session not created: {}", r.message), true)
        }
        Ok(r) => {
            state.agents.native_create_draft = None;
            super::agents::load_catalog(state, ctx);
            state.toast(
                if r.message.is_empty() {
                    "The desktop app may have started the session; it is not submitted again. Check the session list".into()
                } else {
                    format!(
                        "The desktop app may have started the session; it is not submitted again. Check the session list ({})",
                        r.message
                    )
                },
                true,
            );
        }
        Err(error) => state.toast(format!("Desktop session not created: {error}"), true),
    }
}

/// Opens the created session once its Mac lists it.
fn open(state: &mut State, ctx: &Ctx, device: String, provider: String, session: String) {
    let self_id = state.self_id.clone();
    let backend = ctx.backend.clone();
    let network = state.network.as_ref().map(|n| n.network_id.clone());
    spawn(ctx, async move {
        let result = async {
            let client = backend.client_for(&device, &self_id).await?;
            let Resp::AgentSessions(page) = unary(
                &client,
                Req::AgentList(v1::AgentListRequest {
                    provider,
                    limit: 100,
                    ..Default::default()
                }),
            )
            .await?
            else {
                return Err(unexpected());
            };
            let mut found = page
                .sessions
                .into_iter()
                .find(|s| s.id == session)
                .ok_or_else(|| {
                    Error::Unavailable(
                        "Desktop session created; it appears in the session list shortly".into(),
                    )
                })?;
            found.device_id = device.clone();
            Ok(found)
        }
        .await;
        Msg::NativeCreated {
            network,
            device,
            result,
        }
    });
}

pub(super) fn opened(
    state: &mut State,
    ctx: &Ctx,
    network: Option<String>,
    device: String,
    result: Result<v1::AgentSession, Error>,
) {
    if state.network.as_ref().map(|n| n.network_id.clone()) != network {
        return;
    }
    super::agents::load_catalog(state, ctx);
    match result {
        Ok(session) => super::agents::show_chat(state, ctx, device, session),
        Err(error) => state.toast(error.to_string(), false),
    }
}
