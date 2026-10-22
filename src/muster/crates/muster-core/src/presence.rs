//! Admin-approved desktop Presence, anchored in the existing signed roster.
use crate::canonical::{Reader, Writer};
use crate::identity::{sign_domain, verify_domain};
use crate::{
    Body, DeviceId, DeviceKind, Entry, EntryHash, Error, HwSigner, NetworkId, PublicKey, State,
};

const DOMAIN: &str = "MUSTER-PRESENCE-ENROLLMENT-V1";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binding {
    pub network: NetworkId,
    pub device: DeviceId,
    pub epoch: EntryHash,
    pub key: PublicKey,
    pub issuer: DeviceId,
    pub signature: Vec<u8>,
}

impl Binding {
    fn payload(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.u8(1)
            .fixed(&self.network.0)
            .fixed(&self.device.0)
            .fixed(&self.epoch.0)
            .bytes(self.key.raw())
            .fixed(&self.issuer.0);
        w.finish()
    }
    pub fn issue(
        network: NetworkId,
        device: DeviceId,
        epoch: EntryHash,
        key: PublicKey,
        signer: &dyn HwSigner,
    ) -> Result<Self, Error> {
        let mut binding = Self {
            network,
            device,
            epoch,
            key,
            issuer: signer.public_key().device_id(),
            signature: Vec::new(),
        };
        binding.signature = sign_domain(signer, DOMAIN, &binding.payload())?;
        Ok(binding)
    }
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.bytes(&self.payload()).bytes(&self.signature);
        w.finish()
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() > 1024 {
            return Err(Error::Malformed("Presence binding too large".into()));
        }
        let mut outer = Reader::new(bytes);
        let payload = outer.bytes()?;
        let signature = outer.bytes()?.to_vec();
        outer.finish()?;
        let mut r = Reader::new(payload);
        if r.u8()? != 1 {
            return Err(Error::Malformed("Presence binding version".into()));
        }
        let binding = Self {
            network: EntryHash(r.fixed()?),
            device: DeviceId(r.fixed()?),
            epoch: EntryHash(r.fixed()?),
            key: PublicKey::from_raw(r.bytes()?)?,
            issuer: DeviceId(r.fixed()?),
            signature,
        };
        r.finish()?;
        if binding.payload() != payload {
            return Err(Error::Malformed("noncanonical Presence binding".into()));
        }
        Ok(binding)
    }
    pub(crate) fn verify(&self, state: &State, anchor: Option<&Entry>) -> Result<(), Error> {
        let reject =
            || Error::Rejected("Presence binding is stale, foreign or unauthorized".into());
        let member = state.active(&self.device).ok_or_else(reject)?;
        let anchor = anchor.ok_or_else(reject)?;
        if state.network_id != Some(self.network)
            || member.record.kind == DeviceKind::Phone
            || member.record.presence_key.is_some()
            || member.presence_epoch.map(|(_, hash)| hash) != Some(self.epoch)
            || anchor.signer != self.issuer
            || !matches!(&anchor.body, Body::Update { device_id, update } if *device_id == self.device && update.is_presence_epoch())
        {
            return Err(reject());
        }
        // The accepted anchor already proves issuer authority in its causal past,
        // including the removed-admin rule. A removed subject is never active.
        let issuer = state.member(&self.issuer).ok_or_else(reject)?;
        verify_domain(
            &issuer.record.public_key,
            DOMAIN,
            &self.payload(),
            &self.signature,
        )
    }
}
