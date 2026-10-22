# ADR 0029: Page the conversation projection before native layout

- Date: 2026-10-22
- Status: Accepted
- Related: ARCHITECTURE.md §3.7; ADR 0026

## Context

An uploaded phone performance report shows long conversations commonly holding
about 500 messages, up to 2.17 seconds of sampled main-queue delay and a peak
footprint near 625 MB. The latest samples are from 1.0.82 (73), not the already
published 1.0.84. The samples do not contain tap spans or stack traces, so they
do not establish the cause of every delay. Source inspection confirms that all
cached messages are projected into both timeline and trace, then the native
conversation uses eager layout to preserve growing Markdown row heights.

## Decision

Shared Rust owns a recent-history presentation window, initially 80 messages.
Keep the complete cached transcript and provider synchronization unchanged.
Project timeline and trace only for the window. Existing Load earlier reveals
another cached page before requesting remote history; Jump to first can still
reveal/fetch all history explicitly. Keep an ID anchor, not a moving suffix,
so streaming replies and new messages do not evict content being read. Reconcile
a deleted anchor on authoritative history reset, and reset the window when
opening another conversation. Pending interactions and composer state are
independent and remain available regardless of the visible transcript window.

The shell renders the Rust projection without duplicating pagination logic.
Retain existing eager Markdown layout within the smaller window; do not weaken
native input, authorization, unique-session or unknown-send protections.

## Alternatives

Returning to lazy Markdown rows alone risks the previously fixed stale-height
and overlapping-message bug. Dropping cached history would lose offline access.
Only reducing network page size would leave large existing caches unbounded.
The projection window addresses initial layout/snapshot work without those
changes; it is not a hard byte cap for a single unusually large message.

## Consequences

Opening/reopening a long conversation lays out a bounded recent page, with older
cached history accessible offline. Explicitly loading all history or staying in
a long-running conversation can still create a large view. Phone telemetry is
sampled and cannot substitute for physical-device before/after acceptance.

## Verification

Test large cached histories, offline local pagination, remote pagination,
streaming/upserts, history reset, active cards, navigation and trace projection.
Compare encoded snapshot sizes on synthetic identical transcripts. Run app-core
regressions, architecture/public-content/ADR gates and Apple builds. Validate
native sending/answer continuation only in the authorized dedicated test session
on Mac-C, correlate operation records and provider receipts, and do not replay
unknown outcomes. Record source, build, paired publication, installed-device and
physical-phone acceptance separately. GitHub Actions remain disabled.

### Source validation and release preparation

Agent tests passed 225 (four pre-existing ignored), app-core passed 87 (one
pre-existing ignored), and platform passed 34. Strict all-target Clippy and
the 17-crate architecture gate passed. The async completion regression failed
against the old rule and passed after the change. Prepare paired Apple 1.0.85
(76); build, signing, publication, installation and physical-device acceptance
are not implied by these source checks.

Synthetic 500-message projection measurement: `history snapshot: recent=446109 bytes full=2781491 bytes`.
This compares encoded ViewModels on the build host, not phone frame time.
