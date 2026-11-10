# ADR 0175: Reuse per-thread message times during shared catalog discovery

- Date: 2026-11-10
- Status: Accepted
- Related: ADR 0172, Shared Rust agent runtime

## Context

After reducing passive signature work, a desktop still showed substantial Codex store reads. A shared catalog refresh queried and decoded history for every visible native thread, including on title-only updates. The open-viewer optimization does not cover that reader.

## Decision

Keep message times in the read-only discovery service. Reuse a verified value for an unchanged history store. For a changed store, read provider projection cursors and message times within one SQLite snapshot. Reuse unchanged cursors for at most one minute before a trailing reconciliation, even if filesystem events stop. Explicit rescans bypass the cache. Absent cursors or missing metadata use the existing reader. Removed threads leave the cache. Refresh native section metadata only for providers touched by the watch event, preserving the others in provider order. Suppress UI publication when changed catalog inputs produce identical rows.

## Alternatives

Increasing the watcher delay would slow genuine changes and still replay the entire history scan. Sorting by the thread metadata timestamp would incorrectly move renamed conversations.

## Consequences

Catalog work on an ordinary provider update scales with changed threads plus a scan of lightweight metadata. Older schemas can require a full read. Out-of-band edits without cursor progress may take a minute plus scheduling time to appear.

## Verification

Exercise stable files, normal cursor changes, unmarked edits, legacy stores and equivalent visible rows in Rust. Build and sign the desktop candidate separately, then compare process CPU and disk reads on installed devices with the same 30-minute protocol.
