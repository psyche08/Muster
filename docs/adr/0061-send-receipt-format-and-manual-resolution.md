# ADR 0061: Provider URL formatting and explicit send resolution

- Date: 2026-10-29
- Status: Accepted
- Related: supersedes the exact-text-only and provider-only settlement portions of ADR 0046 and ADR 0048; Architecture §3.6–3.7

## Context

A Codex Desktop prompt was delivered after the ten-second receipt wait, but its
bare HTTPS URL was persisted as an equal-label Markdown link. Byte comparison
missed the receipt forever. The client labelled an unknown send as unsent and
only offered a not-delivered release coupled to another send.

## Decision

Shared Rust accepts Codex's equal-label HTTP(S) Markdown link formatting when
comparing a submitted prompt with a newly appended user record. All other text,
URL destinations, record roles, transcript boundaries and rewrite checks remain
binding. The timeline uses the same comparison so it does not keep a duplicate
pending row. This is not general Markdown rendering or fuzzy matching.

Add a separate, explicit user confirmation of delivery, bound to the complete
original send request. The host requires the current provider control grant,
records `user_confirmed` separately from provider-applied evidence, then releases
only the matching in-memory latch. The operation never sends, navigates, unlocks,
or manufactures provider evidence. Missing, rejected and conflicting ledger
rows cannot be acknowledged. Persistence precedes release; retry after a lost
response is idempotent. A late provider receipt remains authoritative.

The client offers a read-only status recheck and a confirmation-dialog-protected
"seen on Mac, do not resend" action independently of composer contents. Durable
pending state settles only after the authenticated host response; newer drafts
are preserved. Old hosts fail closed with an update explanation. Unknown results
are labelled unconfirmed even when they include an error explanation.

## Alternatives

Longer timeouts do not fix text formatting. Fuzzy Markdown equivalence can hide
changed destinations or substantive edits. Clearing client state alone leaves
the host latch blocked; editing a database manually bypasses request binding.
Calling manual acknowledgement a provider receipt loses evidence provenance.

## Consequences

Manual resolution is an operator assertion, visibly distinct from an actual
provider receipt. It can recover older operations after a daemon restart without
reconstructing unsafe historical receipt boundaries. It requires an upgraded
host and client; older clients treat the new terminal status as unknown. No
existing work message is replayed as part of verification.

## Verification

Cover exact formatting equivalence and negative cases, delayed receipts, missing
and mismatched requests, persistence failure, repeated manual confirmation,
revoked access, late provider evidence, newer drafts, and unknown-state wording.
Record source checks, Apple builds, signing, installation and physical acceptance
separately below; no deployment or physical acceptance is claimed at this stage.

### Source and build evidence

- Affected Rust library suites: 244 agent, 111 app-core and 7 store tests passed;
  five existing tests remain ignored. Four daemon RPC integration tests passed.
  The client recovery test additionally simulates an unavailable confirmation
  acknowledgement after persistence and recovers with status reads only.
- Whole-workspace `cargo check --workspace --tests` and targeted Clippy passed. Public-content scanning found no findings. The existing
  layer-map gate fails on `musterd -> muster-acp`; both that dependency and the
  layer script are unchanged from `origin/main`. Workspace formatting also
  reports pre-existing web/browser formatting differences with the installed
  formatter; edited Rust code is formatted without unrelated source churn.
- macOS and arm64 iOS-simulator `MusterUI` targets compiled with the regenerated
  Swift protocol code. These are shared UI compilations, not signed app bundles.
- No signing, notarization, publication, installation or physical desktop/phone
  acceptance is claimed. The existing stuck message was not replayed or altered.
