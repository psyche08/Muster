# ADR 0063: Queue receipts use provider text matching

- Date: 2026-10-29
- Status: Accepted
- Related: ADR 0046, ADR 0048, ADR 0061; Architecture §3.6–3.7

## Context

ADR 0061 made a Codex equal-label HTTP(S) Markdown link equivalent to its
submitted bare URL in transcript receipts. Queue admission still compared raw
text, and the client's queued-row accounting used a separate raw/prefix match.
A prompt persisted in the provider queue could therefore remain unconfirmed
until it appeared in the transcript, or indefinitely if no user record followed.
This is a source-level omission; it does not prove the cause of any historical
unknown delivery whose original prompt and queue snapshot are unavailable.

The queue reader also changes physical identity when a local deferred item moves
to the app-server SQLite queue, despite the server retaining its client message
ID. Comparing only physical IDs can misclassify that older item as newly sent.

## Decision

Apply the same Shared Rust, provider-scoped full-text comparison to queue
admission. The queue source remains session-bound. Snapshot existing provider
item identities before the press, including the server item's client-message
alias when present; require exactly one new matching item. Local-to-server
migration of a pre-existing client message cannot confirm the current send. Edits,
reorders, source failures, duplicate new matches, truncated previews and queue
removal are not delivery evidence. Provider parsing and comparison stay in Rust.

Extend the display-only queued-row matcher with the same observed-side URL
normalization. Preserve occurrence baselines, display clipping and whitespace
handling, and preserve literal submitted Markdown. This is UI accounting only:
it does not settle the durable pending request or release the host latch. Only
the host's full persisted queue item can confirm admission. Acceptance into a
queue is distinct from execution or completion.

This extends ADR 0061 to queues and supersedes only raw-text equality for queue
admission/accounting, retaining ADR 0046 and ADR 0048's evidence boundaries.

## Alternatives

Waiting for the transcript ignores valid queued acceptance. Accepting a clipped
preview, old matching item or queue disappearance can falsely confirm delivery.
General Markdown rendering, fuzzy matching, retrying a press, or relaxing
provider/session boundaries is unnecessary and unsafe.

## Consequences

Codex formatting no longer prevents a queued acceptance from releasing the
matching unconfirmed write. Claude and other providers retain exact comparison.
Missing baseline/source evidence still leaves the outcome unknown. No existing
message is replayed, edited or manually marked delivered by this change.

## Verification

Test formatted persisted queue items without any transcript receipt, late
admission through WriteGuard, old and edited items, duplicate matches, wrong URL
destinations, other providers, source failures and clipped text. Test client
queue accounting before and after admission and subsequent transcript arrival.
The existing lost-acknowledgement test also drops a redundant struct update
reported by the current Clippy; its behavior is unchanged. Record compilation/tests separately from signing, installation and physical
provider acceptance. Live Codex UI access remains unavailable; use synthetic
fixtures and temporary stores, without bypassing that tool restriction.

### Source evidence

- 252 agent and 112 app-core tests pass (364 total; five pre-existing ignored
  tests). New coverage includes local JSON and SQLite/WAL queue acceptance,
  cross-session rejection, both supported client-ID spellings during migration,
  old/edited/ambiguous entries, provider-specific formatting and read failures.
- The write-guard test confirms once from a late queue item with no transcript
  receipt. Client tests preserve unknown outcomes despite a matching preview,
  retain the admitted text when the queue disappears, and retire it only when
  the transcript accounts for it.
- Whole-workspace test-target compilation and targeted strict Clippy pass.
- Signing, notarization, publication, installation and live provider acceptance
  have not been performed. This does not resolve historical unknown deliveries
  lacking their original admission baseline.
