//! The Locked Use cover (ADR 0223). Each display shows black through an
//! all-zero gamma table where Muster can set one and read it back, and only
//! the others keep the black panel. A panel over the whole screen occludes
//! every window under it, and an occluded Electron window (the Claude app)
//! stops updating what Accessibility reads. A black table occludes nothing.
//!
//! Every display starts under the black panel. Its table is set only once the
//! console has been unlocked and the displays have stayed as they are for a
//! while, so that a display still waking or reconfiguring does not get its
//! ColorSync table back right after. The tables need no record: macOS puts
//! them back when the watchdog ends, however it ends.

use std::time::{Duration, Instant};

unsafe extern "C" {
    fn muster_display_online(displays: *mut u32, capacity: u32) -> u32;
    fn muster_display_mirror_master(display: u32) -> u32;
    fn muster_display_asleep(display: u32) -> bool;
    fn muster_gamma_black(display: u32) -> bool;
    fn muster_gamma_black_holds(display: u32) -> bool;
    fn muster_gamma_restore();
    fn muster_console_cover(displays: *const u32, count: u32) -> bool;
    fn muster_console_cover_keep(displays: *const u32, count: u32);
    fn muster_console_wake_begin(assertion: *mut u32) -> bool;
    fn muster_console_wake_end(assertion: u32) -> bool;
    fn muster_console_cover_healthy(unlocked: bool) -> bool;
    fn muster_console_uncover();
}

/// How long the displays must have stayed as they are, awake, online under
/// the same IDs, and with the console neither locking nor unlocking, before
/// their tables are set.
const STEADY: Duration = Duration::from_secs(3);

/// Why a cover no longer hides the screen. The watchdog sends it to musterd
/// in an `L` frame: the code, then a little-endian detail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Fault {
    /// The online displays changed; how many are online now.
    Displays(u16),
    /// A display's table no longer maps every value to black.
    Gamma,
    /// A black panel failed its check.
    Panel,
}

impl Fault {
    fn code(self) -> (u8, u16) {
        match self {
            Fault::Displays(count) => (1, count),
            Fault::Gamma => (2, 0),
            Fault::Panel => (3, 0),
        }
    }

    pub(super) fn frame(self) -> [u8; 4] {
        let (code, detail) = self.code();
        let [low, high] = detail.to_le_bytes();
        [b'L', code, low, high]
    }

    /// The payload of an `L` frame, for the log.
    pub(super) fn describe(payload: [u8; 3]) -> String {
        let detail = u16::from_le_bytes([payload[1], payload[2]]);
        match payload[0] {
            1 => format!("the online displays changed ({detail} now)"),
            2 => "a display's black gamma table was replaced".into(),
            3 => "a black panel failed its check".into(),
            code => format!("unknown fault {code}"),
        }
    }
}

fn online() -> Vec<u32> {
    let mut displays = [0u32; 32];
    let count = unsafe { muster_display_online(displays.as_mut_ptr(), displays.len() as u32) };
    let mut online = displays[..count as usize].to_vec();
    online.sort_unstable();
    online
}

/// The displays online, each with whether it is asleep, and whether the
/// console is locked.
#[derive(Clone, Debug, PartialEq)]
struct Config {
    displays: Vec<(u32, bool)>,
    locked: bool,
}

/// Whether the displays have stayed as they are for `STEADY`.
struct Steady {
    config: Option<Config>,
    since: Instant,
}
impl Steady {
    fn new() -> Self {
        Self {
            config: None,
            since: Instant::now(),
        }
    }

    fn sample(&mut self, locked: bool) {
        let displays = online()
            .into_iter()
            .map(|display| (display, unsafe { muster_display_asleep(display) }))
            .collect();
        self.observe(Config { displays, locked }, Instant::now());
    }

    fn observe(&mut self, config: Config, at: Instant) {
        if self.config.as_ref() != Some(&config) {
            self.config = Some(config);
            self.since = at;
        }
    }

    /// Every one of `online`, and only those, awake under an unlocked
    /// console, once the displays have been steady.
    fn ready(&self, online: &[u32], now: Instant) -> bool {
        now.duration_since(self.since) >= STEADY
            && self.config.as_ref().is_some_and(|config| {
                !config.locked
                    && config.displays.len() == online.len()
                    && config
                        .displays
                        .iter()
                        .all(|(id, asleep)| online.contains(id) && !asleep)
            })
    }
}

/// The cover over every online display: a black table, or the black panel.
pub(super) struct Shield {
    online: Vec<u32>,
    /// Displays whose table is black.
    black: Vec<u32>,
    panels: Vec<u32>,
    waking: Option<Waking>,
    steady: Steady,
    /// Set once the tables were set, or will no longer be.
    settled: bool,
    /// Cleared once the cover is released.
    raised: bool,
    /// Why displays kept the panel, for the owner's log; sent once.
    note: Option<Note>,
}

/// Why displays kept the black panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Note {
    /// The displays could not be woken.
    Unwoken = 1,
}

impl Note {
    /// An `S` frame: the code.
    pub(super) fn frame(self) -> [u8; 2] {
        [b'S', self as u8]
    }

    pub(super) fn describe(code: u8) -> &'static str {
        match code {
            1 => "the displays could not be woken; every display keeps the black panel",
            _ => "unknown note",
        }
    }
}

/// Keeps the displays that were asleep waking until their tables are set.
struct Waking {
    assertion: u32,
}
impl Drop for Waking {
    fn drop(&mut self) {
        unsafe { muster_console_wake_end(self.assertion) };
    }
}

/// A mirror set shows on one display; its panel covers the set.
fn panel_targets(displays: &[u32]) -> Vec<u32> {
    let mut targets: Vec<u32> = displays
        .iter()
        .map(|&display| unsafe { muster_display_mirror_master(display) })
        .collect();
    targets.sort_unstable();
    targets.dedup();
    targets
}

fn any_asleep(displays: &[u32]) -> bool {
    displays
        .iter()
        .any(|&display| unsafe { muster_display_asleep(display) })
}

impl Shield {
    /// Covers every display with the black panel and wakes those that are
    /// asleep; `settle` sets the tables later. Fails, with nothing changed,
    /// unless every online display is covered.
    pub(super) fn raise() -> Result<Self, String> {
        let online = online();
        if online.is_empty() {
            return Err("no display to cover".into());
        }
        let mut shield = Self {
            online,
            black: Vec::new(),
            panels: Vec::new(),
            waking: None,
            steady: Steady::new(),
            settled: false,
            raised: true,
            note: None,
        };
        if any_asleep(&shield.online) {
            // A display that cannot be woken keeps its panel.
            let mut assertion = 0;
            if unsafe { muster_console_wake_begin(&mut assertion) } {
                shield.waking = Some(Waking { assertion });
            } else {
                shield.note = Some(Note::Unwoken);
                shield.settled = true;
            }
        }
        let targets = panel_targets(&shield.online);
        // Always called: it also starts AppKit for the watchdog.
        let covered = unsafe { muster_console_cover(targets.as_ptr(), targets.len() as u32) };
        shield.panels = targets;
        if !covered {
            shield.release();
            return Err("cannot cover every display".into());
        }
        Ok(shield)
    }

    /// Once the console has been unlocked and every display awake and
    /// unchanged for `STEADY`, sets each display's table to black and takes
    /// the panel off each display that reads back black. True when the cover
    /// changed.
    pub(super) fn settle(&mut self, unlocked: bool) -> bool {
        if self.settled || !self.raised {
            return false;
        }
        self.steady.sample(!unlocked);
        if !self.steady.ready(&self.online, Instant::now()) {
            return false;
        }
        self.settled = true;
        // From here the watchdog's display assertion keeps them awake.
        self.waking = None;
        let (black, panels): (Vec<u32>, Vec<u32>) = self
            .online
            .iter()
            .partition(|&&display| unsafe { muster_gamma_black(display) });
        if black.is_empty() {
            return false;
        }
        self.black = black;
        let targets = panel_targets(&panels);
        unsafe { muster_console_cover_keep(targets.as_ptr(), targets.len() as u32) };
        self.panels = targets;
        true
    }

    /// Why displays kept the panel, once.
    pub(super) fn take_note(&mut self) -> Option<Note> {
        self.note.take()
    }

    /// How many displays show a black table, and how many panels are up.
    pub(super) fn counts(&self) -> [u8; 2] {
        let count = |n: usize| n.min(u8::MAX as usize) as u8;
        [count(self.black.len()), count(self.panels.len())]
    }

    /// What, if anything, no longer holds: the same displays online, each
    /// black table still black, each panel still up.
    pub(super) fn fault(&self, unlocked: bool) -> Option<Fault> {
        let now = online();
        if now != self.online {
            return Some(Fault::Displays(now.len() as u16));
        }
        if self
            .black
            .iter()
            .any(|&display| !unsafe { muster_gamma_black_holds(display) })
        {
            return Some(Fault::Gamma);
        }
        if !self.panels.is_empty() && !unsafe { muster_console_cover_healthy(unlocked) } {
            return Some(Fault::Panel);
        }
        None
    }

    /// The cover is ending: no table is set from here on, while the console
    /// locks again. Idempotent.
    pub(super) fn quiet(&mut self) {
        self.settled = true;
        self.waking = None;
    }

    /// Puts every table back and removes the panels. Idempotent.
    pub(super) fn release(&mut self) {
        self.quiet();
        if !std::mem::take(&mut self.raised) {
            return;
        }
        if !std::mem::take(&mut self.black).is_empty() {
            unsafe { muster_gamma_restore() };
        }
        unsafe { muster_console_uncover() };
    }
}

impl Drop for Shield {
    fn drop(&mut self) {
        self.release();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fault_frame_names_what_failed() {
        for (fault, text) in [
            (Fault::Displays(2), "the online displays changed (2 now)"),
            (Fault::Gamma, "a display's black gamma table was replaced"),
            (Fault::Panel, "a black panel failed its check"),
        ] {
            let frame = fault.frame();
            assert_eq!(frame[0], b'L');
            assert_eq!(Fault::describe([frame[1], frame[2], frame[3]]), text);
        }
        assert_eq!(Fault::describe([9, 0, 0]), "unknown fault 9");
    }

    #[test]
    fn displays_are_ready_only_after_staying_unchanged_awake_and_unlocked() {
        let at = Instant::now();
        let config = |locked, asleep| Config {
            displays: vec![(3, asleep)],
            locked,
        };
        let mut steady = Steady::new();
        steady.observe(config(true, false), at);
        // Locked: not ready, however long.
        assert!(!steady.ready(&[3], at + STEADY * 2));
        // Unlocking starts the wait again.
        let unlocked = at + STEADY;
        steady.observe(config(false, false), unlocked);
        assert!(!steady.ready(&[3], unlocked + STEADY - Duration::from_millis(1)));
        assert!(steady.ready(&[3], unlocked + STEADY));
        // A display that went or came is not ready.
        assert!(!steady.ready(&[3, 4], unlocked + STEADY));
        // Nor one that is asleep.
        let asleep = unlocked + STEADY;
        steady.observe(config(false, true), asleep);
        assert!(!steady.ready(&[3], asleep + STEADY));
        // The same sample again does not start the wait again.
        let woke = asleep + STEADY;
        steady.observe(config(false, false), woke);
        steady.observe(config(false, false), woke + STEADY);
        assert!(steady.ready(&[3], woke + STEADY));
    }
}
