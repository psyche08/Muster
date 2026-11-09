//! The main thread's window-server event queue, for a headless process that
//! Launch Services registers as an application anyway.
//!
//! `musterd` runs from `Muster.app/Contents/MacOS`, so it is registered under
//! the app's bundle id and given a window-server connection (ADR 0132). The
//! window server then expects that connection's event queue to be read on the
//! main thread, and a process that never reads it is flagged unresponsive —
//! which Activity Monitor shows as "Not Responding" whatever the process type
//! (ADR 0164). [`AppEvents::serve`] reads it.
//!
//! The loop is `nextEventMatchingMask:` and `sendEvent:` only, not
//! `-[NSApplication run]`: `run` installs AppKit's default Apple Event
//! handlers, and its `quit` handler would end the daemon without its
//! shutdown whenever anything asked "Muster" to quit and resolved to the
//! daemon's record.

use std::ffi::{CStr, c_void};
use std::marker::PhantomData;

type Id = *mut c_void;
type Sel = *const c_void;

#[link(name = "objc")]
unsafe extern "C" {
    fn objc_getClass(name: *const std::ffi::c_char) -> Id;
    fn sel_registerName(name: *const std::ffi::c_char) -> Sel;
    fn objc_msgSend();
    fn objc_autoreleasePoolPush() -> *mut c_void;
    fn objc_autoreleasePoolPop(pool: *mut c_void);
}

#[link(name = "AppKit", kind = "framework")]
unsafe extern "C" {}

/// `NSApplicationActivationPolicyProhibited`: no Dock tile, no menu bar,
/// never frontmost — Launch Services records it as `BackgroundOnly`.
const ACTIVATION_POLICY_PROHIBITED: isize = 2;
/// `NSEventMaskAny`.
const EVENT_MASK_ANY: u64 = u64::MAX;

fn class(name: &CStr) -> Id {
    // SAFETY: a NUL-terminated name; an unknown class returns null.
    unsafe { objc_getClass(name.as_ptr()) }
}

fn sel(name: &CStr) -> Sel {
    // SAFETY: a NUL-terminated selector name; registration is idempotent.
    unsafe { sel_registerName(name.as_ptr()) }
}

/// The shared `NSApplication`, held on the main thread that took it.
pub struct AppEvents {
    app: Id,
    /// AppKit is main-thread only; keep this off other threads.
    _main_thread: PhantomData<*mut ()>,
}

impl AppEvents {
    /// The shared application, when this is the main thread of an
    /// executable inside an `.app`'s `Contents/MacOS` — the one layout Launch
    /// Services registers. Elsewhere (a `cargo` build, a test harness thread)
    /// there is no registration to service and AppKit is not touched.
    pub fn take() -> Option<Self> {
        // SAFETY: no arguments; reads the calling thread's identity.
        if unsafe { libc::pthread_main_np() } == 0 {
            return None;
        }
        super::procs::process_bundle(std::process::id())?;
        let cls = class(c"NSApplication");
        if cls.is_null() {
            return None;
        }
        // SAFETY: `+[NSApplication sharedApplication]` takes no arguments and
        // returns the process-wide instance; this is the main thread.
        let app = unsafe {
            let send = std::mem::transmute::<
                unsafe extern "C" fn(),
                unsafe extern "C" fn(Id, Sel) -> Id,
            >(objc_msgSend);
            send(cls, sel(c"sharedApplication"))
        };
        (!app.is_null()).then_some(Self {
            app,
            _main_thread: PhantomData,
        })
    }

    /// Leave the Dock: set the activation policy to prohibited, which Launch
    /// Services records as `BackgroundOnly` (ADR 0123's outcome, without
    /// `TransformProcessType`). `false` if AppKit refused.
    pub fn leave_the_dock(&self) -> bool {
        // SAFETY: `-setActivationPolicy:` takes an `NSInteger` and returns a
        // `BOOL`; `self.app` is the live shared application.
        unsafe {
            let send = std::mem::transmute::<
                unsafe extern "C" fn(),
                unsafe extern "C" fn(Id, Sel, isize) -> i8,
            >(objc_msgSend);
            send(
                self.app,
                sel(c"setActivationPolicy:"),
                ACTIVATION_POLICY_PROHIBITED,
            ) != 0
        }
    }

    /// Read the event queue for the rest of the process's life. Blocks in the
    /// main run loop between events, so an idle daemon costs nothing here.
    /// The process ends from another thread, with `std::process::exit`.
    pub fn serve(self) -> ! {
        let date_class = class(c"NSDate");
        let distant_future = sel(c"distantFuture");
        let next_event = sel(c"nextEventMatchingMask:untilDate:inMode:dequeue:");
        let send_event = sel(c"sendEvent:");
        // `NSDefaultRunLoopMode` is toll-free bridged to this constant.
        // SAFETY: an immutable CFString constant exported by CoreFoundation.
        let mode = unsafe { core_foundation_sys::runloop::kCFRunLoopDefaultMode } as Id;
        // SAFETY: each signature below matches the selector it is sent with;
        // `self.app` is the shared application and this is the main thread.
        unsafe {
            let class_send = std::mem::transmute::<
                unsafe extern "C" fn(),
                unsafe extern "C" fn(Id, Sel) -> Id,
            >(objc_msgSend);
            let next = std::mem::transmute::<
                unsafe extern "C" fn(),
                unsafe extern "C" fn(Id, Sel, u64, Id, Id, i8) -> Id,
            >(objc_msgSend);
            let dispatch = std::mem::transmute::<
                unsafe extern "C" fn(),
                unsafe extern "C" fn(Id, Sel, Id),
            >(objc_msgSend);
            loop {
                // The event and the date are autoreleased; drain them per turn.
                let pool = objc_autoreleasePoolPush();
                let until = class_send(date_class, distant_future);
                let event = next(self.app, next_event, EVENT_MASK_ANY, until, mode, 1);
                if !event.is_null() {
                    dispatch(self.app, send_event, event);
                }
                objc_autoreleasePoolPop(pool);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stays_out_of_appkit_off_the_main_thread() {
        // The test harness runs each test on a spawned thread, and a cargo
        // build is not inside an `.app`; either alone must refuse.
        assert!(AppEvents::take().is_none());
    }
}
