# ADR 0042: Preserve the reading anchor when history arrives during scrolling

- Date: 2026-10-23
- Status: Accepted
- Related: ADR 0037; ADR 0041; ADR 0034; Architecture §3.7

## Context

The owner rejects installed 1.0.90 history acceptance: earlier content now loads,
but the viewport jumps into the newly revealed content instead of keeping the
reader's current position. The previous tests checked wheel request emission and
idle prepends separately, missing a history response during an active gesture.
The macOS controller currently suppresses anchor restoration while scrolling;
later scroll callbacks can capture rows from the replaced snapshot instead.

## Decision

Treat a transcript projection change as a native geometry transaction. Retain the
visible stable row identity and its screen offset through snapshot application
and self-sizing, including while a wheel/trackpad gesture remains active. Do not
replace that anchor with newly inserted rows before reconciliation. Genuine user
movement after reconciliation must continue to update the reading position.
Keep Shared Rust paging, identities, page limits and loading/error guards intact.
No layout callback may request another history page.

## Alternatives

Waiting until scrolling ends exposes inserted content and loses the old anchor.
Keeping the old absolute offset selects different content after a prepend.
Anchoring by total height difference is unreliable with estimated and changing
row heights. Loading all history or disabling momentum does not fix ownership of
the reading anchor and would change unrelated behavior.

## Consequences

The native controller must distinguish projection-induced geometry changes from
user scrolling. Preserve existing latest-follow, explicit jump, disclosure and
tab behavior. This is row anchoring; character-level anchoring inside a single
expanded block is a separate limitation described in ADR 0034.

## Verification

Reproduce with an earlier response while a native gesture is still active, assert
the original row's screen offset before and after gesture end, then repeat with
another page and subsequent user movement. Cover delayed idle responses and
self-sizing rows. Run existing macOS conversation and lifecycle regressions.
Record source tests, release builds/signing, installation and owner acceptance
separately. Keep GitHub Actions disabled.

### Source evidence and release candidate

The new live-scroll regression failed on the original implementation: the old
row moved from -62 to 19722 points relative to the viewport and stayed there
after gesture end. Synchronous snapshot reconciliation fixes that race; bounded
self-sizing reconciliation remains allowed during the same gesture. Actual scroll
callbacks then capture the reader's new position normally. Synchronous geometry
work does not publish a SwiftUI binding from inside its update callback.

The complete local Swift suite passed 75 tests. The final targeted macOS suite
checks active-response anchoring, two successive 200-row pages with mixed row
heights and real phased wheel events, continued movement, idle prepends,
disclosures, tabs, wrapping, latest-follow and resource release.
Existing linker deployment-target warnings remain; these runs do not establish
compatibility on the minimum supported OS. Release candidate 1.0.91 (82) keeps
paired macOS/iOS versioning. Build, notarization, publication, installation and
physical scrolling acceptance are pending at this source checkpoint.
