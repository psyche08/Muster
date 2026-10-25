//! The user's own input, counted: HID event counters that AX writes do not
//! advance, so a change between snapshot and write means a person touched
//! the machine and the write must not go ahead.

unsafe extern "C" {
    fn CGEventSourceSecondsSinceLastEventType(state_id: i32, event_type: u32) -> f64;
    fn CGEventSourceCounterForEventType(state_id: i32, event_type: u32) -> u32;
}

const HID_SYSTEM_STATE: i32 = 1;
/// Mouse movement, drags, buttons, keys, modifiers and scroll, including
/// release events. AX calls do not advance these HID-source counters.
const TYPES: [u32; 13] = [1, 2, 3, 4, 5, 6, 7, 10, 11, 12, 22, 25, 26];

pub fn user_input_generation() -> u64 {
    TYPES
        .iter()
        // SAFETY: plain C call with constant arguments.
        .map(|t| unsafe { CGEventSourceCounterForEventType(HID_SYSTEM_STATE, *t) } as u64)
        .sum()
}

/// Native observation only; admission duration is chosen by Shared Rust.
pub fn user_input_idle() -> std::time::Duration {
    let seconds = TYPES
        .iter()
        .map(|t| {
            // SAFETY: CoreGraphics scalar query with constant HID state/event types.
            unsafe { CGEventSourceSecondsSinceLastEventType(HID_SYSTEM_STATE, *t) }
        })
        .fold(f64::MAX, f64::min);
    if seconds.is_finite() && seconds >= 0.0 {
        std::time::Duration::from_secs_f64(seconds.min(86400.0))
    } else {
        std::time::Duration::ZERO
    }
}
