# ADR 0188 – Publish trusted dispatch in Muster for macOS 1.1.37

- Date: 2026-11-11
- Status: Accepted
- Related: [0182](0182-publish-native-create-options.md), [0187](0187-trusted-multica-dispatch.md), [desktop updates](../DESKTOP_UPDATES.md)

## Context

The signed 1.1.36 release predates task-scoped Multica dispatch. Diagnostic
builds used the same version during installed acceptance. The owner requested
a macOS release on GitHub and installation on the enrolled Macs.

## Decision

Advance the shared Xcode version source to 1.1.37 (127). Build the macOS app
from clean merged source, sign and notarize the arm64 package, publish the
GitHub archive and signed desktop feed, and install the verified release on
each Mac. The operator selected macOS only for this release.

## Alternatives

Reusing the diagnostic version would make it impossible for installed 1.1.36
clients to discover a newer version through the feed.

## Consequences

Each host retains its enrollment and local scheduler policy. Installing the
application does not enroll a host in unattended dispatch. The next Apple
release can continue using the single Xcode version source.

## Verification

Check source ancestry and stamp, package versions, Presence signing,
notarization, stapling, Gatekeeper, feed signature and public artifact hashes.
Check each installed app, daemon identity and runtime after taking the desktop
update lease. Record publication and device evidence separately from the
locked workflow acceptance in ADR 0187.

## Release evidence

[GitHub release v1.1.37](https://github.com/psyche08/Muster/releases/tag/v1.1.37)
and the public desktop update feed serve the release built from
`3ff2da94804b36cacfe848e8982656e96e49360f`. Developer ID and Presence signing, release ancestry,
Apple notarization, stapling and Gatekeeper checks passed. The final link
targeted macOS 14. The version and signed executables in the full update
matched the notarized portable. The GitHub archive and public desktop
downloads were read back and verified, including the update signature.

| Artifact | SHA-256 |
| --- | --- |
| macOS install ZIP | `e073e663c86a88518e5db2517e1eac0ec140834ba601f79ebfbb0e5c37d462aa` |
| macOS full update | `6062396d7f77a35130d4db6ef73c8955480197732f9d0a6d48ab573dc36a5fb9` |

All three enrolled Apple silicon Macs ran 1.1.37 (127) after independent
installation jobs acquired the desktop update lease. Each reported the
release commit, its original device identity and granted Accessibility.
Existing host-specific dispatch enrollment was preserved. The registered
iOS manifest still pointed to build 126. Locked issue execution was previously
verified on the diagnostic build in ADR 0187.
