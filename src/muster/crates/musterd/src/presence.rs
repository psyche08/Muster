//! Public Presence candidate registration and explicit administrator enrollment.
use crate::{Daemon, Error};
use muster_core::presence::Binding;
use muster_core::{Author, DeviceId, DeviceUpdate, Entry, PublicKey, Scope};
use muster_proto::{
    Message,
    v1::{self, request::Req, response::Resp},
};
use muster_rpc::Client;
use std::sync::Arc;
use std::time::Duration;
const CANDIDATE: &str = "presence.candidate.v1";

impl Daemon {
    pub(crate) fn presence_candidate(&self) -> Result<Option<PublicKey>, Error> {
        Ok(self
            .store()
            .meta(CANDIDATE)?
            .map(|raw| PublicKey::from_raw(&raw))
            .transpose()?
            .or_else(|| self.config.presence_key.clone()))
    }
    pub(crate) fn presence_info(&self) -> Result<v1::PresenceInfo, Error> {
        let candidate = self.presence_candidate()?;
        let enrolled = candidate.as_ref().is_some_and(|key| {
            self.roster()
                .read()
                .unwrap()
                .state()
                .active(&self.device_id())
                .is_some_and(|member| member.record.presence_key.as_ref() == Some(key))
        });
        Ok(v1::PresenceInfo {
            enrolled,
            public_key: self
                .presence_candidate()?
                .map(|key| key.raw().to_vec())
                .unwrap_or_default(),
            enrollment: self.export_presence(),
        })
    }
    pub(crate) fn register_presence(&self, raw: &[u8]) -> Result<v1::PresenceInfo, Error> {
        if !raw.is_empty() {
            let key = PublicKey::from_raw(raw)?;
            self.store().set_meta(CANDIDATE, key.raw())?;
        }
        self.presence_info()
    }
    pub(crate) fn export_presence(&self) -> Option<v1::PresenceEnrollment> {
        let roster = self.roster().read().unwrap();
        let binding = roster.presence_binding(&self.device_id())?;
        Some(v1::PresenceEnrollment {
            binding: binding.encode(),
            roster_entries: roster.entries().map(Entry::encode).collect(),
        })
    }
    pub(crate) fn import_presence(
        &self,
        expected: DeviceId,
        proof: &v1::PresenceEnrollment,
    ) -> Result<(), Error> {
        if proof.encoded_len() > 1024 * 1024 || proof.roster_entries.len() > 4096 {
            return Err(Error::Invalid("Presence enrollment proof too large".into()));
        }
        let binding = Binding::decode(&proof.binding)?;
        if binding.device != expected {
            return Err(Error::Unauthorized(
                "Presence enrollment names another device".into(),
            ));
        }
        if expected == self.device_id() && self.presence_candidate()?.as_ref() != Some(&binding.key)
        {
            return Err(Error::Conflict(
                "desktop Presence candidate changed; enroll the current key".into(),
            ));
        }
        let entries = proof
            .roster_entries
            .iter()
            .map(|bytes| Entry::decode(bytes))
            .collect::<Result<Vec<_>, _>>()?;
        {
            let roster = self.roster().read().unwrap();
            if roster.presence_binding(&expected) == Some(&binding) {
                return Ok(());
            }
            if roster.network_id() != Some(binding.network) {
                return Err(Error::Unauthorized(
                    "Presence enrollment belongs to another network".into(),
                ));
            }
            let mut checked = roster.clone();
            for entry in &entries {
                checked.insert(entry.clone())?;
            }
            checked.verify_presence_binding(&binding)?;
        }
        // Use Node's normal publication path, including live member revocation.
        self.node().absorb_entries(entries);
        let mut roster = self.roster().write().unwrap();
        roster.verify_presence_binding(&binding)?;
        self.store()
            .append_entries(&roster.entries().cloned().collect::<Vec<_>>())?;
        self.store().save_presence_binding(&binding)?;
        roster.install_presence_binding(binding)?;
        log::info!("Presence enrollment installed: device={}", expected.short());
        Ok(())
    }
    async fn presence_client(&self, device: DeviceId) -> Result<Client, Error> {
        let (send, recv) = tokio::time::timeout(
            Duration::from_secs(5),
            self.node()
                .open(device, crate::SERVICE, crate::SERVICE_VERSION),
        )
        .await
        .map_err(|_| Error::Unavailable("Presence enrollment target connection timed out".into()))?
        .map_err(|error| Error::Unavailable(error.to_string()))?;
        Ok(Client::new(tokio::io::join(recv, send)))
    }
    pub(crate) async fn enroll_presence(
        self: &Arc<Self>,
        request: v1::NetworkPresenceRequest,
    ) -> Result<v1::PresenceInfo, Error> {
        let device = DeviceId::from_hex(&request.device_id)?;
        let remote = if request.revoke || device == self.device_id() {
            None
        } else {
            Some(self.presence_client(device).await?)
        };
        let candidate = if request.revoke {
            Vec::new()
        } else if let Some(client) = &remote {
            let result = tokio::time::timeout(
                Duration::from_secs(5),
                client.call(Req::DevicePresence(Default::default())),
            )
            .await
            .map_err(|_| Error::Unavailable("Presence candidate response timed out".into()))?
            .map_err(|error| Error::Unavailable(error.to_string()))?;
            let Resp::PresenceInfo(info) = result else {
                return Err(Error::Unavailable(
                    "update the target's desktop Presence support".into(),
                ));
            };
            info.public_key
        } else {
            self.presence_info()?.public_key
        };
        let key = if request.revoke {
            None
        } else {
            Some(PublicKey::from_raw(&candidate).map_err(|_| {
                Error::Invalid(
                    "open the updated desktop Muster app to register its Presence key first".into(),
                )
            })?)
        };
        let _serial = self.network_config.lock().await;
        // Only an active admin can author the epoch, even over the local socket.
        if !self
            .roster()
            .read()
            .unwrap()
            .state()
            .is_admin(&self.device_id())
        {
            return Err(Error::Unauthorized(
                "Presence enrollment requires an administrator".into(),
            ));
        }
        if request.revoke && request.grant_locked_control {
            return Err(Error::Invalid("revoke cannot grant locked.control".into()));
        }
        let (entries, binding, existing) = {
            let roster = self.roster().read().unwrap();
            let member = roster
                .state()
                .active(&device)
                .ok_or_else(|| Error::Invalid("no active enrollment target".into()))?;
            if member.record.kind == muster_core::DeviceKind::Phone {
                return Err(Error::Invalid(
                    "this enrollment is for desktop/server Presence".into(),
                ));
            }
            if member.record.presence_key.is_some() && roster.presence_binding(&device).is_none() {
                return Err(Error::Invalid("Presence is already part of this member's original pairing; manage its scopes instead".into()));
            }
            let existing = !request.revoke
                && roster
                    .presence_binding(&device)
                    .is_some_and(|binding| Some(&binding.key) == key.as_ref())
                && (!request.grant_locked_control
                    || member.record.scopes.contains(&Scope::LockedControl));
            let mut checked = roster.clone();
            let mut entries = Vec::new();
            if !existing
                && request.grant_locked_control
                && !member.record.scopes.contains(&Scope::LockedControl)
            {
                let mut scopes = member.record.scopes.clone();
                scopes.insert(Scope::LockedControl);
                let entry = Author {
                    roster: &checked,
                    signer: self.signer().as_ref(),
                }
                .update(
                    device,
                    DeviceUpdate {
                        scopes: Some(scopes),
                        ..Default::default()
                    },
                )?;
                checked.insert(entry.clone())?;
                entries.push(entry);
            }
            let binding = if existing {
                roster.presence_binding(&device).cloned()
            } else {
                let epoch = Author {
                    roster: &checked,
                    signer: self.signer().as_ref(),
                }
                .update(device, DeviceUpdate::default())?;
                checked.insert(epoch.clone())?;
                entries.push(epoch.clone());
                let network = checked.network_id().ok_or(Error::NoNetwork)?;
                key.map(|key| {
                    Binding::issue(network, device, epoch.hash(), key, self.signer().as_ref())
                })
                .transpose()?
            };
            (entries, binding, existing)
        };
        for entry in entries {
            self.store().append_entries(std::slice::from_ref(&entry))?;
            self.node().publish(entry)?;
        }
        if let Some(binding) = binding {
            let proof = v1::PresenceEnrollment {
                binding: binding.encode(),
                roster_entries: self
                    .roster()
                    .read()
                    .unwrap()
                    .entries()
                    .map(Entry::encode)
                    .collect(),
            };
            self.import_presence(device, &proof)?;
            if let Some(client) = &remote {
                tokio::time::timeout(Duration::from_secs(5), client.call(Req::DeviceInstallPresence(v1::DeviceInstallPresenceRequest { enrollment: Some(proof.clone()) })))
                    .await.map_err(|_| Error::Unavailable("Presence approved; delivery timed out. Repeat enrollment to deliver the same binding".into()))?
                    .map_err(|error| Error::Unavailable(format!("Presence approved; delivery failed: {error}")))?;
            }
            log::info!(
                "Presence enrollment approved: device={} reused={existing}",
                device.short()
            );
            Ok(v1::PresenceInfo {
                enrolled: true,
                public_key: candidate,
                enrollment: Some(proof),
            })
        } else {
            log::info!("Presence enrollment revoked: device={}", device.short());
            Ok(v1::PresenceInfo::default())
        }
    }
}
