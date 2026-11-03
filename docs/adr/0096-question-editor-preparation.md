# ADR 0096: Rebind and refresh question editors during preparation

- Date: 2026-11-02
- Status: Accepted; source verified, installed question acceptance pending
- Related: ADR 0055, ADR 0056, ADR 0095; AGENT_INTERFACE.md user-confirmed answer delivery
- Follow-up: [ADR 0098](0098-focused-question-field-deletion.md) replaces Codex field clearing after repeated installed failures; fresh binding and exact-answer retention remain.

- Follow-up: [ADR 0101](0101-question-placeholder-projection.md) adds corroborated single-question placeholder recognition.

## Context

After the normal Codex phone-send baseline passed, two phone question replies
failed on the installed 700870b6 build at 21:34 Asia/Shanghai. Authorization and
visible-session preparation succeeded. Each attempted five bounded clears and
rejected after about two seconds because the answer field still read as a draft.
Neither reached final Submit; both drained, relocked and released the cover.

The question path retains one editor proxy throughout clearing, unlike the
fresh-form lookup already used between question actions. It does not refresh
the renderer after a clear. It also treats an exact existing answer as ready
but unconditionally writes it again. Existing diagnostics do not distinguish
stale values, actual drafts or placeholder projections; they do not establish
which caused the observed failure. Direct live App inspection is unavailable
through the computer-use tool, so do not claim a captured field fixture.

## Decision

Extend Shared Rust's bounded editor preparation to decide clear, read-only
refresh, wait, write-once or retain-exact-answer. Resolve the exact pending
question and its unique enabled editor again on each observation, retaining the
original authorization, session, input generation and two-second budget. Never
reuse the old editor as authority after a clear or asynchronous wait.

Only a readable empty value, or a value exactly matching that same editor's
placeholder with an explicit zero character count, establishes emptiness.
Missing values and arbitrary placeholder-like text do not. A literal placeholder
with nonzero/unknown character count remains a draft. An exact existing answer
is retained without another write. Expired observations authorize no mutation.
After a clear, request a bounded read-only renderer refresh before a subsequent
clear; do not replay option presses or final Submit. Log content-free field
length, character count, placeholder equality, rebind and refresh facts.

Preserve ADR 0055's replacement policy and existing nonblocking question-answer
dispatch: no new selected-value or provider-receipt wait. No RPC/ABI, CLI,
permission, unlock duration or Keychain changes. Normal sends remain unchanged.

## Alternatives

Longer timeouts repeat the same observation and do not address editor identity.
Ignoring any nonempty value or matching placeholder labels alone can overwrite
an unverified draft. Rewriting an already exact answer can append duplicate text.
Changing answer acknowledgement semantics would exceed this repair.

## Consequences

Question preparation uses fresh bound controls and stops on unreadable or
ambiguous facts. More precise diagnostics support further field-specific repair
if this is insufficient. The observed rejection is not provider delivery and
does not invalidate the earlier normal-message baseline. Actual question input
and final dispatch on the installed repair require separate phone acceptance.

## Verification

Test Shared Rust preparation for stale-to-empty observations, exact-answer
retention, placeholder/character-count disagreements, unreadable values and late
results. Run affected crate tests, strict Clippy, formatting, layer, public-content
and ADR checks locally. Build, package with pinned Velopack, sign and notarize;
install under the existing update lease. Keep hosted CI and public release off.
Record source, installed runtime and attended question acceptance separately.

Local source verification: affected library suites passed 298 agent, 38 platform
and 97 daemon tests (six pre-existing ignored). All five preparation regressions
passed again after the final placeholder ambiguity cases; the platform suite
also passed after removing the redundant post-toggle lookup. Strict Clippy,
targeted formatting, layer and public-content checks passed. These are synthetic
observations, not a replay of a captured native field. Build/notarization and
installation evidence will be attached to the resulting source commit; question
submission on the installed repair remains pending until the phone test runs.
