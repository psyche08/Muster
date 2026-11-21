# ADR 0281: Verify the Windows Codex thread before sending

- Date: 2026-11-22
- Status: Proposed
- Related: ADR 0045, ADR 0095, ADR 0234

## Context

The Windows Chats controller resolves provider chat conversations. Code threads in the Agents catalog lack a Windows native controller. Recent versions of the signed provider may expose a generic document URL. The title and a successful button invocation cannot identify the destination or establish delivery.

## Decision

Select the canonical catalog route in the official Codex package. Require a fresh exact provider thread identity before editing and submitting. Prefer a document URL when available. The provider also offers a conversation-specific Copy deeplink action. A Windows adapter may read its result only from the verified provider, preserving supported clipboard formats locally and declining before invoking the action when it cannot restore them. Track real local input and the verified provider process throughout. Preserve existing drafts. Use Shared Rust to scope the editor and Send action and to reconcile the selected provider transcript after one submission attempt. Limit this path to ordinary text in an idle Code session.

## Alternatives

A second provider CLI would take ownership outside the selected desktop session. A window title can be duplicated. Blind keyboard submission would give no evidence of the target or receipt.

## Consequences

An ambiguous control tree, unavailable provider action, changing user input or unpreservable clipboard must prevent submission. An interrupted final press with no provider receipt is an unknown outcome. No clipboard contents or prompt text belong in diagnostics.

## Verification

Exercise exact, wrong and ambiguous targets, existing drafts, missing receipts, user intervention and clipboard restoration. Record live acceptance separately using an operator-selected idle Codex thread.

### Implementation evidence

A Windows x64 build and shared tests exercised the exact thread, a changed thread,
an ambiguous editor, local input, an existing draft and a late provider receipt.
The Windows library test duplicated a synthetic text allocation with Win32 and
checked the data after releasing the original handle. The full clipboard
round trip requires an isolated window station. The ordinary test account was
denied permission to create one. The live desktop exposed one header action and
two editor candidates during an active turn. The Copy menu was observed, but
a live deeplink, an ordinary idle send, and a provider-written receipt remain
device acceptance items.
