# ADR 0019: Sample Submit focus immediately before activation

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](../PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

- Date: 2026-10-22
- Status: Accepted
- Related: ADR 0017 (refines focus acquisition order, retains activation policy)

## Context

Installed 1.0.81 delivered dedicated prompt MUSTER81-QUESTION-1022-C from
Mac-B to Mac-A and selected 收到. Submit was rejected by the focus gate before
any activation key. Raising the exact test window and repeating only that
known-not-submitted answer during an owner-confirmed quiet window reproduced
the same rejection. Foreground placement alone does not resolve the failure.
The existing error collapses enabled, control focus and application focus into
one message, so these observations do not establish which focus fact failed.

The adapter focuses Submit before rebuilding and traversing the native tree,
then validates focus after that work. This places avoidable AX discovery between
focus acquisition and its use. This source finding is not proof that traversal
caused the observed device failure.

## Decision

Re-resolve the exact form and unchanged enabled Submit control before focusing
it. Then acquire focus once, sample both control and application focus, apply
the unchanged Shared Rust admission rule and immediately check authorization
and input generation before the single application-directed Space pair.
Log the content-free enabled/focused/application-match values and focused role
on rejection. Do not add refocusing loops, activation fallback, global clicks,
or retries after delivery. Read-only question diagnostics expose these focus
facts separately from selected answers.

## Alternatives

Ignoring application focus, treating AX write success as focus proof, or sending
another activation would weaken existing protection. Foreground-only recovery
was tested and did not resolve the rejection. Indefinite focus retries would
compete with the owner's desktop input and obscure the real failure.

## Consequences

Focus acquisition follows expensive discovery, reducing the pre-dispatch gap.
Unproved focus still fails closed and reports useful facts. Device validation
must determine whether this ordering change resolves this particular failure;
source tests alone cannot establish that. No new permission is granted.

## Verification

Run the exhaustive Shared Rust focus-admission regressions, platform question
regressions, strict Clippy and architecture/ADR gates. Build a new paired Apple
release from a clean commit; keep Actions disabled. Distinguish signed artifact,
installation and actual provider receipt/continued reply. Do not replay the
original Schedule answer or send new messages to business sessions.

### Completed release housekeeping

The owner approved and we deleted exactly the six old OTA files for builds
1.0.79 and 1.0.80 (IPA/install/manifest, hashes f1e6fed3…bd8e4b2 and
5492c285…13a45f9b). Public verification retained 1.0.81 build 72 with IPA SHA256
f16862bd5666ba6c7c20d730622f1721b8604c3e522b1e0f64f1073646594f59.
Both old IPA URLs return 410; old manifest/install URLs resolve to build 72.

### Source validation and release plan

221 agent tests (4 pre-existing ignored), 33 platform tests and 51 daemon tests
passed on Mac-A. Strict all-target Clippy for those three crates and the
17-crate layer gate passed. The existing exhaustive admission test continues to
reject every missing/false enabled, focused or application-match combination.
Prepare paired macOS/iOS 1.0.82 (73) from the next clean source commit; no device
success is inferred from these local results.
