# ADR 0177: Use one session row across providers

- Date: 2026-11-10
- Status: Accepted
- Related: [session catalog](0113-pin-only-session-catalog.md), [provider paging](0148-per-device-provider-catalog-pages.md)

## Context

Apple catalog rows use a compact layout for Claude and a different layout for
other providers. The latter repeat the provider and display their last-message
time and working directory on a different line.

## Decision

Use one session row with a leading title, trailing state, and a secondary line
for the source and last-message time. Display the working directory and search
excerpt when available. The catalog supplies the provider and device label.
Nesting and actions remain on the containing row.

## Alternatives

Provider-specific layouts make equal states look different. Duplicating the
provider badge also reduces the space for titles on a phone.

## Consequences

Every provider uses the same alignment, truncation and metadata order.

## Verification

The macOS rendering tests passed for equal-height Claude, Codex and DeepSeek
catalog rows at 320 and 736 points and for the phone-width catalog fixture.
The shared Apple UI built for the iOS simulator. Physical-device acceptance
is pending.
