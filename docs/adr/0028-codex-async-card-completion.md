# ADR 0028: Keep asynchronous Codex cards after turn completion

- Date: 2026-10-22
- Status: Accepted
- Related: ADR 0026; ARCHITECTURE.md §3.6.2

## Context

In a dedicated native acceptance conversation on Mac-C, the provider received a
single test request, called request_user_input_async, acknowledged it and ended
the turn. The native app still displayed the exact question, radio choices,
custom editor and Send action. Muster's event stream omitted the interaction.
The parser explicitly retired asynchronous questions on task_complete, based on
an older UI observation. This contradicts the current live provider surface.

## Decision

Do not retire an asynchronous question merely because its requesting turn
completed. Preserve its optional status, exact question IDs and per-question
answer tracking. Keep existing retirement on a subsequent turn/new non-answer
user message and resolution on actual provider answer receipts. Native control
must still locate the exact pending form and recheck input/session authorization;
a transcript entry alone does not authorize an absent native control.

## Alternatives

Treating the queued tool acknowledgement as an answer loses the question before
any user reply. Replaying the request or injecting answer text into the composer
would bypass native form proof. Removing all retirement rules exceeds the live
evidence and could resurrect obsolete questions.

## Consequences

A still-visible asynchronous card remains answerable after its turn ends.
Provider dismissal without a transcript event may leave a pending projection;
the native surface proof must reject an absent form without sending anything.
No business session or unknown send is replayed.

## Verification

A synthetic regression must fail against the old completion rule, retain the
card after queued acknowledgement plus task_complete, and resolve only after
its matching answer. Retain new-turn retirement and partial-answer regressions.
Run agent/app-core/platform tests, Clippy and ADR/public-content gates. Validate
the existing authorized card on the installed Mac-C build and require provider
answer receipt plus continuation; source tests alone do not establish acceptance.

### Source validation and release preparation

Agent tests passed 225 (four pre-existing ignored), app-core passed 87 (one
pre-existing ignored), and platform passed 34. Strict all-target Clippy and
the 17-crate architecture gate passed. The async completion regression failed
against the old rule and passed after the change. Prepare paired Apple 1.0.85
(76); build, signing, publication, installation and physical-device acceptance
are not implied by these source checks.
