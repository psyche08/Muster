# ADR 0179: Publish the unified Apple session catalog

- Date: 2026-11-10
- Status: Accepted
- Related: [0178](0178-consistent-session-list-rows.md), [0166](0166-release-apple-1-1-28.md), [desktop and iOS updates](../DESKTOP_UPDATES.md)

## Context

The owner requested a paired release from main and a physical comparison of Claude, Codex and DeepSeek rows on the home screen. An installed desktop already reported 1.1.33 (123) from ea5b3fb, which predates the row change. The public macOS feed and iOS manifest previously offered 1.1.28 (118).

## Decision

Use version 1.1.34 (124) on both platforms. Package the clean main commit containing ADR 0178 and this version change as a signed macOS update and a registered-device iOS OTA release. Publish immutable packages before switching the feed, manifest and download page. Verify the delivered bytes through the page before device acceptance.

## Alternatives

Reusing 1.1.33 would leave existing installations at the same version despite different contents. A simulator and the width regression test can measure synthetic catalog rows. Checking the installed app also exercises the live catalog, nested sessions and real device typography.

## Consequences

The OTA page serves registered iOS devices. Physical iPhone layout acceptance requires an installation on a registered phone.

## Verification

Check the source stamp, macOS notarization and signature, iOS provisioning, public page, package hashes and desktop feed signature. Record physical observations independently for each platform.
