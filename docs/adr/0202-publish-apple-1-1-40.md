# ADR 0202: Publish Apple 1.1.40 (130)

- Date: 2026-11-13
- Status: Accepted
- Related: [ADR 0198](0198-publish-apple-1-1-39.md), [ADR 0200](0200-optional-requester-locked-use-auth.md), [ADR 0201](0201-background-claude-card-selection.md)

## Context

The installed release predates the requesting Mac's Locked Use preference and the target Mac's background Claude approval-card lookup.

## Decision

Package the macOS and iOS clients from one clean main commit as 1.1.40 (130). Publish the signed desktop feed, GitHub portable and registered-device iOS OTA. Select this iOS build for the existing public TestFlight group and update reachable devices.

## Alternatives

A client-only release would leave the previous session selection implementation on the target Mac.

## Consequences

The target Mac needs the new daemon for background card lookup. Public TestFlight availability depends on Apple's processing and review.

## Verification

Verify the daemon source stamp, package versions, signatures, notarization, download checksums, installed versions, and the selected external build separately.
