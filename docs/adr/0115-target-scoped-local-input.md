# ADR 0115: Count only local input that could reach the target app

- Date: 2026-11-04
- Status: Accepted; source and tests verified, device acceptance pending.
  Amends [ADR 0046](0046-send-recovery-and-quiet-admission.md) and
  [ADR 0087](0087-idle-compaction-on-a-locked-or-busy-mac.md).
- Related: [SECURITY.md](../SECURITY.md) invariant I10,
  [ADR 0039](0039-exact-session-navigation.md),
  [LOCKED_USE.md](../LOCKED_USE.md)

## Context

Native control already writes through Accessibility without moving the
pointer, typing through the HID stream or activating the app: drafts are
`AXValue` or `AXSelectedText` writes, sends are `AXPress`, and the two keys
Muster sends (Space for a question's Submit, Tab for a completion) are posted
to the app with `AXUIElementPostKeyboardEvent`.

The guards around those writes did not match. Every check compared the
Mac-wide HID event counters (`user_input_generation`) and every admission
wait read the Mac-wide idle time (`user_input_idle`). A person typing in a
terminal or scrolling a browser:

- delayed a phone send by up to 3 s and then refused it with "The Mac is
  being used locally";
- aborted a send, a question answer or a navigation at any of its
  generation checks, with the draft preserved and nothing sent;
- held idle compaction for up to 30 s, then skipped it.

None of that input could reach the app Muster was writing to. The person and
Muster could share the Mac, as a person and an assistant using another app
in the background do, if the checks counted only input that could reach the
target.

## Decision

Native writes to one app count only local input that could reach that app.
Locked Use and native session creation keep the Mac-wide counters.

- **What reaches an app** is decided in Shared Rust,
  `muster-agent::local_input`:
  - Key events (key down, key up, modifier changes) reach the app with
    keyboard focus.
  - Pointer events (movement, buttons, drags, scroll) reach the app whose
    window is under the pointer. They also count when the target has keyboard
    focus, so its menu bar and Dock menu are covered.
  - An owner that cannot be read counts as the target.
  - Activation without input does not count. Exact-route navigation opens
    the target with `open -a` (ADR 0039); that alone must not refuse the
    following write. Typing after it reaches the now-focused target and does
    count.
  - A gap of more than 250 ms between two samples, or samples out of order,
    count as input that reached the target: a person could have switched to
    it, typed and switched back between them.
- **Each target has its own generation.** `local_input::generation(pid,
  sample)` returns a counter that changes only when input could have reached
  that pid since its previous sample. Trackers are never dropped, so a
  generation stays comparable for the life of the daemon. A check compares
  two values of the same pid's generation exactly as the Mac-wide counter was
  compared.
- **A sampler fills the gaps.** While any target was queried in the last
  20 s, a platform thread samples every 50 ms. Otherwise it parks. Samples
  reach Shared Rust in the order they were taken.
- **The platform supplies facts only** (`muster-platform::macos::input`):
  key and pointer HID counters and idle times, the focused application
  (`AXFocusedApplication` on the system-wide element), and the owner of the
  frontmost visible window under the pointer (`CGWindowListCopyWindowInfo`,
  which needs no Screen Recording permission). `target_input_generation`,
  `element_input_generation` and `target_input_idle` wrap them.
- **Call sites.**
  - The native app controller, its question, queue and option adapters, and
    the chat driver compare `target_input_generation(pid)` for the process
    they attached to. Draft and clear helpers use the owner of the element
    they write.
  - The send admission wait (250 ms within 3 s) and the idle-compaction wait
    (1 s within 30 s) read `target_input_idle` for the processes of the
    session's app. When that app is unknown or not running they read the
    Mac-wide idle time, as before.
  - The refusal now reads "The app is being used locally".
- **Held modifiers block posted keys.** An application-directed key is not
  part of the HID stream, and whether the app merges a held Command or Option
  into it is not proven. Space for a question's Submit, the question
  editor's Backspace and the completion Tab are refused while
  `CGEventSourceFlagsState` reports Shift, Control, Option, Command or Fn
  held. The Tab is also refused if the target's generation changed since the
  completion started.
- **Unchanged.**
  - Locked Use leases and the watchdog end on any local input anywhere.
  - Native session creation activates the app and keeps the Mac-wide
    counters and its 500 ms idle wait.
  - Input that reaches the target still refuses the write: the person and
    Muster cannot use the same app at the same time.

## Alternatives

- **Keep the Mac-wide counters.** Simple and strict, but it makes Muster
  unusable on a Mac someone is working at, which is the situation remote
  control is for.
- **Tag each operation with the app that had focus when it started.**
  Stateless, but exact-route navigation activates the target, which changes
  the tag without any input and would refuse every routed send.
- **Compare only the two samples at the ends of each check.** A person could
  switch to the target, type and switch back between them. The sampler and
  the gap rule close that window.
- **A virtual display or a separate login session for the target app.**
  Investigated and set aside: it moves the app away from the person who also
  uses it, and Screen Recording only adds observation, not isolation.

## Consequences

- A person can keep working in other apps while the phone sends, answers or
  navigates. Muster still yields the moment input could reach the target.
- Remaining exposure, each falling back to a refusal or to a narrow window:
  - A click-through overlay window (alpha above zero, ignoring the pointer)
    over the target attributes pointer input to the overlay's owner.
  - A drag that ends on a background target window between two samples is
    seen only as pointer input under the other window.
  - A daemon too busy to sample within 250 ms refuses rather than guesses.
- Every query reads the focused application and the window list, under a
  millisecond each in local probes (p50 0.03 ms and 0.23 ms).
- Device acceptance has to show sends succeeding while a person types and
  scrolls in another app, and refusing while they type in the target.

## Verification

- Source and tests: `muster-agent` unit tests cover input elsewhere, input
  that may reach the target (including a focused target with the pointer
  elsewhere), unknown owners, gaps, out-of-order samples, activation without
  input, per-target generations and idle time by input class.
- Builds: `cargo build`, strict clippy and rustfmt on the workspace crates.
- Signing, publication and installation: not part of this change.
- Runtime and device acceptance: pending. Repeat phone sends to an agent
  session while a person types and scrolls in another app, then while they
  type in the target app; record success and refusal counts.
