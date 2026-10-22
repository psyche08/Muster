//! What a member may do as an accessor (ARCHITECTURE §3.4). Stored in the
//! roster as strings so the set can grow without a format change.
//!
//! An admin grants each device three modules, each with a device-level
//! switch: Chats and Agents, narrowed per provider (`chats:<provider>`,
//! `agents:<provider>`), and Terminals. A provider scope counts only while
//! its module's switch is held, so switching a module off keeps the provider
//! choice for when it comes back on. `locked.control`, `browser.*`, `net:*`
//! and `network.admin` stand on their own.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Proto {
    Tcp,
    Udp,
    Any,
}

impl Proto {
    pub fn covers(self, other: Proto) -> bool {
        self == Proto::Any || self == other
    }
}

/// The roster writes a record's scopes in this enum's derived order and a
/// decoder insists on byte-identical re-encoding (PROTOCOL §3.1). Existing
/// variants therefore keep their relative order; new ones go at the end.
/// Providers stay text, so a member may hold one this build does not know:
/// it parses, sorts the same everywhere, and matches nothing here.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum Scope {
    BrowserView,
    BrowserControl,
    /// Legacy: every agent provider, read only. Honoured, no longer offered.
    AgentView,
    /// Legacy: every agent provider, send / approve / interrupt.
    AgentControl,
    /// Legacy: every terminal, read only.
    TerminalView,
    /// Legacy: every terminal, input / create / resize / close.
    TerminalControl,
    LockedControl,
    NetworkAdmin,
    /// Reach `proto`/`port` on the target (vnet packets and forward streams alike);
    /// `port: None` means every port of that protocol.
    Net {
        proto: Proto,
        port: Option<u16>,
    },
    /// The Chats module's device-level switch.
    Chats,
    /// Chats from one provider; needs `Chats`.
    ChatProvider(String),
    /// The Agents module's device-level switch.
    Agents,
    /// Agent sessions from one provider (view and control); needs `Agents`.
    AgentProvider(String),
    /// Every terminal session, to view and to type into.
    Terminals,
}

impl Scope {
    pub const ALL_NET: Scope = Scope::Net {
        proto: Proto::Any,
        port: None,
    };

    /// High-risk scopes need a presence proof at use time (§3.4).
    pub fn high_risk(&self) -> bool {
        matches!(self, Scope::LockedControl | Scope::NetworkAdmin)
    }

    /// The provider of a `chats:<provider>` scope.
    pub fn chat_provider(&self) -> Option<&str> {
        match self {
            Scope::ChatProvider(p) => Some(p),
            _ => None,
        }
    }

    /// The provider of an `agents:<provider>` scope.
    pub fn agent_provider(&self) -> Option<&str> {
        match self {
            Scope::AgentProvider(p) => Some(p),
            _ => None,
        }
    }

    /// Grants from before the Chats / Agents / Terminals modules.
    pub fn legacy(&self) -> bool {
        matches!(
            self,
            Scope::AgentView | Scope::AgentControl | Scope::TerminalView | Scope::TerminalControl
        )
    }

    /// Whether holding `self` satisfies a request for `wanted`.
    pub fn covers(&self, wanted: &Scope) -> bool {
        match (self, wanted) {
            (
                Scope::Net { proto, port },
                Scope::Net {
                    proto: wp,
                    port: wport,
                },
            ) => proto.covers(*wp) && (port.is_none() || port == wport),
            (a, b) => a == b,
        }
    }
}

impl fmt::Display for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Scope::BrowserView => f.write_str("browser.view"),
            Scope::BrowserControl => f.write_str("browser.control"),
            Scope::AgentView => f.write_str("agent.view"),
            Scope::AgentControl => f.write_str("agent.control"),
            Scope::TerminalView => f.write_str("terminal.view"),
            Scope::TerminalControl => f.write_str("terminal.control"),
            Scope::LockedControl => f.write_str("locked.control"),
            Scope::NetworkAdmin => f.write_str("network.admin"),
            Scope::Chats => f.write_str("chats"),
            Scope::ChatProvider(provider) => write!(f, "chats:{provider}"),
            Scope::Agents => f.write_str("agents"),
            Scope::AgentProvider(provider) => write!(f, "agents:{provider}"),
            Scope::Terminals => f.write_str("terminals"),
            Scope::Net {
                proto: Proto::Any,
                port: None,
            } => f.write_str("net:*"),
            Scope::Net { proto, port } => {
                let proto = match proto {
                    Proto::Tcp => "tcp",
                    Proto::Udp => "udp",
                    Proto::Any => "*",
                };
                match port {
                    Some(port) => write!(f, "net:{proto}/{port}"),
                    None => write!(f, "net:{proto}/*"),
                }
            }
        }
    }
}

impl FromStr for Scope {
    type Err = Error;

    fn from_str(text: &str) -> Result<Self, Error> {
        Ok(match text {
            "browser.view" => Scope::BrowserView,
            "browser.control" => Scope::BrowserControl,
            "agent.view" => Scope::AgentView,
            "agent.control" => Scope::AgentControl,
            "terminal.view" => Scope::TerminalView,
            "terminal.control" => Scope::TerminalControl,
            "locked.control" => Scope::LockedControl,
            "network.admin" => Scope::NetworkAdmin,
            "chats" => Scope::Chats,
            "agents" => Scope::Agents,
            "terminals" => Scope::Terminals,
            "net:*" => Scope::ALL_NET,
            other if other.starts_with("chats:") => Scope::ChatProvider(provider(other, "chats:")?),
            other if other.starts_with("agents:") => {
                Scope::AgentProvider(provider(other, "agents:")?)
            }
            other => {
                let rest = other
                    .strip_prefix("net:")
                    .ok_or_else(|| Error::Malformed(format!("unknown scope {other:?}")))?;
                let (proto, port) = rest.split_once('/').ok_or_else(|| {
                    Error::Malformed(format!("net scope wants proto/port: {other:?}"))
                })?;
                let proto = match proto {
                    "tcp" => Proto::Tcp,
                    "udp" => Proto::Udp,
                    "*" => Proto::Any,
                    _ => return Err(Error::Malformed(format!("unknown protocol in {other:?}"))),
                };
                let port = if port == "*" {
                    None
                } else {
                    Some(
                        port.parse()
                            .map_err(|_| Error::Malformed(format!("bad port in {other:?}")))?,
                    )
                };
                Scope::Net { proto, port }
            }
        })
    }
}

/// A provider id: 1–32 of `[a-z0-9-]`, so it can never smuggle a separator.
fn provider(text: &str, prefix: &str) -> Result<String, Error> {
    let id = &text[prefix.len()..];
    if id.is_empty()
        || id.len() > 32
        || !id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err(Error::Malformed(format!("bad provider in {text:?}")));
    }
    Ok(id.to_string())
}

impl TryFrom<String> for Scope {
    type Error = Error;
    fn try_from(value: String) -> Result<Self, Error> {
        value.parse()
    }
}

impl From<Scope> for String {
    fn from(scope: Scope) -> String {
        scope.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_cover() {
        for text in [
            "agent.view",
            "net:*",
            "net:tcp/22",
            "net:udp/*",
            "network.admin",
            "chats",
            "chats:chatgpt",
            "agents",
            "agents:codex",
            "agents:some-future-app",
            "terminals",
        ] {
            let scope: Scope = text.parse().unwrap();
            assert_eq!(scope.to_string(), text);
        }
        let ssh: Scope = "net:tcp/22".parse().unwrap();
        assert!(Scope::ALL_NET.covers(&ssh));
        assert!("net:tcp/*".parse::<Scope>().unwrap().covers(&ssh));
        assert!(!"net:udp/*".parse::<Scope>().unwrap().covers(&ssh));
        assert!(!"net:tcp/23".parse::<Scope>().unwrap().covers(&ssh));
        assert!(!Scope::AgentView.covers(&Scope::AgentControl));
        for bad in [
            "agents:",
            "agents:Codex",
            "chats:a/b",
            "agents:codex:x",
            "chat",
        ] {
            assert!(bad.parse::<Scope>().is_err(), "{bad}");
        }
    }

    /// Records signed before the modules existed must re-encode byte for byte.
    #[test]
    fn existing_scopes_keep_their_order_and_new_ones_follow() {
        let order: Vec<Scope> = [
            "browser.view",
            "browser.control",
            "agent.view",
            "agent.control",
            "terminal.view",
            "terminal.control",
            "locked.control",
            "network.admin",
            "net:tcp/22",
            "net:*",
            "chats",
            "chats:chatgpt",
            "chats:claude",
            "agents",
            "agents:claude",
            "agents:codex",
            "terminals",
        ]
        .iter()
        .map(|s| s.parse().unwrap())
        .collect();
        let mut sorted = order.clone();
        sorted.sort();
        assert_eq!(sorted, order);
    }
}
