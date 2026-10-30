# ADR 0070: Publish paired Apple 1.1.5

- Date: 2026-10-30
- Status: Accepted; publication and installation pending
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
