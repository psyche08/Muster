# ADR 0083: Keep Awake and Keep Screen On in the Mac app menu

- Date: 2026-11-02
- Status: Accepted; local dev build verified, release pending
- Related: [architecture §3.6.2](../architecture/agent-halo.md) (power);
  [ADR 0082](0082-progressive-architecture-documentation.md)

## Context

A Mac that sleeps drops its links, and the phone then cannot see or drive its
sessions. Until now the owner ran `caffeinate -dimsu` by hand in a terminal.
The app held power assertions only while the privacy cover was up. The owner
asked for this to be built in:
- Muster runs `caffeinate -imsu` while it is running.
- The app menu has a switch to turn keep-awake on or off.
- While keep-awake is on, a **Keep Screen On** switch selects
  `caffeinate -dimsu`.

## Decision

- **The menu.** The macOS app menu gains two toggles after **Settings…**:
  - **Keep Awake**, on by default.
  - **Keep Screen On**, off by default and disabled while Keep Awake is off.

  Both are stored in the app's user defaults (`keepAwake`, `keepAwakeDisplay`).
- **Which flags run.** `MusterCore.KeepAwakePreference` maps the choice to
  arguments:
  - Keep Awake alone: `-imsu`.
  - Keep Awake with Keep Screen On: `-dimsu`.
  - Keep Awake off: nothing runs.

  It always adds `-w <Muster's pid>`, so `caffeinate` ends with Muster even when
  Muster is killed.
- **The process.** `MusterUI.Caffeinate` keeps at most one `/usr/bin/caffeinate`
  child:
  - It applies the stored choice at launch and on every toggle.
  - A change replaces the running child.
  - A child that exits on its own is restarted after one second.
  - The child is terminated when the app quits.
- **The daemon is unchanged.** The privacy cover's own IOPM assertions are also
  unchanged.

This is a local OS preference with no session, authorization or send state, like
the conversation style. So it lives in the Apple shell and adds no ViewModel
field, intent or ABI change. The user asked for `caffeinate` specifically. IOPM
assertions remain the mechanism inside the app for the cover and locked
operation (§3.6.2).

## Alternatives

- **IOPM assertions in the app instead of `caffeinate`.** They are equivalent in
  effect. But the owner asked for the `caffeinate` flags they already use, and a
  visible `caffeinate` process is easy to check with `pmset -g assertions`.
- **Keep-awake in `musterd`.** The daemon outlives the app and runs while
  nobody is logged in to the UI. The owner tied the behavior to Muster running,
  and a daemon-side switch would need a new intent and ViewModel field for a
  purely local choice.
- **A menu-bar status item.** The app has none today. The app menu is where the
  owner asked for the switch.

## Consequences

- With the defaults, a Mac running Muster no longer idle-sleeps or system-sleeps
  (`-s` applies on AC power only), and the display may still sleep. With Keep
  Screen On, the display stays on too.
- `-u` declares the user active when the child starts, which also wakes a
  sleeping display at that moment.
- A user who prefers macOS's own sleep settings turns Keep Awake off. The choice
  survives relaunches.
- `MusterDev.app` (terminal development) does not get the menu.

## Verification

- **Unit tests.** `swift test --filter KeepAwakePreferenceTests` (3 tests) passed
  against a debug `libmuster_ffi.a`. They cover the default `-imsu`, the
  `-dimsu` screen choice, and nothing running when keep-awake is off even with
  the screen choice stored.
- **Build.** An unsigned Debug `xcodebuild` of `MusterMac` succeeded.
- **The built app, on one Mac.** It ran with a scratch `MUSTER_DIR`, so the
  installed daemon and LaunchAgent were untouched. The menu was driven through
  System Events. Results:
  - At launch it started `/usr/bin/caffeinate -imsu -w <pid>`.
    `pmset -g assertions` showed `PreventUserIdleSystemSleep` and
    `PreventSystemSleep`.
  - Keep Screen On → `-dimsu`. Keep Awake off → no child, and Keep Screen On
    greyed out. Keep Awake on → `-dimsu` again. Screen off → `-imsu`.
  - A killed child was replaced within two seconds.
  - Quitting left no child.
  - A relaunch restored the stored `-dimsu`.
  - `kill -9` of the app ended its child through `-w`.
  - The test's keys were removed from the `dev.linsheng.muster` defaults
    afterwards.
- **Not done.** A signed, notarized release, installation on the Macs, and
  behavior across a lid close or on battery.
