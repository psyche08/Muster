# ADR 0191 – Match native composer records and include the assigned issue

- Date: 2026-11-12
- Status: Accepted
- Related: [0190](0190-background-desktop-admission.md), [0187](0187-trusted-multica-dispatch.md)

## Context

Two service runs created distinct native sessions in the requested directories.
Each recorded first prompt had escaped Markdown punctuation and hard line breaks.
Reconstructing the original complete request reproduced each stored ledger hash.
The native agent also reported that its task-scoped CLI was unavailable in its
own sandbox. The assignment text included an issue reference without the body.

## Decision

Compare a Codex desktop receipt with the complete expected plain-composer input
or its observed deterministic serialization. Keep directory, session and
once-only submission checks. Use the same comparison for streaming the result.
For managed native assignments the ACP adapter can include the title and
description fetched with that run's existing task token from the configured
HTTPS service. Check the issue, workspace, assignee and live run first.
The locked-use issuer independently verifies its existing authorization contract.

## Alternatives

Retrying an unknown submit can duplicate a task. Treating any new transcript in
the directory as its receipt loses the message binding. Copying credentials into
the provider conversation would expose the task token.

## Consequences

The agent can read its assigned task from the first turn. Service operations
beyond that snapshot still depend on the available task tools. Older contexts
without a matching task credential use their original assignment instructions.

## Verification

Use synthetic tests for escaped punctuation, changed text, ambiguous issue
references and incorrect response bindings. Separate native receipt and
service result evidence from the foreground-input measurements.

On the installed candidate both final real service runs had status completed.
Each run had exactly one root Codex session, one user turn with the scoped
issue snapshot, and the same exact arithmetic answer in its final assistant
message and the service result. Earlier diagnostic runs ended in failure and
remain recorded separately. No terminal outcome was edited in the service.
