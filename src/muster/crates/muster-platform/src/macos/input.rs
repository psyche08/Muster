//! The user's own input, counted: HID event counters that AX writes do not
//! advance, so a change between snapshot and write means a person touched
//! the machine and the write must not go ahead.
//!
//! Native writes to one app count only input that could reach it (ADR 0115):
//! this adapter reads which app has keyboard focus and whose window is under
//! the pointer, and `muster_agent::local_input` decides. Locked Use keeps the
//! Mac-wide counters.

use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use core_foundation::array::CFArray;
use core_foundation::base::{CFType, TCFType};
use core_foundation_sys::base::{CFRelease, CFTypeRef};
use core_foundation_sys::dictionary::{CFDictionaryGetValue, CFDictionaryRef};
use core_foundation_sys::number::{CFNumberGetValue, CFNumberRef};
use core_foundation_sys::string::CFStringRef;
use muster_agent::local_input::{self, Owner, Sample};

use super::ax::Element;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct CGPoint {
    x: f64,
    y: f64,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct CGRect {
    origin: CGPoint,
    size: CGPoint,
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventSourceSecondsSinceLastEventType(state_id: i32, event_type: u32) -> f64;
    fn CGEventSourceCounterForEventType(state_id: i32, event_type: u32) -> u32;
    fn CGEventSourceFlagsState(state_id: i32) -> u64;
    fn CGEventCreate(source: CFTypeRef) -> CFTypeRef;
    fn CGEventGetLocation(event: CFTypeRef) -> CGPoint;
    fn CGWindowListCopyWindowInfo(option: u32, relative_to: u32) -> CFTypeRef;
    fn CGRectMakeWithDictionaryRepresentation(dict: CFDictionaryRef, rect: *mut CGRect) -> bool;
    static kCGWindowOwnerPID: CFStringRef;
    static kCGWindowBounds: CFStringRef;
    static kCGWindowAlpha: CFStringRef;
}

const COMBINED_SESSION_STATE: i32 = 0;
const HID_SYSTEM_STATE: i32 = 1;
/// Mouse movement, drags, buttons, keys, modifiers and scroll, including
/// release events. AX calls do not advance these HID-source counters.
const TYPES: [u32; 13] = [1, 2, 3, 4, 5, 6, 7, 10, 11, 12, 22, 25, 26];
/// Key down/up and modifier changes: they reach the app with keyboard focus.
const KEY_TYPES: [u32; 3] = [10, 11, 12];
/// Pointer movement, buttons, drags and scroll: they reach the window under
/// the pointer.
const POINTER_TYPES: [u32; 11] = [1, 2, 3, 4, 5, 6, 7, 22, 25, 26, 27];
/// Shift, Control, Option, Command and Fn.
const MODIFIERS: u64 = 0x0002_0000 | 0x0004_0000 | 0x0008_0000 | 0x0010_0000 | 0x0080_0000;
/// `kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements`.
const ON_SCREEN_WINDOWS: u32 = 1 | 16;
/// `kAXErrorNoValue`: no application has keyboard focus.
const NO_VALUE: i32 = -25212;

fn count(types: &[u32]) -> u64 {
    types
        .iter()
        // SAFETY: plain C call with constant arguments.
        .map(|t| unsafe { CGEventSourceCounterForEventType(HID_SYSTEM_STATE, *t) } as u64)
        .sum()
}

fn idle_of(types: &[u32]) -> Duration {
    let seconds = types
        .iter()
        .map(|t| {
            // SAFETY: CoreGraphics scalar query with constant HID state/event types.
            unsafe { CGEventSourceSecondsSinceLastEventType(HID_SYSTEM_STATE, *t) }
        })
        .fold(f64::MAX, f64::min);
    if seconds.is_finite() && seconds >= 0.0 {
        Duration::from_secs_f64(seconds.min(86400.0))
    } else {
        Duration::ZERO
    }
}

/// Every local input on this Mac, whatever it reaches.
pub fn user_input_generation() -> u64 {
    count(&TYPES)
}

/// Native observation only; admission duration is chosen by Shared Rust.
pub fn user_input_idle() -> Duration {
    idle_of(&TYPES)
}

/// The application with keyboard focus.
pub(super) fn focus() -> Owner {
    match Element::system_wide().element_result("AXFocusedApplication") {
        Ok(app) => app.pid().map_or(Owner::Unknown, Owner::Process),
        Err(NO_VALUE) => Owner::Nobody,
        Err(_) => Owner::Unknown,
    }
}

/// The owner of the frontmost visible window under the pointer. Window
/// owners and bounds need no Screen Recording permission.
fn under_pointer() -> Owner {
    // SAFETY: a NULL source is allowed; the +1 event is released below.
    let event = unsafe { CGEventCreate(std::ptr::null()) };
    if event.is_null() {
        return Owner::Unknown;
    }
    // SAFETY: a valid event, released once after reading its location.
    let point = unsafe { CGEventGetLocation(event) };
    unsafe { CFRelease(event) };
    // SAFETY: returns a +1 array (front to back) or NULL.
    let raw = unsafe { CGWindowListCopyWindowInfo(ON_SCREEN_WINDOWS, 0) };
    if raw.is_null() {
        return Owner::Unknown;
    }
    // SAFETY: a valid +1 CFArray, released when dropped.
    let windows: CFArray<CFType> = unsafe { CFArray::wrap_under_create_rule(raw as _) };
    for window in windows.iter() {
        let info = window.as_CFTypeRef() as CFDictionaryRef;
        // SAFETY: CoreGraphics' constant keys; values are owned by `info`.
        let (bounds, alpha, owner) = unsafe {
            (
                CFDictionaryGetValue(info, kCGWindowBounds as _),
                CFDictionaryGetValue(info, kCGWindowAlpha as _),
                CFDictionaryGetValue(info, kCGWindowOwnerPID as _),
            )
        };
        let mut rect = CGRect::default();
        // SAFETY: `bounds` is the window's bounds dictionary, checked non-null.
        if bounds.is_null()
            || !unsafe { CGRectMakeWithDictionaryRepresentation(bounds as _, &mut rect) }
        {
            continue;
        }
        let inside = point.x >= rect.origin.x
            && point.y >= rect.origin.y
            && point.x < rect.origin.x + rect.size.x
            && point.y < rect.origin.y + rect.size.y;
        let mut opacity = 1.0f64;
        if !alpha.is_null() {
            // SAFETY: kCFNumberFloat64Type into an f64.
            unsafe { CFNumberGetValue(alpha as CFNumberRef, 6, (&mut opacity as *mut f64).cast()) };
        }
        if !inside || opacity <= 0.0 {
            continue;
        }
        let mut pid = 0i32;
        // SAFETY: kCFNumberSInt32Type into an i32.
        let read = !owner.is_null()
            && unsafe { CFNumberGetValue(owner as CFNumberRef, 3, (&mut pid as *mut i32).cast()) };
        return if read && pid > 0 {
            Owner::Process(pid as u32)
        } else {
            Owner::Unknown
        };
    }
    Owner::Nobody
}

fn sample() -> Sample {
    Sample {
        at: Instant::now(),
        keys: count(&KEY_TYPES),
        pointer: count(&POINTER_TYPES),
        focus: focus(),
        under_pointer: under_pointer(),
    }
}

/// Samples reach Shared Rust in the order they were taken.
static SAMPLING: Mutex<()> = Mutex::new(());
static SAMPLER: OnceLock<Option<std::thread::Thread>> = OnceLock::new();

fn take_sample(record: impl FnOnce(Sample) -> u64) -> u64 {
    let _order = SAMPLING.lock().unwrap_or_else(PoisonError::into_inner);
    record(sample())
}

/// Between queries, a person could switch to the app, type and switch back.
/// While any app is watched, sample often enough that the gap rule never
/// has to stand in for a sample.
fn wake_sampler() {
    let sampler = SAMPLER.get_or_init(|| {
        std::thread::Builder::new()
            .name("muster-input".into())
            .spawn(|| {
                loop {
                    if local_input::watching(Instant::now()) {
                        take_sample(|sample| {
                            local_input::record(sample);
                            0
                        });
                        std::thread::sleep(local_input::SAMPLE_EVERY);
                    } else {
                        std::thread::park_timeout(Duration::from_secs(60));
                    }
                }
            })
            .inspect_err(|error| log::warn!("local input sampler not started: {error}"))
            .ok()
            .map(|handle| handle.thread().clone())
    });
    if let Some(sampler) = sampler {
        sampler.unpark();
    }
}

/// A generation of the input that could reach the app `pid`; it changes only
/// when such input happened. Compare it like [`user_input_generation`].
pub fn target_input_generation(pid: u32) -> u64 {
    let generation = take_sample(|sample| local_input::generation(pid, sample));
    wake_sampler();
    generation
}

/// [`target_input_generation`] for the app that owns `element`; every local
/// input counts when its owner is unknown.
pub fn element_input_generation(element: &Element) -> u64 {
    element
        .pid()
        .map_or_else(user_input_generation, target_input_generation)
}

/// How long input that could reach any of `pids` has been quiet.
pub fn target_input_idle(pids: &[u32]) -> Duration {
    local_input::idle(
        pids,
        focus(),
        under_pointer(),
        idle_of(&KEY_TYPES),
        idle_of(&POINTER_TYPES),
    )
}

/// Whether a modifier key is held now, by a person or by software. An
/// application-directed key posted meanwhile could pick it up.
pub fn modifiers_held() -> bool {
    // SAFETY: CoreGraphics scalar query with a constant state.
    unsafe { CGEventSourceFlagsState(COMBINED_SESSION_STATE) & MODIFIERS != 0 }
}
