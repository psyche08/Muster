//! Device key backends: the Secure Enclave on macOS (through the SepSign
//! Swift static library in src/native/apple/MusterMacShim, behind the `sep`
//! feature), a stored software key elsewhere. Which one musterd uses is
//! decided here, not in musterd. An identity already in the store is kept
//! whatever the build: the roster names this device by it, and a build
//! with the feature must not silently become a different device.

use std::sync::Arc;

#[cfg(all(target_os = "macos", feature = "sep"))]
use muster_core::HwSigner;
use muster_core::{SharedSigner, SoftSigner};
use muster_store::Store;

/// The best signer this machine offers, created on first use and kept in
/// the store (the SEP blob is useless off this machine; the software key
/// is development only).
pub fn device_signer(store: &Store) -> anyhow::Result<SharedSigner> {
    #[cfg(all(target_os = "macos", feature = "sep"))]
    {
        if let Some(blob) = store.meta("identity.sep")? {
            return Ok(Arc::new(crate::macos::sep::SepSigner::from_blob(blob)));
        }
        if let Some(bytes) = store.meta("identity.soft")? {
            log::warn!("keeping the software identity this store was created with");
            return Ok(Arc::new(SoftSigner::from_bytes(&bytes)?));
        }
        match crate::macos::sep::SepSigner::create() {
            Ok(signer) => {
                store.set_meta("identity.sep", signer.blob())?;
                log::info!(
                    "new Secure Enclave identity {}",
                    signer.public_key().device_id().fingerprint()
                );
                return Ok(Arc::new(signer));
            }
            Err(error) => log::warn!("Secure Enclave unavailable ({error}); using a software key"),
        }
    }
    #[cfg(all(target_os = "macos", not(feature = "sep")))]
    log::warn!("built without the `sep` feature: using a software key, not the Secure Enclave");
    if let Some(bytes) = store.meta("identity.soft")? {
        return Ok(Arc::new(SoftSigner::from_bytes(&bytes)?));
    }
    let signer = SoftSigner::generate();
    store.set_meta("identity.soft", &signer.to_bytes())?;
    Ok(Arc::new(signer))
}
