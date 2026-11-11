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
