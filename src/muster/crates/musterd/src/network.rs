//! The Network service: create, invite, join, approve, remove, scopes and
//! policy. Every mutation authors a roster entry (or answers a pairing) and
//! persists it; the link learns of it through the shared roster.

use std::collections::BTreeSet;
use std::net::Ipv4Addr;
use std::sync::Arc;

use muster_core::roster::{Author, DeviceUpdate, Role};
use muster_core::{DeviceId, Invite, LocalPolicy, PortRule, Proto, Scope};
use muster_net::pair::Verdict;
use muster_proto::v1;

use crate::{Daemon, Error, kind_str};

pub const DEFAULT_CIDR: (Ipv4Addr, u8) = (Ipv4Addr::new(192, 168, 13, 0), 24);
pub const DEFAULT_INVITE_TTL: u64 = 600;

impl Daemon {
    pub fn readdress_network(
        &self,
        cidr: &str,
        addresses: &std::collections::HashMap<String, String>,
    ) -> Result<(), Error> {
        let cidr = parse_cidr(cidr)?;
        let addresses = addresses
            .iter()
            .map(|(id, ip)| {
                Ok((
                    DeviceId::from_hex(id).map_err(|e| Error::Invalid(e.to_string()))?,
                    ip.parse::<Ipv4Addr>()
                        .map_err(|e| Error::Invalid(e.to_string()))?,
                ))
            })
            .collect::<Result<std::collections::BTreeMap<_, _>, Error>>()?;
        let entry = {
            let roster = self.roster().read().unwrap();
            let entry = Author {
                roster: &roster,
                signer: self.signer().as_ref(),
            }
            .readdress(cidr, addresses)?;
            let mut checked = roster.clone();
            checked.insert(entry.clone())?;
            entry
        };
        // Persist before publication so a successful migration survives restart.
        self.store().append_entries(std::slice::from_ref(&entry))?;
        self.node().publish(entry.clone())?;
        self.persist(&[entry]);
        self.stop_stale_vnet();
        Ok(())
    }
    /// This device is an active member of a network.
    pub fn has_network(&self) -> bool {
        let roster = self.roster().read().unwrap();
        !roster.is_empty() && roster.state().active(&self.device_id()).is_some()
    }

    /// A roster this device was removed from is dropped before it creates
    /// or joins again (clean re-pair, D2).
    fn drop_dead_roster(&self) -> Result<(), Error> {
        let dead = {
            let roster = self.roster().read().unwrap();
            !roster.is_empty() && roster.state().active(&self.device_id()).is_none()
        };
        if dead {
            log::info!("dropping the roster this device was removed from");
            self.store().clear_roster()?;
            *self.roster().write().unwrap() = muster_core::roster::Roster::new();
            let _ = self.changed.send(());
        }
        Ok(())
    }

    pub fn network_status(&self) -> v1::NetworkStatus {
        let node = self.node();
        let peers = node.peers();
        let me = self.device_id();
        let roster = self.roster().read().unwrap();
        let state = roster.state();
        let members = state
            .active_members()
            .map(|m| {
                let peer = peers.iter().find(|p| p.id == m.record.device_id);
                let build = if m.record.device_id == me {
                    Some(node.build())
                } else {
                    peer.and_then(|p| p.build.clone())
                };
                let mut view = v1::MemberView {
                    device_id: m.record.device_id.to_hex(),
                    fingerprint: m.record.device_id.fingerprint(),
                    name: m.record.name.clone(),
                    kind: kind_str(m.record.kind).into(),
                    role: match m.record.role {
                        Role::Admin => "admin".into(),
                        Role::Member => "member".into(),
                    },
                    vip: m.record.vip.map(|v| v.to_string()).unwrap_or_default(),
                    scopes: m.record.scopes.iter().map(|s| s.to_string()).collect(),
                    is_self: m.record.device_id == me,
                    // Online from evidence: a link lost inside its retries
                    // is not a disconnection (muster_net::presence).
                    connected: peer.is_some_and(|p| p.presence.online()),
                    presence: peer
                        .map(|p| p.presence.as_str().to_owned())
                        .unwrap_or_default(),
                    connection_status: peer
                        .map(|p| p.connection_status.clone())
                        .unwrap_or_default(),
                    path: peer
                        .and_then(|p| p.path.as_ref())
                        .map(|path| {
                            if path.direct.is_some() {
                                "direct"
                            } else {
                                "relay"
                            }
                        })
                        .unwrap_or("")
                        .into(),
                    rtt_ms: peer
                        .and_then(|p| p.rtt)
                        .map(|d| d.as_millis() as u32)
                        .unwrap_or(0),
                    version: build
                        .as_ref()
                        .map(|b| b.version.clone())
                        .unwrap_or_default(),
                    commit: build.map(|b| b.commit).unwrap_or_default(),
                    link: String::new(),
                };
                view.link = link_label(&view);
                view
            })
            .collect();
        v1::NetworkStatus {
            has_network: !roster.is_empty() && state.active(&me).is_some(),
            network_id: roster.network_id().map(|n| n.to_hex()).unwrap_or_default(),
            name: state.name.clone(),
            vnet_cidr: state
                .vnet_cidr
                .map(|(ip, bits)| format!("{ip}/{bits}"))
                .unwrap_or_default(),
            self_id: me.to_hex(),
            members,
            relay: format!("{:?}", node.relay_state()).to_ascii_lowercase(),
            reflected_addr: node.reflected().map(|a| a.to_string()).unwrap_or_default(),
            pending_joins: self.pending.lock().unwrap().len() as u32,
        }
    }

    pub fn device_state(&self) -> v1::DeviceState {
        let vip = self
            .roster()
            .read()
            .unwrap()
            .state()
            .active(&self.device_id())
            .and_then(|m| m.record.vip)
            .map(|v| v.to_string())
            .unwrap_or_default();
        let (manual_locked_use_owner, manual_locked_use_nonce, manual_locked_use_active) =
            self.locked_use.manual_status();
        v1::DeviceState {
            manual_locked_use_owner,
            manual_locked_use_nonce,
            manual_locked_use_active,
            locked: self.platform().locked(),
            on_console: self.platform().on_console(),
            covered: self.covered(),
            iterm2_automation: {
                #[cfg(target_os = "macos")]
                {
                    muster_platform::macos::automation::iterm_permission(false).into()
                }
                #[cfg(not(target_os = "macos"))]
                {
                    String::new()
                }
            },
            iterm2_api_available: self
                .terminals()
                .any(|t| t.source() == "iterm2" && t.available()),
            terminal_source: self
                .terminal()
                .map(|t| t.source().to_owned())
                .unwrap_or_default(),
            ghostty_automation: {
                #[cfg(target_os = "macos")]
                {
                    if self.terminals().any(|t| t.source() == "ghostty") {
                        muster_platform::macos::ghostty::permission(false).into()
                    } else {
                        String::new()
                    }
                }
                #[cfg(not(target_os = "macos"))]
                {
                    String::new()
                }
            },
            locked_use_setup: if cfg!(target_os = "macos") {
                match self.plugin_setup.load(std::sync::atomic::Ordering::Relaxed) {
                    1 => "checking",
                    2 if std::path::Path::new(
                        "/Library/Security/SecurityAgentPlugins/MusterLockedUse.bundle",
                    )
                    .is_dir() =>
                    {
                        "ready"
                    }
                    2 | 3 => "needs_installation",
                    _ => "unavailable",
                }
                .into()
            } else {
                String::new()
            },
            accessibility: match self.platform().accessibility() {
                None => String::new(),
                Some(true) => "granted".into(),
                Some(false) => "missing".into(),
            },
            vip,
            vnet_up: self.vnet().is_some(),
            relay: format!("{:?}", self.node().relay_state()).to_ascii_lowercase(),
            uptime_secs: self.uptime_secs(),
        }
    }

    pub fn device_info(&self) -> v1::DeviceInfo {
        let id = self.device_id();
        let mut services = Vec::new();
        if cfg!(target_os = "macos") {
            services.push("locked.device.v1".to_string());
        }
        if self.agent().is_some() {
            services.push("agent".to_string());
            if self.agent().is_some_and(|a| a.orca_source().is_ok()) {
                services.push("agent.orca.v1".to_string());
            }
            services.push("agent.attachments".to_string());
            services.push("agent.questions.v1".to_string());
            if cfg!(target_os = "macos") {
                // Desktop Chat mode (Claude / ChatGPT apps through Accessibility).
                services.push("agent.chat".to_string());
                services.push("agent.chat.catalog.v1".to_string());
                services.push("agent.chat.options".to_string());
                services.push("agent.options.v1".to_string());
                services.push("agent.completion.v1".to_string());
                services.push("agent.chat.locked.v1".to_string());
            }
        }
        if self.terminal().is_some() {
            services.push("terminal".to_string());
        }
        services.push("presence.enrollment.v1".to_string());
        services.push("forward".to_string());
        if self.vnet().is_some() {
            services.push("vnet".to_string());
        }
        v1::DeviceInfo {
            device_id: id.to_hex(),
            fingerprint: id.fingerprint(),
            name: self.name(),
            kind: kind_str(self.kind()).into(),
            version: self.build().version,
            commit: self.build().commit,
            platform: std::env::consts::OS.into(),
            state: Some(self.device_state()),
            services,
            relay: self.relay_text().unwrap_or_default(),
            key_backend: self.signer().backend().into(),
            key_label: key_label(self.signer().backend()).into(),
        }
    }

    pub async fn create_network(self: &Arc<Self>, name: &str, cidr: &str) -> Result<(), Error> {
        if self.has_network() {
            return Err(Error::Conflict(
                "this device is already in a network".into(),
            ));
        }
        self.drop_dead_roster()?;
        let name = name.trim();
        if name.is_empty() {
            return Err(Error::Invalid("network name is required".into()));
        }
        let cidr = if cidr.trim().is_empty() {
            DEFAULT_CIDR
        } else {
            parse_cidr(cidr)?
        };
        let record = self.own_record();
        let entry = {
            let roster = self.roster().read().unwrap();
            Author {
                roster: &roster,
                signer: self.signer().as_ref(),
            }
            .genesis(name, cidr, record)?
        };
        // Through the node like every other entry: it learns the vnet prefix
        // (never a direct-path candidate) and deposits the genesis in the
        // relay mailbox.
        self.node().publish(entry.clone())?;
        self.persist(&[entry]);
        log::info!("created network {name} ({}/{})", cidr.0, cidr.1);
        if self.config.vnet {
            self.maybe_start_vnet().await;
        }
        Ok(())
    }

    pub fn make_invite(&self, ttl_secs: u32) -> Result<Invite, Error> {
        let (network_id, name) = {
            let roster = self.roster().read().unwrap();
            let Some(network_id) = roster.network_id() else {
                return Err(Error::NoNetwork);
            };
            (network_id, roster.state().name.clone())
        };
        if !self
            .roster()
            .read()
            .unwrap()
            .state()
            .is_admin(&self.device_id())
        {
            return Err(Error::Unauthorized("only an admin can invite".into()));
        }
        let Some(relay) = self.relay_text() else {
            return Err(Error::Invalid(
                "no relay configured; start musterd with --relay".into(),
            ));
        };
        let ttl = if ttl_secs == 0 {
            DEFAULT_INVITE_TTL
        } else {
            ttl_secs as u64
        };
        let invite = Invite::new(network_id, &name, self.signer().public_key(), &relay, ttl);
        self.node().open_invite(invite.clone());
        *self.invite.lock().unwrap() = Some(invite.clone());
        Ok(invite)
    }

    /// Joins by invite text; `progress` is told each phase.
    pub async fn join(
        self: &Arc<Self>,
        invite_text: &str,
        device_name: &str,
        device_kind: &str,
        progress: &(dyn Fn(v1::join_progress::Phase, String) + Send + Sync),
    ) -> Result<(), Error> {
        if self.has_network() {
            return Err(Error::Conflict(
                "this device is already in a network".into(),
            ));
        }
        self.drop_dead_roster()?;
        let invite = Invite::decode(invite_text.trim())
            .map_err(|e| Error::Invalid(format!("invite: {e}")))?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if invite.expired(now) {
            return Err(Error::Invalid("the invite has expired".into()));
        }
        let kind = if device_kind.is_empty() {
            self.kind()
        } else {
            crate::parse_kind(device_kind)
                .ok_or_else(|| Error::Invalid("kind must be desktop, phone or server".into()))?
        };
        self.set_identity_labels(device_name, kind);
        if self.relay_text().as_deref() != Some(invite.relay.as_str()) {
            progress(
                v1::join_progress::Phase::Connecting,
                format!("connecting to relay {}", invite.relay),
            );
            self.restart_node(&invite.relay).await?;
        }
        progress(
            v1::join_progress::Phase::Connecting,
            format!("reaching {}", invite.admin_id.short()),
        );
        let record = self.own_record();
        // Pairing has no separate "waiting" callback: the admin's approval
        // is what completes it, so report that phase once we are connected.
        progress(
            v1::join_progress::Phase::WaitingApproval,
            "waiting for the admin to approve".into(),
        );
        let node = self.node();
        node.join(&invite, record).await?;
        let all: Vec<_> = self.roster().read().unwrap().entries().cloned().collect();
        self.persist(&all);
        progress(
            v1::join_progress::Phase::Joined,
            format!("joined {}", invite.network_name),
        );
        if self.config.vnet {
            self.maybe_start_vnet().await;
        }
        Ok(())
    }

    pub fn pending_joins(&self) -> v1::PendingJoins {
        let pending = self.pending.lock().unwrap();
        let mut joins: Vec<_> = pending
            .values()
            .map(|p| v1::PendingJoin {
                invite_id: hex::encode(p.candidate.invite_id),
                device_id: p.candidate.record.device_id.to_hex(),
                fingerprint: p.candidate.record.device_id.fingerprint(),
                name: p.candidate.record.name.clone(),
                kind: kind_str(p.candidate.record.kind).into(),
            })
            .collect();
        joins.sort_by(|a, b| a.device_id.cmp(&b.device_id));
        v1::PendingJoins { joins }
    }

    pub fn decide(
        &self,
        device_id: &str,
        approve: bool,
        scopes: &[String],
        reason: &str,
    ) -> Result<(), Error> {
        let id =
            DeviceId::from_hex(device_id).map_err(|e| Error::Invalid(format!("device id: {e}")))?;
        let mut pending = self.pending.lock().unwrap();
        let Some(mut join) = pending.remove(&id) else {
            return Err(Error::NotFound(format!(
                "no pending join from {}",
                id.short()
            )));
        };
        let reply = join.reply.take().expect("reply present until decided");
        let verdict = if approve {
            let mut record = join.candidate.record.clone();
            if !scopes.is_empty() {
                record.scopes = parse_scopes(scopes)?;
            }
            log::info!(
                "approved {} ({}) with {:?}",
                record.name,
                id.short(),
                record.scopes
            );
            Verdict::Approve(record)
        } else {
            log::info!("rejected {}: {reason}", id.short());
            Verdict::Reject(if reason.is_empty() {
                "rejected by admin".into()
            } else {
                reason.to_string()
            })
        };
        let _ = reply.send(verdict);
        drop(pending);
        let _ = self.changed.send(());
        Ok(())
    }

    pub fn remove_member(self: &Arc<Self>, device_id: &str) -> Result<(), Error> {
        let id =
            DeviceId::from_hex(device_id).map_err(|e| Error::Invalid(format!("device id: {e}")))?;
        if id == self.device_id() {
            return Err(Error::Invalid("a device cannot remove itself".into()));
        }
        let entry = {
            let roster = self.roster().read().unwrap();
            if roster.state().active(&id).is_none() {
                return Err(Error::NotFound(format!(
                    "{} is not an active member",
                    id.short()
                )));
            }
            Author {
                roster: &roster,
                signer: self.signer().as_ref(),
            }
            .remove(id)?
        };
        self.node().publish(entry.clone())?;
        self.persist(&[entry]);
        log::info!("removed {}", id.short());
        Ok(())
    }

    pub fn set_scopes(
        self: &Arc<Self>,
        device_id: &str,
        scopes: &[String],
        role: &str,
    ) -> Result<(), Error> {
        let id =
            DeviceId::from_hex(device_id).map_err(|e| Error::Invalid(format!("device id: {e}")))?;
        let role = match role {
            "" => None,
            "admin" => Some(Role::Admin),
            "member" => Some(Role::Member),
            other => return Err(Error::Invalid(format!("role {other:?}: admin or member"))),
        };
        let update = DeviceUpdate {
            name: None,
            role,
            scopes: Some(parse_scopes(scopes)?),
            vip: None,
        };
        let entry = {
            let roster = self.roster().read().unwrap();
            if roster.state().active(&id).is_none() {
                return Err(Error::NotFound(format!(
                    "{} is not an active member",
                    id.short()
                )));
            }
            Author {
                roster: &roster,
                signer: self.signer().as_ref(),
            }
            .update(id, update)?
        };
        self.node().publish(entry.clone())?;
        self.persist(&[entry]);
        Ok(())
    }

    pub fn policy_view(&self) -> v1::Policy {
        let policy = self.policy().read().unwrap();
        v1::Policy {
            open_ports: policy.open_ports.iter().map(port_rule_str).collect(),
            allow_locked_control: policy.allow_locked_control,
            ax_allowlist: policy.ax_allowlist.clone(),
            offer_agent: policy.offer_agent,
            offer_terminal: policy.offer_terminal,
            offer_tasks: policy.offer_tasks,
        }
    }

    pub fn set_policy(&self, view: &v1::Policy) -> Result<(), Error> {
        let policy = LocalPolicy {
            open_ports: view
                .open_ports
                .iter()
                .map(|s| parse_port_rule(s))
                .collect::<Result<_, _>>()?,
            allow_locked_control: view.allow_locked_control,
            ax_allowlist: view.ax_allowlist.clone(),
            offer_agent: view.offer_agent,
            offer_terminal: view.offer_terminal,
            offer_tasks: view.offer_tasks,
        };
        self.store().set_policy(&policy)?;
        *self.policy().write().unwrap() = policy;
        let _ = self.changed.send(());
        Ok(())
    }
}

/// The link as a member list shows it: "direct · 12 ms", "relay",
/// "connecting", "not connected"; nothing for this device.
fn link_label(member: &v1::MemberView) -> String {
    if member.is_self {
        return String::new();
    }
    match member.presence.as_str() {
        "reconnecting" => return "reconnecting".into(),
        "offline" => return "offline".into(),
        "unreachable" => return "unreachable".into(),
        "local_offline" => return "this device is offline".into(),
        _ => {}
    }
    if !member.connected {
        return if member.connection_status.is_empty() {
            "not connected".into()
        } else {
            member.connection_status.clone()
        };
    }
    let path = if member.path.is_empty() {
        "connected"
    } else {
        member.path.as_str()
    };
    if member.rtt_ms > 0 {
        format!("{path} · {} ms", member.rtt_ms)
    } else {
        path.into()
    }
}

/// How a signer backend is shown.
fn key_label(backend: &str) -> &str {
    match backend {
        "sep" => "Secure Enclave",
        "soft" => "software (development)",
        "platform" => "platform keystore",
        "" => "not reported",
        other => other,
    }
}

pub fn parse_cidr(text: &str) -> Result<(Ipv4Addr, u8), Error> {
    let (ip, bits) = text
        .trim()
        .split_once('/')
        .ok_or_else(|| Error::Invalid("cidr must look like 192.168.13.0/24".into()))?;
    let ip: Ipv4Addr = ip
        .parse()
        .map_err(|_| Error::Invalid("cidr: bad address".into()))?;
    let bits: u8 = bits
        .parse()
        .map_err(|_| Error::Invalid("cidr: bad prefix".into()))?;
    if !(8..=30).contains(&bits) {
        return Err(Error::Invalid(
            "cidr prefix must be between /8 and /30".into(),
        ));
    }
    Ok((ip, bits))
}

pub fn parse_scopes(scopes: &[String]) -> Result<BTreeSet<Scope>, Error> {
    scopes
        .iter()
        .map(|s| {
            s.parse::<Scope>()
                .map_err(|e| Error::Invalid(format!("scope {s:?}: {e}")))
        })
        .collect()
}

pub fn parse_port_rule(text: &str) -> Result<PortRule, Error> {
    let (proto, port) = text
        .trim()
        .split_once('/')
        .ok_or_else(|| Error::Invalid(format!("port rule {text:?}: use tcp/22 or udp/*")))?;
    let proto = match proto.to_ascii_lowercase().as_str() {
        "tcp" => Proto::Tcp,
        "udp" => Proto::Udp,
        "any" | "*" => Proto::Any,
        _ => {
            return Err(Error::Invalid(format!(
                "port rule {text:?}: proto must be tcp, udp or any"
            )));
        }
    };
    let port = match port {
        "*" | "" => None,
        p => Some(
            p.parse::<u16>()
                .map_err(|_| Error::Invalid(format!("port rule {text:?}: bad port")))?,
        ),
    };
    Ok(PortRule { proto, port })
}

pub fn port_rule_str(rule: &PortRule) -> String {
    let proto = match rule.proto {
        Proto::Tcp => "tcp",
        Proto::Udp => "udp",
        Proto::Any => "any",
    };
    match rule.port {
        Some(port) => format!("{proto}/{port}"),
        None => format!("{proto}/*"),
    }
}

#[cfg(test)]
mod label_tests {
    use super::*;

    #[test]
    fn member_links_read_as_the_lists_show_them() {
        let member = |is_self, connected, path: &str, rtt_ms, status: &str| v1::MemberView {
            is_self,
            connected,
            path: path.into(),
            rtt_ms,
            connection_status: status.into(),
            ..Default::default()
        };
        assert_eq!(link_label(&member(true, true, "direct", 3, "")), "");
        assert_eq!(
            link_label(&member(false, true, "direct", 12, "")),
            "direct · 12 ms"
        );
        assert_eq!(link_label(&member(false, true, "relay", 0, "")), "relay");
        assert_eq!(link_label(&member(false, true, "", 0, "")), "connected");
        assert_eq!(
            link_label(&member(false, false, "", 0, "connecting")),
            "connecting"
        );
        assert_eq!(
            link_label(&member(false, false, "", 0, "")),
            "not connected"
        );
        assert_eq!(key_label("sep"), "Secure Enclave");
        assert_eq!(key_label(""), "not reported");
    }
}
