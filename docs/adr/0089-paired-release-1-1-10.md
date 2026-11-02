# ADR 0089: Publish Apple 1.1.10 (100) and install it on the three Macs

- Date: 2026-11-02
- Status: Accepted; publication and installation pending
- Related: [ADR 0088](0088-unattended-desktop-updates.md),
  [ADR 0085](0085-paired-release-1-1-9.md),
  [ADR 0081](0081-paired-release-1-1-8.md),
  [DESKTOP_UPDATES](../DESKTOP_UPDATES.md)

## Context

The owner asked to merge unattended desktop updates (ADR 0088), publish them
and install them on the Macs. The published line is 1.1.9 (99), built from
`c8e6e117`. Since then `main` gained ADR 0088: the app checks the signed
Velopack feed on a schedule and installs a verified update when the Mac has
been idle, and it trusts a new feed signing key.

Builds before this one trust the old feed key, so they can never accept a feed
signed with the new key. This release is therefore the last one that has to be
installed by hand.

## Decision

Publish **Apple 1.1.10 (100)** from the merge commit that carries this version
bump, following the ADR 0081 and ADR 0085 process, with one addition:

- **Source.** This PR sets `MARKETING_VERSION: 1.1.10` and
  `CURRENT_PROJECT_VERSION: 100` and regenerates the committed Xcode project
  with xcodegen 2.46.0. The release worktree is a clean detached checkout of
  the merge commit `C`; `verify-release.sh` must prove the daemon's
  `MUSTER-BUILD-COMMIT` equals `C`.
- **macOS.** Release daemon, CLI, relay and browser host, Apple FFI archive and
  Developer ID app; Velopack 1.2.158 portable and full package; notarize,
  staple, strict signature, Gatekeeper and presence-signing checks.
- **Signed feed (new).** `releases.osx-arm64.json` is signed with the ADR 0088
  key by `sign-feed.py`, which verifies the signature against the public key
  committed in the app before it writes the `.sig`. The full package must hold
  an app with the same code directory hash as the notarized portable. The
  publisher receives the package first, then the feed and its signature, so a
  client never sees a feed that names a missing package. The private key never
  leaves the release Mac.
- **iOS.** The paired development-signed OTA build 100 for the registered
  devices; no TestFlight upload or App Store export.
- **OTA pages.** As in ADR 0085, read back over HTTPS, including the feed
  signature against the committed public key.
- **Installation.** The same notarized portable on M4Mini, M1Max and M4Pro,
  one at a time, preserving device identity and Secure Enclave key, keeping a
  previous-bundle copy, never over the virtual-network address.
- **Not in this release.** GitHub Release, TestFlight, App Store, physical
  iPhone installation and the server prune (needs explicit owner
  authorization).

## Alternatives

- **Let 1.1.9 update itself.** It trusts only the old key, which is not
  available, so it would reject the new feed.
- **Publish the feed without installing 1.1.10 by hand.** The Macs would stay
  on 1.1.9 and never see it.

## Consequences

- From 1.1.10 on, the Macs pick up later releases by themselves when idle,
  unless the owner turns Automatic Updates off.
- An unattended update is first exercised by the release after this one.
- Superseded releases stay on the server until the owner authorizes a prune.

## Verification

Pending; recorded after publication and installation.
