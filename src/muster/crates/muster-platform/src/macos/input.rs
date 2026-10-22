//! The user's own input, counted: HID event counters that AX writes do not
//! advance, so a change between snapshot and write means a person touched
//! the machine and the write must not go ahead.

unsafe extern "C" {
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
