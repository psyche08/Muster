//! Host-owned device permissions. A stream holds only its current admission.
use crate::{Daemon, Error};
use muster_core::{
    DeviceId,
    roster::{DeviceKind, EntryHash},
};
use muster_proto::v1::{DesktopAccess, NetworkStatus};
use serde::{Deserialize, Serialize};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct Pairing {
    network: EntryHash,
    host: EntryHash,
    viewer: EntryHash,
}

impl Pairing {
    fn current(daemon: &Daemon, peer: DeviceId) -> Option<Self> {
        let me = daemon.device_id();
        if peer == me {
            return None;
        }
        let roster = daemon.roster().read().unwrap();
        let state = roster.state();
        let member = state.active(&peer)?;
        if member.record.kind != DeviceKind::Desktop {
            return None;
        }
        Some(Self {
            network: state.network_id?,
            host: state.active(&me)?.joined_at?,
            viewer: member.joined_at?,
        })
    }
}

#[derive(Serialize, Deserialize)]
struct Record {
    pairing: Pairing,
    mouse: bool,
}

fn key(peer: DeviceId) -> String {
    format!("desktop-access/{}", peer.to_hex())
}

fn read(daemon: &Daemon, peer: DeviceId, pairing: Pairing) -> Option<DesktopAccess> {
    let bytes = daemon.store().meta(&key(peer)).ok()??;
    let record: Record = serde_json::from_slice(&bytes).ok()?;
    (record.pairing == pairing).then_some(if record.mouse {
        DesktopAccess::Mouse
    } else {
        DesktopAccess::View
    })
}

#[derive(Default)]
pub(crate) struct Permissions {
    active: Mutex<Option<(DeviceId, Arc<AtomicBool>)>>,
}

impl Permissions {
    pub(super) fn set(
        &self,
        daemon: &Daemon,
        peer: DeviceId,
        mode: DesktopAccess,
    ) -> Result<(), Error> {
        let mut active = self.active.lock().unwrap();
        let bytes = if mode == DesktopAccess::None {
            Vec::new()
        } else {
            let pairing = Pairing::current(daemon, peer)
                .ok_or_else(|| Error::Unauthorized("select a paired desktop".into()))?;
            let record = Record {
                pairing,
                mouse: mode == DesktopAccess::Mouse,
            };
            serde_json::to_vec(&record).map_err(|e| Error::Invalid(e.to_string()))?
        };
        daemon.store().set_meta(&key(peer), &bytes)?;
        if active.as_ref().is_some_and(|(id, _)| *id == peer) {
            if let Some((_, valid)) = active.take() {
                valid.store(false, Ordering::Release);
            }
        }
        let _ = daemon.changed.send(());
        Ok(())
    }

    pub(crate) fn project(&self, daemon: &Daemon, network: &mut NetworkStatus) {
        for m in &mut network.members {
            let Ok(peer) = DeviceId::from_hex(&m.device_id) else {
                continue;
            };
            m.desktop_access = Pairing::current(daemon, peer)
                .and_then(|pairing| read(daemon, peer, pairing))
                .unwrap_or(DesktopAccess::None) as i32;
        }
    }

    #[cfg(any(target_os = "macos", test))]
    pub(super) fn admit(&self, daemon: &Daemon, peer: DeviceId) -> Option<(DesktopAccess, Lease)> {
        let mut active = self.active.lock().unwrap();
        let pairing = Pairing::current(daemon, peer)?;
        let mode = read(daemon, peer, pairing)?;
        let valid = Arc::new(AtomicBool::new(true));
        if let Some((_, old)) = active.replace((peer, valid.clone())) {
            old.store(false, Ordering::Release);
        }
        Some((
            mode,
            Lease {
                peer,
                pairing,
                valid,
            },
        ))
    }
}

#[cfg(any(target_os = "macos", test))]
pub(super) struct Lease {
    peer: DeviceId,
    pairing: Pairing,
    valid: Arc<AtomicBool>,
}

#[cfg(any(target_os = "macos", test))]
impl Lease {
    pub(super) fn current(&self, daemon: &Daemon) -> bool {
        self.valid.load(Ordering::Acquire)
            && Pairing::current(daemon, self.peer) == Some(self.pairing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use muster_core::{Author, DeviceRecord, HwSigner, SoftSigner};

    #[tokio::test]
    async fn remembered_access_follows_the_pairing_and_revokes_its_live_lease() {
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
        let record = DeviceRecord::new(viewer.public_key(), "Viewer", DeviceKind::Desktop);
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
        assert!(
            permissions
                .set(&daemon, other, DesktopAccess::Mouse)
                .is_err()
        );
        permissions.set(&daemon, peer, DesktopAccess::View).unwrap();
        let (mode, first) = permissions.admit(&daemon, peer).unwrap();
        assert_eq!(mode, DesktopAccess::View);
        assert!(first.current(&daemon));
        permissions
            .set(&daemon, peer, DesktopAccess::Mouse)
            .unwrap();
        assert!(!first.current(&daemon));
        let (mode, second) = permissions.admit(&daemon, peer).unwrap();
        assert_eq!(mode, DesktopAccess::Mouse);
        drop(second);
        let restarted = Permissions::default();
        let (mode, lease) = restarted.admit(&daemon, peer).unwrap();
        assert_eq!(mode, DesktopAccess::Mouse);
        restarted.set(&daemon, peer, DesktopAccess::None).unwrap();
        assert!(!lease.current(&daemon));
        assert!(restarted.admit(&daemon, peer).is_none());
        restarted.set(&daemon, peer, DesktopAccess::Mouse).unwrap();
        let (_, old) = restarted.admit(&daemon, peer).unwrap();
        {
            let mut roster = daemon.roster().write().unwrap();
            let remove = (Author {
                roster: &roster,
                signer: &**daemon.signer(),
            })
            .remove(peer)
            .unwrap();
            roster.insert(remove).unwrap();
            let add = (Author {
                roster: &roster,
                signer: &**daemon.signer(),
            })
            .add(record)
            .unwrap();
            roster.insert(add).unwrap();
        }
        assert!(!old.current(&daemon));
        assert!(restarted.admit(&daemon, peer).is_none());
    }
}
