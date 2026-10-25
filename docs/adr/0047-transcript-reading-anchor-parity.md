# ADR 0047: One reading-anchor policy for both Apple transcripts

- Date: 2026-10-25
- Status: Accepted
- Related: extends ADR 0042; ADR 0041; ADR 0037; ADR 0030; Architecture §3.7

## Context

The owner reports that both shells still jump to a position that does not match
the reader after a load-more during an upward or downward scroll, on iOS and on
macOS. ADR 0042 was implemented in the macOS controller only; the iOS
controller kept a single anchor, suppressed every reconciliation while
`isDragging` or `isDecelerating` was true, and therefore could not be driven
through a mid-gesture history response in a test at all. Three further gaps are
common to both shells:

- The anchor was the topmost row whose bottom edge was still on screen. That
  row usually starts *above* the viewport, and its own measured height can
  change without moving its origin, so pinning it pushes down everything the
  reader can actually see. With `.estimated(100)` rows this happens on every
  upward scroll, as rows above the viewport are measured for the first time
  when they come into view.
- Any viewport move the controller did not observe — keyboard scrolling,
  Accessibility, scroll-to-visible, keyboard avoidance — was treated as
  geometry drifting under a still-valid anchor, so the next reconciliation
  pulled the transcript back to the older position.
- A scroll callback arriving between the reconciliation passes that follow a
  projection change adopted that intermediate, partly estimated geometry as the
  new reading position.

## Decision

Both Apple controllers implement one policy, with the same names and the same
comments where the policy is identical:

1. The reading anchors are the visible transcript rows, ordered, each with its
   screen offset. Rows that start at or below the viewport top come first,
   because those are the rows whose position the reader watches; a partially
   visible row is kept only as a fallback. The permanent header is never an
   anchor while any content row survives.
2. A projection change is a geometry transaction: it is reconciled
   synchronously, before any scroll callback can observe the new rows at the
   old absolute offset, and it keeps reconciling over bounded self-sizing
   passes. No binding is published from inside that update callback.
3. Reconciliation is not suppressed during a gesture. Anchors captured by the
   gesture's own callbacks make it a no-op for plain scrolling, which is what
   keeps a row measured on the way up from displacing the reader.
4. Each controller records the offset its anchors were taken at, or that it
   last reconciled to. An offset it never saw is the new reading position and
   is adopted, not corrected away.
5. Scroll callbacks do not replace the anchors while a projection change is
   still reconciling, except for a new gesture that has none of its own.

Shared Rust keeps paging, identities, page limits and the loading/error guards.
No layout callback requests another history page. Latest-follow, explicit jump,
disclosure and tab behavior are unchanged.

## Alternatives

Deriving the correction from the content-height difference fails with estimated
and changing row heights. Waiting for the gesture to end exposes the inserted
content first and was already rejected by ADR 0042. Keeping the iOS policy
separate from macOS is what produced this divergence. Routing AppKit's measured
-height invalidations through a separate controller callback was implemented and
discarded: once reconciliation is allowed during a gesture, the existing layout
callback already covers them. Invalidating the iOS layout, and reconfiguring the
visible rows that moved, were both implemented to clear the measured height a
moved row inherits from its new index path; neither changed that height, and
both were discarded rather than kept as unexplained cost.

## Consequences

The anchor is now the first row that starts on screen rather than the topmost
partially visible one. That is a deliberate change to the property ADR 0042's
`testRecyclingPrependGrowthResizeAndRelease` asserted, and that assertion was
updated with the reason; the ADR 0042 decision itself stands. Correcting the
offset during momentum can end a deceleration, which is accepted: the reading
position matters more than the remaining momentum. Row anchoring remains row
level; character-level anchoring inside one expanded Markdown block is still
the limitation ADR 0034 records.

A remaining UIKit limitation, found while verifying this and not introduced by
it: a row that moves to another index path inherits the height measured for
that index path, and UIKit does not measure it again while it sits above the
viewport top. The reader therefore keeps a correct position, but the off-screen
row above can hold a wrong height until it is scrolled fully into view. The
anchors and their offsets come from that same geometry, so the correction stays
self-consistent and converges when UIKit does measure the row. The iOS
disclosure fixtures read a row's rendered height, and one `scrollToItem` on
estimated rows lands short of the row, so they now bring the row fully inside
the viewport before measuring instead of relying on where a top-aligned scroll
happened to land. That is a fixture correction: it keeps each assertion's
subject, the retained disclosure choice, unchanged. A row scrolled far away now
keeps a hidden hosting cell instead of none, so that assertion accepts either;
both mean the row no longer renders the choice.

## Verification

Reproduce each case before the fix, assert the anchored row's screen offset
across the projection change and after the gesture ends, and confirm that later
genuine movement still moves the reader. Record source tests, release builds,
signing, installation and owner acceptance separately.

### Source evidence

The two new iOS regressions failed on the previous implementation. The replaced
boundary row case moved the surviving visible row from 204 to 20268 points. The
mid-gesture history case could not even emit a history request (0 instead of 1),
because the previous controller read `UIScrollView.isDragging` directly and no
test can set it; that is why the iOS regression went unnoticed. The new macOS
regression for rows measured during an upward scroll failed with the reader's
row displaced by 183, 444, 507, 531, 618, 705 and 792 points instead of the 120
points scrolled.

After the change the macOS conversation suite passed 14 tests, including the two
new ones, with the pre-existing anchor assertion updated as described above. The
complete local Swift suite passed 78 tests. The iOS conversation suite passed 12
tests, including the four new ones.

The complete iOS simulator suite ran 26 tests: 25 passed and
`KeyboardAndLiveChatTests.testBuiltInKeyboardWaitsForWindowAndCanSwitchWhileFocused`
failed on "The built-in keyboard must actually be on screen". That test
exercises `PasteTextEditor` and `KeyboardInputView`, which this change does not
touch, and it depends on the simulator's own keyboard state. An attempt to
record a before-the-change baseline for it was abandoned: stopping the competing
test runs took the simulator's launch service down (Mach error -308), so no
baseline is claimed and the failure is reported as unresolved rather than
attributed.

Builds, signing, notarization, publication, installation and physical scrolling
acceptance on a Mac and on an iPhone are not claimed by this source checkpoint.
The 1.0.92 boundary-fallback follow-up recorded in ADR 0042 also never completed
physical acceptance, and this change does not establish it.
