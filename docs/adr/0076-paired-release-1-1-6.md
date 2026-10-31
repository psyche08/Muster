# ADR 0076: Publish Apple 1.1.6 to the OTA page and this Mac

- Date: 2026-10-31
- Status: Accepted; published and installed on one Mac; server prune, Claude send and physical iPhone acceptance pending
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

### Publication and installation — 2026-10-31

**Source and checks.** PR 74 landed the version configuration. The release
tree was a clean, detached checkout of `0da52647182d00a2e08d1375ffeb4f5e90fb4ccc`,
which contains PR 72 and PR 73. `xcodegen generate` reproduced the committed
project. No hosted CI checks were reported, and no hosted CI pass is claimed.

**Build and signing.**
- The release daemon, CLI, relay and browser host, the FFI libraries and the
  Developer ID macOS app all built. The first app build stopped at its
  bundling phase because the browser host had not been built. That binary is
  now part of the release's Rust build.
- The presence-signing, release-ancestry and compiled-fingerprint gates
  passed on the built app.
- Apple notarization `24cd4eea-f18a-4351-a745-1ef2894b3721` returned
  `Accepted`. Stapling, strict signatures, Gatekeeper ("Notarized Developer
  ID") and the release gates passed again on the final portable. Its daemon
  is stamped with the release commit.
- The iOS archive succeeded. It is development-signed for three registered
  devices, and its profile expires on 2027-09-30. No App Store export was
  made.

**Publication.**
- The private publisher serves the Mac portable with SHA-256
  `28a6ae471980408811e17fb14429a880f0f254d0ceb941bc68b939e30136640c`.
- It serves the iOS OTA build 96 with SHA-256
  `1d5f94718978feba3ad1529fb5b992f34c7aef340e93ca73d2b655eb208ea1be`.
- It serves the unified page at its root, `ios/` and `desktop/`.
- The publish step verified all 11 staged hashes on the server. It published
  hash-named files without overwriting any, and replaced the aliases and
  pages atomically.
- Read back over HTTPS:
  - all three pages equal the generated page;
  - both downloads match their hashes;
  - the manifest names build 96 and the hash-named IPA;
  - the 1.1.5 manifest URL redirects to the current manifest;
  - pages, manifest and packages carry `no-store, no-cache, must-revalidate,
    max-age=0`.
- No signed update feed was published. The superseded releases were not
  pruned: the server-side prune was not run in this session and is left to
  the owner.

**Installation and runtime (one Mac).**
- The verified portable was installed through the Rust desktop-update lease.
  The scheduler was idle and the previous app copy was retained. The Mac runs
  1.1.6 (96) from `0da52647` with the expected daemon hash
  `c85b7daaa5096cd87e610359b03f3cd00e315e0c2d256522d048fab2634503ee`.
- Its device ID, fingerprint and Secure Enclave backend are unchanged, and
  Accessibility, relay registration and Vnet are healthy.
- The installed daemon lists both DeepSeek Harness sessions with the
  `plugin` controller, process `running` and state `idle`, through the
  plugin the owner installed.

**Unverified acceptance.**
- No phone send to a Claude session has been repeated with this build, so
  ADR 0075's fix is not yet confirmed on a device.
- The other Macs, GitHub Release, TestFlight and physical iPhone
  installation are outside this release.

