# ADR 0034: Preserve transcript continuity across paging and tabs

- Date: 2026-10-23
- Status: Accepted
- Related: ARCHITECTURE.md §3.7; extends ADR 0030 and supersedes ADR 0031's controller-only lifetime

## Context

Physical iPhone testing of 1.0.87 (78) confirmed normal cell reuse retains
expanded and collapsed choices. It also exposed two remaining boundaries:
loading history inside a Working group changes its ID, resetting that group's
choice and reading anchor; switching to Activity removes the transcript
controller, so returning to Chat resets its viewport and choices.

## Decision

Shared Rust computes grouped timeline identities from cached history before the
visible window. It emits only the requested message window; hidden history is
used for identity and turn context, not cloned into the native projection. A
partial Working or activity group has the same ID when earlier messages are
revealed. Provider boundaries and grouping remain Rust-owned.

The iOS opened Chat view keeps one conversation pane mounted across its content
tabs and releases it when the conversation closes. Inactive content is invisible,
excluded from hit testing and accessibility, and resigns keyboard focus. UIKit keeps
viewport, disclosure choices and the existing unsent interaction host. No disk
cache or cross-conversation state is added. The native shell does not reconstruct
groups, authorization or provider state.

## Alternatives

Rendering all history would defeat bounded pagination. Matching native groups by
text or overlapping message IDs would duplicate Rust semantics. Persisting choices
on disk is unnecessary for a tab switch; retaining every row defeats recycling.

## Consequences

The first partial turn uses earlier cached context for stable identities, while
native rows and message payload remain bounded by the Rust window. Tabs retain
one collection controller and its reuse pool, not every transcript row. Closing
the conversation still releases transient presentation state. An expanded group's
newly revealed content can change its height; row anchoring is not character-level
scroll anchoring inside Markdown.

## Verification

Add Rust regression coverage for a page boundary inside Working and tool groups,
subsequent prepends, completed/live turns and no hidden message payload. Exercise
real UIKit/SwiftUI tab switches, viewport and disclosure retention,
interaction draft retention, session isolation and teardown. Run existing paging
and collection regressions and source gates. Record build, signing, installation,
publication and physical-device acceptance independently.

Baseline physical device: iPhone 17 Pro, iOS 27.0.1, installed Muster 1.0.87 (78)
read back through CoreDevice. Browsing used an existing long conversation without
sending or changing its draft. Normal offscreen disclosure reuse passed; the two
boundaries above failed. Twenty-one foreground telemetry samples over about
three minutes had peak sampled main-queue lag 8.22 ms, CPU 2.11–5.50%, and maximum
footprint about 65 MiB. These are periodic samples, not frame-rate or input-latency
measurements. Owner feedback to the earlier 1.0.86 acceptance prompt also reported
smooth scrolling and correct position; its exact installed version at that time
was not independently read back.

### Source verification

The complete app-core suite passed 111 tests with one existing ignored test.
The window regression checks every start offset across live/completed and native/
CLI presentations, including provider-card and final-answer boundaries, and
ensures hidden payloads never enter the projection. All seven collection tests
passed, including repeated content-tab switches with retained viewport,
disclosure, unsent answer and teardown. macOS shared UI compilation and its
footer regression passed. The complete iOS UI suite is recorded separately.

An initial attempt to reattach the retained controller failed the new unsent
answer and teardown assertions. Keeping the conversation pane mounted across
tabs passed both; that intermediate implementation was discarded. Public-content
and 17-crate architecture checks passed. No new release or corrected physical
installation is claimed by these source results.
