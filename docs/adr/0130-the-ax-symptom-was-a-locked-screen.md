# ADR 0130: The Accessibility symptom does not reproduce on an unlocked screen

- Date: 2026-11-05
- Status: Accepted; measured on the owner's Mac, locked and unlocked, against
  two controls. Corrects the premise of ADR 0127 and one claim in ADR 0129.
- Related: [ADR 0127](0127-the-daemon-stays-out-of-launch-services.md) (whose
  premise this corrects), [ADR 0129](0129-ask-ax-for-windows-by-name.md) (the
  instrument, and one sentence this corrects),
  [ADR 0128](0128-the-panel-helper-keeps-its-own-identity.md),
  [ADR 0123](0123-the-daemon-leaves-the-dock.md),
  [ADR 0007](0007-desktop-manual-locked-use.md) (the cover a Locked Use
  transaction raises, which is part of why "unlocked" is not one state)

## Context

A line of work starting at ADR 0127 was built on one observation: an
Accessibility client could not see Muster's windows. The window server showed a
window; an AX client enumerating the application's windows got nothing. ADR 0127
attributed it to the daemon registering as a second copy of the app, removed
that registration, and the symptom survived — recorded in that ADR's install
acceptance. ADR 0129 then added `muster debug ax-windows` to ask the attributes
by name, because no instrument could tell "no tree" from "tree, empty list".

The instrument was installed from `396cb72` and run against three applications:
Muster, a minimal SwiftUI control built for the purpose, and Finder.

## Decision

**Record that the symptom does not reproduce on an unlocked screen, and treat
the screen's lock state as part of every Accessibility measurement.**

The first run, at 11:35 with `CGSSessionScreenIsLocked = Yes`, produced
nonsense that looked like an instrument fault: index 0 of every array, and
`AXMainWindow` and `AXFocusedWindow`, resolved to something whose `AXRole` was
`AXApplication` and whose title was the application's own — in all three
applications. `elementAtPosition` returned `loginwindow` (pid 657) for three
different coordinates.

The second run, at 11:41 with the screen unlocked and nothing else changed:

```
Muster      AXChildren: count=2 roles=[AXWindow, AXMenuBar]
            AXWindows: status=0 count=1
              [0] role=AXWindow subrole="AXStandardWindow" title=""
            AXMainWindow / AXFocusedWindow: both resolve to that AXWindow
MinSwiftUI  AXWindows count=1  [0] role=AXWindow subrole="AXStandardWindow" title="MinSwiftUI"
Finder      AXWindows count=2  [0] role=AXWindow subrole="AXStandardWindow" title="Meituan"
                               AXMainWindow / AXFocusedWindow: -25212
```

Every mislabel is gone. **Muster's window is in the Accessibility tree.** The
computer-use client that returned `[]` returns the window too, matching the
window server exactly: `window_id 166908`, `is_main`, 1728x995 at (0,34).

So the premise is withdrawn. What was measured was real each time it was
measured; what it was taken to mean was not.

## What is not claimed

- **Not that every earlier reading was taken locked.** The `[]` readings at
  11:03–11:06 cannot be placed either way: `CGSSessionScreenLockedTime` records
  only the current lock's start, and the daemon log holds one lock-state line
  in that window, `03:26:04Z screen_locked=false`, one second before a Locked
  Use transaction finished and relocked under cover. That relock is where the
  11:26 lock came from — the machine's own, not the owner's.
- **Not that "unlocked" is one state.** A Locked Use transaction unlocks the
  screen beneath a cover at `CGShieldingWindowLevel() + 1` (ADR 0007). A
  measurement taken then is not the same environment as an ordinary unlocked
  desktop, and nothing here establishes which one the early readings saw.
- **Not that ADR 0127's change should be reverted.** Removing the duplicate
  registration stands on its own: one ASN instead of two, no Dock tile. Only
  the causal claim about Accessibility goes.

## One difference that survives

Muster's `AXWindow` reports `title=""`, where the minimal control reports
`"MinSwiftUI"` and Finder reports `"Meituan"`. This is a trusted AX read, so
unlike `kCGWindowName` — redacted without Screen Recording — nothing is
withholding it. The scene is `WindowGroup("Muster")`. This is the only thing in
these readings that still separates the app from both controls. It is recorded,
not explained.

## Consequences

- **ADR 0127's premise is corrected** in that record: its install acceptance
  reported the symptom surviving, and that reading does not reproduce unlocked.
- **ADR 0129 keeps its instrument and loses one sentence.** "For an application
  element `AXChildren` yields the menu bar, not the windows" is contradicted by
  its own output: `AXChildren` is count=2 for Muster and count=3 for Finder,
  with `AXWindow` first in both. Why `ax-dump` shows no `AXWindow` for any
  application, Finder's eight included, is therefore **unexplained** — the
  measurement stands, the account of it does not. One untested hypothesis, left
  untested here: `dump` skips a node it has already seen, and if `AXChildren[0]`
  ever resolves to the application element the walk would never descend into it.
- **`elementAtPosition` hit-tests the front window**, as it should. Unlocked it
  returned `pid 42272` — Claude.app — for all three coordinates, because that
  window was in front of all three targets. To say anything about a particular
  application, its window has to be frontmost, or the read has to start from a
  known element.
- Accessibility findings on this fleet now carry the lock state they were taken
  under, or they cannot be read later.

## Verification

Two runs of `muster debug ax-windows` against pids 92083 (Muster), 83952 (the
minimal SwiftUI control) and 2248 (Finder), at 11:35 locked and 11:41 unlocked,
on the daemon built and installed from `396cb72`. Lock state read from
`ioreg -n Root -d1` both times. The computer-use client's `app_list_windows`
re-run unlocked against `dev.linsheng.muster`. Window geometry from
`CGWindowListCopyWindowInfo`.
