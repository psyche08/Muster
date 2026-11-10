# ADR 0172: Bound passive discovery and reuse unchanged catalog projections

- Date: 2026-11-10
- Status: Accepted
- Related: ADR 0142, ADR 0161, ADR 0168, ADR 0170

## Context

Sampling an installed desktop showed repeated signature verification, AX surface walks, shared history database reads and session grouping. A filesystem trace found reads of the same runtime file category every three seconds during passive discovery.

## Decision

Reuse a successful signature result only in passive discovery, for the same PID, launch time, kernel code hash, valid signing status and requirement, with bounded lifetime. Operations keep their fresh checks. Observe idle native surfaces less often while checking lifecycle and lock transitions each tick. A changed shared Codex history database can cheaply identify unrelated progress through its per-thread projection cursor; use the full reader for legacy stores and periodically reconcile unmarked edits.

Cache display rows and groups from their source data, presentation controls and offline-device set. Compare those inputs before projecting. Continue rebuilding on changes to rows, search results, grouping, reachability and organization. Build desktop 1.1.31 (121) for direct installation.

## Alternatives

A PID-only cache cannot distinguish a replacement executable. Slowing the entire daemon would delay open conversations. Reusing a directory solely by its row count would conceal same-size edits and revocations.

## Consequences

An idle native observation can take up to three seconds to resume. The cache retains one catalog per renderer. Out-of-band history edits without an advancing provider cursor require bounded reconciliation.

## Verification

Exercise cache invalidation, legacy history, same-size edits and uncached output equivalence. Check production and test builds, signing, notarization and installation separately. Compare thirty-minute per-process CPU and disk counters with the previous desktop build, including time lost to sleep.
