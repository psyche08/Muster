//! The one authorization function (ARCHITECTURE §3.4):
//!
//!   allowed = roster.active(subject)
//!           && the applicable member scope or target-owned grant allows it
//!           && local_policy.allows(permission, target_state)
//!           && (!permission.high_risk || presence_proof)
//!
//! Service streams, forward streams and the vnet packet filter all call it.

use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::identity::DeviceId;
use crate::roster::{NetworkAccessMode, Role, State};
use crate::scope::{Proto, Scope};

/// What one request asks of its caller. The roster holds `Scope`s; this is
/// the question put to them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Permission {
    /// A desktop app's chats from `provider`; `None` asks for any provider
    /// (a listing, filtered per provider afterwards).
    Chat {
        provider: Option<String>,
    },
    /// Coding-agent sessions from `provider`; `None` asks for any provider.
    /// `control` sends, answers and interrupts.
    Agent {
        provider: Option<String>,
        control: bool,
    },
    Terminal {
        control: bool,
    },
    Desktop {
        mouse: bool,
    },
    Browser {
        control: bool,
    },
    Multica {
        workspace: Option<String>,
        control: bool,
    },
    LockedControl,
    NetworkAdmin,
    Net {
        proto: Proto,
        port: u16,
    },
}

impl Permission {
    /// High-risk permissions need an additional proof at use time (§3.4).
    /// Administering the network is the one that still does: the prompt
    /// belongs to joining and to changing who may do what. `locked.control`
    /// left this set in ADR 0120 — the admin's grant at join is its
    /// authorization, and the requesting device has already authenticated
    /// its own user.
    pub fn high_risk(&self) -> bool {
        matches!(self, Permission::NetworkAdmin)
    }

    /// Whether a member with `scopes` and `role` holds this permission.
    pub fn granted(&self, scopes: &BTreeSet<Scope>, role: Role) -> bool {
        let has = |scope: Scope| scopes.contains(&scope);
        match self {
            Permission::Chat { provider } => {
                module_grants(scopes, Scope::Chats, provider, Scope::chat_provider)
            }
            Permission::Agent { provider, control } => {
                module_grants(scopes, Scope::Agents, provider, Scope::agent_provider)
                    || (has(Scope::Agents)
                        && legacy_cli_grants(
                            scopes.iter().filter_map(Scope::agent_provider),
                            provider.as_deref(),
                        ))
                    || has(if *control {
                        Scope::AgentControl
                    } else {
                        Scope::AgentView
                    })
            }
            Permission::Terminal { control } => {
                has(Scope::Terminals)
                    || has(if *control {
                        Scope::TerminalControl
                    } else {
                        Scope::TerminalView
                    })
            }
            Permission::Desktop { mouse } => {
                has(Scope::DesktopMouse) || (!mouse && has(Scope::DesktopView))
            }
            Permission::Browser { control } => has(if *control {
                Scope::BrowserControl
            } else {
                Scope::BrowserView
            }),
            Permission::Multica { control, .. } => {
                has(Scope::MulticaView) && (!control || has(Scope::MulticaControl))
            }
            Permission::LockedControl => has(Scope::LockedControl),
            Permission::NetworkAdmin => role == Role::Admin || has(Scope::NetworkAdmin),
            Permission::Net { proto, port } => {
                let wanted = Scope::Net {
                    proto: *proto,
                    port: Some(*port),
                };
                scopes.iter().any(|s| s.covers(&wanted))
            }
        }
    }
}

/// A module's provider scopes count only under its device-level switch;
/// `None` asks whether any provider is on.
/// A signed pre-split aggregate covered only the two native workers. A
/// narrower explicit native selection for this subject supersedes it.
fn legacy_cli_grants<'a>(held: impl Iterator<Item = &'a str>, wanted: Option<&str>) -> bool {
    let Some(wanted) = wanted else { return false };
    if wanted != "orca" && crate::apps::native_cli(wanted).is_none() {
        return false;
    }
    let mut legacy = false;
    let mut explicit = false;
    for provider in held {
        legacy |= provider == "orca";
        explicit |= crate::apps::native_cli(provider).is_some();
    }
    if wanted == "orca" {
        legacy || explicit
    } else {
        legacy && !explicit
    }
}

fn module_grants(
    scopes: &BTreeSet<Scope>,
    switch: Scope,
    wanted: &Option<String>,
    provider_of: fn(&Scope) -> Option<&str>,
) -> bool {
    scopes.contains(&switch)
        && scopes
            .iter()
            .filter_map(provider_of)
            .any(|held| wanted.as_deref().is_none_or(|p| p == held))
}

impl fmt::Display for Permission {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let verb = |control: bool| if control { "control" } else { "view" };
        match self {
            Permission::Chat { provider: None } => f.write_str("chats"),
            Permission::Chat { provider: Some(p) } => write!(f, "chats:{p}"),
            Permission::Agent {
                provider: None,
                control,
            } => write!(f, "agents ({})", verb(*control)),
            Permission::Agent {
                provider: Some(p),
                control,
            } => write!(f, "agents:{p} ({})", verb(*control)),
            Permission::Terminal { control } => write!(f, "terminals ({})", verb(*control)),
            Permission::Desktop { mouse } => f.write_str(if *mouse {
                "desktop.mouse"
            } else {
                "desktop.view"
            }),
            Permission::Browser { control } => write!(f, "browser.{}", verb(*control)),
            Permission::Multica { control, .. } => write!(f, "multica.{}", verb(*control)),
            Permission::LockedControl => f.write_str("locked.control"),
            Permission::NetworkAdmin => f.write_str("network.admin"),
            Permission::Net { proto, port } => write!(
                f,
                "{}",
                Scope::Net {
                    proto: *proto,
                    port: Some(*port)
                }
            ),
        }
    }
}

/// Receiver-owned rules. Absence of a category preserves its legacy grants.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceGrants {
    pub agents: Option<GrantSet>,
    pub terminals: Option<GrantSet>,
    pub network: Option<GrantSet>,
    pub desktop: Option<GrantSet>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrantSet {
    pub all_devices: bool,
    pub rules: Vec<DeviceGrant>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceGrant {
    #[serde(default)]
    pub keyboard: bool,
    pub scope: Scope,
    pub devices: BTreeSet<DeviceId>,
}

impl ServiceGrants {
    fn for_permission(&self, wanted: &Permission) -> Option<&GrantSet> {
        match wanted {
            Permission::Agent { .. } => self.agents.as_ref(),
            Permission::Terminal { .. } => self.terminals.as_ref(),
            Permission::Net { .. } => self.network.as_ref(),
            Permission::Desktop { .. } => self.desktop.as_ref(),
            _ => None,
        }
    }
}

impl GrantSet {
    fn allows(&self, subject: &DeviceId, wanted: &Permission, mode: NetworkAccessMode) -> bool {
        self.all_devices
            || matches!(wanted, Permission::Agent { provider, .. } if legacy_cli_grants(
                self.rules.iter().filter(|r| r.devices.contains(subject)).filter_map(|r| r.scope.agent_provider()),
                provider.as_deref(),
            ))
            || self.rules.iter().any(|rule| {
                if !rule.devices.contains(subject) {
                    return false;
                }
                match (&rule.scope, wanted) {
                    (Scope::DesktopMouse, Permission::Desktop { .. })
                    | (Scope::DesktopView, Permission::Desktop { mouse: false })
                    | (Scope::Agents, Permission::Agent { .. })
                    | (Scope::Terminals, Permission::Terminal { .. })
                    | (Scope::TerminalView, Permission::Terminal { control: false })
                    | (Scope::TerminalControl, Permission::Terminal { control: true }) => true,
                    (Scope::AgentProvider(granted), Permission::Agent { provider, .. }) => {
                        provider.as_ref().is_none_or(|wanted| wanted == granted)
                    }
                    (
                        Scope::Net {
                            proto: Proto::Any,
                            port: None,
                        },
                        Permission::Net { .. },
                    ) => true,
                    (scope @ Scope::Net { .. }, Permission::Net { proto, port })
                        if mode == NetworkAccessMode::Advanced =>
                    {
                        scope.covers(&Scope::Net {
                            proto: *proto,
                            port: Some(*port),
                        })
                    }
                    _ => false,
                }
            })
    }
}

/// This machine's own say over what it offers. Local configuration, never
/// part of the roster.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalPolicy {
    /// Ports reachable over vnet and forward; empty means none.
    pub open_ports: Vec<PortRule>,
    /// None retains the stored port offer; Basic uses an explicit offer as a switch.
    #[serde(default)]
    pub offer_network: Option<bool>,
    #[serde(default)]
    pub service_grants: ServiceGrants,
    /// Pair admissions attached to explicitly assigned desktop viewers.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub desktop_pairings: std::collections::BTreeMap<DeviceId, DesktopPairing>,
    pub allow_locked_control: bool,
    /// Bundle identifiers the AX controller may drive.
    pub ax_allowlist: Vec<String>,
    pub offer_agent: bool,
    pub offer_terminal: bool,
    /// Whether this machine offers itself as an external task scheduler's
    /// runtime (`muster acp`, ADR 0009). Unlike the two above it starts off:
    /// it admits work this machine's owner did not start, so it is a decision
    /// the owner makes rather than a default they inherit.
    #[serde(default)]
    pub offer_tasks: bool,
    /// Local-owner-only, default-off Multica sharing. Empty workspace set offers nothing.
    #[serde(default)]
    pub offer_multica: bool,
    #[serde(default)]
    pub multica_workspaces: BTreeSet<String>,
    /// The web window (ADR 0049): the one foreign origin this machine serves
    /// on a loopback port, for members with no route to it. `None` is off,
    /// which is how a machine starts — it carries somebody else's site, so
    /// the owner says which one, if any.
    #[serde(default)]
    pub web_window: Option<WebWindow>,
}

/// One origin this machine's browser fetches for members who cannot reach it.
/// Opening the window and letting a member through stay two decisions: the
/// member also needs `browser.view` to read through it and `browser.control`
/// to act, both under `offer_agent` and, while locked, `allow_locked_control`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebWindow {
    /// Scheme, host and port, no path: "https://example.test".
    pub origin: String,
}

impl Default for LocalPolicy {
    fn default() -> Self {
        Self {
            open_ports: vec![PortRule {
                proto: Proto::Tcp,
                port: Some(22),
            }],
            offer_network: None,
            service_grants: ServiceGrants::default(),
            desktop_pairings: Default::default(),
            allow_locked_control: false,
            // CatDesk is allowed explicitly per target, not by default.
            ax_allowlist: vec![
                crate::apps::CLAUDE.bundle_id.into(),
                crate::apps::CODEX.bundle_id.into(),
            ],
            offer_agent: true,
            offer_terminal: true,
            offer_tasks: false,
            offer_multica: false,
            multica_workspaces: BTreeSet::new(),
            web_window: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortRule {
    pub proto: Proto,
    /// `None` opens every port of the protocol.
    pub port: Option<u16>,
}

impl PortRule {
    pub fn matches(&self, proto: Proto, port: u16) -> bool {
        self.proto.covers(proto) && self.port.is_none_or(|p| p == port)
    }
}

impl LocalPolicy {
    pub fn port_open(&self, proto: Proto, port: u16) -> bool {
        self.open_ports.iter().any(|rule| rule.matches(proto, port))
    }

    fn allows(&self, wanted: &Permission, target: &TargetState, mode: NetworkAccessMode) -> bool {
        // Chats are desktop apps' own conversations, offered with Agent Halo.
        let offered = match wanted {
            Permission::Chat { .. } | Permission::Agent { .. } | Permission::Browser { .. } => {
                self.offer_agent
            }
            Permission::Desktop { .. } => !target.locked,
            Permission::Terminal { .. } => self.offer_terminal,
            Permission::Multica { workspace, .. } => {
                self.offer_multica
                    && !self.multica_workspaces.is_empty()
                    && workspace
                        .as_ref()
                        .is_none_or(|id| self.multica_workspaces.contains(id))
            }
            Permission::LockedControl => self.allow_locked_control,
            Permission::NetworkAdmin => true,
            Permission::Net { proto, port } => match self.offer_network {
                Some(false) => false,
                Some(true) if mode == NetworkAccessMode::Basic => true,
                _ => self.port_open(*proto, *port),
            },
        };
        offered
            && (!target.locked
                || wanted.high_risk()
                || matches!(wanted, Permission::Net { .. })
                || self.allow_locked_control)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesktopPairing {
    pub network: crate::roster::EntryHash,
    pub host: crate::roster::EntryHash,
    pub viewer: crate::roster::EntryHash,
}

impl DesktopPairing {
    pub fn current(state: &State, host: DeviceId, peer: DeviceId) -> Option<Self> {
        let viewer = state.active(&peer)?;
        if peer == host
            || !matches!(
                viewer.record.kind,
                crate::roster::DeviceKind::Desktop | crate::roster::DeviceKind::Phone
            )
        {
            return None;
        }
        Some(Self {
            network: state.network_id?,
            host: state.active(&host)?.joined_at?,
            viewer: viewer.joined_at?,
        })
    }
}

/// What the target machine is doing right now, as far as authorization cares.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TargetState {
    pub locked: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decision {
    Allow,
    /// Allowed once a presence proof for this operation verifies.
    NeedsPresence,
    Deny(&'static str),
}

impl Decision {
    pub fn is_allow(&self) -> bool {
        matches!(self, Decision::Allow)
    }
}

pub fn authorize(
    state: &State,
    subject: &DeviceId,
    wanted: &Permission,
    policy: &LocalPolicy,
    target: &TargetState,
) -> Decision {
    let Some(member) = state.active(subject) else {
        return Decision::Deny("not an active member");
    };
    let is_network = matches!(wanted, Permission::Net { .. });
    let mode = if is_network {
        state.network_access_mode()
    } else {
        NetworkAccessMode::Basic
    };
    let grants = policy.service_grants.for_permission(wanted);
    let member_granted = || {
        if is_network && mode == NetworkAccessMode::Basic {
            member.record.scopes.contains(&Scope::ALL_NET)
        } else {
            wanted.granted(&member.record.scopes, member.record.role)
        }
    };
    if (is_network || grants.is_none()) && !member_granted() {
        return Decision::Deny("scope not granted");
    }
    if matches!(wanted, Permission::Desktop { .. })
        && grants.is_some_and(|g| g.all_devices)
        && member.record.kind != crate::roster::DeviceKind::Desktop
    {
        return Decision::Deny("blanket desktop access covers desktops only");
    }
    if grants.is_some_and(|grants| !grants.allows(subject, wanted, mode)) {
        return Decision::Deny("device not assigned to this service");
    }
    if !policy.allows(wanted, target, mode) {
        return Decision::Deny("refused by local policy");
    }
    if wanted.high_risk() {
        return Decision::NeedsPresence;
    }
    Decision::Allow
}

/// Convenience for the vnet packet filter and forward streams.
pub fn authorize_port(
    state: &State,
    subject: &DeviceId,
    policy: &LocalPolicy,
    proto: Proto,
    port: u16,
) -> Decision {
    authorize(
        state,
        subject,
        &Permission::Net { proto, port },
        policy,
        &TargetState::default(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{HwSigner, SoftSigner};

    #[test]
    fn multica_is_independent_default_off_and_workspace_scoped() {
        let view = Permission::Multica {
            workspace: Some("workspace-a".into()),
            control: false,
        };
        let control = Permission::Multica {
            workspace: Some("workspace-a".into()),
            control: true,
        };
        let (roster, peer) = roster_with(&["multica.view"]);
        let mut policy = LocalPolicy::default();
        assert!(
            !authorize(
                roster.state(),
                &peer,
                &view,
                &policy,
                &TargetState::default()
            )
            .is_allow()
        );
        policy.offer_multica = true;
        assert!(
            !authorize(
                roster.state(),
                &peer,
                &view,
                &policy,
                &TargetState::default()
            )
            .is_allow()
        );
        policy.multica_workspaces.insert("workspace-a".into());
        assert!(
            authorize(
                roster.state(),
                &peer,
                &view,
                &policy,
                &TargetState::default()
            )
            .is_allow()
        );
        assert!(
            !authorize(
                roster.state(),
                &peer,
                &control,
                &policy,
                &TargetState::default()
            )
            .is_allow()
        );
        let other = Permission::Multica {
            workspace: Some("workspace-b".into()),
            control: false,
        };
        assert!(
            !authorize(
                roster.state(),
                &peer,
                &other,
                &policy,
                &TargetState::default()
            )
            .is_allow()
        );
        for scopes in [
            vec!["agents", "agents:deepseek", "network.admin"],
            vec!["multica.control"],
        ] {
            let (roster, peer) = roster_with(&scopes);
            assert!(
                !authorize(
                    roster.state(),
                    &peer,
                    &view,
                    &policy,
                    &TargetState::default()
                )
                .is_allow()
            );
            assert!(
                !authorize(
                    roster.state(),
                    &peer,
                    &control,
                    &policy,
                    &TargetState::default()
                )
                .is_allow()
            );
        }
        let (roster, peer) = roster_with(&["multica.view", "multica.control"]);
        assert!(
            authorize(
                roster.state(),
                &peer,
                &control,
                &policy,
                &TargetState::default()
            )
            .is_allow()
        );
    }

    /// Serving an external scheduler's tasks admits work this machine's owner
    /// did not start, so it waits to be switched on. The two services that
    /// only expose what the owner is already doing stay on.
    #[test]
    fn offering_this_machine_as_a_task_runtime_is_off_by_default() {
        let policy = LocalPolicy::default();
        assert!(!policy.offer_tasks);
        assert!(policy.offer_agent);
        assert!(policy.offer_terminal);
    }

    /// The window reuses the browser's own permission, so the browser's lock
    /// rule is the window's: viewing or acting while the screen is locked
    /// needs the owner to have offered control while locked.
    #[test]
    fn a_locked_machine_lends_its_browser_only_with_locked_control_on() {
        let (roster, phone) = roster_with(&["browser.view"]);
        let mut policy = LocalPolicy::default();
        assert!(policy.web_window.is_none());
        policy.web_window = Some(WebWindow {
            origin: "https://example.test".into(),
        });
        let read = Permission::Browser { control: false };
        let locked = TargetState { locked: true };

        assert!(
            authorize(
                roster.state(),
                &phone,
                &read,
                &policy,
                &TargetState::default()
            )
            .is_allow()
        );
        assert_eq!(
            authorize(roster.state(), &phone, &read, &policy, &locked),
            Decision::Deny("refused by local policy")
        );
        policy.allow_locked_control = true;
        assert!(authorize(roster.state(), &phone, &read, &policy, &locked).is_allow());
    }

    /// A stored policy written before the field existed must read as off, not
    /// as missing-and-therefore-whatever.
    #[test]
    fn a_policy_stored_without_the_field_reads_as_not_offering() {
        let stored = serde_json::json!({
            "open_ports": [],
            "allow_locked_control": false,
            "ax_allowlist": [],
            "offer_agent": true,
            "offer_terminal": true,
        });
        let policy: LocalPolicy = serde_json::from_value(stored).expect("older policy still loads");
        assert!(!policy.offer_tasks);
        assert!(policy.web_window.is_none());
    }

    use crate::roster::{Author, DeviceKind, DeviceRecord, Roster};
    use std::net::Ipv4Addr;

    /// A roster whose admin added one phone holding `scopes`.
    fn roster_with(scopes: &[&str]) -> (Roster, DeviceId) {
        let admin = SoftSigner::generate();
        let phone = SoftSigner::generate();
        let mut roster = Roster::new();
        let genesis = Author {
            roster: &roster,
            signer: &admin,
        }
        .genesis(
            "home",
            (Ipv4Addr::new(192, 168, 14, 0), 24),
            DeviceRecord::new(admin.public_key(), "a", DeviceKind::Desktop),
        )
        .unwrap();
        roster.insert(genesis).unwrap();
        let mut record = DeviceRecord::new(phone.public_key(), "p", DeviceKind::Phone);
        record
            .scopes
            .extend(scopes.iter().map(|s| s.parse::<Scope>().unwrap()));
        let add = Author {
            roster: &roster,
            signer: &admin,
        }
        .add(record)
        .unwrap();
        roster.insert(add).unwrap();
        (roster, phone.public_key().device_id())
    }

    fn agent(provider: Option<&str>, control: bool) -> Permission {
        Permission::Agent {
            provider: provider.map(str::to_string),
            control,
        }
    }

    fn chat(provider: Option<&str>) -> Permission {
        Permission::Chat {
            provider: provider.map(str::to_string),
        }
    }

    #[test]
    fn old_cli_grants_cover_only_native_workers_until_narrowed() {
        let subject = DeviceId([1; 32]);
        let other = DeviceId([2; 32]);
        for (native, codex, claude) in [
            (vec!["orca"], true, true),
            (vec!["orca", "codex-cli"], true, false),
            (vec!["claude-cli"], false, true),
            (vec![], false, false),
        ] {
            let scopes = std::iter::once(Scope::Agents)
                .chain(native.iter().map(|id| Scope::AgentProvider((*id).into())))
                .collect();
            let mut grants = GrantSet {
                all_devices: false,
                rules: native
                    .iter()
                    .map(|id| DeviceGrant {
                        scope: Scope::AgentProvider((*id).into()),
                        devices: [subject].into(),
                        keyboard: false,
                    })
                    .collect(),
            };
            grants.rules.push(DeviceGrant {
                scope: Scope::AgentProvider("claude-cli".into()),
                devices: [other].into(),
                keyboard: false,
            });
            for (name, expected) in [
                ("codex-cli", codex),
                ("claude-cli", claude),
                ("orca-ide", false),
                ("codex", false),
            ] {
                let wanted = agent(Some(name), true);
                assert_eq!(wanted.granted(&scopes, Role::Member), expected);
                assert_eq!(
                    grants.allows(&subject, &wanted, NetworkAccessMode::Basic),
                    expected
                );
            }
        }
    }

    #[test]
    fn owner_assignments_gate_service_and_selector() {
        let (roster, peer) = roster_with(&["net:*"]);
        let grant = |scope: &str| GrantSet {
            all_devices: false,
            rules: vec![DeviceGrant {
                keyboard: false,
                scope: scope.parse().unwrap(),
                devices: BTreeSet::from([peer]),
            }],
        };
        let mut policy = LocalPolicy {
            offer_network: Some(true),
            service_grants: ServiceGrants {
                agents: Some(grant("agents:codex")),
                terminals: Some(grant("terminal.view")),
                network: Some(grant("net:*")),
                ..Default::default()
            },
            ..LocalPolicy::default()
        };
        let decide = |wanted: Permission| {
            authorize(
                roster.state(),
                &peer,
                &wanted,
                &policy,
                &TargetState::default(),
            )
        };
        assert!(decide(agent(Some("codex"), true)).is_allow());
        assert!(!decide(agent(Some("claude"), false)).is_allow());
        assert!(decide(Permission::Terminal { control: false }).is_allow());
        assert!(!decide(Permission::Terminal { control: true }).is_allow());
        assert!(
            !authorize(
                roster.state(),
                &SoftSigner::generate().public_key().device_id(),
                &agent(Some("codex"), false),
                &policy,
                &TargetState::default(),
            )
            .is_allow()
        );
        assert!(authorize_port(roster.state(), &peer, &policy, Proto::Udp, 2049).is_allow());
        policy.service_grants.network = Some(grant("net:udp/2049"));
        assert!(!authorize_port(roster.state(), &peer, &policy, Proto::Udp, 2049).is_allow());

        let mut advanced = roster.state().clone();
        advanced.network_access = Some(NetworkAccessMode::Advanced);
        policy.open_ports.push(PortRule {
            proto: Proto::Udp,
            port: Some(2049),
        });
        assert!(authorize_port(&advanced, &peer, &policy, Proto::Udp, 2049).is_allow());
        assert!(!authorize_port(&advanced, &peer, &policy, Proto::Tcp, 22).is_allow());
        policy.offer_network = Some(false);
        assert!(!authorize_port(&advanced, &peer, &policy, Proto::Udp, 2049).is_allow());
    }

    #[test]
    fn scope_policy_and_presence() {
        let (roster, phone_id) = roster_with(&["agent.view", "net:tcp/22", "locked.control"]);
        let policy = LocalPolicy::default();
        let unlocked = TargetState::default();
        let locked = TargetState { locked: true };
        let decide = |wanted: &Permission, policy: &LocalPolicy, target: &TargetState| {
            authorize(roster.state(), &phone_id, wanted, policy, target)
        };

        // Legacy grants keep their meaning: agent.view reads every provider.
        assert_eq!(
            decide(&agent(Some("codex"), false), &policy, &unlocked),
            Decision::Allow
        );
        assert_eq!(
            decide(&agent(None, false), &policy, &unlocked),
            Decision::Allow
        );
        assert_eq!(
            decide(&agent(Some("codex"), true), &policy, &unlocked),
            Decision::Deny("scope not granted")
        );
        assert_eq!(
            authorize_port(roster.state(), &phone_id, &policy, Proto::Tcp, 22),
            Decision::Allow
        );
        assert_eq!(
            authorize_port(roster.state(), &phone_id, &policy, Proto::Tcp, 5900),
            Decision::Deny("scope not granted")
        );
        // Granted but the target does not open it.
        let mut strict = policy.clone();
        strict.open_ports.clear();
        assert_eq!(
            authorize_port(roster.state(), &phone_id, &strict, Proto::Tcp, 22),
            Decision::Deny("refused by local policy")
        );
        // Locked screen: viewing is fine, locked control needs the local policy.
        assert_eq!(
            decide(&agent(Some("codex"), false), &policy, &locked),
            Decision::Deny("refused by local policy")
        );
        let mut lenient = policy.clone();
        lenient.allow_locked_control = true;
        assert_eq!(
            decide(&agent(Some("codex"), false), &lenient, &locked),
            Decision::Allow
        );
        // ADR 0120: the admin's grant is the authorization; no per-operation
        // signature. Administering the network still asks.
        assert_eq!(
            decide(&Permission::LockedControl, &lenient, &locked),
            Decision::Allow
        );
        assert_eq!(
            decide(&Permission::LockedControl, &policy, &locked),
            Decision::Deny("refused by local policy")
        );
        let (admin_roster, admin_id) = roster_with(&["network.admin"]);
        assert_eq!(
            authorize(
                admin_roster.state(),
                &admin_id,
                &Permission::NetworkAdmin,
                &policy,
                &unlocked
            ),
            Decision::NeedsPresence
        );
        assert!(matches!(
            authorize(
                roster.state(),
                &SoftSigner::generate().public_key().device_id(),
                &agent(None, false),
                &policy,
                &unlocked
            ),
            Decision::Deny(_)
        ));
    }

    #[test]
    fn modules_need_their_switch_and_a_provider() {
        let policy = LocalPolicy::default();
        let unlocked = TargetState::default();

        let (roster, phone) = roster_with(&["agents", "agents:codex", "chats:claude", "terminals"]);
        let decide =
            |wanted: &Permission| authorize(roster.state(), &phone, wanted, &policy, &unlocked);
        // Agents: switch on, Codex only; view and control alike.
        assert_eq!(decide(&agent(Some("codex"), false)), Decision::Allow);
        assert_eq!(decide(&agent(Some("codex"), true)), Decision::Allow);
        assert_eq!(decide(&agent(None, false)), Decision::Allow);
        assert!(!decide(&agent(Some("claude"), false)).is_allow());
        assert!(!decide(&agent(Some("claude"), true)).is_allow());
        // Chats: a provider without the switch grants nothing.
        assert!(!decide(&chat(Some("claude"))).is_allow());
        assert!(!decide(&chat(None)).is_allow());
        // Terminals: one switch for viewing and typing.
        assert!(decide(&Permission::Terminal { control: false }).is_allow());
        assert!(decide(&Permission::Terminal { control: true }).is_allow());
        // Nothing else rides along.
        assert!(!decide(&Permission::Browser { control: false }).is_allow());
        assert!(!decide(&Permission::LockedControl).is_allow());

        // The switch alone, with every provider off, opens nothing.
        let (roster, phone) = roster_with(&["agents", "chats", "chats:chatgpt"]);
        let decide =
            |wanted: &Permission| authorize(roster.state(), &phone, wanted, &policy, &unlocked);
        assert!(!decide(&agent(None, false)).is_allow());
        assert!(!decide(&agent(Some("codex"), false)).is_allow());
        assert!(decide(&chat(Some("chatgpt"))).is_allow());
        assert!(decide(&chat(None)).is_allow());
        assert!(!decide(&chat(Some("claude"))).is_allow());
        assert!(!decide(&Permission::Terminal { control: false }).is_allow());

        // A target that stops offering agents refuses chats too.
        let mut closed = policy.clone();
        closed.offer_agent = false;
        assert_eq!(
            authorize(
                roster.state(),
                &phone,
                &chat(Some("chatgpt")),
                &closed,
                &unlocked
            ),
            Decision::Deny("refused by local policy")
        );
    }
}
