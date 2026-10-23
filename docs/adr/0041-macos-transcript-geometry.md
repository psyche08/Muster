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
