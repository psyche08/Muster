//! Workspace-selected service policy. Never turns a peer into a local owner.
use crate::{Daemon, Error};
use muster_core::{Permission, TargetState};
use muster_proto::v1;
use muster_rpc::{Context, Principal};

pub(crate) fn authorize(
    daemon: &Daemon,
    ctx: &Context,
    workspace: Option<&str>,
    control: bool,
) -> Result<(), Error> {
    let Principal::Remote(peer) = ctx.principal else {
        return Ok(());
    };
    let permission = Permission::Multica {
        workspace: workspace.map(str::to_owned),
        control,
    };
    if !muster_core::authorize(
        daemon.roster().read().unwrap().state(),
        &peer,
        &permission,
        &daemon.policy().read().unwrap(),
        &TargetState {
            locked: control && daemon.platform().locked(),
        },
    )
    .is_allow()
    {
        return Err(Error::Unauthorized(
            "Multica workspace access denied".into(),
        ));
    }
    Ok(())
}

pub(crate) async fn configure(
    daemon: &Daemon,
    ctx: &Context,
    request: v1::MulticaServiceRequest,
) -> Result<v1::MulticaService, Error> {
    let local = matches!(ctx.principal, Principal::Local);
    let action = v1::multica_service_request::Action::try_from(request.action)
        .map_err(|_| Error::Invalid("Unknown Multica service action".into()))?;
    if action == v1::multica_service_request::Action::Save {
        if !local {
            return Err(Error::Unauthorized(
                "Multica sharing is managed on its host".into(),
            ));
        }
        if request.workspaces.len() > 1000
            || request
                .workspaces
                .iter()
                .any(|id| id.is_empty() || id.len() > 128)
        {
            return Err(Error::Invalid("Select valid Multica workspaces".into()));
        }
        let selected: std::collections::BTreeSet<_> = request.workspaces.into_iter().collect();
        if request.offered {
            if selected.is_empty() {
                return Err(Error::Invalid(
                    "Select at least one workspace before sharing".into(),
                ));
            }
            let page = daemon
                .multica
                .browse(v1::MulticaBrowseRequest::default())
                .await?;
            if selected
                .iter()
                .any(|id| !page.workspaces.iter().any(|w| &w.id == id))
            {
                return Err(Error::Invalid(
                    "A selected Multica workspace is no longer available".into(),
                ));
            }
        }
        let mut policy = daemon.policy().write().unwrap();
        let mut updated = policy.clone();
        updated.offer_multica = request.offered;
        updated.multica_workspaces = selected;
        daemon.store().set_policy(&updated)?;
        *policy = updated;
        let _ = daemon.changed.send(());
    }
    let policy = daemon.policy().read().unwrap().clone();
    let view = authorize(daemon, ctx, None, false).is_ok();
    // This protocol version exposes the read-only workbench. A stored control
    // grant does not advertise an operation path that has not been accepted.
    let control = false;
    Ok(v1::MulticaService {
        version: 1,
        offered: local && policy.offer_multica || !local && view,
        can_view: view,
        can_control: control,
        can_manage: local,
        workspaces: if local || view {
            policy.multica_workspaces.into_iter().collect()
        } else {
            Vec::new()
        },
        message: if view {
            String::new()
        } else {
            "此设备未开放 Multica，或当前设备没有查看权限".into()
        },
    })
}
