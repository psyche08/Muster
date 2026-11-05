# ADR 0129: Ask the Accessibility API for windows by name, not by walking children

- Date: 2026-11-05
- Status: Accepted; the action stands on a smaller claim, its premise is
  withdrawn by [ADR 0131](0131-a-locked-screen-explains-the-missing-windows.md)
  — `ax-dump` has no blind spot, and the zero `AXWindow` counts below were
  a locked-session artifact
- Related: [ADR 0127](0127-the-daemon-stays-out-of-launch-services.md) (the
  investigation this instruments, and the record whose causal claim is still
  open), [ADR 0128](0128-the-panel-helper-keeps-its-own-identity.md) (the other
  change that came out of it), [ADR 0116](0116-failure-screenshot-analysis.md)
  (what reads the app's windows)

## Context

> **Premise withdrawn by [ADR 0131](0131-a-locked-screen-explains-the-missing-windows.md).**
> The four-app table below was measured while the screen was locked, which makes
> the Accessibility API return the application element in place of window
> elements for every app. Unlocked, `ax-dump` prints Finder's window as its
> first child, and the sentence "`AXChildren` yields the menu bar, not the
> windows" is false. `ax-windows` is kept because asking attributes by name with
> their error codes is a sharper read than a walk, and because it does not
> mutate the app it measures — not because a walk cannot see windows.

The app's windows are missing from the Accessibility API on the owner's Mac.
ADR 0127 removed a duplicate Launch Services record that was a real defect, and
the symptom survived it, so the cause is still open.

The investigation then produced a wrong conclusion from the right tool used
wrongly. `muster debug ax-dump <pid>` was read as showing that the app's AX
server "publishes no windows at all", because its output contained no
`AXWindow` line. The control settles it:

| app | `ax-dump` `AXWindow` lines | windows in the window server |
| --- | --- | --- |
| Muster | 0 | 1 |
| Tusk | 0 | 6 |
| Finder | 0 | 8 |

Finder has eight windows and `ax-dump` reports none, so the dump says nothing
about windows for any application. The reason is in the code:
`ax-dump` calls `Element::application(pid).dump(..)`, `dump` descends through
`Element::children`, and `children` reads **`AXChildren`**, never `AXWindows`.

**The explanation that followed here was wrong and is withdrawn.** This record
said that for an application element `AXChildren` yields the menu bar and not
the windows. This action's own output contradicts it: unlocked, `AXChildren` is
count=2 for Muster and count=3 for Finder, with `AXWindow` first in both
(see [ADR 0130](0130-the-ax-symptom-was-a-locked-screen.md)). The measurement
that `ax-dump` reports no `AXWindow` for any application, Finder's eight
included, still stands — taken unlocked at 11:03–11:06. Why it does is
unexplained. One hypothesis, untested: `dump` skips a node it has already
seen, so a first child that resolved to the application element would stop the
walk descending into it.

So the question that matters — what does a trusted process get when it asks
this application for its windows — had no instrument. The two other routes were
both closed, and for good reasons: granting Accessibility to a throwaway probe
binary is a real privilege change on the owner's machine, and the computer-use
client that reports `[]` reaches only `app_list_windows`, has no grant for a
control app, and had already been caught resolving by display name where only
the bundle id worked.

## Decision

**`muster debug ax-windows <pid|App> [x,y]` asks the application element for
`AXWindows`, `AXMainWindow`, `AXFocusedWindow` and `AXMenuBar` by name, reports
each one's `AXError` beside its result, and lists `AXChildren` for comparison.**

It also prints the element's advertised attribute names, because an empty
window list reads differently depending on whether `AXWindows` is offered at
all, and — given an optional point — asks the system-wide element what is at
that screen position. The positional read reaches a window by a route that does
not go through the application's own window list, so it separates "this window
is outside the Accessibility tree" from "this window is in the tree and the
application's list omits it". Two small read-only additions to the AX wrapper
carry those: `Element::attribute_names` and `Element::element_at`.

It runs in `musterd`, which already holds Accessibility, so it needs no new TCC
grant and no per-application approval, and it accepts a bare pid so it never
depends on resolving an app by name.

It is **read only, deliberately**. Unlike `ax-dump` it does not call
`enable_manual_accessibility`, because setting `AXManualAccessibility` or
`AXEnhancedUserInterface` alters the application being measured. `ax-dump`
setting them is why an earlier reading of this symptom had to be taken twice,
before and after, to mean anything.

Reporting `AXChildren` next to `AXWindows` is the point, not padding: side by
side they show why a children walk reaches no window, so the next person does
not repeat the mistake this record exists to correct.

## Alternatives

- **Grant Accessibility to a standalone probe.** Direct, and it was built.
  Rejected: it is a privilege grant on the owner's machine for a binary in a
  session scratchpad, and not something to press for a diagnostic.
- **Extend `ax-dump` to also ask `AXWindows`.** It would make one action mean
  two things, and it would keep the mutation that makes its readings
  conditional. A separate read-only action stays interpretable.
- **Trust the computer-use client's `[]`.** It is the only direct evidence of a
  problem and it may well be right, but it cannot be checked against a control
  app from here, so on its own it does not establish that this app differs from
  a healthy one.
- **Fix `Element::children` to include windows.** It is not broken;
  `AXChildren` is the attribute it names, and the agent code depends on that
  behaviour. The error was in reading a children walk as a window query.

## Consequences

- The open question in ADR 0127 becomes answerable with one command on three
  applications, with no new permission: whether `AXWindows` returns windows for
  Muster, and whether it returns them for a control app on the same machine.
- `ax-dump`'s blind spot is now documented in the code, next to the action that
  does not share it.
- Nothing in the product's behaviour changes; this adds a diagnostic the
  desktop's own user can run.
- The symptom is not fixed, and no claim here says it is. ADR 0127's causal
  step from the duplicate record to the invisible window remains unestablished.

## Verification

- **Source.** `cargo build --release -p musterd --features sep` builds; the
  action is dispatched beside `ax-dump` in `lock.rs`, and the diagnostic list
  in `Platform::debug`'s documentation and in the CLI's help both name it.
  `Element::attribute_names` wraps `AXUIElementCopyAttributeNames`, and
  `Element::element_at` wraps the already-declared
  `AXUIElementCopyElementAtPosition` and returns the element rather than a
  bool, which `query_renderer`'s existing use of that call does not.
- **Gates.** `cargo test --workspace`, `cargo clippy --workspace --all-targets
  -- -D warnings`, `cargo fmt --all --check`,
  `src/muster/scripts/check-layers.py`, `scripts/check-public-content.py`,
  `scripts/check-adr.sh origin/main`.
- **Runtime: not verified.** The action returns `Accessibility not granted`
  unless the process asking has the grant, which belongs to the installed
  bundle's daemon. A build from a scratch path cannot exercise it, and this
  record does not claim otherwise. Installing is the owner's call.
- **No CLI replacement needed.** `muster debug` forwards `what` verbatim in
  `DeviceDebugRequest`, so the installed command line already reaches a new
  action; only the daemon has to be the new build.
- **What the output will mean.** `AXWindows` returning the window for a control
  app and not for Muster would be the first controlled evidence that this app
  differs. The same empty answer for every application would say the window
  list is not how these windows are reachable, and move the question to how a
  SwiftUI `WindowGroup` window is published on macOS 27.0.1 (build 26A434).
  A positional read that lands on a window the application does not list would
  say the window is in the tree and only the list is wrong.
- **Three readings settle it**, all available without a new grant: Muster, a
  minimal SwiftUI control app built for this (one `WindowGroup`, a `.commands`
  menu, no delegate, no activation-policy call, no accessibility call), and
  Finder as a known-good AppKit app. `ax-dump` cannot tell those three apart —
  it returns zero `AXWindow` for all of them, Finder's eight windows included,
  which is the error this record corrects.
