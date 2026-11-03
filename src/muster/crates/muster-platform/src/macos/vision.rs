//! What a failed native send's look needs from macOS (ADR 0116): a PNG of
//! the app's window, and musterd's own login-keychain item for the owner's
//! LLM API key. The policy lives in `muster_agent::vision`.

use std::ffi::{CString, c_char};
use std::time::Duration;

unsafe extern "C" {
    fn muster_capture_allowed() -> bool;
    fn muster_capture_request() -> bool;
    fn muster_capture_window_png(
        pids: *const i32,
        count: usize,
        max_side: u32,
        timeout_ms: u32,
        out: *mut *mut u8,
        len: *mut usize,
    ) -> i32;
    fn muster_vision_free(buffer: *mut u8, len: usize);
    fn muster_secret_set(
        service: *const c_char,
        account: *const c_char,
        secret: *const u8,
        len: usize,
    ) -> i32;
    fn muster_secret_exists(service: *const c_char, account: *const c_char) -> i32;
    fn muster_secret_get(
        service: *const c_char,
        account: *const c_char,
        interactive: bool,
        out: *mut *mut u8,
        len: *mut usize,
    ) -> i32;
    fn muster_secret_delete(service: *const c_char, account: *const c_char) -> i32;
}

/// The longest side of a capture, in pixels: what vision models read at full
/// detail without the provider scaling it down.
pub const MAX_SIDE: u32 = 1568;
const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;

/// Whether musterd may capture windows (Screen Recording).
pub fn capture_allowed() -> bool {
    unsafe { muster_capture_allowed() }
}

/// Asks for Screen Recording; macOS shows its prompt only the first time.
pub fn request_capture() -> bool {
    unsafe { muster_capture_request() }
}

/// A PNG of the largest on-screen, normal-level window any of `pids` owns.
/// Blocks for up to twice `timeout` (window list, then the capture).
pub fn window_png(pids: &[u32], timeout: Duration) -> Result<Vec<u8>, String> {
    let pids: Vec<i32> = pids.iter().filter_map(|p| i32::try_from(*p).ok()).collect();
    if pids.is_empty() {
        return Err("the app is not running".into());
    }
    let timeout_ms = u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX);
    let mut out = std::ptr::null_mut();
    let mut len = 0usize;
    let status = unsafe {
        muster_capture_window_png(
            pids.as_ptr(),
            pids.len(),
            MAX_SIDE,
            timeout_ms,
            &mut out,
            &mut len,
        )
    };
    let bytes = take(out, len);
    match status {
        0 => bytes.ok_or_else(|| "the capture is empty".into()),
        1 => Err("Screen Recording is not allowed for musterd".into()),
        2 => Err("the app shows no window on screen".into()),
        3 => Err("the capture timed out".into()),
        _ => Err("the capture failed".into()),
    }
}

fn take(out: *mut u8, len: usize) -> Option<Vec<u8>> {
    if out.is_null() {
        return None;
    }
    // SAFETY: the shim returned `len` bytes it allocated; it clears and
    // frees them.
    let bytes = unsafe { std::slice::from_raw_parts(out, len) }.to_vec();
    unsafe { muster_vision_free(out, len) };
    Some(bytes)
}

/// musterd's own generic password in the login keychain.
pub struct Secret {
    service: CString,
    account: CString,
}

impl Secret {
    pub fn new(service: &str, account: &str) -> Self {
        Self {
            service: CString::new(service).expect("service without NUL"),
            account: CString::new(account).expect("account without NUL"),
        }
    }

    pub fn set(&self, secret: &str) -> Result<(), String> {
        let status = unsafe {
            muster_secret_set(
                self.service.as_ptr(),
                self.account.as_ptr(),
                secret.as_ptr(),
                secret.len(),
            )
        };
        check(status)
    }

    /// Whether the item exists; reading attributes never prompts.
    pub fn exists(&self) -> bool {
        unsafe { muster_secret_exists(self.service.as_ptr(), self.account.as_ptr()) == 0 }
    }

    /// The secret, or `None` when there is none. With `interactive` false a
    /// read that would need the person to allow it fails instead.
    pub fn get(&self, interactive: bool) -> Result<Option<String>, String> {
        let mut out = std::ptr::null_mut();
        let mut len = 0usize;
        let status = unsafe {
            muster_secret_get(
                self.service.as_ptr(),
                self.account.as_ptr(),
                interactive,
                &mut out,
                &mut len,
            )
        };
        let bytes = take(out, len);
        if status == ERR_SEC_ITEM_NOT_FOUND {
            return Ok(None);
        }
        check(status)?;
        let bytes = bytes.ok_or("the keychain item is empty")?;
        String::from_utf8(bytes)
            .map(Some)
            .map_err(|_| "the keychain item is not text".into())
    }

    pub fn delete(&self) -> Result<(), String> {
        let status = unsafe { muster_secret_delete(self.service.as_ptr(), self.account.as_ptr()) };
        if status == ERR_SEC_ITEM_NOT_FOUND {
            return Ok(());
        }
        check(status)
    }
}

fn check(status: i32) -> Result<(), String> {
    match status {
        0 => Ok(()),
        -25308 => Err("the keychain is locked or needs permission".into()),
        -128 => Err("keychain access was denied".into()),
        other => Err(format!("keychain error {other}")),
    }
}
