# 0302. Render Codex questions at their transcript position

- Date: 2026-11-24
- Status: Accepted; Rust and SwiftUI component verified
- Related: [0096](0096-question-editor-preparation.md), [0101](0101-question-placeholder-projection.md)

## Context

Codex stores an asynchronous question as an agent message with structured questions and a Markdown fallback. The SQLite transcript reader projected only its text. The pending-interaction reader skipped SQLite-backed catalog entries even when the provider had recorded a thread-specific rollout.

## Decision

Retain the provider's structured questions and call ID in the transcript. For live responses use the existing validated rollout lookup, pending-question reader and native answer operation. Display the recorded question in place in Apple chats. Offer controls when that call has a current pending interaction. Keep archived or answered cards readable.

## Alternatives

Recognizing bullet lists would misidentify ordinary assistant messages. Treating every historical question as pending would allow stale actions.

## Consequences

Older clients can continue displaying the text fallback. A missing or mismatched rollout supplies no live answer authority. The existing optional-question lifetime and confirmation semantics continue to apply.

## Verification

Rust tests cover structured SQLite options, plain bullet lists, matching and mismatched rollout identities, missing rollouts, reply retirement and cache refresh. A SwiftUI component test renders the expanded live form at 320 and 736 points. Installed application answer delivery requires a pending provider interaction on the target device.

Device validation found that paginated records use a turn-qualified key while a live response uses the provider item ID. Keep that key for pagination. Match recorded questions to the pending interaction with the provider ID and invalidate cached projections that used the pagination key for both purposes.
