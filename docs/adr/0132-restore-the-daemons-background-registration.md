# ADR 0132: Restore the daemon's background registration, and the Dock tile goes

- Date: 2026-11-05
- Status: Accepted; source verified, timeline measured on the owner's Mac.
  Install acceptance pending — see Verification.
- Restores the code ADR 0127 removed, on ADR 0123's reasoning.
- Related: [ADR 0123](0123-the-daemon-leaves-the-dock.md) (the decision this
  brings back), [ADR 0127](0127-the-daemon-stays-out-of-launch-services.md)
  (which removed it, on a premise [ADR 0131](0131-a-locked-screen-explains-the-missing-windows.md)
  withdrew), [ADR 0125](0125-screen-recording-is-the-bundles-and-only-a-restart-takes-it.md)
  (why the daemon keeps the app's identity at all),
  [ADR 0128](0128-the-panel-helper-keeps-its-own-identity.md) (the same problem
  solved by moving an executable, which is not available here)

## Context

ADR 0123 called `TransformProcessType` on the daemon at startup so it would not
hold a Dock tile it could never service. ADR 0127 removed that call, on two
beliefs that have both since failed:

- that the duplicate Launch Services record hid the app's windows from the
  Accessibility API. It did not. The symptom was a locked screen, withdrawn in
  ADR 0131.
- that the call **created** the record — "the call cannot move a process
  between types without first making it a registered application", established
  by intervention over 4 minutes, 24 of 24 samples. That test ran a copy of the
  bundle from a scratch path, which TCC does not recognise, so the daemon in it
  logged `Accessibility not granted to musterd` and never did the work a real
  daemon does. It could not have registered under any circumstances.

The owner found the consequence before the reasoning was unpicked: two
`AXDockItem/AXApplicationDockItem title="Muster"` entries in the Dock's own
accessibility tree, which is ADR 0123's problem returned.

## Decision

**Restore `leave_the_dock` and its call at daemon startup.** ADR 0123's
reasoning is unchanged and is the reason this is safe: it drops the tile, keeps
the executable's path, and so keeps the daemon's TCC grants, which are filed
under the app bundle (ADR 0125). Moving the executable — what ADR 0128 did for
the panel helpers — is what ADR 0125 rules out for the daemon.

What the measurement changes is the account of *why* it works. The call does
not create the record:

| | records | daemon's type | Dock tile |
| --- | --- | --- | --- |
| with the call | 2 | `BackgroundOnly` | no |
| without it | 2 | `Foreground` | **yes** |

The duplicate is there either way. It is the price of the daemon sharing the
app's identity for TCC, and the call only sets its type — which is the thing
that decides whether there is a tile.

## What was measured

On the owner's Mac, 2026-11-05, against the installed daemon from `396cb72`,
restarting the real launchd job with `launchctl kickstart -k`:

- A fresh daemon holds **no** record. The second one appears at **t+2.60 s**
  (0.2 s sampling), and reads `Foreground`, and stays.
- It is not the diagnostics: after one restart the count had already gone to 2
  **before** any `ax-dump` was run.
- `leave_the_dock` runs immediately after the logger is initialised, well
  before t+2.6 s, so it acts on a process that is not yet registered — and the
  historical readings say the result sticks: `Foreground` before ADR 0123,
  `BackgroundOnly` with the call, `Foreground` again after ADR 0127.

**Which startup step registers it is not known**, and this record does not
guess. What the daemon is doing at t+2.6 s, from its own log, is a cluster at
the same second: the Locked Use plugin install, the vnet helper open
(`completed after 1.904283042s`), the socket coming up, and the `l4forwarder`
spawn. The `AX controller attached` line is at the *next* second, after the
record already exists, so attaching an accessibility observer is not it.

Ruled out, all negative, each tested in a throwaway bundle: reading the main
bundle, `AXIsProcessTrusted`, an AX attribute query, `CGEventCreate`,
`CGPreflightScreenCaptureAccess`, `CGWindowListCopyWindowInfo`,
`CGSessionCopyCurrentDictionary`, `SCShareableContent`,
`LSCopyApplicationURLsForBundleIdentifier` — the one Launch Services call in
the daemon's crates, at `automation.rs:8` — and
`AEDeterminePermissionToAutomateTarget`. **These are negatives for an
untrusted process**, which is what makes them sound; anything needing
Accessibility trust is untested, not excluded.

Untested and still open: a successful privileged XPC round trip to the root
helper `dev.linsheng.muster.helper`, which covers both the plugin install and
the vnet open since both go through `nethelper.rs`
(`xpc_connection_create_mach_service` with
`XPC_CONNECTION_MACH_SERVICE_PRIVILEGED`); and the `l4forwarder` child spawn.

A method constraint for whoever continues: a hand-started daemon is never
Accessibility-trusted, because trust follows the responsible process and a
hand-started one answers to a shell rather than to launchd under the app's
identity. Only `launchctl kickstart -k` on the real job reproduces it.

## Alternatives

- **Leave the tile.** It is what the owner reported twice in one evening, and
  it reads as a hung application next to the real one.
- **Move the daemon out of `Contents/MacOS`.** ADR 0125: TCC is keyed to the
  path, so the Accessibility grant would need re-approving before Locked Use
  worked again.
- **`LSUIElement` in the bundle.** Hides the real app too.
- **Find the trigger first.** Each attempt costs a daemon restart on the
  owner's machine, and the fix does not depend on the answer.

## Consequences

- The daemon goes back to `BackgroundOnly` and the tile goes.
- The duplicate record stays. Anything resolving "Muster" by bundle id still
  sees two, and ADR 0131's reading of that stands: it is not what hides
  windows.
- ADR 0127's code change is reverted; its index row and the rest of its record
  stay, already corrected by ADR 0131.
- ADR 0128's `main()` is untouched. Only the call in `run()` and the function
  in `procs.rs` come back.

## Verification

`cargo test --workspace`, `cargo clippy --workspace --all-targets -D warnings`,
`cargo fmt --all --check`, `scripts/check-layers.py`,
`check-public-content.py`, `check-adr.sh origin/main`.

Install acceptance, not yet run: build and install from this branch, restart
the daemon, and confirm `lsappinfo` reports the daemon `BackgroundOnly` and the
Dock's accessibility tree holds one `AXDockItem` for Muster rather than two.
Until that runs, this record claims the reasoning and the timeline, not the
outcome.
