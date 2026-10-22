//! Apple Events permission observation in the responsible daemon process.
use core_foundation::{array::CFArray, base::TCFType, string::CFString, url::CFURL};
use core_foundation_sys::{array::CFArrayRef, base::CFRelease, string::CFStringRef};
use std::ffi::c_void;

#[link(name = "CoreServices", kind = "framework")]
unsafe extern "C" {
    fn LSCopyApplicationURLsForBundleIdentifier(
        bundle: CFStringRef,
        error: *mut *const c_void,
    ) -> CFArrayRef;
}

const ITERM: &str = "com.googlecode.iterm2";

/// Launch Services also finds apps outside /Applications. Ignore stale entries
/// for bundles that have been removed. This launches nothing and asks no TCC.
pub fn installed(bundle: &str) -> Option<bool> {
    let bundle = CFString::new(bundle);
    let mut error = std::ptr::null();
    // SAFETY: CF objects are retained by the copy API and released exactly once.
    let raw = unsafe {
        LSCopyApplicationURLsForBundleIdentifier(bundle.as_concrete_TypeRef(), &mut error)
    };
    let not_found = if error.is_null() {
        false
    } else {
        // kLSApplicationNotFoundErr (-10814) means absent, not a TCC denial.
        let code = unsafe { core_foundation_sys::error::CFErrorGetCode(error.cast_mut().cast()) };
        unsafe {
            CFRelease(error);
        }
        code == -10814
    };
    if raw.is_null() {
        return if error.is_null() || not_found {
            Some(false)
        } else {
            None
        };
    }
    let urls: CFArray<CFURL> = unsafe { CFArray::wrap_under_create_rule(raw) };
    Some(
        urls.iter()
            .any(|url| url.to_path().is_some_and(|path| path.is_dir())),
    )
}

#[repr(C)]
struct Descriptor {
    kind: u32,
    handle: *mut c_void,
}
#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AECreateDesc(kind: u32, bytes: *const c_void, len: isize, out: *mut Descriptor) -> i32;
    fn AEDisposeDesc(desc: *mut Descriptor) -> i32;
    fn AEDeterminePermissionToAutomateTarget(
        target: *const Descriptor,
        class: u32,
        event: u32,
        ask: u8,
    ) -> i32;
}

/// `ask` is true only for a local user's explicit setup action. No event is sent.
pub fn iterm_permission(ask: bool) -> &'static str {
    // iTerm2's scripting dictionary: request cookie, Itrm/rqck.
    permission(ITERM, *b"Itrm", *b"rqck", ask)
}

/// Whether this process may send `bundle` the Apple Event `class`/`event`:
/// granted | missing | not_running | not_installed | unavailable. Without
/// `ask` it never prompts, so no consent dialog waits unseen (under a Locked
/// Use cover, or on a locked screen).
pub fn permission(bundle: &str, class: [u8; 4], event: [u8; 4], ask: bool) -> &'static str {
    if installed(bundle) == Some(false) {
        return "not_installed";
    }
    let bundle = bundle.as_bytes();
    let mut target = Descriptor {
        kind: 0,
        handle: std::ptr::null_mut(),
    };
    // SAFETY: initialized AE descriptor and a valid, length-delimited bundle ID;
    // both APIs are synchronous and the descriptor is disposed exactly once.
    let status = unsafe {
        if AECreateDesc(
            u32::from_be_bytes(*b"bund"),
            bundle.as_ptr().cast(),
            bundle.len() as isize,
            &mut target,
        ) != 0
        {
            return "unavailable";
        }
        let status = AEDeterminePermissionToAutomateTarget(
            &target,
            u32::from_be_bytes(class),
            u32::from_be_bytes(event),
            u8::from(ask),
        );
        AEDisposeDesc(&mut target);
        status
    };
    match status {
        0 => "granted",
        -1743 | -1744 => "missing",
        -600 => "not_running",
        _ => "unavailable",
    }
}
