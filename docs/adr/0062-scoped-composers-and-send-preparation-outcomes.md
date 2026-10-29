# ADR 0062: Scope duplicate composers and record preparation failures

- Date: 2026-10-29
- Status: Accepted
- Related: ADR 0058, ADR 0046, ADR 0061; Architecture §3.6–3.7
- Partially supersedes: [0056](0056-bounded-scans-skip-transcripts-and-retry-vanished-nodes.md), only its single raw composer requirement; scan completeness and retry rules remain unchanged.

## Context

A retrospective 24-hour audit found four attempts refused with two matching
Codex prompt editors in one conversation view. The scanner scopes the view but
counts every matching editor below it, including auxiliary panes. Its existing
permission-card scope already distinguishes the main landmark from complementary,
navigation and tab-panel branches. The historical failure logs do not retain
those editors' ancestry; the precise duplicate layout cannot be asserted.

Two other attempts logged only the beginning of quiet-input admission. Its
error return precedes the preparation error logger, obscuring whether the
operation was refused or cancelled before reaching the send ledger.

## Decision

Add a Shared Rust composer-scope policy to the provider profile. For Codex,
select among multiple editors only when exactly one has main-conversation
ancestry and every other editor has explicit auxiliary-pane ancestry. Unknown
ancestry and multiple main editors remain ambiguous. A sole editor retains the
existing behavior unless it is explicitly in an auxiliary pane. Other providers
keep their unique-editor rule.

Submit controls must share an exclusive branch with the selected editor. A
branch containing another editor, or only reaching the selected editor through
a common ancestor of both, cannot supply its Send button. Incomplete walks,
input-generation checks, exact draft echo, session identity, and the final
pre-press validation remain unchanged. Platform code supplies batched AX ancestry
facts; all selection rules live in Shared Rust.

Log queue-admission and quiet-input preparation failures before returning them,
including the operation and stage but never prompt contents. Do not convert a
preparation failure into an unknown delivery or automatically retry a send.

## Alternatives

Choosing the first/focused/enabled editor could target another conversation.
Unconditionally requiring a main landmark would regress existing single-editor
layouts. Retrying a submitted prompt or extending receipt timeouts cannot fix
editor ambiguity. Removing quiet-input admission would weaken the input guard.

## Consequences

This resolves only structurally attributable duplicates, not every two-editor
layout. Cases with insufficient evidence still refuse safely. The historical
four failures need physical confirmation after deployment; live Codex UI access
was unavailable to this investigation. No production prompt is resent.

## Verification

Test main/auxiliary/unknown and malformed ancestry, foreign Send controls,
multiple main editors, the single-editor compatibility path, and unchanged
queue-row scoping. Record compilation, tests, signing, installation and physical
acceptance separately. Raw audit logs remain outside the public repository.

### Implementation evidence

- Source/tests: 247 agent tests and 37 macOS platform tests pass (284 total;
  four pre-existing ignored agent tests). Synthetic cases cover scoped selection,
  unknown/multiple-main refusal, foreign/common-ancestor submit rejection,
  invalid ancestry and single-editor compatibility. The main policy test uses
  the generated Codex provider profile.
- Compilation: `cargo check --workspace --tests` passes on macOS.
- Static checks: targeted clippy, formatting of changed Rust files, public-content
  scan and ADR check are required before landing; see the PR for final results.
- Signing/publication/installation: not performed in this source change.
- Runtime acceptance: existing logs were read only. No new provider prompt or
  live Accessibility verification was performed. Historical duplicate-editor
  ancestry and one later unknown delivery remain unresolved.
