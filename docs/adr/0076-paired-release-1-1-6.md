# ADR 0076: Publish Apple 1.1.6 to the OTA page and this Mac

- Date: 2026-10-31
- Status: Accepted; publication and installation pending
- Related: ADR 0070, ADR 0073, ADR 0074, ADR 0075; PR 72–73;
  [DESKTOP_UPDATES.md](../DESKTOP_UPDATES.md)

## Context

The owner asked to merge PR 72 and PR 73, deploy the result to this Mac,
and publish the OTA page.

- PR 72 views DeepSeek Harness sessions and drives them through Muster's
  plugin (ADR 0073, ADR 0074).
- PR 73 fixes phone sends to Claude Code sessions, which failed whenever
  Claude.app's registry held a conversation without a CLI session
  (ADR 0075).

Both are merged. The installed baseline is 1.1.5 (95).

## Decision

Publish 1.1.6 (96) from clean, merged source containing PR 72 and PR 73.

- **Build.** Build the release daemon, CLI and relay, the Apple FFI
  libraries, the Developer ID macOS app and the iOS archive. Verify source
  ancestry, compiled fingerprints, presence signing, notarization, stapling
  and Gatekeeper.
- **Publication.** Publish to the existing private publisher, as
  DESKTOP_UPDATES.md describes:
  - the notarized Mac portable;
  - the registered-device iOS OTA build;
  - the unified page that `package-ota.py` generates.

  Read back the public pages, package hashes, manifest and cache headers,
  then prune superseded releases.
- **Installation.** Install the same verified Mac portable on this Mac only.
  Use the Rust desktop-update lease and preserve the device identity, as for
  1.1.5.

The request names only this Mac and the OTA page. So this release does not
create a GitHub release, upload to TestFlight or install on the other Macs.
`deploy/public/download.html`, which describes the GitHub release and the
TestFlight build, keeps pointing at 1.1.5. Without the update feed signing
key, no in-app update feed is published, as for 1.1.5.

## Alternatives

- **The full paired release of 1.1.5** (GitHub, TestFlight review, all three
  Macs) goes beyond what was asked. It remains available as a later step on
  this version.
- **Reinstalling 1.1.5 or keeping its version** would make the new artifacts
  indistinguishable from the installed release.
- **A local `install-mac.sh` build** would install bytes different from the
  published artifact.

## Consequences

This Mac runs the DeepSeek Harness viewing and plugin control and the
Claude navigation fix. Phones and iPads on the registered-device OTA page
can install the matching iOS build. Users of the GitHub and TestFlight
channels stay on 1.1.5 until a later release. DeepSeek Harness control on
this Mac also needs the plugin installed from the main checkout
(ADR 0074).

## Verification

Code verification for the included changes is recorded in ADR 0073, ADR 0074
and ADR 0075. Record builds, signatures, notarization, publication,
installation and runtime checks separately below. No real conversation is
used as a release test.
