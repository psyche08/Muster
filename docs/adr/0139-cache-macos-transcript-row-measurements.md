# ADR 0139: Reuse unchanged macOS transcript row measurements

- Date: 2026-11-05
- Status: Accepted; source regression verified, installed acceptance pending
- Related: Architecture §3.7; ADR 0037; ADR 0042; ADR 0047

## Context

The owner reports stuttering while scrolling the desktop transcript. Sampling
installed 1.1.22 shows the main thread spending substantial time in
`MacConversationItem.measuredHeight`, AppKit subtree layout and SwiftUI sizing,
both during viewport reconciliation and AppKit layout. A later idle sample is
mostly waiting for events; this is not evidence of a permanent deadlock.
Long Markdown tables amplify synchronous measurement work. Every preferred-size
request currently lays out the hosted subtree even when its content and width
have not changed.

## Decision

Cache only the current recycled item's measured height at its measured width.
Reuse it for repeated sizing requests. Invalidate it on content replacement,
reuse, attachment changes and hosted intrinsic-size invalidation. Width changes
must measure again. Keep asynchronous height checks coalesced and preserve
viewport anchoring, latest following and disclosure updates.

This is AppKit presentation geometry, not shared product or authorization state.
Rust remains the owner of transcript projection, identities and history paging.
No transcript text, identities or measurements are persisted by this cache.

## Alternatives

Throttling scroll events would delay user input. Removing anchor reconciliation
would reintroduce reading-position jumps. Keeping every row's hosting view
would defeat recycling. Caching without intrinsic-size invalidation would clip
streaming content, disclosure expansion or resized text.

## Consequences

Repeated fitting requests can avoid synchronous SwiftUI subtree layout. New or
changed content still needs measurement; a single large Markdown block can
remain expensive to construct. The cache is bounded to each recycled item and
must never survive reuse as if it belonged to the next message.

## Verification

First reproduce redundant subtree layout in a mounted row fixture. Verify reuse
of stable measurements and fresh height after width/content/intrinsic changes.
Run the existing AppKit conversation regressions for anchors during gestures,
prepends, disclosures, wrapping, latest following, unsent input and teardown.
Add a synthetic long-table scroll case. Run public-content and ADR checks.

Source tests and local builds are separate from production signing,
notarization, publication, installation and physical scrolling acceptance.
### Source evidence

The original mounted-item regression failed: 30 unchanged fitting requests
caused 30 additional subtree layout calls (counter 3 to 33). The fixed fixture
performs none for those requests and still measures width changes, observed
streaming text, replacement content and retained interaction height changes.
It also checks that an old recycled item cannot clear the intrinsic-size
callback of an input host that has moved to a new item.

A temporary-counter comparison on the existing variable-height upward-scroll
fixture recorded 13 fitting requests in both runs: 13 subtree measurements with
the cache bypassed, versus 2 with the cache enabled. The long-table fixture
recorded 1 versus 0 measurements during its scroll sequence. These are counts of
avoided synchronous work, not a claim about frame rate or end-to-end latency.
The counters were removed before the final source tests.

The new long-table fixture uses separate began, changed and ended events, with
a run-loop turn before ending the gesture. Its initial began/ended-only sequence
was not a valid scrolling fixture: AppKit applied no movement on either version.
The corrected sequence passes with and without caching; it checks geometry,
while the mounted-item regression reproduces the redundant measurement defect.

All 16 macOS conversation tests pass, including existing scrolling anchors,
prepends, disclosure resizing, multiline wrapping, tabs, retained drafts and
teardown. `git diff --check` and the public-content gate pass.

A fresh local Rust FFI archive and the Swift test package compile successfully.
The linker reports that some FFI objects target a newer macOS than the package's
14.0 minimum; this is not verification of compatibility on older macOS.
Production build/signing, notarization, publication, installation and physical
scrolling acceptance of the fix are not performed. The observed installed
1.1.22 runtime is the pre-fix application. Hosted Actions stays disabled under
ADR 0016; absent hosted checks are not a passing CI result.
