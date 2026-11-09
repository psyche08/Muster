# ADR 0166: Release Apple 1.1.28 (118) to every Mac and iPhone

- Date: 2026-11-09
- Status: Accepted; build, publication and installation pending
- Related: [ADR 0143](0143-paired-release-1-1-23.md),
  [ADR 0144](0144-local-release-and-independent-installation.md) (local build
  and independent installation jobs),
  [ADR 0157](0157-publish-apple-1-1-27.md) (previous paired publication),
  [DESKTOP_UPDATES](../DESKTOP_UPDATES.md)

## Context

The operator asked for a build of current `main` on every Mac and iPhone. The
three Macs and the public download page run 1.1.27 (117), built from
`7b489084`. Since then, `main` has landed ADRs 0158–0165, including queued
message actions, the growing history window, turn pages, Multica task sessions
and the daemon's main-thread event loop. None of them is installed anywhere.

`project.yml` still says 1.1.27 (117). Building `main` under that number would
give two different programs the same version. The installer and Velopack
compare versions, so neither could tell them apart, and Velopack would not
offer the new build as an update.

No iPhone is connected to the release Mac, so the only route to the phones is
the registered-device OTA page.

The first release build of the bumped commit failed: `muster-cli` did not
compile. ADR 0161 (`d2c59e49`) added `AgentEvent.unavailable`, but the CLI's
`agent-watch` match was not updated. CI is disabled for the repository, so no
check caught it. `main` has not built the bundled CLI since then, and a release
bundle cannot be built without it.

## Decision

- **Version.** Set `MARKETING_VERSION: 1.1.28` and
  `CURRENT_PROJECT_VERSION: 118` in `project.yml`, and regenerate the committed
  Xcode project with xcodegen 2.46.0.
- **CLI build fix.** `muster agent-watch` prints an `unavailable` event as
  `unavailable: <session ids>`. Like the other events, it is only reported;
  the CLI has no session list to update.
- **Source.** Build both platforms only from a clean, detached checkout of the
  merge commit that lands the CLI fix. The daemon's build stamp must equal that
  commit.
- **macOS.** Follow the ADR 0143/0144 process: release Rust binaries, the Apple
  FFI, a Developer ID build with the Presence provisioning profile,
  `verify-release.sh` and the Presence signing check, a Velopack 1.2.158
  portable and full package, notarization, stapling, a strict signature check
  and Gatekeeper. Sign the feed locally with the existing key; the key is not
  copied anywhere.
- **iOS.** Archive the same commit for registered devices and package it with
  `package-ota.py`.
- **Publication.** Publish the portable, full package, signed feed and iOS OTA
  output in the documented order: hash-named files first, then the fixed
  aliases, the feed and its signature, then the pages. Verify every file over
  public HTTPS. Pruning superseded server files needs the operator's separate
  approval.
- **Mac installation.** Use the ADR 0144 one-shot job in each Mac's GUI launchd
  domain. It holds the Rust update lease, checks that the Multica scheduler is
  idle, exchanges the bundle atomically without keeping the old program, and
  verifies identity, version, daemon hash, Accessibility, Relay and Vnet.
- **iPhone installation.** The operator installs from the OTA page on each
  registered phone. Publication alone does not count as installation.
- **Record.** After publication, add the 1.1.28 row to
  `deploy/desktop/released-commits.txt` and record the evidence in this ADR.

## Alternatives

- **Build `main` as 1.1.27.** Rejected: two programs would share one version,
  and Velopack would not offer the new one as an update.
- **Install the Macs only.** Rejected: AGENTS.md pairs every macOS publication
  with an iOS build, and the operator asked for the iPhones too.
- **TestFlight.** Not used: the phones already install from the OTA page, and
  TestFlight is a separate, opt-in distribution.

## Consequences

Every device can run the same `main` build, and the in-app updater offers 1.1.28
to any Mac that has not been installed directly. Device acceptance of the
features in ADRs 0158–0165 is still separate work. Installing it is not the same
as accepting those features.

## Verification

Record each item separately:

- source commit and build stamp;
- macOS signing, notarization and Gatekeeper;
- iOS archive version and IPA hash;
- public download, feed signature and cache headers;
- on each Mac: the job's durable result, version, daemon hash, preserved
  identity, Accessibility, Relay and Vnet;
- iPhone installation, as reported by the phone or the operator.
