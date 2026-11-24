//! Admission for the desktop service and migration of existing receiver grants.
use crate::{Daemon, Error};
use muster_core::{
    DeviceId, LocalPolicy, Scope,
    policy::{DesktopPairing, DeviceGrant, GrantSet, Permission, TargetState, authorize},
    roster::{EntryHash, State},
};
use muster_proto::v1::{DesktopAccess, NetworkStatus};
use std::{
    collections::BTreeSet,
    sync::atomic::{AtomicU64, Ordering},
};

pub(crate) fn migrate(
    store: &muster_store::Store,
    state: &State,
    host: DeviceId,
    policy: &mut LocalPolicy,
) -> Result<(), Error> {
    const DONE: &str = "desktop-policy-migrated";
    if store.meta(DONE)?.is_some() {
        return Ok(());
    }
    #[derive(serde::Deserialize)]
    struct Record {
        pairing: DesktopPairing,
        mouse: bool,
    }
    if policy.service_grants.desktop.is_none() {
        let mut rules = Vec::new();
        for member in state.active_members() {
            let peer = member.record.device_id;
            let Some(pairing) = DesktopPairing::current(state, host, peer) else {
                continue;
            };
            let Some(bytes) = store.meta(&format!("desktop-access/{}", peer.to_hex()))? else {
                continue;
            };
            if bytes.is_empty() {
                continue;
            }
            let record: Record = serde_json::from_slice(&bytes)
                .map_err(|e| Error::Invalid(format!("invalid stored desktop grant: {e}")))?;
            if record.pairing == pairing {
                rules.push(DeviceGrant {
                    scope: if record.mouse {
                        Scope::DesktopMouse
                    } else {
                        Scope::DesktopView
                    },
                    devices: BTreeSet::from([peer]),
                });
                policy.desktop_pairings.insert(peer, pairing);
            }
        }
        if !rules.is_empty() {
            policy.service_grants.desktop = Some(GrantSet {
                all_devices: false,
                rules,
            });
            store.set_policy(policy)?;
        }
    }
    store.set_meta(DONE, b"1")?;
    Ok(())
}

#[derive(Default)]
pub(crate) struct Permissions {
    generation: AtomicU64,
}

impl Permissions {
    pub(crate) fn changed(&self) {
        self.generation.fetch_add(1, Ordering::Release);
    }

    fn resolve(&self, daemon: &Daemon, peer: DeviceId) -> Option<(DesktopAccess, Lease)> {
        let roster = daemon.roster().read().unwrap();
        let state = roster.state();
        let pairing = DesktopPairing::current(state, daemon.device_id(), peer)?;
        let policy = daemon.policy().read().unwrap();
        if let Some(grants) = &policy.service_grants.desktop {
            if !grants.all_devices && policy.desktop_pairings.get(&peer) != Some(&pairing) {
                return None;
            }
        }
        let allowed = |mouse| {
            authorize(
                state,
                &peer,
                &Permission::Desktop { mouse },
                &policy,
                &TargetState::default(),
            )
            .is_allow()
        };
        let mode = if allowed(true) {
            DesktopAccess::Mouse
        } else if allowed(false) {
            DesktopAccess::View
        } else {
            return None;
        };
        Some((
            mode,
            Lease {
                peer,
                pairing,
                revision: state.active(&peer)?.defined_by,
                mode,
                generation: self.generation.load(Ordering::Acquire),
            },
        ))
    }

    pub(crate) fn project(&self, daemon: &Daemon, network: &mut NetworkStatus) {
        for member in &mut network.members {
            member.desktop_access = DeviceId::from_hex(&member.device_id)
                .ok()
                .and_then(|peer| self.resolve(daemon, peer))
                .map_or(DesktopAccess::None, |(mode, _)| mode)
                as i32;
        }
    }

    #[cfg(any(windows, target_os = "macos", test))]
    pub(super) fn admit(&self, daemon: &Daemon, peer: DeviceId) -> Option<(DesktopAccess, Lease)> {
        self.resolve(daemon, peer)
    }
}

#[derive(PartialEq, Eq)]
pub(super) struct Lease {
    peer: DeviceId,
    pairing: DesktopPairing,
    revision: EntryHash,
    mode: DesktopAccess,
    generation: u64,
}

#[cfg(any(windows, target_os = "macos", test))]
impl Lease {
    pub(super) fn current(&self, daemon: &Daemon) -> bool {
        daemon
            .desktop_permissions
            .resolve(daemon, self.peer)
            .is_some_and(|(_, lease)| lease == *self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use muster_core::roster::DeviceKind;
    use muster_core::{Author, DeviceRecord, HwSigner, SoftSigner};
    use std::sync::Arc;

    #[tokio::test]
    async fn remembered_access_follows_the_pairing_and_revokes_its_live_lease() {
        verify_pairing(DeviceKind::Desktop).await;
    }
    #[tokio::test]
    async fn phone_requires_an_explicit_grant_and_loses_the_lease_on_revocation() {
        verify_pairing(DeviceKind::Phone).await;
    }
    async fn verify_pairing(kind: DeviceKind) {
        let dir = tempfile::tempdir().unwrap();
        let mut config = crate::DaemonConfig::new(dir.path());
        config.listen = false;
        config.agent = None;
        config.cloud = false;
        config.bind = "127.0.0.1:0".parse().unwrap();
        let daemon = Daemon::start_with(
            config,
            Arc::new(crate::DefaultPlatform),
            Some(Arc::new(SoftSigner::generate())),
        )
        .await
        .unwrap();
        let viewer = SoftSigner::generate();
        let record = DeviceRecord::new(viewer.public_key(), "Viewer", kind);
        let peer = record.device_id;
        let other = SoftSigner::generate().public_key().device_id();
        {
            let mut roster = daemon.roster().write().unwrap();
            let creator = Author {
                roster: &roster,
                signer: &**daemon.signer(),
            };
            let genesis = creator
                .genesis(
                    "test",
                    ("192.168.14.0".parse().unwrap(), 24),
                    daemon.own_record(),
                )
                .unwrap();
            roster.insert(genesis).unwrap();
            let add = (Author {
                roster: &roster,
                signer: &**daemon.signer(),
            })
            .add(record.clone())
            .unwrap();
            roster.insert(add).unwrap();
        }
        let permissions = &daemon.desktop_permissions;
        assert!(permissions.admit(&daemon, peer).is_none());
        // A signed grant is effective immediately and on the next connection.
        daemon
            .set_scopes_checked(&peer.to_hex(), &["desktop.mouse".into()], "")
            .await
            .expect_err("offline peer cannot prove decoder compatibility");
        {
            let mut roster = daemon.roster().write().unwrap();
            let update = muster_core::roster::DeviceUpdate {
                scopes: Some(BTreeSet::from([Scope::DesktopMouse])),
                name: None,
                role: None,
                vip: None,
            };
            let entry = (Author {
                roster: &roster,
                signer: &**daemon.signer(),
            })
            .update(peer, update)
            .unwrap();
            roster.insert(entry).unwrap();
        }
        assert_eq!(
            permissions.admit(&daemon, peer).unwrap().0,
            DesktopAccess::Mouse
        );
        assert_eq!(
            permissions.admit(&daemon, peer).unwrap().0,
            DesktopAccess::Mouse
        );
        let deny = |inherit: bool| {
            let mut policy = daemon.policy_view();
            let grants = policy.service_grants.as_mut().unwrap();
            grants.manages_desktop = true;
            grants.desktop = if inherit {
                None
            } else {
                Some(Default::default())
            };
            daemon.set_policy(&policy).unwrap();
        };
        deny(false);
        assert!(permissions.admit(&daemon, peer).is_none());
        deny(true);
        assert_eq!(
            permissions.admit(&daemon, peer).unwrap().0,
            DesktopAccess::Mouse
        );
        let set = |mode: &str| {
            let mut policy = daemon.policy_view();
            let grants = policy.service_grants.as_mut().unwrap();
            grants.manages_desktop = true;
            grants.desktop = Some(muster_proto::v1::GrantSet {
                all_devices: false,
                rules: vec![muster_proto::v1::DeviceGrant {
                    scope: mode.into(),
                    device_ids: vec![peer.to_hex()],
                }],
            });
            daemon.set_policy(&policy).unwrap();
        };
        let mut blanket = daemon.policy_view();
        let grants = blanket.service_grants.as_mut().unwrap();
        grants.manages_desktop = true;
        grants.desktop = Some(muster_proto::v1::GrantSet {
            all_devices: true,
            rules: vec![],
        });
        daemon.set_policy(&blanket).unwrap();
        assert_eq!(
            permissions.admit(&daemon, peer).is_some(),
            kind == DeviceKind::Desktop
        );
        set("desktop.mouse");
        assert!(permissions.admit(&daemon, other).is_none());
        let (mode, mouse) = permissions.admit(&daemon, peer).unwrap();
        assert_eq!(mode, DesktopAccess::Mouse);
        assert!(mouse.current(&daemon));
        set("desktop.view");
        assert!(!mouse.current(&daemon));
        let mut old_client = daemon.policy_view();
        old_client.service_grants.as_mut().unwrap().desktop = None;
        daemon.set_policy(&old_client).unwrap();
        assert_eq!(
            daemon.store().policy().unwrap().service_grants.desktop,
            daemon.policy().read().unwrap().service_grants.desktop
        );
        let (mode, view) = permissions.admit(&daemon, peer).unwrap();
        assert_eq!(mode, DesktopAccess::View);
        {
            let mut roster = daemon.roster().write().unwrap();
            let author = Author {
                roster: &roster,
                signer: &**daemon.signer(),
            };
            let remove = author.remove(peer).unwrap();
            roster.insert(remove).unwrap();
            let add = (Author {
                roster: &roster,
                signer: &**daemon.signer(),
            })
            .add(record)
            .unwrap();
            roster.insert(add).unwrap();
        }
        assert!(!view.current(&daemon));
        assert!(permissions.admit(&daemon, peer).is_none());
    }
}
