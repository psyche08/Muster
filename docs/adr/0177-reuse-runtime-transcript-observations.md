# ADR 0177: Reuse passive transcript observations and suppress identical runtime notifications

- Date: 2026-11-10
- Status: Accepted
- Related: ADRs 0168, 0172 and 0175, Shared Rust agent runtime

## Context

Physical sampling after the catalog optimizations still found transcript reads under the pending-interactions RPC and process-activity discovery. Runtime discovery also notified every list subscriber after each observation, including when all displayed values were identical.

## Decision

Use the existing incremental Claude tracker for passive pending-card queries, with bounded session retention. Read provider desktop tasks afresh and keep execution-time controller validation independent. Never interpret a migrated Codex SQLite store as a JSONL question source. Reuse transcript-derived activity for an unchanged file, provider, worker launch and child scope. Continue observing process liveness every tick. Publish catalog changes on a changed activity, process state, presence or project, including after evidence expires. Keep unaffected Apple row bodies equal when their own inputs match.

## Alternatives

Lengthening the process timer would delay real liveness updates. Persisting an observation across launches would require new recovery and invalidation semantics. Freezing list order would hide real changes to displayed activity.

## Consequences

Active transcripts still incur work when they grow. Repeated status requests on an unchanged source avoid replaying its contents. Actual changes can still move a session in the sorted list.

## Verification

Test unchanged observations and registration changes, source replacement, partial appends, pending-card resolution and process generations. Build, sign, notarize and compare the installed desktop processes over separate thirty-minute windows. Physical acceptance and source verification are distinct.
