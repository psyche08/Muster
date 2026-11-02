# ADR 0098: Delete a proven question-editor selection through the application

- Date: 2026-11-02
- Status: Accepted; source verified, device acceptance pending
- Related: ADR 0020, ADR 0055, ADR 0096
- Partially supersedes: ADR 0096's repeated attribute-based clearing for Codex question fields

## Context

Three further phone replies failed after ADR 0096. The installed descendant
has identical question-editor code to 1983f627. Every fresh read reports seven
value characters and seven document characters, not a matching placeholder.
Selection replacement and empty AXValue writes return without error, but do
not change the observed field; renderer refresh is accepted and does not help.
One operation was cancelled on phone backgrounding, two exhausted preparation's
two-second budget. None reached final Submit; all drained and relocked.

These observations reject stale-proxy/placeholder handling as a sufficient
repair. They do not establish the provider's internal reason for ignoring the
attribute writes, and do not justify ignoring the remaining draft.

## Decision

For Codex question editors only, Shared Rust chooses one app-directed Backspace
after proof of the whole current value being selected in the exact enabled,
focused question field. Require AXValue to be settable, unchanged value, UTF-16 range `(0, value length)`
and the existing ADR 0020 application/window focus proof. The platform rechecks
the original live authorization, input generation and preparation deadline just
before dispatch. A failed, missing or contradictory selection/focus observation
refuses the key. No global keys, pointer/clipboard route, select-all shortcut,
Return or final-submit retry is introduced.

After the one delete, perform read-only refresh/observation until the field is
confirmed empty, an exact answer is already present, or the original two-second
budget expires. Never dispatch another deletion or fall back to attribute
mutation after this attempt. Other providers retain their existing clearing
policy. Preserve exact-question rebinding and exact-answer retention from 0096;
the final answer action and its nonblocking acknowledgement remain unchanged.

## Alternatives

Increasing the timeout repeats two mechanisms observed to have no effect.
Treating seven characters as empty risks merging answers. Unproven keyboard
focus can target another control. Retrying a key after an uncertain result
can delete new content. Direct UI inspection remains unavailable through the
computer-use tool; verification uses Shared Rust fixtures and user-driven phone
acceptance, not another tool to bypass that restriction.

## Consequences

This adds a strictly scoped editing event to the existing AX transport, using
the same application-directed API as guarded Submit Space. It authorizes no
new provider/session or unlock lifetime. Unsupported focus or range reporting
fails explicitly. The next artifact must preserve any newer installed source;
an unrelated installed change must not be silently reverted for this repair.
When the installed descendant is not yet on main, keep the repair PR isolated
and build a local integration of the already installed committed source plus
the repair. Preserve that integration commit for artifact provenance; do not
publish or land the unrelated branch as part of the repair.

## Verification

Test exactly one deletion, no replay after refresh or persistent text, deadlines,
empty/exact-answer admission and unchanged other-provider policy. Test complete
UTF-16 selection, missing/partial ranges, changed value, conflicting focus and
the supported absent-application-focus window proof. Run affected Rust suites,
strict Clippy, format, layer, public-content and ADR checks locally. Build,
package, sign, notarize and install with the guarded update lease only after
source ancestry is compatible with the installed build. Actual phone answer
delivery and cleanup are separate evidence; no hosted CI or public release.

Local verification passed 300 agent, 38 platform and 97 daemon library tests
(six pre-existing ignored). All seven editor-preparation/selection-proof tests
passed after the final read-only-field rejection case. Strict Clippy for the
three affected crates, targeted formatting, layer and public-content checks
passed. This is source verification; the installed question path remains failed
on the prior build until the repaired artifact completes attended acceptance.
