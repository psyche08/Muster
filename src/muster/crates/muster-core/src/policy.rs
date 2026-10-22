//! The one authorization function (ARCHITECTURE §3.4):
//!
//!   allowed = roster.active(subject)
//!           && roster.scopes(subject) grant permission
//!           && local_policy.allows(permission, target_state)
//!           && (!permission.high_risk || presence_proof)
//!
//! Service streams, forward streams and the vnet packet filter all call it.

use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::identity::DeviceId;
use crate::roster::{Role, State};
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
    Browser {
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
    /// The agent Locked Use gate also accepts paired phone identity signatures;
    /// browser and administrative callers continue to verify Presence.
    pub fn high_risk(&self) -> bool {
        matches!(self, Permission::LockedControl | Permission::NetworkAdmin)
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
            Permission::Browser { control } => has(if *control {
                Scope::BrowserControl
            } else {
                Scope::BrowserView
            }),
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
            Permission::Browser { control } => write!(f, "browser.{}", verb(*control)),
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

/// This machine's own say over what it offers. Local configuration, never
/// part of the roster.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalPolicy {
    /// Ports reachable over vnet and forward; empty means none.
    pub open_ports: Vec<PortRule>,
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
}

impl Default for LocalPolicy {
    fn default() -> Self {
        Self {
            open_ports: vec![PortRule {
                proto: Proto::Tcp,
                port: Some(22),
            }],
            allow_locked_control: false,
            // CatDesk is allowed explicitly per target, not by default.
            ax_allowlist: vec![
                crate::apps::CLAUDE.bundle_id.into(),
                crate::apps::CODEX.bundle_id.into(),
            ],
            offer_agent: true,
            offer_terminal: true,
            offer_tasks: false,
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

    fn allows(&self, wanted: &Permission, target: &TargetState) -> bool {
        // Chats are desktop apps' own conversations, offered with Agent Halo.
        let offered = match wanted {
            Permission::Chat { .. } | Permission::Agent { .. } | Permission::Browser { .. } => {
                self.offer_agent
            }
            Permission::Terminal { .. } => self.offer_terminal,
            Permission::LockedControl => self.allow_locked_control,
            Permission::NetworkAdmin => true,
            Permission::Net { proto, port } => self.port_open(*proto, *port),
        };
        offered
            && (!target.locked
                || wanted.high_risk()
                || matches!(wanted, Permission::Net { .. })
                || self.allow_locked_control)
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
    if !wanted.granted(&member.record.scopes, member.record.role) {
        return Decision::Deny("scope not granted");
    }
    if !policy.allows(wanted, target) {
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
        // Locked screen: viewing is fine, locked control needs the policy and presence.
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
        assert_eq!(
            decide(&Permission::LockedControl, &lenient, &locked),
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
