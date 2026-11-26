//! The local-owner service rules projected through the daemon RPC.
use muster_core::{
    DeviceId, Scope,
    policy::{DeviceGrant, GrantSet, ServiceGrants},
};
use muster_proto::v1;

use crate::Error;

pub fn view(grants: &ServiceGrants) -> v1::ServiceGrants {
    v1::ServiceGrants {
        agents: grants
            .agents
            .as_ref()
            .map(|g| grant_view(&native_cli_rules(g.clone()))),
        terminals: grants.terminals.as_ref().map(grant_view),
        network: grants.network.as_ref().map(grant_view),
        desktop: grants.desktop.as_ref().map(grant_view),
        manages_desktop: false,
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
                keyboard: rule.keyboard,
                device_ids: rule.devices.iter().map(DeviceId::to_hex).collect(),
            })
            .collect(),
    }
}

/// Materialize historical aggregate rules before an owner edits them. In a
/// mixed policy, explicit native selectors take precedence for that device.
fn native_cli_rules(mut grants: GrantSet) -> GrantSet {
    if !grants
        .rules
        .iter()
        .any(|r| r.scope.agent_provider() == Some("orca"))
    {
        return grants;
    }
    let explicit = grants
        .rules
        .iter()
        .filter(|r| {
            r.scope
                .agent_provider()
                .is_some_and(|p| muster_core::apps::native_cli(p).is_some())
        })
        .flat_map(|r| r.devices.iter().copied())
        .collect::<std::collections::BTreeSet<_>>();
    let mut inherited = std::collections::BTreeSet::new();
    grants.rules.retain(|r| {
        if r.scope.agent_provider() == Some("orca") {
            inherited.extend(r.devices.difference(&explicit).copied());
            false
        } else {
            true
        }
    });
    if !inherited.is_empty() {
        grants.rules.extend(
            muster_core::apps::NATIVE_CLIS
                .iter()
                .map(|cli| DeviceGrant {
                    scope: Scope::AgentProvider(cli.provider.into()),
                    devices: inherited.clone(),
                    keyboard: false,
                }),
        );
    }
    grants
}

pub fn parse(view: &v1::ServiceGrants) -> Result<ServiceGrants, Error> {
    Ok(ServiceGrants {
        agents: parse_grants(view.agents.as_ref(), |scope| {
            matches!(scope, Scope::Agents | Scope::AgentProvider(_))
        })?
        .map(native_cli_rules),
        terminals: parse_grants(view.terminals.as_ref(), |scope| {
            matches!(
                scope,
                Scope::Terminals | Scope::TerminalView | Scope::TerminalControl
            )
        })?,
        desktop: parse_grants(view.desktop.as_ref(), |scope| {
            matches!(scope, Scope::DesktopView | Scope::DesktopMouse)
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
            if rule.keyboard && scope != Scope::DesktopMouse {
                return Err(Error::Invalid(
                    "keyboard requires device-specific desktop mouse access".into(),
                ));
            }
            Ok(DeviceGrant {
                scope,
                devices,
                keyboard: rule.keyboard,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(Some(GrantSet {
        all_devices: view.all_devices,
        rules,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editing_a_mixed_cli_assignment_cannot_restore_a_revoked_provider() {
        let a = DeviceId([1; 32]);
        let b = DeviceId([2; 32]);
        let mut grants = ServiceGrants::default();
        grants.agents = Some(GrantSet {
            all_devices: false,
            rules: vec![
                DeviceGrant {
                    scope: "agents:orca".parse().unwrap(),
                    devices: [a, b].into(),
                    keyboard: false,
                },
                DeviceGrant {
                    scope: "agents:codex-cli".parse().unwrap(),
                    devices: [a].into(),
                    keyboard: false,
                },
            ],
        });
        let mut displayed = view(&grants);
        let rules = &mut displayed.agents.as_mut().unwrap().rules;
        assert!(rules.iter().all(|r| r.scope != "agents:orca"));
        let targets = |provider: &str| -> std::collections::BTreeSet<String> {
            rules
                .iter()
                .filter(|r| r.scope == provider)
                .flat_map(|r| r.device_ids.clone())
                .collect()
        };
        assert_eq!(targets("agents:codex-cli"), [a.to_hex(), b.to_hex()].into());
        assert_eq!(targets("agents:claude-cli"), [b.to_hex()].into());
        for rule in rules {
            rule.device_ids.retain(|id| *id != a.to_hex());
        }
        let saved = parse(&displayed).unwrap().agents.unwrap();
        assert!(saved.rules.iter().all(|r| !r.devices.contains(&a)));
    }
}
