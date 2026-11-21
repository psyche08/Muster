# ADR 0265. Show the provider message queue beside the Windows composer

- Date: 2026-11-21
- Status: Accepted
- Related: [ADR 0158](0158-queued-message-actions-and-codex-paginated-runtime.md), architecture §3.7

## Context

Shared Rust separates queued messages from questions and approvals. The Mac
renders both projections. The Windows conversation rendered only interactions,
so a user could neither see a pending message nor choose an offered queue action.

## Decision

Show a bounded queue above the conversation composer. Present messages in the
projected order, show the complete selected prompt, and render its offered
options. Pass the selected interaction ID and exact option through the normal
draft-bearing response intent. Resolve selection within the current device
and session. Disable actions during submission or an unknown outcome. Display
a queue error even if the last message has left. Rust owns all queue changes
and delivery outcomes.

## Alternatives

Adding queue items to the transcript would mix pending messages with history.
Reconstructing a queue from prompt text would make two identical prompts
indistinguishable and would duplicate provider semantics on Windows.

## Consequences

A user can inspect a pending message, send it now or delete it when offered.
A read-only item remains visible. A new projection determines whether a row
disappears or changes after an action.

## Verification

Use real Windows controls with synthetic projections to exercise equal prompts,
ordered IDs, per-item options, scope changes, submission and error states.
Read a synthetic provider queue through the installed GUI and an isolated
daemon. Check package update, running processes and provider-side delivery
separately.
