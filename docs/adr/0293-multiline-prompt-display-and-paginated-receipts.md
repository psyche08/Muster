# ADR 0293: Display Codex hard breaks and follow each thread's rollout for send receipts

- Date: 2026-11-23
- Status: Accepted
- Related: [0061](0061-send-receipt-format-and-manual-resolution.md), [0063](0063-queue-receipts-use-provider-text-matching.md), [0158](0158-queued-message-actions-and-codex-paginated-runtime.md), [0292](0292-working-composer-queue-receipt.md)

## Context

Muster writes a multiline draft as plain text. Codex can serialize a rich editor's line break as a Markdown backslash followed by LF. Its user message then shows the extra slash in Muster and the stored text can miss the pending prompt. Claude stores plain line breaks. Separately, a paginated Codex thread's catalog path names a shared SQLite history projection. Opening a JSONL tail at that path cannot observe the user record appended to its individual rollout.

## Decision

Keep provider input as plain text. In Shared Rust, recognize Codex's hard-break representation in user history and in full-text receipt and queue comparisons. Preserve ambiguous code and literal backslashes in display. Keep raw provider queue items for actions and evidence. Text matching still requires the complete expected prompt and the original session's evidence baseline.

For a paginated Codex session, resolve its rollout from the provider's thread registry and verify its session metadata before positioning the tail at EOF. If it exposes no verifiable rollout, continue with the existing queue and composer receipts. A shared history file cannot identify a particular submission. The working-composer receipt from ADR 0292 continues to apply to both providers.

## Alternatives

Synthesizing a Return in a desktop editor can submit a partial prompt. Changing the system clipboard introduces a separate editing path. Stripping every trailing slash would alter code and ordinary file paths. A global SQLite file offset cannot distinguish peers' threads.

## Consequences

Muster can display the normal two-line message and accept the matching delayed receipt after the UI operation ends. Provider-owned history remains as the provider wrote it. Missing evidence leaves the operation pending.

## Verification

Use synthetic multiline Codex and Claude records, paginated state databases and separate rollout files to check display, queue accounting, original-operation and cross-thread boundaries. Signing, installation and real desktop sends are separate from source verification.

The Rust regression fixtures cover ordinary and encoded line breaks, mixed prose and code, literal backslashes, separate text blocks, provider-specific receipt matching and queue display. A temporary thread registry exercises a mismatched header, a preexisting identical record, a different writer and a partial append before the matching complete line. Device acceptance remains pending.
