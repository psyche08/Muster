# ADR 0030: Recycle iOS conversation rows while retaining Rust pagination

- Date: 2026-10-23
- Status: Accepted
- Related: ARCHITECTURE.md §3.7; partially supersedes ADR 0029's iOS eager layout decision

## Context

ADR 0029 bounds the initial projection to 80 messages but explicitly loading
history or a long running conversation still grows the eager native view tree.
The user requested a UICollectionView implementation after comparing containers.
Markdown changes height during streaming; a previous lazy layout overlapped rows.
Question cards contain unsent local input that must not disappear with cell reuse.

## Decision

Retain Shared Rust's complete history, presentation window, stable message IDs,
load-earlier intents and interaction semantics unchanged. On iOS only, render the
projected timeline in self-sizing reusable UICollectionView cells hosting the
existing SwiftUI blocks. Diff stable row IDs and reconfigure changed blocks;
forward SwiftUI rendering environment and actions across the UIKit boundary.
Keep one separately retained hosting controller for the active interaction area,
so scrolling it outside the viewport does not discard unsent question selections.
It is released with the conversation controller, not retained across sessions.

Native presentation owns only viewport state: follow the bottom until the user
scrolls away; otherwise preserve a visible row ID and its viewport offset across
history insertion, width/keyboard changes and self-sizing layout. A latest button
explicitly resumes following. Cancel pending anchoring when the user drags.
The macOS transcript and Rust business/authorization/send state are unchanged.

## Alternatives

Keeping eager layout only postpones the cost. LazyVStack has a smaller migration
but requires revisiting the earlier height bug and gives less explicit reuse and
layout control. List limits custom chat geometry. Rewriting Markdown at the same
time would combine separate risks. Retaining a host for every message would
negate recycling; only the interaction area receives a persistent host.

## Consequences

Visible cells and UIKit's reuse pool bound live transcript views, not cached
message bytes. One enormous Markdown block can still be costly. Native layout
requires tests for dynamic heights, environment propagation, viewport stability,
interactive input retention and controller release. Presentation-only disclosure
state may reset when a message scrolls out of the reuse pool; no provider state
or pending answer may reset. Physical iPhone performance remains a separate gate.

## Verification

Run simulator integration tests against real collection views with long synthetic
histories, prepend, same-ID growth, height changes, width/viewport changes, latest
navigation, cell reuse, interaction input retention and teardown. Run existing
conversation/keyboard/navigation tests, Rust paging regressions, public-content,
architecture and ADR gates. Compile both Apple platforms, then record signing,
notarization, paired OTA publication and installed runtime checks separately.
Keep GitHub Actions disabled and do not start Cloud builds. Physical-device
navigation/scrolling acceptance must not be inferred from simulator or builds.

### Source and simulator evidence

The iOS 27 simulator passed 16 tests covering the new real collection view plus
existing Markdown, keyboard and native navigation suites. A 500-row fixture
kept fewer than 25 visible cells; an unchanged projection did not invoke row
content factories again. Prepending history, updates below a reading anchor,
same-ID Markdown growth, viewport resize, jump-to-latest, retained answer input
and controller deallocation passed. The newly added header-visible prepend test
failed before switching the anchor from the history button to a message, then
passed. The first complete run had a keyboard-window attachment assertion fail;
the isolated rerun and final complete run passed without changing keyboard code.
Synthetic light-mode and live-keyboard screenshots were inspected; these are not
physical-device latency measurements.

The app-core unit and integration suites passed 110 tests (one existing ignored).
Two older watch tests still assumed unpaged projections; their first run failed.
They now verify the recent window, cached full-history reveal, stream updates
and unchanged delta-fetch contract. No Rust runtime paging behavior was changed.
Prepare paired Apple 1.0.86 (77). Builds, signing, publication, installation and
physical iPhone acceptance are recorded separately after completion.
