//! Local input that can reach one desktop app (ADR 0115).
//!
//! The HID counters count every key and pointer event on the Mac, but most of
//! them go to whatever the person is using, not to the app Muster drives in
//! the background. A key reaches the app with keyboard focus; a pointer event
//! reaches the window under the pointer. The platform reads those owners and
//! the counters; this module decides which input could have reached a target
//! app and turns that into a generation that changes only then. Native writes
//! compare it exactly as they compared the Mac-wide generation before.
//!
//! Locked Use keeps the Mac-wide generation: there, any local input means a
//! person is at the Mac, whatever app it reaches.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

/// Which process an input class would reach when a sample was taken.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Owner {
    Process(u32),
    /// No app has keyboard focus, or no window is under the pointer.
    Nobody,
    /// The OS could not say; counted as the target.
    Unknown,
}

/// One reading of the platform's input facts. It names no target, so one
/// sample serves every tracked app.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sample {
    pub at: Instant,
    /// Key and modifier events so far.
    pub keys: u64,
    /// Pointer movement, button, drag and scroll events so far.
    pub pointer: u64,
    /// The application with keyboard focus.
    pub focus: Owner,
    /// The owner of the frontmost visible window under the pointer.
    pub under_pointer: Owner,
}

/// Samples further apart than this cannot show where input went in between:
/// any input in such a gap counts as reaching every target.
pub const MAX_GAP: Duration = Duration::from_millis(250);
/// How often the platform samples while a target is watched.
pub const SAMPLE_EVERY: Duration = Duration::from_millis(50);
/// How long after its last query a target stays watched.
pub const WATCH: Duration = Duration::from_secs(20);

fn reaches(owner: Owner, targets: &[u32]) -> bool {
    match owner {
        Owner::Process(pid) => targets.contains(&pid),
        Owner::Nobody => false,
        Owner::Unknown => true,
    }
}

/// Whether a target is the focused app. Then every input class may act on
/// it: its menus and the Dock take pointer events outside its windows.
fn focused(sample: &Sample, targets: &[u32]) -> bool {
    reaches(sample.focus, targets)
}

/// Whether a pointer event now would reach a target.
fn pointed(sample: &Sample, targets: &[u32]) -> bool {
    focused(sample, targets) || reaches(sample.under_pointer, targets)
}

/// Whether input counted between two consecutive samples could have reached
/// one of `targets`: a key while a target had focus at either end, a pointer
/// event while a target had focus or a window under the pointer at either
/// end, or any input across a gap too long to tell. A focus change with no
/// input, such as Muster's own navigation activating the app, reaches nothing.
pub fn reached(targets: &[u32], before: &Sample, after: &Sample) -> bool {
    let keys = after.keys != before.keys;
    let pointer = after.pointer != before.pointer;
    if !keys && !pointer {
        return false;
    }
    if after.at < before.at || after.at.duration_since(before.at) > MAX_GAP {
        return true;
    }
    (keys && (focused(before, targets) || focused(after, targets)))
        || (pointer && (pointed(before, targets) || pointed(after, targets)))
}

/// How long input that could reach `targets` now has been quiet. An input
/// class that reaches none of them does not delay admission.
pub fn idle(
    targets: &[u32],
    focus: Owner,
    under_pointer: Owner,
    keys_idle: Duration,
    pointer_idle: Duration,
) -> Duration {
    let mut idle = Duration::MAX;
    if reaches(focus, targets) {
        idle = idle.min(keys_idle).min(pointer_idle);
    }
    if reaches(under_pointer, targets) {
        idle = idle.min(pointer_idle);
    }
    idle
}

#[derive(Debug)]
struct Track {
    last: Sample,
    reached: u64,
    queried: Instant,
}

/// Every target's generation, fed the same samples in order. A target is
/// never forgotten: a generation read long ago must stay comparable, and the
/// gap rule makes a late comparison as strict as the Mac-wide one.
#[derive(Debug, Default)]
pub struct Tracks(HashMap<u32, Track>);

impl Tracks {
    /// Feed one sample to every target.
    pub fn record(&mut self, sample: Sample) {
        for (&pid, track) in &mut self.0 {
            if reached(&[pid], &track.last, &sample) {
                track.reached += 1;
            }
            // An out-of-order sample cannot replace a newer one.
            if sample.at >= track.last.at {
                track.last = sample;
            }
        }
    }

    /// The target's generation after `sample`. It changes only when input
    /// could have reached the target since the previous sample.
    pub fn generation(&mut self, target: u32, sample: Sample) -> u64 {
        self.record(sample);
        let track = self.0.entry(target).or_insert(Track {
            last: sample,
            reached: 0,
            queried: sample.at,
        });
        track.queried = track.queried.max(sample.at);
        track.reached
    }

    /// Whether a target was queried recently enough to need sampling.
    pub fn watching(&self, now: Instant) -> bool {
        self.0
            .values()
            .any(|track| now.saturating_duration_since(track.queried) < WATCH)
    }
}

static TRACKS: LazyLock<Mutex<Tracks>> = LazyLock::new(Default::default);

/// The process-wide [`Tracks::generation`]. The caller supplies samples in
/// the order they were taken.
pub fn generation(target: u32, sample: Sample) -> u64 {
    TRACKS.lock().unwrap().generation(target, sample)
}

/// The process-wide [`Tracks::record`].
pub fn record(sample: Sample) {
    TRACKS.lock().unwrap().record(sample);
}

/// The process-wide [`Tracks::watching`].
pub fn watching(now: Instant) -> bool {
    TRACKS.lock().unwrap().watching(now)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CODEX: u32 = 10;
    const EDITOR: u32 = 20;

    fn sample(at: Instant, keys: u64, pointer: u64, focus: Owner, under: Owner) -> Sample {
        Sample {
            at,
            keys,
            pointer,
            focus,
            under_pointer: under,
        }
    }

    #[test]
    fn input_elsewhere_does_not_reach_the_target() {
        let t0 = Instant::now();
        let t1 = t0 + Duration::from_millis(50);
        let editor = Owner::Process(EDITOR);
        let before = sample(t0, 1, 1, editor, editor);
        // Typing and scrolling in the editor.
        assert!(!reached(
            &[CODEX],
            &before,
            &sample(t1, 9, 9, editor, editor)
        ));
        // Switching between two other apps.
        let other = Owner::Process(30);
        assert!(!reached(
            &[CODEX],
            &before,
            &sample(t1, 9, 1, other, editor)
        ));
        // Keys while nobody has focus, a pointer over no window.
        assert!(!reached(
            &[CODEX],
            &sample(t0, 1, 1, Owner::Nobody, Owner::Nobody),
            &sample(t1, 2, 2, Owner::Nobody, Owner::Nobody)
        ));
    }

    #[test]
    fn input_that_may_reach_the_target_counts() {
        let t0 = Instant::now();
        let t1 = t0 + Duration::from_millis(50);
        let codex = Owner::Process(CODEX);
        let editor = Owner::Process(EDITOR);
        // Typing into the target.
        assert!(reached(
            &[CODEX],
            &sample(t0, 1, 1, codex, editor),
            &sample(t1, 2, 1, codex, editor)
        ));
        // Switching to it, or away from it, with a key.
        assert!(reached(
            &[CODEX],
            &sample(t0, 1, 1, editor, editor),
            &sample(t1, 2, 1, codex, editor)
        ));
        assert!(reached(
            &[CODEX],
            &sample(t0, 1, 1, codex, editor),
            &sample(t1, 2, 1, editor, editor)
        ));
        // Clicking or scrolling its window without activating it.
        assert!(reached(
            &[CODEX],
            &sample(t0, 1, 1, editor, codex),
            &sample(t1, 1, 2, editor, codex)
        ));
        assert!(reached(
            &[CODEX],
            &sample(t0, 1, 1, editor, editor),
            &sample(t1, 1, 2, editor, codex)
        ));
        // Keys while the pointer rests on its window do not; pointer events do.
        assert!(!reached(
            &[CODEX],
            &sample(t0, 1, 1, editor, codex),
            &sample(t1, 2, 1, editor, codex)
        ));
        // While it has focus, pointer events outside its windows count too:
        // its menus and the Dock act on it.
        assert!(reached(
            &[CODEX],
            &sample(t0, 1, 1, codex, Owner::Nobody),
            &sample(t1, 1, 2, codex, Owner::Nobody)
        ));
    }

    #[test]
    fn unknown_owners_gaps_and_disorder_count_as_reaching() {
        let t0 = Instant::now();
        let editor = Owner::Process(EDITOR);
        let near = t0 + Duration::from_millis(50);
        assert!(reached(
            &[CODEX],
            &sample(t0, 1, 1, Owner::Unknown, editor),
            &sample(near, 2, 1, editor, editor)
        ));
        assert!(reached(
            &[CODEX],
            &sample(t0, 1, 1, editor, editor),
            &sample(near, 1, 2, editor, Owner::Unknown)
        ));
        let far = t0 + MAX_GAP + Duration::from_millis(1);
        let before = sample(t0, 1, 1, editor, editor);
        assert!(reached(
            &[CODEX],
            &before,
            &sample(far, 2, 1, editor, editor)
        ));
        // A long gap with no input reaches nothing.
        assert!(!reached(
            &[CODEX],
            &before,
            &sample(far, 1, 1, editor, editor)
        ));
        assert!(reached(
            &[CODEX],
            &sample(near, 2, 1, editor, editor),
            &before
        ));
    }

    #[test]
    fn muster_activating_the_target_is_not_input() {
        let t0 = Instant::now();
        let t1 = t0 + Duration::from_millis(50);
        let before = sample(t0, 5, 5, Owner::Process(EDITOR), Owner::Process(EDITOR));
        let after = sample(t1, 5, 5, Owner::Process(CODEX), Owner::Process(CODEX));
        assert!(!reached(&[CODEX], &before, &after));
    }

    #[test]
    fn generation_changes_only_for_reaching_input_and_stays_comparable() {
        let mut tracks = Tracks::default();
        let t0 = Instant::now();
        let step = |n: u64| t0 + Duration::from_millis(50 * n);
        let editor = Owner::Process(EDITOR);
        let codex = Owner::Process(CODEX);
        let first = tracks.generation(CODEX, sample(step(0), 1, 1, editor, editor));
        // The editor's tracker sees the same samples independently.
        let editor_first = tracks.generation(EDITOR, sample(step(1), 2, 1, editor, editor));
        assert_eq!(
            tracks.generation(CODEX, sample(step(2), 3, 1, editor, editor)),
            first
        );
        assert_ne!(
            tracks.generation(EDITOR, sample(step(3), 4, 1, editor, editor)),
            editor_first
        );
        // A brief visit to the target between two queries is caught by a
        // sample recorded in between (the platform's sampler).
        tracks.record(sample(step(4), 5, 1, codex, editor));
        let later = tracks.generation(CODEX, sample(step(5), 6, 1, editor, editor));
        assert_ne!(later, first);
        // Never forgotten, and strict after a gap.
        let long = t0 + Duration::from_secs(60);
        assert!(!tracks.watching(long));
        assert_ne!(
            tracks.generation(CODEX, sample(long, 7, 1, editor, editor)),
            later
        );
        assert!(tracks.watching(long));
    }

    #[test]
    fn idle_counts_only_classes_that_reach_the_targets() {
        let editor = Owner::Process(EDITOR);
        let codex = Owner::Process(CODEX);
        let keys = Duration::from_millis(10);
        let pointer = Duration::from_millis(400);
        assert_eq!(idle(&[CODEX], editor, editor, keys, pointer), Duration::MAX);
        assert_eq!(idle(&[CODEX], editor, codex, keys, pointer), pointer);
        assert_eq!(idle(&[CODEX], editor, codex, pointer, keys), keys);
        assert_eq!(idle(&[CODEX], codex, editor, keys, pointer), keys);
        assert_eq!(idle(&[CODEX], codex, editor, pointer, keys), keys);
        assert_eq!(
            idle(&[CODEX, 11], Owner::Process(11), editor, keys, pointer),
            keys
        );
        assert_eq!(idle(&[], Owner::Unknown, editor, keys, pointer), keys);
        assert_eq!(idle(&[], editor, editor, keys, pointer), Duration::MAX);
    }
}
