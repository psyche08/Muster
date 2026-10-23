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

### Follow-up: partial boundary rows

Installed 1.0.91 did not complete physical acceptance. A single Load earlier
click in an existing long conversation exposed different older cards at the
top, without continued wheel input. Keep that failed observation distinct from
the passing synthetic stable-row tests.

The next regression covers a partial boundary row being replaced while later
visible rows retain their identities. Retain the visible rows and their offsets
as ordered geometry anchors, and choose the first surviving content row when
applying a snapshot. Never fall back to the permanent header if previously visible
content was available: its identity survives every prepend but its location is
not the reader's content. This is native geometry fallback, not reconstruction
of provider group semantics. Verify the failure before the fix, repeated pages,
and installed single-click and wheel behavior before declaring acceptance.

The follow-up regression reproduced a lost primary anchor when a partial row is
replaced: the surviving next row moved from 118 to 19950 points. The installed
conversation's fetched history crossed tool-group prefixes; cached-window
identity stability from ADR 0034 does not give a previously unknown network
prefix the same first-message identity. Fallback uses only retained native row
identities and coordinates, with no text matching or provider reconstruction.

Paired 1.0.91 (82) was built from clean `abe0c066e4fb55ccf23d6c550dba2e3f7cef915e`,
published and verified from public downloads. macOS notarization
`69037ad7-4011-4229-8d74-82ab08e3f308` was Accepted; signatures, staple and Gatekeeper
passed. All three Macs installed it with unchanged identities, Accessibility
granted, relay registered and Vnet up. Six directed RPC checks, six virtual-IP
pings and three installed integrity checks passed. These do not supersede the
failed physical transcript acceptance above. iPhone installation is unverified.
The boundary fallback prepares paired release 1.0.92 (83).

The boundary fallback passed all 12 macOS conversation tests and the complete
local Swift suite (76 tests). Publication and installed acceptance of the
follow-up remain pending at this source checkpoint.
