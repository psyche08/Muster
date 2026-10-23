# ADR 0037: Recycle desktop transcripts and load history near the top

- Date: 2026-10-23
- Status: Accepted
- Related: Architecture §3.7; extends ADR 0030, ADR 0031 and ADR 0034; ADR 0032

## Context

The desktop still eagerly renders its loaded transcript. The user requested the
same recycling, metadata and changed-file presentation as iOS, automatic history
loading near the top, and larger pages. UIKit is unavailable in the native Mac
shell; AppKit provides NSCollectionView.

## Decision

Use NSCollectionView on macOS and retain UICollectionView on iOS, hosting the same
SwiftUI blocks and shared disclosure choices. Keep the opened conversation pane
mounted across tabs on both platforms. Reuse transcript hosts; retain only the
current interaction host to preserve unsent input. Native code owns viewport and
presentation state only. Stable group identities and paging stay in Shared Rust.

Increase the shared presentation page from 80 to 200 messages, both initially and
for each reveal. Earlier remote requests also use the shared 200-message page;
protocol limits and cache validation remain unchanged.
Both Apple lists request one earlier page when user scrolling approaches 160
points from the top. Layout, tab restoration and incoming updates never trigger
paging. One gesture emits at most one intent, and loading/error/no-history states
suppress it. The explicit button remains available for retry and accessibility.
Preserve the visible message anchor across prepends, resizing and streaming.

ADR 0032 already removes Cache TTL and defaults changed-file summaries to closed
in shared Apple views; use those views unchanged on desktop, with choices retained
across recycling. No cache protocol or authorization semantics change.

## Alternatives

Mac Catalyst would replace the desktop application architecture merely to reuse
UIKit. An eager stack retains the scaling problem; retaining every hosting view
also defeats recycling. Unbounded automatic fetching or loading all history on
open would defeat Rust pagination and amplify network work after failures.

## Consequences

A larger page projects more data, while native live views remain bounded by reuse.
A single large grouped Markdown row can remain expensive. Anchors are row-based,
not character positions inside growing Markdown. A new user gesture or explicit
button is needed after each automatic page or failed request.

## Verification

Test real AppKit collection reuse, prepend anchors, same-ID height changes,
resize, latest navigation, disclosure/input retention and teardown. Test the
shared gesture gate and UIKit automatic loading. Run Rust paging regressions,
Apple builds, public-content and ADR gates. Record source tests, build, signing,
publication, installation and physical acceptance separately. Keep Actions off.

### Source and build evidence

Rust app-core passed 115 tests, with one pre-existing manual diagnostic ignored.
The iOS Simulator suite passed 27 tests, including automatic pagination and the
existing disclosure, tab, unsent-answer and viewport regressions. All 68 macOS
tests passed. The desktop fixtures use
real AppKit windows and recycled cells with synthetic data, covering 500-row
reuse, prepends, stream updates, resizing, disclosure resizing and preservation,
retained unsent answers across tabs/recycling, scroll notifications and teardown.
The disclosure test exposed missing invalidation after a hosted SwiftUI row
changed intrinsic height; the Mac host now schedules a coalesced height check
and invalidates layout only when the measured height actually changes.

These are source/UI fixture checks. Swift builds linked existing local FFI test
archives; the modified Rust code was tested separately. Existing Mac archive
objects warn about targeting a newer OS than the Swift package minimum, so these
builds do not establish compatibility with older macOS. Production signing,
notarization, paired publication, installation and real-session acceptance of
this change remain pending. Earlier physical acceptance of another build is not
evidence for this change. GitHub Actions remains disabled.


### Paired release follow-up

This change shipped in 1.0.88 (79), built with current Rust FFI archives for
both Apple platforms. The notarized macOS release is installed on three Macs;
iOS OTA was independently downloaded and verified. See
[ADR 0039](0039-exact-session-navigation.md#published-and-installed-1088-79)
for source provenance, artifact hashes and runtime checks. Publication does not
extend the earlier physical-device acceptance or establish new provider delivery.
