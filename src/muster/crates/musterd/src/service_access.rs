//! The local-owner service rules projected through the daemon RPC.
use muster_core::{
    DeviceId, Scope,
    policy::{DeviceGrant, GrantSet, ServiceGrants},
};
use muster_proto::v1;

use crate::Error;

pub fn view(grants: &ServiceGrants) -> v1::ServiceGrants {
    v1::ServiceGrants {
        agents: grants.agents.as_ref().map(grant_view),
        terminals: grants.terminals.as_ref().map(grant_view),
        network: grants.network.as_ref().map(grant_view),
    }
}

fn grant_view(grants: &GrantSet) -> v1::GrantSet {
    v1::GrantSet {
        all_devices: grants.all_devices,
        rules: grants
            .rules
            .iter()
            .map(|rule| v1::DeviceGrant {
                scope: rule.scope.to_string(),
                device_ids: rule.devices.iter().map(DeviceId::to_hex).collect(),
            })
            .collect(),
    }
}

pub fn parse(view: &v1::ServiceGrants) -> Result<ServiceGrants, Error> {
    Ok(ServiceGrants {
        agents: parse_grants(view.agents.as_ref(), |scope| {
            matches!(scope, Scope::Agents | Scope::AgentProvider(_))
        })?,
        terminals: parse_grants(view.terminals.as_ref(), |scope| {
            matches!(
                scope,
                Scope::Terminals | Scope::TerminalView | Scope::TerminalControl
            )
        })?,
        network: parse_grants(view.network.as_ref(), |scope| {
            matches!(scope, Scope::Net { .. })
        })?,
    })
}

fn parse_grants(
    view: Option<&v1::GrantSet>,
    valid: impl Fn(&Scope) -> bool,
) -> Result<Option<GrantSet>, Error> {
    let Some(view) = view else {
        return Ok(None);
    };
    let rules = view
        .rules
        .iter()
        .map(|rule| {
            let scope = rule.scope.parse()?;
            if !valid(&scope) {
                return Err(Error::Invalid(format!(
                    "{} is not a selector for this service",
                    rule.scope
                )));
            }
            let devices = rule
                .device_ids
                .iter()
                .map(|id| DeviceId::from_hex(id))
                .collect::<Result<_, _>>()?;
            Ok(DeviceGrant { scope, devices })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(Some(GrantSet {
        all_devices: view.all_devices,
        rules,
    }))
}
