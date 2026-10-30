# ADR 0070: Publish paired Apple 1.1.5

- Date: 2026-10-30
- Status: Accepted; published and installed; Apple external review and manual acceptance remain pending
- Related: ADR 0067, ADR 0068, ADR 0069, ADR 0024; PR 66–67

## Context

The owner requests one release containing both the retry/ignore interaction and
Mac Enter/Shift+Enter mapping, installation on all three Macs, and paired iOS
publication. Both source changes are merged. The installed baseline is 1.1.4 (94).

## Decision

Publish 1.1.5 (95) from clean, merged source containing both changes. Build the
shared Rust libraries and both Apple applications; verify source ancestry,
compiled fingerprints, signing, Apple notarization and public download hashes.
Publish the Mac portable artifact to GitHub and the existing private publisher,
with the paired registered-device iOS OTA and App Store Connect build. Select
only build 95 for public TestFlight review and report Apple's actual state;
default Xcode Cloud external distribution remains disabled.

Install the same verified Mac artifact on all three Macs using Rust update
leases and management paths independent of the virtual network. Preserve device
identity and active tasks; verify the scheduler is idle before replacement and
running afterward. Remove any temporary loopback-only SSH route after acceptance.

## Alternatives

Publishing only one change does not meet the request. Reusing 1.1.4 would make
new artifacts indistinguishable from the installed release. An unsigned update
feed cannot substitute for the configured signing key; publish the notarized
portable if the feed signing key is unavailable.

## Consequences

Retry uses normal submission; Ignore dismisses presentation without claiming
delivery. Enter sends and Shift+Enter inserts a newline in the Mac composer.
The iOS release includes the shared retry/ignore change; its keyboard behavior
is unchanged. Native Codex creation/directory acceptance and physical iPhone
acceptance remain separate from build, publication and mesh startup checks.

## Verification

PR 66's source evidence includes 388 passing Rust tests and strict Clippy; PR 67
includes three native editor tests and a standalone native App event-path test.
Check the version/project and public-content/ADR gates for this release, then
record code, builds, signatures, publication, installation, mesh and provider
acceptance separately below. No real conversation is used as a release test.

### Publication and device verification — 2026-10-30

**Source and checks.** PR 68 landed the paired version configuration before
building. Tag `v1.1.5` targets clean source
`3ae46a2f571d06a695dec8768a3143cffc0151e6`, which contains PR 66 and PR 67.
Project generation, public-content and ADR checks passed. The source tests
listed above remain distinct from installation acceptance. No hosted CI checks
were reported on the release PR; no hosted CI pass is claimed.

**Build and signing.** Release daemon/CLI, the three Apple FFI targets, macOS,
the iOS archive and App Store export passed. The Mac presence-signing,
release-ancestry and compiled-fingerprint gates passed. Apple notarization
`eb979ae0-b490-4e7e-beeb-a7c5c71c7ead` returned `Accepted`; stapling,
strict signatures and Gatekeeper assessment passed for the final portable.
The App Store IPA has production APNs and debugging disabled.

**Publication.** GitHub Release `v1.1.5` and the existing public download site
serve the verified Mac artifact. Its SHA-256 is
`8f8af3712dd6104cbbf4b16a3b3645f62b1f6ed19375fcab31231e2003e36b7c`.
Unauthenticated GitHub metadata and checksums match the published asset digest
and exact source commit. The paired iOS OTA build 95 has SHA-256
`cd6727fc026a9d4aac685c34e6d7948d888208df1f447f0e20d2c371d1fcf22e`;
its profile contains three registered devices and expires on 2027-09-30.
Public pages, both package hashes, the hash-bound manifest and legacy manifest
redirect were read back successfully with `no-store` caching. No new signed
application update feed was published because its signing key was unavailable.

**TestFlight.** App Store Connect accepted build 95 as `VALID`. This specific
build was added to Internal and Public Beta and submitted for external review.
The last readback is `IN_BETA_TESTING` internally and
`WAITING_FOR_BETA_REVIEW` externally. The public invitation remains enabled;
this is not evidence that external testers can install build 95 yet. Default
Cloud external distribution was not changed.

**Installation and runtime.** All three Macs run 1.1.5 (95), source `3ae46a2f`,
with the identical final daemon hash. Device IDs, fingerprints and Secure Enclave
backends match the pre-installation snapshots. Installed signatures, Accessibility,
relay registration, virtual networking and Locked Use readiness passed. Six
cross-device RPC directions and 24/24 ping packets passed without loss.
The managed scheduler exceeded the installer's initial recovery window, then
recovered automatically; a fresh full verification confirmed it running with
keep-alive restored. This initial timeout was retained in the private evidence,
not treated as an initial pass. Rollback application copies were retained on
all three Macs. The temporary management route was closed after the original
route was verified again.

**Unverified acceptance.** No existing provider conversation was replayed.
Real installed Retry/Ignore and keyboard interaction, native Codex creation and
directory selection, and physical iPhone acceptance were not performed in this
release run. Earlier source tests do not stand in for those device interactions.
Detailed build and deployment records are retained in the ignored local release
directory; private device and deployment identities are not included here.
