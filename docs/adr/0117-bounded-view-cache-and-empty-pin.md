# ADR 0117: Bound the view cache on disk and drop an empty Pin

- Date: 2026-11-04
- Status: Accepted; source verified, device acceptance pending
- Related: [ADR 0113](0113-pin-only-session-catalog.md), [ADR 0109](0109-uuid-session-refresh-and-permission-lifetime.md), [ADR 0090](0090-reuse-transcript-heads-between-rescans.md), [ADR 0091](0091-prune-conversation-views-only-where-separate.md)
- Renumbered from 0114, then from 0116, when
  [ADR 0114](0114-restore-unlanded-1-0-21-field-fixes.md) and
  [ADR 0116](0116-failure-screenshot-analysis.md) landed on `main` first; the
  decision and its evidence are unchanged.

## Context

The owner asks for two things: a session list that does not show Pin when
nothing is pinned, and a session cache that mirrors the providers' log format,
transfers incrementally, verifies the whole text and backfills only what is
missing. The second describes behaviour the client already has, so the question
the owner also asks — whether there is a better approach — is answered by what
is already built and by what measurement says is worth adding.

Reading the current implementation: the client persists one `AgentMessagePage`
per host, device and session, and the catalog beside it. Reopening sends the
cached range's first and last record ids with a SHA-256 over the canonical
encoding of the cached prefix; the daemon verifies that range against its own
parse and returns only the records after it, with separate flags for older and
newer records still to fetch. A mismatch is reported as a reset. On the daemon
side a transcript is read through a byte-offset tail that appends what the
provider's log grew by and detects a rewrite by its length; record ids are those
byte offsets, so cursors are already the providers' own append positions.

Two things are genuinely absent. Nothing ever removes a file from the cache
directory, so it grows with every session ever opened on every device: two
authorized Macs held 87 MB over 240 entries and 58 MB over 166. And a durable
draft is stored beside the evictable pages under the same naming, so nothing
distinguishes the owner's unsent text from a page that may be discarded.

Pin is seeded unconditionally beside Recent, so the list shows an empty Pin
section to an owner who has pinned nothing.

## Decision

Leave an empty Pin out of the projection entirely. Keep it while it still has
rows, while the daemon reports a count for it, while it is still paging or while
a page is in flight, so it does not disappear between a request and its rows.
Recent is the catalog itself and stays whether or not it is empty. This remains
Shared Rust: the platforms render the sections they are given.

Bound the evictable part of the cache on disk and give durable entries a name of
their own. A durable write lands under a kept suffix, reads prefer it and fall
back to the plain name so an entry written before this still loads, and pruning
removes neither a kept entry, a write still in flight, nor anything small enough
to be a draft from before the suffix existed. Pruning runs once when the cache
starts, on a blocking task, oldest first, until the directory is within budget.
Losing a page costs one ordinary history fetch the next time it is opened.

Do not cache the providers' raw log records. The normalized record stays the
cached form: provider formats are undocumented and change under us, the grammar,
hooks, usage accounting, compaction and subagent scope that turn records into
messages are Rust-owned, and the integrity hash is stable only over a canonical
encoding we define. What is worth keeping aligned with those logs is record
identity and append-only semantics, and the byte-offset tail already does that.

Do not add a daemon-side persistent parse cache. A cold open re-reads the whole
log, which measured 20–30 ms for transcripts of 5 to 9 MB and 140 ms for the
largest one to hand, against under 5 ms warm, on an authorized Mac. That does not pay for a second cache whose validity
depends on file identity surviving restarts, rewrites and inode reuse.

## Alternatives

Showing an empty Pin with placeholder text states nothing the owner does not
know. Bounding the cache by age alone keeps an unbounded number of recent large
pages; bounding by count ignores how much each one holds. Telling drafts from
pages by size alone would guess, so the suffix decides and size only protects
entries written before it. Chunked hashes would turn a reset into a partial
resync, but a reset follows a rewritten or recompacted log, where refetching is
the correct answer, and no evidence yet shows it firing otherwise.

## Consequences

An owner with nothing pinned sees one section. The first start after this
release prunes an over-budget directory, so some sessions fetch their history
again on next open. A draft written before this release is read from its old
name and rewritten under the new one when it is next saved. In-memory eviction
still clears the whole map rather than the least recently used, which can drop a
remembered submitted buffer; that is unchanged here and remains a known limit.

## Verification

Cover Pin present with a pinned row, absent with none, absent when a search
matches none of them, and present while a count, a continuation or a pending
page says its rows are still coming. Cover pruning for order, budget, kept
entries, small legacy entries, in-flight writes, idempotence, and a durable
draft that survives a prune of an over-budget directory. Run the Rust workspace,
strict Clippy and the repository gates.

## Implementation verification

The whole Rust workspace passes, including the new catalog and cache coverage
and the client integration tests that assert where a draft is written. Strict
Clippy passes for the workspace. Format, public-content and ADR gates pass.
These are source checks; no Apple build, installation or device acceptance is
claimed for this record.

A browser bridge test allowed one second for a debug-build child process to
start and bind its socket. It passed alone and failed under a full workspace
run, which this change made busier; its budget is now twenty seconds. The test's
subject is unchanged.
