# ADR 0131: A locked screen explains the missing windows

- Date: 2026-11-05
- Status: Accepted; measured on the owner's Mac, locked and unlocked, against
  three applications
- Corrects: [ADR 0127](0127-the-daemon-stays-out-of-launch-services.md) (its
  premise) and [ADR 0129](0129-ask-ax-for-windows-by-name.md) (its premise and
  its account of `ax-dump`). Both decisions stand; what they say about the
  Accessibility API does not.
- Related: [ADR 0120](0120-locked-control-without-a-per-operation-signature.md)
  (Locked Use, which unlocks the screen under a cover),
  [ADR 0116](0116-failure-screenshot-analysis.md) (what reads the app's windows)

## Context

The app's windows were reported missing from the Accessibility tree. Two
records were written on that premise. **In an unlocked session the symptom does
not reproduce.** The window is in the tree, and the client that returned an
empty list returns the window:

```
AXChildren: count=2 roles=[AXWindow, AXMenuBar]
AXWindows:  status=0 count=1
  [0] role=AXWindow subrole="AXStandardWindow"
AXMainWindow / AXFocusedWindow: status=0 role=AXWindow subrole="AXStandardWindow"

app_list_windows -> [{"window_id":166908,"is_main":true,
                      "bounds":{"w":1728,"h":995,"x":0,"y":34}}]
```

Those bounds are the window the window server reports.

**What a locked screen does.** With `CGSSessionScreenIsLocked=1`, the
Accessibility API hands back the *application* element in place of window
elements — in every application, not only this one. Measured on three
unrelated apps while locked:

| asked | Muster | minimal SwiftUI control | Finder |
| --- | --- | --- | --- |
| `AXChildren[0]` | `AXApplication` | `AXApplication` | `AXApplication` |
| `AXWindows[0]` | `AXApplication` | `AXApplication` | `AXApplication` |
| `AXWindows[1]` | — | — | `AXScrollArea` |
| `ax-dump` `AXWindow` lines | 0 | 0 | 0 |

The giveaway is Finder: `AXWindows[0]` came back `title="Finder"`, the
application's own title, where a real Finder window's title is a folder name.
Index 1 and beyond label correctly, and `elementAtPosition` returned
`loginwindow` for every coordinate. Unlocked, every one of those readings is
correct, and `ax-dump` prints Finder's window as the first child:

```
AXApplication title="Finder"
  AXWindow/AXStandardWindow title="Meituan" id="FinderWindow"
```

So a locked session reproduces every part of the original report — the empty
window list, the "agent process or not fully launched" answer, the zero
`AXWindow` dumps, the mislabelled roles — and an unlocked session reproduces
none of it.

**What this establishes only by inference.** That the original readings were
taken locked. There is no direct record: `CGSSessionScreenLockedTime` holds
only the current lock's start, and the daemon log has one lock-state line for
that window — `03:26:04Z screen_locked=false`, one second before a Locked Use
transaction finished and relocked under cover, which is where the 11:26 lock
came from: the machine's own, not the owner's. What points at it is the
behaviour itself. `ax-dump` reports an `AXWindow` for every application tried
when the screen is unlocked, and the 11:03–11:06 run reported none for any of
four, so that session cannot have been unlocked in the ordinary sense. That is
an inference from reproduction, not a log line, and it is recorded as one. Worth keeping in
view: Locked Use unlocks the screen *under a full-screen cover* at
`CGShieldingWindowLevel() + 1` (ADR 0120), so "unlocked" during a transaction
is not the same Accessibility environment as an ordinary unlocked desktop.

## Decision

**Window questions put to the Accessibility API are only meaningful in an
unlocked session, and a record may not treat a locked reading as a fact about
an application.**

Concretely:

- ADR 0127's premise — "the app's windows were invisible to the Accessibility
  API ... the app's own AX server was never broken" — is withdrawn as a
  description of a defect. Its **decision stands on its own merits**: the
  daemon did hold a second `.prohibited` Launch Services record under the app's
  bundle id, that was measured by intervention, and one running-application
  record per app is right regardless of what it did or did not cause.
- ADR 0129's premise is withdrawn entirely. `ax-dump` has **no** blind spot;
  its four-app table of zero `AXWindow` counts was a locked-session artifact,
  and the sentence "for an application element `AXChildren` yields the menu
  bar, not the windows" is false. Unlocked, `AXChildren[0]` is the window.
- `ax-windows` is kept, on a smaller and true claim: asking `AXWindows`,
  `AXMainWindow`, `AXFocusedWindow` and `AXMenuBar` by name and printing each
  one's `AXError` is a sharper read than a tree walk, and unlike `ax-dump` it
  does not set `AXManualAccessibility`/`AXEnhancedUserInterface` on the app it
  is measuring. It is not there because a walk cannot see windows.
- The comment this investigation put on `ax-dump` in `lock.rs` said that action
  can never show windows. That was wrong and is replaced by the lock caveat.
- `ax-windows`' positional read hit-tests whatever is frontmost at the point,
  so it says something about a given app only when that app's window is in
  front there. During these readings the front window belonged to another app,
  which is why it answered `AXCell` for all three coordinates.

**One real difference survives, and it is not the Accessibility API.** The
app's window reports an empty `AXTitle` where both controls report theirs. That
is a trusted read with nothing redacting it. The cause is in the app:
`AgentScreens.swift` sets `.navigationTitle("")`, which on macOS sets the
window's title and so overrides `WindowGroup("Muster")`. Not a defect in AX,
and not what was being hunted — but an empty window title does make the window
harder for any accessibility client or window list to identify, so it is
recorded here rather than left as an loose end. Whether to give the window a
title is a product decision and is not taken in this record.

## Alternatives

- **Leave both records standing.** Rejected: ADR 0127 is the first thing anyone
  will read about this symptom, and it currently says the windows were
  invisible and that the duplicate record is why. ADR 0129 teaches a false fact
  about a tool the next person will reach for.
- **Withdraw ADR 0127's decision too.** Rejected: the duplicate registration
  was real and measured, and removing it is correct on its own grounds. Only
  the causal framing was wrong.
- **Delete `ax-windows`.** Rejected: it earns its place on the smaller claim,
  and it is what produced the locked/unlocked comparison that settled this.
- **Correct the three affected records separately.** The third is PR #133,
  which belongs to another session and is being corrected by it. Keeping the
  two that are mine in one record keeps one coherent account rather than three
  partial ones.

## Consequences

- There is no known Accessibility defect in the app. Anyone who sees the
  symptom again should check `CGSSessionScreenIsLocked` and whether a Locked
  Use transaction is in progress **before** reading anything else.
- Two merged records carry corrected premises, and the code comment that taught
  the false fact is gone.
- `ax-windows` and the locked/unlocked contrast are the lasting output of the
  investigation, along with the explanation of the empty window title.
- The original report cannot be attributed to the lock with certainty, only
  shown to be reproducible under one and not otherwise. That gap is stated, not
  closed.

## Verification

- **Unlocked** (`CGSSessionScreenIsLocked` absent), via `musterd`'s own grant:
  `ax-windows` on the app gives `AXChildren` `[AXWindow, AXMenuBar]`,
  `AXWindows` `status=0 count=1` with `AXWindow/AXStandardWindow`, and
  `AXMainWindow`/`AXFocusedWindow` both resolving. `ax-dump` prints one
  `AXWindow` line for each of the app, the minimal SwiftUI control and Finder.
  The computer-use client returns window 166908 with the window server's
  bounds.
- **Locked** (`CGSSessionScreenIsLocked=1`): the table above, reproduced
  independently from two sessions.
- **The empty title** is read from the unlocked `ax-windows` output and traced
  to `.navigationTitle("")` in `AgentScreens.swift`; the controls, which set no
  such modifier, report their titles.
- **Gates.** `cargo test --workspace`, `cargo clippy --workspace --all-targets
  -- -D warnings`, `cargo fmt --all --check`,
  `src/muster/scripts/check-layers.py`, `scripts/check-public-content.py`,
  `scripts/check-adr.sh origin/main`.
- **Not verified.** That the first readings were taken under a lock; see
  Context. No code behaviour changes here, so there is nothing to install.
