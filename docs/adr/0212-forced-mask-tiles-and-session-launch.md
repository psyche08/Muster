# ADR 0212: Forced mask beside Locked Use, session launch origin and Muster archive

- Date: 2026-11-15
- Status: Accepted; device acceptance pending
- Related: [ADR 0171](0171-mobile-display-blackout.md), [ADR 0111](0111-native-organization-ax-and-file-confirmation.md), [ADR 0007](0007-desktop-manual-locked-use.md)

## Context

ADR 0171 lets a phone cover every screen of a Mac and keep it awake. The
switch sat only on the device page, read "黑屏模式", showed the Mac's last
report with no pending state, and stayed enabled while the Mac was locked even
though the Mac refuses that request. The operator wants it in the Settings
device list beside Locked Use, available only while the Mac is unlocked.

The session catalog did not say how a session was started. Claude transcripts
record an `entrypoint` (`claude-desktop`, `cli`, `sdk-cli`) and Codex session
metadata records an `originator` and `source`. Native organization (ADR 0111)
archives through the desktop app. A Claude session started from the terminal
is not in Claude.app's registry, `codex exec` runs are not listed by the Codex
app, and Muster's own CLI sessions have no desktop app at all, so none of them
could be archived.

## Decision

**Forced mask.** `muster-app-core` projects one `DeviceBlackoutView` per
connected or known macOS desktop when it runs on a phone. The view carries the
switch position, whether it can change, a pending flag and the status. Turning
the mask on requires an unlocked Mac. Turning it off is always allowed. A
tapped change shows as pending until the Mac reports the requested preference,
the request fails, authentication is cancelled, or 12 seconds pass. The
daemon contract, persistence, the panel helper's coverage of every screen and
its sleep assertions are unchanged from ADR 0171. The Apple, Android and
HarmonyOS shells render Locked Use and the forced mask as two side-by-side
tiles in the Settings device list and on the device page.

**Launch origin.** `muster-agent` classifies each discovered session as
`desktop`, `cli`, `exec`, `ide`, `sdk` or `muster` from provider metadata. The
source is the Claude `entrypoint`, the Codex `originator`, then the Codex
`source`. The value travels as `AgentSession.launch`, and the core turns it
into a display label: one per catalog row, and one per entry of the flat
session list. The classification is display-only and grants nothing.

**Muster archive.** A session is `muster_archivable` when no desktop app can
archive it. That covers Muster's CLI sessions, Claude sessions not started
from Claude.app and absent from its registry, and Codex `exec` runs. Archiving
one records its id and activity time in `session-archive.json` under the
daemon's data directory. It never writes the provider's store. The catalog
leaves the session out until the provider records newer activity. The file is
replaced atomically and keeps at most 10,000 entries. The daemon ledgers the
operation like native organization. It skips Locked Use and the console
reservation because no native window is driven. Local policy still decides
control of a locked Mac. Pin stays limited to native organization.

## Alternatives

- Writing `archived` into Claude or Codex stores would bypass ADR 0111's rule
  that providers own their stores.
- Showing a mask switch that fails on a locked Mac would make the phone guess.
  Projecting the Mac's reported lock state keeps the decision in Rust.
- Inferring the launch origin from the process tree only works while the
  session runs.

## Consequences

The forced mask is reachable from the device list. A locked Mac shows why it
cannot be turned on. Older Macs without the mask are listed but disabled with
an update hint. The launch label shows how each session started. CLI sessions
can be archived in Muster, and the provider's own tools still list them. A
Codex CLI interactive thread stays on native organization because the Codex
app lists it.

## Verification

- Source and tests: `cargo test` for `muster-app-core` (the forced-mask
  projection, pending settlement, the old-Mac state and launch labels),
  `muster-agent` (launch classification, archive persistence and a
  CLI-started Claude session that leaves and returns with new activity) and
  `musterd` (`cli_archive`: a CLI archive is applied on a locked Mac while
  the desktop session still needs Locked Use).
- On the authoring Mac, debug builds overflow the default 2 MiB worker stack
  in `musterd::handler::dispatch`. This happens on `origin/main` too. The
  daemon and Swift suites were run with `RUST_MIN_STACK=16777216`. The new
  archive arm is boxed out of `dispatch`.
- `muster-agent`'s
  `slow_poll_releases_catalog_and_cannot_overwrite_replaced_viewer` failed in
  2 of 15 full runs on this branch and in none of 4 runs on `origin/main`.
  The service's queued initial scan can replace the test's fixture. This
  change does not touch that path. The last 6 consecutive runs passed.
- Builds: the Swift package builds and its tests pass, including a row-height
  check with a launch badge. MusterUI builds for the iOS Simulator. Android was not built
  locally because the Mac has no Android SDK or Java runtime. HarmonyOS was
  not built locally because the installed DevEco SDK is not version 26. The
  regenerated Swift and ArkTS protobuf code is checked in. The ArkTS files were
  stale before this change.
- Device acceptance is pending: the forced mask from a phone on an unlocked
  and on a locked Mac, and archiving a terminal-started Claude session.
