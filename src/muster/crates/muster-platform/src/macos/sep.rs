//! The Secure Enclave device key, through the SepSign Swift library
//! (`src/native/apple/MusterMacShim`, compiled and linked statically into
//! musterd by this crate's build.rs under the `sep` feature). The blob is
//! the SEP-wrapped key: useless off this machine, safe to store.

use std::ffi::{CStr, c_char};

use muster_core::{HwSigner, PublicKey};

unsafe extern "C" {
    fn sepsign_create(out: *mut *mut u8, out_len: *mut usize, err: *mut *mut c_char) -> i32;
    fn sepsign_public_key_raw(
        blob: *const u8,
        blob_len: usize,
        out: *mut *mut u8,
        out_len: *mut usize,
        err: *mut *mut c_char,
    ) -> i32;
    fn sepsign_sign(
        blob: *const u8,
        blob_len: usize,
        msg: *const u8,
        msg_len: usize,
        out: *mut *mut u8,
        out_len: *mut usize,
        err: *mut *mut c_char,
    ) -> i32;
    fn sepsign_free(p: *mut u8);
    fn sepsign_free_str(p: *mut c_char);
}

fn take(
    status: i32,
    out: *mut u8,
    len: usize,
    err: *mut c_char,
) -> Result<Vec<u8>, muster_core::Error> {
    if status != 0 {
        let text = if err.is_null() {
            "unknown Secure Enclave error".to_string()
        } else {
            // SAFETY: a NUL-terminated string the library allocated for us.
            let text = unsafe { CStr::from_ptr(err) }
                .to_string_lossy()
                .into_owned();
            // SAFETY: freeing what the library allocated.
            unsafe { sepsign_free_str(err) };
            text
        };
        return Err(muster_core::Error::Signature(format!("sep: {text}")));
    }
    // SAFETY: the library returned `len` readable bytes at `out`.
    let bytes = unsafe { std::slice::from_raw_parts(out, len) }.to_vec();
    // SAFETY: freeing what the library allocated.
    unsafe { sepsign_free(out) };
    Ok(bytes)
}

pub struct SepSigner {
    blob: Vec<u8>,
    public: PublicKey,
}

impl std::fmt::Debug for SepSigner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SepSigner({})", self.public.device_id().short())
    }
}

impl SepSigner {
    pub fn create() -> Result<Self, muster_core::Error> {
        let (mut out, mut len, mut err) = (std::ptr::null_mut(), 0usize, std::ptr::null_mut());
        // SAFETY: out-pointers are valid for the call.
        let status = unsafe { sepsign_create(&mut out, &mut len, &mut err) };
        let blob = take(status, out, len, err)?;
        Self::from_blob_checked(blob)
    }

    pub fn from_blob(blob: Vec<u8>) -> Self {
        Self::from_blob_checked(blob).expect("a stored SEP blob opens on this machine")
    }

    fn from_blob_checked(blob: Vec<u8>) -> Result<Self, muster_core::Error> {
        let (mut out, mut len, mut err) = (std::ptr::null_mut(), 0usize, std::ptr::null_mut());
        // SAFETY: the blob slice and out-pointers are valid for the call.
        let status = unsafe {
            sepsign_public_key_raw(blob.as_ptr(), blob.len(), &mut out, &mut len, &mut err)
        };
        let raw = take(status, out, len, err)?;
        Ok(Self {
            blob,
            public: PublicKey::from_raw(&raw)?,
        })
    }

    pub fn blob(&self) -> &[u8] {
        &self.blob
    }
}

impl HwSigner for SepSigner {
    fn public_key(&self) -> PublicKey {
        self.public.clone()
    }
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, muster_core::Error> {
        let (mut out, mut len, mut err) = (std::ptr::null_mut(), 0usize, std::ptr::null_mut());
        // SAFETY: all slices and out-pointers are valid for the call.
        let status = unsafe {
            sepsign_sign(
                self.blob.as_ptr(),
                self.blob.len(),
                message.as_ptr(),
                message.len(),
                &mut out,
                &mut len,
                &mut err,
            )
        };
        take(status, out, len, err)
    }
    fn backend(&self) -> &'static str {
        "sep"
    }
}
