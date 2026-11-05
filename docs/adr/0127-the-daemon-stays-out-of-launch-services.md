# ADR 0127: The daemon stays out of Launch Services instead of leaving the Dock

- Date: 2026-11-05
- Status: Accepted; its decision stands, its premise is corrected by
  [ADR 0131](0131-a-locked-screen-explains-the-missing-windows.md) — the
  windows were not invisible to the Accessibility API, and removing the
  duplicate Launch Services record was right on its own grounds
- Related: [ADR 0123](0123-the-daemon-leaves-the-dock.md) (superseded by this
  record), [ADR 0125](0125-screen-recording-is-the-bundles-and-only-a-restart-takes-it.md)
  (the daemon's grants are filed under the app bundle, which is what makes its
  identity worth keeping), [ADR 0006](0006-desktop-presence-and-console-routing.md)
  (the bundled daemon's stable identity),
  [ADR 0116](0116-failure-screenshot-analysis.md) (what reads the app's windows),
  [ADR 0128](0128-the-panel-helper-keeps-its-own-identity.md) (the other half
  of this bug: the AppKit panel helpers, proposed separately)

## Context

> **Corrected by [ADR 0131](0131-a-locked-screen-explains-the-missing-windows.md).**
> In an unlocked session the app's windows are in the Accessibility tree and the
> client quoted below returns them. A locked screen makes the Accessibility API
> return the application element in place of window elements, in every app,
> which reproduces everything described here. The duplicate Launch Services
> record was real and removing it was right; it was not the cause of a missing
> window, and no Accessibility defect in the app is known.

The app's windows were reported missing from the Accessibility API on the
owner's Mac (1.1.21/111). The window server was healthy throughout:
`CGWindowListCopyWindowInfo` listed the window as `layer=0`, `onscreen=1`,
1728x995, the owner could see and use it, and `screencapture -l` captured it.
But an AX client enumerating the app's windows got nothing, and asking for its
menu bar got "Muster does not expose a menu bar (agent process or not fully
launched)" — the answer for a process that is not a foreground application.

Launch Services held **two** running-application records for
`dev.linsheng.muster`, both named "Muster":

```
ASN:0x0-0x7a24a1d  executable=…/Contents/MacOS/musterd  pid 10785  type="BackgroundOnly"
ASN:0x0-0x7a25a1e  executable=…/Contents/MacOS/Muster   pid 10812  type="Foreground"
```

`NSRunningApplication.runningApplications(withBundleIdentifier:)` returns both,
**the daemon first**:

```
[0] pid=10785 policy=prohibited exe=…/Contents/MacOS/musterd
[1] pid=10812 policy=regular    exe=…/Contents/MacOS/Muster
```

An AX client resolves "Muster" by name or by bundle id, takes the first match,
and lands on the daemon — a `.prohibited` process that answers no AX message at
all. The app is never asked. This was written as the whole bug; the install
acceptance below shows it is not. Removing the daemon's record leaves one ASN
and the symptom survives, so the misresolution described here is at most part
of the cause. What the probe below does establish is narrower: the app's own AX
server answers like a healthy application's. Asking each process for `kAXWindows` from an untrusted probe
separates the two cases by error code, because the trust gate is only reached
on a process that has an AX server to gate:

| process | result |
| --- | --- |
| Finder, iTerm2, Telegram, Tusk | `-25211 kAXErrorAPIDisabled` |
| **Muster, pid 10812** | **`-25211 kAXErrorAPIDisabled`** |
| musterd, pid 10785 | `-25204 kAXErrorCannotComplete` |
| l4forwarder (headless helper) | `-25204 kAXErrorCannotComplete` |

The app answers exactly as every healthy GUI application does. The daemon
answers as every headless process does. (This table separates "has an AX
server" from "has none" and nothing more; see ADR 0131.)

One note before the cause, because it decides what a re-check sees:
**`lsappinfo list` does not show the daemon's record.** It lists one "Muster",
the app. `lsappinfo find bundleID=dev.linsheng.muster` returns both ASNs,
`lsappinfo info -app ASN:0x0-0x7a24a1d` prints the daemon's entry in full, and
`NSRunningApplication` — the API an AX client actually resolves through —
returns both. Reading `lsappinfo list` alone is how an observer concludes the
record is absent while it is live.

The record is created by `TransformProcessType`, ADR 0123's own fix. The call
cannot move a process between types without first making it a registered
application, and it succeeds only because the daemon's main bundle resolves to
`Muster.app`, which is also where it picks up the app's bundle id and the name
"Muster". That is an intervention, not a reading: the stock 1.1.21 daemon run
from a copy of the bundle added a third `.prohibited` record, and the same
daemon rebuilt without the call added none, at 14 s and again over 4 minutes
with the agent watchers and terminal automation running.

ADR 0123 recorded something different — two **`Foreground`** records, pids
43867 and 43917, before the call existed. Nothing measured here produces a
`Foreground` registration for the daemon, so that entry is not this one and
its trigger is not settled. There is a second source that does register under
the app's identity, and it is not the daemon: the short-lived AppKit helpers
spawned from the same executable. `musterd --muster-notice` draws the banner
(ADR 0087) and calls `[NSApp setActivationPolicy:Accessory]`; the locked-use
watchdog console does the same. Measured here, a running notice helper adds a
**third** record named "Muster" under `dev.linsheng.muster` at `.accessory`,
which goes when the helper exits:

```
pid=10785 policy=prohibited  …/Contents/MacOS/musterd   (the daemon)
pid=10812 policy=regular     …/Contents/MacOS/Muster    (the app)
pid=54409 policy=accessory   …/Contents/MacOS/musterd   (the notice helper)
```

A transient helper of that kind is a likelier reading of ADR 0123's second
entry than the daemon is, but which one pid 43917 was cannot be decided from
here.

Measured in a throwaway `AxProbe.app` with a GUI host plus one headless daemon,
counting records for the host's bundle id:

| daemon | records | the daemon's record |
| --- | --- | --- |
| `Contents/MacOS` + `TransformProcessType` | **2** | app's bundle id, `.prohibited` |
| `Contents/MacOS`, no transform | 1 | none |
| `Contents/MacOS` + embedded `__TEXT,__info_plist` | 1 | its own bundle id |
| `Contents/Helpers` + `TransformProcessType` | 1 | none; the call fails `-50` |

Nothing else the daemon does registers it. A daemon in `Contents/MacOS` that
reads its main bundle, calls `AXIsProcessTrusted`, queries AX, calls
`CGEventCreate`, calls `CGPreflightScreenCaptureAccess`, lists windows with
`CGWindowListCopyWindowInfo`, reads the session dictionary, or fetches
`SCShareableContent` gets no record from any of them.

So ADR 0123 traded the app's AX visibility — the capability this product is
built on — for a cosmetic Dock tile, and the trade was invisible because the
tile it removed and the record it created are the same registration.

## Decision

**The daemon does not transform its process type. Nothing in `musterd`
registers it with Launch Services, and the executable stays where it is.**

`muster_platform::macos::procs::leave_the_dock` and its one call site in
`musterd`'s `run()` are removed. The daemon keeps its path,
`Muster.app/Contents/MacOS/musterd`, so launchd's `BundleProgram`, the
bundle layout, `install-mac.sh`, and the daemon's TCC grants are untouched.

The invariant to hold: **`dev.linsheng.muster` has exactly one running
application record, the app's.** A process inside the bundle that registers
itself takes the app's identity, and an AX client that resolves the app by
identity will find that process instead of the app.

This change reaches that invariant for the daemon, which held the record
permanently, and not yet for the notice and watchdog helpers, which hold one
while they run. That remainder is stated under Consequences rather than
folded in here: it is a separate defect with a separate fix, and the bundle
layout change it wants does not belong in the same change as a deletion.

## Alternatives

- **Move `musterd` to `Contents/Helpers`.** The conventional layout and what
  the other four helpers in this bundle do; it removes the record under any
  circumstances rather than only the one known trigger. Rejected for the same
  reason ADR 0123 rejected it, and ADR 0125 has since measured that reason
  precisely: the daemon's grants are filed under the app bundle, because its
  main bundle resolves to `Muster.app`. `tccutil` refuses
  `dev.linsheng.musterd` outright, and a Screen Recording grant takes only
  with the bundle's row switched on. Moving the executable out of
  `Contents/MacOS` is what ends that association, so it would cost the owner a
  re-approval of Accessibility and break the Screen Recording arrangement
  ADR 0125 has just settled. Measured here too: the same daemon run from a
  copy of the bundle at another path is not trusted ("Accessibility not
  granted to musterd"). Removing the call achieves the same result at no cost,
  so the move is not needed to fix this.
- **An embedded `__TEXT,__info_plist` in the daemon.** Keeps the path and
  stops the record carrying the app's identity, so it would survive a future
  caller that registers the daemon. Rejected now: with its own
  `CFBundleIdentifier` or with none, the daemon's main bundle is no longer
  `Muster.app`, which is the same association ADR 0125 depends on — and
  `LSBackgroundOnly` there would itself register the process. Worth revisiting
  if something ever registers the daemon again.
- **Keep ADR 0123 and change the app instead.** Nothing the app can do makes a
  third-party AX client prefer its record over the daemon's.
- **`LSUIElement` in the bundle's `Info.plist`.** It would hide the app too, as
  ADR 0123 recorded.

## Consequences

- An AX client resolving "Muster", by name or by bundle id, finds the app —
  there is only one record left to find. **The rest of this bullet did not
  survive the install acceptance.** Window enumeration and screenshots of the
  app do *not* work again: a trusted client still gets zero windows. See
  Verification. The intermittency claim rested on the same reasoning and is
  withdrawn with it.
- The daemon's path, signing identifier, launchd job, and TCC grants are
  unchanged, so no permission has to be re-approved.
- **The Dock tile ADR 0123 removed.** This change deletes the mechanism that
  removed it, so the question has to be answered rather than assumed. On the
  evidence it does not come back, and for a reason worth stating: a tile needs
  a `Foreground` registration. ADR 0123 turned one into `BackgroundOnly`;
  deleting the call leaves **no registration at all**, measured over 4 minutes
  with the agent watchers and terminal automation running. `BackgroundOnly`
  and `.accessory` processes get no tile either, so nothing this change
  removes was holding a tile away.
  What cannot be claimed is that the trigger is gone, because the
  `Foreground` registration ADR 0123 saw was never reproduced here at all. If
  the tile returns it will be from that unidentified trigger, and the fix
  belongs there — the Accessibility prompt in `lock.rs` and the AppKit helpers
  above are the first places to look — not in registering the daemon at every
  startup to pre-empt it.
  And if it did return, this record still prefers it: an unresponsive tile is
  cosmetic, while the app being unreachable through the Accessibility API is
  the capability the product is built on. That is the trade ADR 0123 made in
  the other direction without knowing it was making it.
- **Remaining, not fixed here: the AppKit helpers.** `musterd --muster-notice`
  and the locked-use watchdog console register under the app's identity for as
  long as they run — up to the banner's 10-minute lifetime — so an AX client
  resolving "Muster" during that window can still land on a helper. The
  invariant above is therefore not yet whole. The fix is to give those helpers
  an executable of their own outside `Contents/MacOS`, next to the four
  helpers already in `Contents/Helpers`, which ends the bundle association for
  them without touching the daemon's: they draw a borderless panel and need
  none of the daemon's TCC grants. That is a bundle layout and build change,
  it is independent of this deletion, and it is left to its own record:
  [ADR 0128](0128-the-panel-helper-keeps-its-own-identity.md), proposed in
  PR #132, which moves both panel bodies into a `muster-panel` helper under
  `Contents/Helpers`. Until that lands the window described here is open, and
  the `musterd --muster-notice` subcommand this record measures still exists;
  afterwards it does not, and the measurement above describes the state before
  it.
- Screen Recording and terminal authorization are untouched. Because the
  executable does not move, the daemon's grants stay filed under the app
  bundle, so ADR 0125 continues to hold as written.

## Verification

- **Source.** `leave_the_dock` and its call site removed; no reference to
  `leave_the_dock` or `TransformProcessType` remains in the Rust tree.
- **Root cause, measured on the owner's Mac (2026-11-05, 1.1.21/111).** Two
  Launch Services records for `dev.linsheng.muster` with the daemon first and
  `.prohibited`; the app at `.regular` with its main thread idle in the normal
  AppKit event loop (`sample 10812`), so no main-thread block; the bundle's
  `Info.plist` sets neither `LSUIElement` nor `LSBackgroundOnly`; the AX error
  codes in the table above. The reported
  `osascript … count windows of process "Muster"` returning `-25211` is a
  separate thing — the caller's own missing Accessibility trust — and not this
  bug.
- **Cause isolated, by intervention.** The stock 1.1.21 daemon, run from a
  copy of the bundle with a scratch state directory, added a third
  `.prohibited` record under `dev.linsheng.muster`. The same daemon rebuilt
  without the call added none: over 14 s, again from the `sep` release build
  used for shipping, and again over 4 minutes with the agent watchers and
  terminal automation running — 24 samples at 10 s intervals, every one of
  them showing two records, with the daemon alive throughout.
- **Reading the record.** `lsappinfo list` shows one "Muster" and omits the
  daemon's entry; `lsappinfo find bundleID=`, `lsappinfo info -app ASN:…` and
  `NSRunningApplication` all show it. A re-check that uses only
  `lsappinfo list` will wrongly conclude there is no second record.
- **The AppKit helpers, measured.** A running `musterd --muster-notice` adds a
  third record named "Muster" under the app's bundle id at `.accessory`, gone
  when the helper exits. This change does not address that; see Consequences.
- **Not reproduced.** The `Foreground` registration ADR 0123 recorded for the
  daemon. Neither the daemon with the call, nor without it, nor a synthetic
  daemon exercising the APIs listed above produced one here.
- **Gates.** `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo fmt --all --check`, `src/muster/scripts/check-layers.py`,
  `scripts/check-public-content.py`, `scripts/check-adr.sh origin/main`, and
  `cargo test --workspace` (1097 tests over 85 suites, exit 0). One earlier run of
  `locked_use::tests::a_revoked_grant_stops_its_scheduled_lease_before_the_desktop`
  failed on `try_write_owned().is_ok()` while a four-minute daemon run and
  other cargo jobs were competing for the machine; it passes 5/5 in isolation,
  and nothing in this change touches leases. It is the family ADR 0120
  documents: `relocked()` waits through `ended()` for `owner.current` to
  clear, then asserts on `owner.writes`, which that wait does not cover, so a
  loaded machine can reach the assertion while the lease guard is still held.
  The `unlock_started()` helper in the same file is the right shape for it.
  Left to that cleanup, not mixed into this change.
- **Install acceptance: run, and failed on its second half.** 2026-11-05, on
  the owner's Mac, built and installed from main at `2ebe0a3` (which carries
  this change and ADR 0128); the daemon reports that commit, bundle 1.1.21
  (111).
  - First half, passed: `lsappinfo find bundleID=dev.linsheng.muster` returned
    two ASNs before the install and **one** after, and the only Launch Services
    record left is the app's own `…/Contents/MacOS/Muster`, `type="Foreground"`.
    No Dock tile returned, which this record predicted and ADR 0123's author
    had doubted.
  - Second half, failed: an AX client holding a full grant for
    `dev.linsheng.muster` got `[]` from an enumeration of the app's windows,
    and a capture attempt answered "the window server shows a window of Muster
    on screen, but accessibility does not list it". Twice, seconds apart, with
    both processes running (app pid 73887, musterd pid 73906).
  - So the duplicate record was real and removing it was right, but it was not
    what hid the window. A window the window server shows and the Accessibility
    API does not list, with a single ASN and no daemon record, points
    elsewhere: at the app's own activation policy or `NSApplication`
    configuration, or at the window being owned by a process other than the
    app's main one.
  - The deletion stands. The causal story does not, and is corrected above
    rather than erased.
- **Two supports for the causal claim collapsed, separately from the
  acceptance.**
  - The probe table above (`-25211` for Finder, iTerm2, Telegram, Tusk and
    Muster; `-25204` for musterd and l4forwarder) was read as showing the
    app's AX server answers like a healthy application's. Re-running that
    probe shape shows the probing process reporting
    `AXIsProcessTrusted() = false`, and an untrusted client gets `-25211`
    from every application that has an AX server, whatever its state. The
    table separates "has an AX server" from "has none" and nothing more.
  - An `app_list_windows` call answering "App Muster is not in the
    granted-applications list" was read as a client resolving to the daemon's
    record. It is not: the same call succeeds when passed the bundle id
    `dev.linsheng.muster` instead of the display name `Muster`, with one ASN
    and the grant in place. It was a lookup-key difference in the tooling.
- **What the window server says after this change**, for whoever picks the
  symptom up: `CGWindowListCopyWindowInfo` with `.optionAll` returns exactly
  one window for the app process — `win=166897 layer=0 onscreen=true
  alpha=1.0 1728x995 at (0,34)`. The four offscreen 1728x33 surfaces seen
  before ADR 0128 are gone. Its `kCGWindowName` reads empty, which says
  nothing: the window server redacts that field from a client without Screen
  Recording, and the client here had none.
