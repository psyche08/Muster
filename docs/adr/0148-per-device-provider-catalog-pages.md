# ADR 0148: Ten-session pages per device and provider

- Date: 2026-11-06
- Status: Accepted
- Related: ADR 0147; architecture/services.md

## Context

The operator requests ten sessions per device/provider on initial load and ten
more per device/provider on Load more. The current Pin/Recent page budgets span
providers and the client applies one display-wide limit, allowing a busy source
to hide other sources.

## Decision

Use optional provider-page requests with one source cursor per device/provider.
A source page contains ten root conversations in descending activity order;
existing nested Claude family rows accompany their root without consuming slots.
Pin and Recent remain presentation sections and share the provider's ten-root
budget. Either section's Load more advances all eligible source cursors by ten,
with one outstanding batch; no global display limit may hide another source's
first page. Previously loaded families remain eligible for live refresh.
Bound initial cache-visible IDs to the current per-provider display budget.

Retain legacy grouped paging for old clients. New clients request provider pages;
old servers may return legacy groups, whose existing cursors remain usable until
updated. Search retains its separate query-bound cursor contract.

## Alternatives

One global ten-session page can starve less active devices/providers. Ten per Pin
and ten per Recent doubles the requested provider budget. Grouping the UI by
provider is unnecessary for changing transport and display limits.

## Consequences

Total initial roots scale with reachable devices and providers. Pin/Recent counts
remain explicit, but some pinned roots require paging if outside the source's
newest ten. Live updates can refresh loaded rows without adding hidden history.

## Verification

Cover two devices and multiple providers, initial ten and next ten per source,
no duplicate/omitted boundaries, shared Pin/Recent budget, family roots, loaded
row refresh, cache limits and legacy compatibility. Validate the real two-Mac
catalog alongside ADR 0147 diagnostics before further publication.

## Implementation evidence

Focused Rust checks passed: 160 application-core tests, eight RPC chat-watch
integration tests, 17 RPC tests, and 11 catalog tests (two existing ignored tests).
Three QUIC link tests and strict Clippy also passed. The new live probe exercises shared-core paging and cross-device entry without
printing conversation content. Native build and two-device acceptance are pending.
The two-Mac candidate uses 1.1.25 (115); no public OTA is claimed here.
