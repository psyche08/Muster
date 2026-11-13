# ADR 0207: Identify the macOS retest with Apple build 133

- Date: 2026-11-13
- Status: Accepted
- Related: [0205](0205-background-native-tasks.md), [0206](0206-publish-apple-1-1-42.md)

## Context

The operator requested a fresh version and build to exercise background session selection on a physical Mac.

## Decision

Advance the shared Apple project settings to 1.1.43 (133) and build the macOS application for paired-device acceptance.

## Alternatives

Reusing build 132 would leave the installed version unable to distinguish this test from the previous run.

## Consequences

This version change retains the session-selection behavior of build 132. Device acceptance depends on a verifiable session and writable composer.

## Verification

Compare the generated Xcode settings with the project definition. Verify the daemon source stamp, signed application version, notarization, and running target identity. Exercise remote sends and inspections, checking the destination, foreground and return to the original session.
