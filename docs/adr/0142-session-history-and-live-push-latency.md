# ADR 0142: Keep session history and live pushes independent of catalog work

- Date: 2026-11-05
- Status: Accepted; source verified, installed acceptance pending
- Related: Architecture §3.6.2; ADR 0090; ADR 0110; ADR 0135; ADR 0139

## Context

The owner reports seconds of delay before fresh content appears on remote-session
entry on macOS, with similar delays and poor live synchronization on iPhone.
Source inspection finds that the transcript poll loop awaits every full catalog
rescan, including filesystem and provider database discovery. A slow scan stops
incremental message pushes. History reads also project every stored message to
protobuf before selecting the requested page, copying unrelated historical text
under the shared catalog mutex. These are source-level bottlenecks, not measured
attribution of every observed device delay.

## Decision

Run full discovery at startup. With healthy filesystem coverage, subsequent
catalog updates invalidate only reported paths: directly inspect changed
transcripts, walk only created/moved subtrees, remove deleted sources and retain
other source candidates. Refresh a shared SQLite store's own snapshot on its
DB/WAL events, without walking unrelated transcript roots. Include native title,
archive, group and Claude Local Storage sources. Normalize physical filesystem
paths against configured aliases, including deletion events.

Coalesce paths before waking the bounded channel; overflow and watcher errors
explicitly request recovery. Missing registrations use the existing rescan
interval only until coverage can be restored. Re-register changed roots, and
repeat a changed batch once to cover creation races. Healthy coverage never
triggers a periodic full scan. Keep one discovery operation in flight while
continuing open-transcript polling, preserving the existing scan lock and
source-generation checks. Existing explicit discovery/readback calls used by
native creation, controller attachment and authorized organization operations
remain available; this does not weaken their freshness requirements. Run periodic
transcript and queue file I/O on the blocking pool so it does not occupy an
asynchronous runtime worker.

Select history boundaries against source message IDs, hash only the cache span
needed for validation, and project only the returned page. Retain identical
cursor, reset, ordering and content-hash semantics. Shared Rust continues to own
all synchronization; platform applications only render the resulting state.

## Alternatives

Shorter polling intervals would not remove time spent awaiting scans. Longer
client deadlines would hide symptoms without reducing work. Removing the
subscription/history barrier or cache content validation could lose messages or
retain rewritten history. Adding native-platform refresh loops would duplicate
shared semantics.

## Consequences

A catalog scan may overlap transcript I/O; at most one background rescan and one
periodic poll run at a time. Existing source replacement and generation guards
must discard obsolete results. Cold source parsing, large cached-prefix hashing,
network reconnection and native rendering can still contribute to latency.
No authorization, provider controls or send-replay behavior changes.

## Verification

Add a regression proving message pushes continue while a catalog scan waits,
and verify asynchronous runtime progress during blocking viewer work. Compare
projected pagination against existing pagination across valid, changed, missing
and reversed cursors, and count projections for a large history's small page.
Run affected Rust tests including remote watch/history catch-up, formatting,
Clippy, ADR and public-content checks. Production builds, signing, publication,
installation and physical Mac/iPhone acceptance remain separate and unverified.

### Source evidence

- Agent/protocol suites: 439 passed, five existing ignores. Remote chat watch
  suite: six passed, covering history catch-up and reconnect behavior.
- New regressions cover changed-file isolation, directory moves/deletes, source
  duplicates, title-index changes, DB/WAL/native metadata invalidation, bounded
  event coalescing, path aliases after deletion, and removal of periodic full
  discovery with healthy coverage.
- A held discovery lock does not stop a synthetic appended message from being
  pushed. A blocking viewer yields the async runtime and cannot overwrite a
  replaced source generation.
- A 10,000-message source with a ten-message page performs ten projections;
  a cache proof of 14 messages plus a six-message delta performs 20. These are
  work counts, not physical-device latency measurements.
- Strict Clippy passes for all agent/protocol targets. The first sandboxed run
  could not create local sockets; rerunning with socket access passed. A real
  filesystem-notification test exposed physical/configured path alias mismatch;
  the corrected discovery test passes.
- Production builds/signing, publication, installation, and physical macOS/iPhone
  acceptance have not been performed. Cold parsing and network/rendering latency
  remain separate from the eliminated catalog wait and whole-history projection.
