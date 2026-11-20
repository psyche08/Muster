#![cfg(windows)]
//! Per-user, non-exportable Windows TPM identity. Only public material leaves CNG.
use std::ptr::{null, null_mut};

use anyhow::{Result, ensure};
use muster_core::{Error, HwSigner, PublicKey};
use p256::ecdsa::Signature;
use sha2::{Digest, Sha256};
use windows_sys::Win32::Foundation::NTE_BAD_KEYSET;
use windows_sys::Win32::Security::Cryptography::*;
use windows_sys::core::w;

#[derive(Debug, Default)]
struct Handle(usize);

impl Drop for Handle {
    fn drop(&mut self) {
        if self.0 != 0 {
            unsafe { NCryptFreeObject(self.0) };
        }
    }
}

fn check(status: i32, operation: &str) -> Result<()> {
    ensure!(
        status == 0,
        "{operation} failed with CNG status {status:#x}"
    );
    Ok(())
}

#[derive(Debug)]
pub struct Tpm {
    key: Handle,
    _provider: Handle,
    public: PublicKey,
}

impl Tpm {
    pub fn open() -> Result<Self> {
        let mut provider = Handle::default();
        let mut key = Handle::default();
        let name = w!("Muster.Vnet.Device.P256");
        let mut blob = [0u8; 72];
        let mut size = 0;
        // Each buffer is sized as passed, and both handles live through all calls.
        unsafe {
            check(
                NCryptOpenStorageProvider(&mut provider.0, MS_PLATFORM_CRYPTO_PROVIDER, 0),
                "open TPM provider",
            )?;
            let status = NCryptOpenKey(provider.0, &mut key.0, name, 0, 0);
            if status == NTE_BAD_KEYSET {
                check(
                    NCryptCreatePersistedKey(
                        provider.0,
                        &mut key.0,
                        NCRYPT_ECDSA_P256_ALGORITHM,
                        name,
                        0,
                        0,
                    ),
                    "create TPM key",
                )?;
                check(NCryptFinalizeKey(key.0, 0), "finalize TPM key")?;
            } else {
                check(status, "open TPM key")?;
            }
            check(
                NCryptExportKey(
                    key.0,
                    0,
                    BCRYPT_ECCPUBLIC_BLOB,
                    null(),
                    blob.as_mut_ptr(),
                    blob.len() as u32,
                    &mut size,
                    0,
                ),
                "read public point",
            )?;
        }
        ensure!(
            size == 72
                && blob[..4] == BCRYPT_ECDSA_PUBLIC_P256_MAGIC.to_le_bytes()
                && blob[4..8] == 32u32.to_le_bytes(),
            "unexpected TPM public key format"
        );
        let mut point = [4u8; 65];
        point[1..].copy_from_slice(&blob[8..]);
        Ok(Self {
            key,
            _provider: provider,
            public: PublicKey::from_raw(&point)?,
        })
    }
}

impl HwSigner for Tpm {
    fn public_key(&self) -> PublicKey {
        self.public.clone()
    }

    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, Error> {
        let digest = Sha256::digest(message);
        let mut bytes = [0u8; 64];
        let mut size = 0;
        let status = unsafe {
            NCryptSignHash(
                self.key.0,
                null_mut(),
                digest.as_ptr(),
                32,
                bytes.as_mut_ptr(),
                64,
                &mut size,
                0,
            )
        };
        if status != 0 || size != 64 {
            return Err(Error::Signature(format!(
                "TPM sign failed ({status:#x}, size {size})"
            )));
        }
        let sig = Signature::from_slice(&bytes).map_err(|e| Error::Signature(e.to_string()))?;
        Ok(sig.to_der().as_bytes().to_vec())
    }

    fn backend(&self) -> &'static str {
        "tpm"
    }
}
