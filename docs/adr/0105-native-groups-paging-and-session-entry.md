# ADR 0105: Native groups, incremental catalog pages and fresh session entry

- Date: 2026-11-03
- Status: Accepted; implemented, device acceptance pending
- Related: [ADR 0104](0104-three-category-session-catalog.md), [ADR 0102](0102-session-sections-hierarchy-search.md)

## Context

The owner replaces the single Section category with each actual native Section
group and Project Group, requests ten main sessions initially and the next ten on
Load more, and reports slow refresh and inconsistent titles on session entry.
The client currently requests up to 1000 catalog rows per device and holds the
first State event behind history catch-up. Claude desktop names can be overridden
by an already-open transcript viewer's older title.

## Decision

Supersede ADR 0104's combined Section bucket. Keep Pin and Recent, and project each
native custom/project group directly between them without provider/device/date
subdivisions. Aggregate equal group names across devices and providers for display while retaining every device/provider/native UUID for operations.
Shared Rust owns classification, page cursors, stale-response rejection and group
counts. Fetch bounded pages of ten main sessions; Claude descendants travel with their root and do not consume page slots. Load more requests the next page instead of
increasing a client-side slice of a preloaded entire catalog. Native metadata and
change notifications remain distinct from paged session contents.

On entry subscribe first, immediately apply non-message state/control events, then
reconcile history using the existing message-ID barrier and catch-up protocol.
Do not queue a current title or approval card behind an unrelated history read.
Refresh provider metadata on demand, using native desktop titles by verified UUID
when present, with transcript titles only as fallback. Keep catalog and opened
chat projections consistent and reject late events after navigation.

## Alternatives

A local ten-row slice still transfers the entire catalog and does not implement
next-page reads. Aggregating display labels must not collapse the underlying native identities. Discarding the
subscription barrier can miss messages during snapshot loading. Re-reading all
transcripts before emitting State makes entry latency worse.

## Consequences

Group membership can change between pages; cursors must be scoped and validated,
and refresh must repair moved/deleted rows without duplicating visible sessions.
A temporarily unreachable host must not block other hosts or erase a usable chat.
Native organization writes and their authorization are recorded separately. The
owner requires AX plus files: AX performs native control; local files supply
read-only titles, grouping, history and incremental state. No native app-server
control or direct provider database writes are part of this implementation.

## Verification

Use synthetic fixtures for duplicate names/IDs, group type, ten-main-session boundaries,
continuation, stale pages and membership changes. Verify State arrives before slow
history and native rename cannot regress after viewer refresh. Run focused Rust
suites, protocol generation, Apple builds and policy checks. Track source, signing,
macOS deployment, iOS publication and physical acceptance separately.

## Implementation evidence

The Shared Rust catalog now returns ten roots per native source group and a
source-bound continuation. Aggregated display groups reveal ten roots at a time;
Claude children accompany the root without consuming its quota. Watch refreshes
retain loaded families. Native title reads run independently of history catch-up,
and a verified title survives a temporarily unavailable active-account sidebar.
Synthetic paging, duplicate-name aggregation and native-title entry/rescan tests
pass. Message ordering still uses the existing subscription/history barrier.

Release candidate: 1.1.12 (102), shared macOS/iOS source. The owner subsequently
selected direct SSH installation to the two designated Macs without notarization.
Signing, installation, iOS publication and physical acceptance remain separate
release steps and are not established by the source checks.

The final owner-selected distribution is signed macOS installation over SSH and
iOS OTA only. Do not push this turn to GitHub or upload to TestFlight. Apple
notarization is explicitly omitted for the Mac installation.

### Native catalog consistency follow-up

Native file reads can fail during provider writes. Preserve the last complete
provider grouping on a failed read, with its unavailable notice, instead of
flashing all sessions into Recent. A successful empty snapshot still removes
old groups. Keep session rows, native sections and group continuations together
in the client cache. Publish one atomic native catalog projection so membership
and section names cannot come from different rescans. Refresh generations cancel
pending pagination; delayed pages from a previous native group revision must not
restore stale memberships. Tests will cover transient reads, empty successful
snapshots, cache restoration and watch/page ordering. Native mutations continue
to require fresh source readback and never trust a retained display snapshot.

Serialize filesystem rescans so an older result cannot replace a newer snapshot;
a failed scan worker preserves the last committed catalog. The daemon projects
sessions, sections and notices under one catalog lock and filters provider grants
before paging. Cache native group descriptors with their sessions, and stage all
continuations until the pending batch finishes. Reject continuations whose base
group changed in the live watch; restart the watch with all newly visible UUIDs.

Follow-up source verification: 616 affected Rust library tests passed, with seven
existing ignores; strict Clippy, ADR and public-content checks passed. The release
candidate is 1.1.13 (103). Native mutation acceptance and deployed runtime results
are recorded separately from these source checks.

The unavailable-notice presentation decision is superseded by
[ADR 0107](0107-silent-native-catalog-read-failures.md). Native read diagnostics
and last-complete snapshot retention remain internal.

[ADR 0108](0108-pin-only-session-catalog.md) supersedes named-group presentation
and Section/Project UI controls at the owner's request. Native data is preserved;
Section acceptance is paused.
