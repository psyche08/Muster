# ADR 0041: Measure desktop transcript rows at their displayed width

- Date: 2026-10-23
- Status: Accepted
- Related: ADR 0037; ADR 0031; Architecture §3.7

## Context

Installed desktop transcripts overlap multiline Markdown rows. Upward wheel
scrolling also returns to the latest content instead of reaching earlier history.
The current AppKit wrapper measures unconstrained fitting size and relies only on
live-scroll notifications to suspend latest following. Existing fixtures mostly
use short messages and post notifications directly, missing these input and
measurement paths.

## Decision

Keep AppKit collection reuse and shared Rust paging. Measure each hosted row at
its actual column width, update its height after content/disclosure/width changes,
and retain viewport anchors. Include multiline Markdown and nested block geometry
in regressions rather than only comparing adjacent layout attributes.

Handle native scroll-wheel events at the scroll view, including phase-less mouse
wheels. Suspend latest following before AppKit moves the viewport, then capture
the resulting anchor and emit at most one earlier-page intent per gesture. Retain
live-scroll support for scroller dragging, loading/error guards and the explicit
history button. Layout and tab restoration must not generate history requests.

## Alternatives

Clipping overflowing text hides content without fixing row measurement. Fetching
history from layout callbacks can create unbounded requests. Removing recycling
would restore long-session rendering costs. Increasing estimated heights merely
changes where overlap appears.

## Consequences

Native code continues to own geometry and input adaptation only. Shared Rust
retains page size, cursor, availability and serialization. Width changes require
remeasurement, while recycled hosts and disclosure state remain bounded as before.

## Verification

First reproduce the failures with real AppKit fixture rows and a phase-less wheel
event. Test long/wrapping text, nested Markdown, resize, disclosure, prepends,
latest following and automatic earlier loading; retain existing reuse/teardown
checks. Inspect the installed UI separately. Record builds, signing, publication,
installation and runtime acceptance independently; keep GitHub Actions disabled.

### Source regression evidence

Before the fix, both new AppKit regression cases failed: the long row retained
134 points of ideal-width height at multiple window widths, and a phase-less
wheel reaching the top emitted zero earlier-page intents. Width-constrained
hosting corrects the measurement; the reference fixture is mounted before
comparison so Markdown's asynchronous margin preferences have settled.

The full macOS suite passed 72 tests. After adding explicit trackpad phase
handling, all nine conversation tests passed, including the new paused-trackpad
case. Tests cover nested provider-card Markdown at three widths, repeated wheel
bursts, loading/error suppression, reading-position retention, native scroll
notifications, disclosure resizing/reuse, prepend anchors, unsent answers across
tabs and teardown. Swift tests link an existing local Rust FFI archive; native
linker warnings about that archive's newer deployment target do not establish
older-macOS compatibility. Rust paging logic is unchanged.

Prepare paired Apple release 1.0.90 (81). Production rebuild, notarization,
publication, installation and installed UI acceptance remain pending at this
source commit. GitHub Actions remains disabled.

### Paired release and installed acceptance

- Source PR #29 merged. Clean release source is
  `eb47f620437f5a580b51f2c318ee3a6269f43da1`. Local daemon, current macOS/iOS
  FFI archives, macOS Release app and iOS archive built successfully.
- macOS notarization accepted request `99f48ae2-d03f-4313-90ed-df3c2b05ed90`.
  Presence/source ancestry, strict signatures, staple and Gatekeeper checks
  passed. Independent public downloads verified macOS 1.0.90 and iOS build 81,
  the actual OTA install link, hashes and cache headers. macOS ZIP SHA-256:
  `289468aad4536c84811a1bfe5c3e0b02a566c2e0f170bcc42d9934ac645da252`;
  iOS IPA SHA-256:
  `4d20fd39000099d687c0a5e914111cbc4027b8439650e8353d015ca6b7f60a0f`.
- All three Macs installed using the Rust update lease with device identities
  retained. Each reports 1.0.90 (81), Accessibility granted, relay registered
  and Vnet active. Runtime source stamps match the release source and each
  installed daemon SHA-256 is
  `b586e8f8e45a2c2822fae24ff5f48a6dfb759cbe1e3dea64cf639e07a4172d17`.
  All six directed RPC paths, six directed virtual-IP ping paths, and three
  installed signature/staple/Gatekeeper checks passed.
- Before/after screenshots of the same real conversation show the formerly
  overlapping next-step card, long Markdown body and changed-file summary
  separated correctly on installed 1.0.90. Changed files remain collapsed.
  No provider message or answer was submitted during this inspection.
- Installed wheel acceptance remains pending: the UI-control tool could read
  and operate semantic controls but refused both coordinate and element-targeted
  wheel input with `windowNotFoundAtPosition`, even after bringing the window
  forward. The console was unlocked. An accessibility Scroll Up action produced
  no visible change; it does not establish wheel behavior. The owner was asked
  to verify actual scrolling. AppKit wheel/trackpad fixture tests passed, but
  these are not a substitute for that installed interaction check.
- A relay QUIC closure during startup reauthenticated the next second and later
  selected QUIC again; peer timeouts during the other devices' installation
  recovered. No persistent network failure is inferred from those messages.
  These checks are not new transport fault injection. Two Macs lack Claude Cloud
  credentials; native provider sends were not tested by this UI release.
- Physical iPhone installation and UI acceptance were not performed. GitHub
  Actions stayed disabled; no hosted CI result or Xcode Cloud build is claimed.
  No historical OTA artifacts were deleted.

### Owner feedback after release

The owner rejected the pending installed wheel acceptance: earlier history loads,
but the viewport moves into the additional content rather than keeping the
current reading position. This is an unresolved 1.0.90 pagination regression,
not successful acceptance. ADR 0042 tracks response-during-scroll anchoring.
