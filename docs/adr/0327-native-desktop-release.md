# ADR 0327: Build the desktop release from a shared main commit

- Date: 2026-11-27
- Status: Accepted for packaging. Distribution and installed-device verification pending.
- Related: [Windows portable distribution](0240-windows-portable-distribution.md), [console selection](0326-select-console-by-session.md), [desktop updates](../DESKTOP_UPDATES.md)

## Context

Several development hosts held different local main references. The original console worker commit had already landed through a cherry-pick followed by fixes. The predecessor and the integrated commit had different identifiers.

## Decision

Join the original worker lineage to main with an unchanged source tree. Build Apple 1.1.48 (138) and Windows x64 1.1.128 natively from the same merged commit. Keep the Apple version in the XcodeGen project and regenerate the committed project. Use the established desktop and registered-device release channels.

## Alternatives

Shipping packages from separate feature branches would make the installed source harder to reproduce. Reapplying the old worker patch would discard subsequent integration fixes.

## Consequences

Both desktop builds can report the same source revision. Packaging, publication and installation have independent acceptance results. The existing physical-login-screen and disconnected-viewer keyboard limitations still apply.

## Verification

Check the ancestry of development branch heads, the embedded commit, package versions, platform signing and release artifacts. Read back published hashes and verify the running version and identity on each installation target.
