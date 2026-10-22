# ADR 0017: Activate Claude question Submit through its verified keyboard focus

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](../PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

- Date: 2026-10-22
- Status: Accepted
- Focus-proof requirement superseded by ADR 0020; activation policy retained.
- Related: ADR 0007, ADR 0010; AGENT_INTERFACE.md user-confirmed answer delivery

## Context

Mac-B 1.0.79 reported two answer actions applied for Mac-A's Schedule question,
but its provider transcript had no answer and the form remained open. The user
confirmed selecting Cloud schedule. We did not replay that business answer.

A dedicated Mac-B-to-Mac-A test reproduced the same failure: AXPress returned
success and focused the enabled Submit button, but neither the form nor the
provider transcript advanced. After separate user authorization to retry that
specific test card, one physical mouse click submitted it and produced the
answer receipt. In a new independent test, focusing Submit through keyboard
navigation and pressing Space once also submitted it and resumed Claude.
These observations establish the failing activation path; they do not explain
Claude's internal handling of AXPress or all historical send failures.

Mac-C's installed Codex `approvalRequestCard` implementation also names its
primary Computer Use approval `Allow this conversation` and its persistent
option `Always allow`. Muster's exact selector omitted both spellings, while
recognizing `Deny` and `Allow once`. This is a confirmed mapping omission, not
proof that it caused the original screenshot: disabled approval controls are
also intentionally withheld.

## Decision

Declare Claude's question-submit activation in its Shared Rust profile. For a
separate Submit button, focus the exact matched control, revalidate the current
session, question, input generation, enabled state, control identity and actual
application focus, then deliver one application-directed Space down/up pair.
Never try AXPress first or fall back after a potentially delivered key event.
Other providers, immediate option cards, Back and Next retain their existing
activation paths. Preserve the nonblocking answer-dispatch contract: an applied
operation is not a provider receipt and does not fabricate InteractionResolved.
Add content-free diagnostics identifying the submit method and dispatch stage.

Extend only Codex's exact approval vocabulary with those two observed names;
keep their scope visible verbatim and require enabled AX buttons as before.
Never fabricate an approval option or execute it automatically. Log enabled and
disabled known-option counts on option-set changes without exposing card text.

## Alternatives

A second AXPress can duplicate an operation and reproduced no reliable delivery.
Global pointer clicks worked in the diagnostic test but are inappropriate under
Locked Use cover and can target another app. Global shortcuts risk submitting a
composer. Polling for receipts alone would detect the symptom without fixing
activation and would reverse the user's prior nonblocking-answer preference.

## Consequences

Claude submission now requires provable focus on the same live Submit control.
Unverifiable focus fails closed. A key error after dispatch remains an unknown
outcome under the existing question-write accounting; it is never retried.
The source strategy still needs installed-device and Locked Use acceptance.

## Verification

Run profile and guarded-submit regressions, platform question-form tests,
macOS build/Clippy, layer and ADR gates. Hosted CI is disabled by the owner's
request (ADR 0016). Record source/build versus installed-device results below.
The successful mouse and keyboard tests above ran through external UI controls
on installed 1.0.79, not the new daemon implementation.

The Mac-C permission-card test was delivered to the owner's specified idle
session, but the environment auto-approved `/usr/bin/true`; it did not create
an approval card. The mapping regression can be verified from the installed bundle and source
fixtures; live permission-card acceptance remains unverified.

### Local verification

- Mac-A: `cargo test -p muster-agent -p muster-platform -p musterd --lib`
  passed 220 agent tests (4 pre-existing ignored), 33 platform tests, and
  51 daemon tests. Platform tests passed again after the final session guard.
- Strict Clippy for all targets of these three crates passed. The 17-crate
  architecture layer check passed. ADR gate runs against the committed diff.
- Device reproduction and external UI comparison are recorded above. No new
  signed artifact has been published or installed, and no Locked Use test of
  the new dispatch implementation has been performed.

- macOS arm64 debug `cargo build -p musterd` compiled and linked successfully;
  this is not a signed/notarized release or installation.

### Release and device acceptance plan: 1.0.80 (71)

Publish the merged repair as paired macOS Developer ID/notarized and iOS OTA
artifacts from a clean committed source. Keep GitHub Actions disabled under
ADR 0016. Install through the Rust update lease on Mac-A, Mac-B and Mac-C,
retaining device identities and refusing updates during protected console use.
Check each installed source stamp, signature, accessibility, relay and Vnet.

Use the existing dedicated Claude acceptance session for a new Mac-B-to-Mac-A
question and answer. Require the provider's answer record and subsequent reply,
not only applied. Never replay the original Schedule answer. On Mac-C, wait
for the owner's specified Codex session to be idle, then attempt a harmless
native permission-card test; do not change approval policy or manufacture a
security-sensitive command to defeat automatic approval. Compare the native
card's actual enabled options with Muster's exposed options. Record blockers,
source/build, signing, publication, installation and device results separately.

### Installed 1.0.80 acceptance (2026-10-22)

- Clean source `1022dfe6e717e02d1478405badfd92b67767512b`; both Apple Release
  builds passed. Apple notarization `17244002-e8bb-485e-9e74-9071e09b797b`
  returned Accepted; stapler and Gatekeeper passed. macOS and iOS build 71
  were published and independently downloaded through the actual OTA page.
- All three Macs ran 1.0.80 with the same daemon SHA256
  `f27caee15738c867dd4b8088bb830506d01550528619c6a7b6d883b2a73e30fc`,
  unchanged identities, Accessibility granted, registered relay and Vnet up.
- Mac-B -> Mac-A marker `MUSTER80-QUESTION-1022-A`, interaction
  `question:toolu_01B67ztE1gXhJzVqs5ZbSBf5`: one answer operation, diagnostic
  `method=focused_space`, provider answer record for 收到, then final reply
  `MUSTER80-QUESTION-1022-A 收到`. After daemon restart, the test session
  needed to be opened in Claude before its controller attached. Initial
  preflight stopped before any send; user paused concurrent UI operations.
- Mac-C original session `f136c6e8-9362-553d-8a9d-db3d7fae6c46`: a real
  Calculator permission card exposed Always allow, Deny and Allow this
  conversation. After explicit owner approval, one Muster answer selected
  Allow this conversation; the tool returned the Calculator window and the
  agent replied with `MUSTER80-APPROVAL-1022-A`. No permanent grant was selected.
- This also exposed an unrelated review-pane Reject option. ADR 0018 records
  the follow-up scope repair. Physical iPhone installation and Locked Use
  acceptance of the new answer dispatch remain unverified.
