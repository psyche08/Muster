//! Process identity: the code signature requirement a target must satisfy
//! before the controller drives it, and the start time that makes a pid
//! unambiguous.

use std::ffi::c_void;

use core_foundation::base::TCFType;
use core_foundation::dictionary::CFDictionary;
use core_foundation::number::CFNumber;
use core_foundation::string::CFString;
use core_foundation_sys::base::{CFTypeRef, OSStatus};
use core_foundation_sys::dictionary::CFDictionaryRef;
use core_foundation_sys::string::CFStringRef;

type SecCodeRef = *mut c_void;
type SecRequirementRef = *mut c_void;

#[link(name = "Security", kind = "framework")]
unsafe extern "C" {
    fn SecCodeCopyGuestWithAttributes(
        host: SecCodeRef,
        attributes: CFDictionaryRef,
        flags: u32,
        guest: *mut SecCodeRef,
    ) -> OSStatus;
    fn SecRequirementCreateWithString(
        text: CFStringRef,
        flags: u32,
        requirement: *mut SecRequirementRef,
    ) -> OSStatus;
    fn SecCodeCheckValidity(
        code: SecCodeRef,
        flags: u32,
        requirement: SecRequirementRef,
    ) -> OSStatus;
    fn CFRelease(cf: CFTypeRef);
}

/// Whether the running process `pid` satisfies `requirement` (a Security
/// framework requirement string). Anything unexpected is `false`.
pub fn process_satisfies(pid: u32, requirement: &str) -> bool {
    let attrs = CFDictionary::from_CFType_pairs(&[(
        CFString::new("pid"),
        CFNumber::from(pid as i32).as_CFType(),
    )]);
    let mut code: SecCodeRef = std::ptr::null_mut();
    // SAFETY: valid CF objects; `code` receives a +1 reference on success.
    let status = unsafe {
        SecCodeCopyGuestWithAttributes(
            std::ptr::null_mut(),
            attrs.as_concrete_TypeRef(),
            0,
            &mut code,
        )
    };
    if status != 0 || code.is_null() {
        return false;
    }
    let text = CFString::new(requirement);
    let mut req: SecRequirementRef = std::ptr::null_mut();
    // SAFETY: as above.
    let status = unsafe { SecRequirementCreateWithString(text.as_concrete_TypeRef(), 0, &mut req) };
    if status != 0 || req.is_null() {
        // SAFETY: releasing the +1 reference.
        unsafe { CFRelease(code as CFTypeRef) };
        return false;
    }
    // SAFETY: both references are valid.
    let ok = unsafe { SecCodeCheckValidity(code, 0, req) } == 0;
    // SAFETY: releasing +1 references.
    unsafe {
        CFRelease(req as CFTypeRef);
        CFRelease(code as CFTypeRef);
    }
    ok
}

/// The process start time in microseconds since the epoch, from
/// `proc_pidinfo(PROC_PIDTBSDINFO)`; `None` when the pid is gone.
pub fn process_start(pid: u32) -> Option<u64> {
    observe_process_start(pid).ok().flatten()
}

/// Preserve the difference between a confirmed exit and unavailable OS evidence.
pub fn observe_process_start(pid: u32) -> std::io::Result<Option<u64>> {
    #[repr(C)]
    struct ProcBsdInfo {
        pbi_flags: u32,
        pbi_status: u32,
        pbi_xstatus: u32,
        pbi_pid: u32,
        pbi_ppid: u32,
        pbi_uid: u32,
        pbi_gid: u32,
        pbi_ruid: u32,
        pbi_rgid: u32,
        pbi_svuid: u32,
        pbi_svgid: u32,
        rfu_1: u32,
        pbi_comm: [u8; 16],
        pbi_name: [u8; 32],
        pbi_nfiles: u32,
        pbi_pgid: u32,
        pbi_pjobc: u32,
        e_tdev: u32,
        e_tpgid: u32,
        pbi_nice: i32,
        pbi_start_tvsec: u64,
        pbi_start_tvusec: u64,
    }
    const PROC_PIDTBSDINFO: i32 = 3;
    unsafe extern "C" {
        fn proc_pidinfo(
            pid: i32,
            flavor: i32,
            arg: u64,
            buffer: *mut c_void,
            buffersize: i32,
        ) -> i32;
    }
    let mut info = std::mem::MaybeUninit::<ProcBsdInfo>::zeroed();
    let size = std::mem::size_of::<ProcBsdInfo>() as i32;
    // SAFETY: the buffer is sized for the struct the flavor fills.
    let n = unsafe {
        proc_pidinfo(
            pid as i32,
            PROC_PIDTBSDINFO,
            0,
            info.as_mut_ptr() as *mut c_void,
            size,
        )
    };
    if n != size {
        let error = std::io::Error::last_os_error();
        return if error.raw_os_error() == Some(libc::ESRCH) {
            Ok(None)
        } else {
            Err(error)
        };
    }
    // SAFETY: the call filled the whole struct.
    let info = unsafe { info.assume_init() };
    if info.pbi_status == 5 {
        // SZOMB: no live process behind this session.
        return Ok(None);
    }
    Ok(Some(
        info.pbi_start_tvsec * 1_000_000 + info.pbi_start_tvusec,
    ))
}

#[cfg(test)]
mod process_tests {
    use super::*;

    #[test]
    fn kernel_process_generation_and_confirmed_exit() {
        let current = observe_process_start(std::process::id()).unwrap().unwrap();
        assert_eq!(process_start(std::process::id()), Some(current));
        // Only this test-owned child exits; no user's application is touched.
        let mut child = std::process::Command::new("/bin/cat")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let pid = child.id();
        assert!(observe_process_start(pid).unwrap().is_some());
        drop(child.stdin.take());
        child.wait().unwrap();
        assert_eq!(observe_process_start(pid).unwrap(), None);
    }
}
