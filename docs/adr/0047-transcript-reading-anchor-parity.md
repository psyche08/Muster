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

### Release 1.0.93 (84)

This decision and ADR 0048 are released together as paired macOS and iOS build
1.0.93 (84), so the owner can accept the reading anchor and the acknowledged
send on real hardware. 1.0.92 (83) was built but never published or accepted,
so `released-commits.txt` continues from 1.0.91 (82).

Clean source `c91a4704088b11dd5a3bf6b1dbbf3aebb5220d71`. The release daemon
carries exactly that one build stamp, and `verify-release.sh` accepted the
bundle: presence signing `89LGY6BD53.dev.linsheng.muster`, release ancestry over
`released-commits.txt`, and every string in `release-fingerprints.txt`.

macOS was signed with the Developer ID identity and the existing
"Muster Developer ID Presence" profile, the same profile the installed 1.0.92
carries, so the device identity and hardware key backend are not disturbed. The
first build left the outer bundle without a secure timestamp while its nested
helpers had one; it was rebuilt with `--timestamp` before submission, which is
why notarization was attempted only once. Apple notarization
`116f7550-cf6c-4120-b044-96f95d6fb1f4` was Accepted. Stapling and
`stapler validate` passed, `spctl -a -t install` reported
"accepted, source=Notarized Developer ID", and strict deep `codesign --verify`
passed after stapling. The notarized ZIP's SHA-256 is
`79e3cbff1460d3ebd8a57af0fe2f23df2bb4dbf5d03dd937ad36bb2aea59799c`.

The iOS Release archive is 1.0.93 (84), development-signed for the registered
devices, and passed strict verification. `package-ota.py` prepared the OTA
directory for three registered devices with a profile that expires 2027-09-30:
the hash-named and fixed IPA, the hash-named and fixed manifests, the install
plist, the page and `SHA256SUMS`. Both checksums verified, each fixed alias is
byte-identical to its hash-named file, and the manifest binds build 84 to the
hash-named IPA. The IPA's SHA-256 is
`5c89611e5073590af62bdfe9d1ed9f758a59143d31f89e99c325da4cd40e19fa`.

Not done, and not claimed. No Velopack package or desktop feed was produced and
nothing was published. The release destination (`MUSTER_RELEASE_HOST`,
`MUSTER_RELEASE_SSH_PORT`, `MUSTER_RELEASE_ROOT`) is not available in this
environment, and on being told so the owner chose to stop at the local
artifacts: no iOS OTA publication this time, and no installation on the local
Mac, which deliberately stays on 1.0.92 (83). The prepared OTA directory was
therefore never uploaded and no public download was verified, so
`released-commits.txt` still ends at 1.0.91 (82). The physical iPhone was not at
hand either. There is consequently no installed build of this branch anywhere,
and no physical acceptance of the reading anchor or of the acknowledged send;
both remain open on exactly the evidence recorded above.

The artifacts are retained for a later publication, outside the repository, at
`src/native/apple/MusterApp/build/releases/`: `mac-1.0.93-84/` holds the
notarized and stapled ZIP, `ota-1.0.93-84/` the seven checked OTA files, and
`iOS-1.0.93-84/Muster.xcarchive` the archive and its dSYMs.
