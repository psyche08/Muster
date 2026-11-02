# ADR 0088: Unattended desktop updates and a new feed signing key

- Date: 2026-11-02
- Status: Accepted; signed feed published with 1.1.10, unattended update pending
- Related: [DESKTOP_UPDATES](../DESKTOP_UPDATES.md),
  [ADR 0085](0085-paired-release-1-1-9.md),
  [ADR 0084](0084-keep-awake-menu.md)

## Context

The owner asked for automatic updates. The Mac app already had a complete,
manual updater: Settings checks a signed Velopack feed over DoH, downloads and
verifies a full package, takes the daemon's update lease, stops the service and
lets Velopack replace the bundle. Two things kept it from ever running:

- **No feed.** Since 1.0.28 every Mac release was published as a notarized
  portable only, because the private key behind the compiled
  `release-public.pem` was not on the release Mac. The publisher has no
  `releases.osx-arm64.json`, so a check finds nothing.
- **No schedule.** Checking, downloading and installing each need a click on
  the Mac. The three Macs are mostly locked and driven from the phone, so a
  click rarely happens; every release so far was installed by hand.

## Decision

- **Policy in Shared Rust.** `desktop_update::auto` decides each step:
  - check two minutes after launch, then every six hours;
  - download an available update at once;
  - install a downloaded update when nobody has touched this Mac's keyboard or
    pointer for 15 minutes (CoreGraphics HID idle; Accessibility writes and
    remote control do not count) and the core's restart guards hold: no send,
    terminal, Locked Use or presence prompt, and no privacy cover.
  - A locked screen does not block an unattended install
    (`State::can_update_unattended`); the attended Restart and Install buttons
    keep requiring an unlocked screen.
  - The steps reuse the Settings actions, so the signed feed, package checks,
    daemon lease and service stop are unchanged.
  - An install that fails after stopping the service sets `install_failed` and
    waits for the next check instead of stopping the daemon every tick; a lease
    refusal (the daemon is busy) leaves the daemon running and retries on the
    next five-minute tick.
- **Choice in the shell.** `AutoUpdatePreference` (`autoUpdate` in the app's
  defaults, on by default) is sent at core start as `DesktopUpdateAction`
  `AUTO_ON`/`AUTO_OFF`; Settings → Muster Updates shows an **自动更新**
  (Automatic updates) toggle bound to `DesktopUpdateView.automatic`. A
  generation number makes ticks from an earlier on/off loop no-ops.
- **New feed key.** A new P-256 key pair was generated on the release Mac. The
  private key lives outside the repository at
  `~/.config/muster-release/desktop-feed-key.pem` (mode 0600) and is never
  uploaded; the public half replaces `release-public.pem`.
- **Feed publication.** Each notarized Mac release now also publishes, in
  order, `Muster-<version>-osx-arm64-full.nupkg`, `releases.osx-arm64.json` and
  its `.sig` made by `sign-feed.py`. The package comes from the same `vpk pack`
  run as the portable; its app must carry the notarized portable's CDHash.
  `prune-releases.py` already keeps only the current feed and package.
- **Transition.** Builds before 1.1.10 trust the old key and cannot verify the
  new feed, so 1.1.10 is installed by hand once; later releases reach the Macs
  through the feed.

## Alternatives

- **Notify and let the owner click Install.** Safer for a Mac in use, but these
  Macs are rarely attended; the owner chose unattended installation.
- **Install only behind a locked screen.** M4Mini often sits unlocked and
  untouched, and would never update; HID idle covers both cases.
- **Keep the old key.** Its private half is not available on the release Mac;
  searching other machines for credentials was not authorized.
- **Store the choice in Rust.** The Rust display cache deliberately holds no
  settings; like Keep Awake (ADR 0084), the OS preference stays in the shell
  and only the decision logic is shared.
- **A daemon-side updater.** Velopack replaces the app bundle from the running
  app; musterd is inside that bundle and is the process being stopped.

## Consequences

- After 1.1.10, publishing a release updates the three Macs within about six
  hours plus their next 15-minute quiet period, without SSH installs.
- An unattended install briefly drops remote control and Vnet on that Mac
  (as a manual install does). Managed Multica tasks are not part of the daemon
  lease; an install during a Multica task interrupts its bridge for the
  restart.
- The previous feed key is retired. Anyone holding a pre-1.1.10 build needs the
  portable once.
- The release process gains a signing step that needs the private key file on
  the release Mac.

## Verification

### Source and tests — 2026-11-02

- Rust: `desktop_update::auto` unit tests (check only when due, download at
  once, install only when quiet and idle, a failed install waits for the next
  check), an intent test (the shell's choice starts and stops the loop; a
  repeated `AUTO_ON` does not start a second loop; a stale tick does nothing),
  and a guard test (an unattended update may proceed behind a locked screen but
  not under the cover or while busy). The existing feed-signature, cached
  package and lease tests pass. `cargo clippy -p muster-app-core --all-targets`
  is clean.
- The Swift bindings were regenerated with the vendored `protoc` 36.2 and
  `protoc-gen-swift` built from the resolved SwiftProtobuf checkout, as CI does.
- Swift: `AutoUpdatePreferenceTests` and `KeepAwakePreferenceTests` pass; an
  unsigned Debug `xcodebuild` of `MusterMac` succeeded.
- `sign-feed.py` signed the 1.1.9 feed with the new key and verified it against
  the new compiled public key. It no longer needs Python 3.11's
  `hashlib.file_digest`, which the release Mac's system Python lacks. The 1.1.9
  package's app has the notarized portable's CDHash
  (`717848ca727340952e7a967bd7e5cff22a56fd9c`).

### First signed feed — 2026-11-02

[ADR 0089](0089-paired-release-1-1-10.md) published 1.1.10 (100) with the
first feed signed by the new key; its public signature verifies against the
committed public key and its package holds the notarized app. 1.1.10 was
installed by hand on the three Macs.

### Not yet verified

An unattended update from 1.1.10 to a later release on the three Macs.
