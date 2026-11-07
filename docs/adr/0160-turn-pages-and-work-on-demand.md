# ADR 0160: Page history by turns and load a turn's work on demand

- Date: 2026-11-07
- Status: Accepted; implementation in progress
- Related: [ADR 0153](0153-large-session-history-costs.md), [ADR 0159](0159-grow-the-history-window.md); supersedes the display window of both

## Context

The phone reads and shows a conversation in records. Pages are 200 records,
and the window ADR 0159 grows is counted in records. On screen, though, a turn
is a few rows: the prompt, a collapsed Worked group, and the answer. Records
and rows are not related. In the long Claude Code session reported on
2026-11-07, the newest 200 records held 172 tool records, so a whole page
rendered as two or three rows, and one earlier page moved the reader off the
answer they were reading. ADR 0159 stops the window from moving away, but every
page still transfers, caches and projects the tool calls, tool output and
intermediate prose inside Worked groups. Those groups are collapsed by default,
and they are most of the records and most of the bytes.

The owner asked for a different unit: a turn. Read ten turns at a time; read
the tool calls and text inside a turn's work only when it is expanded; stream
them live while the turn is running; cache all of it.

## Decision

**The outline.** Shared Rust (`muster_proto::outline`) defines a session's
outline. It is the transcript with the work of every completed turn replaced
by one `work` record. Work is what the conversation shows inside a Worked
group: tool calls and results, and prose that is not the prompt, the final
answer, an interaction, a provider card, a compaction summary or a turn status.
Records the presentation never shows (empty text, superseded plan cards, a
tool call a provider card replaces) belong to a work span when they fall
inside one and stay in the outline otherwise. A turn is complete once a later
prompt or a turn summary follows it. The latest turn without either keeps
every record, so a running turn is never collapsed. The presentation rules in
`muster-app-core/src/timeline.rs` and the outline use the same classification,
so a collapsed turn and a fully loaded one render the same blocks with the same
IDs.

A `work` record (`AgentMessage.kind = "work"`, ID `work:<first record ID>`)
carries an `AgentWorkSpan`: the first and last record IDs, the record count,
whether the span starts with a tool record, its end time, and a SHA-256 over
the span's encoded records in the same form as cache proofs. Any change to a
span changes the work record, so existing byte-exact proofs over the outline
detect it.

**Turn pages.** `AgentMessagesRequest.turns` asks for the outline. Without a
valid cache proof, a page holds the last `turns` turns before `before_id`, at
most `tail` records (200 when unset); one larger turn is cut at that cap and
continues on the earlier page. With a valid proof, catch-up, `has_newer` and
`reset` work over outline records exactly as they did over records. A server
that pages the outline sets `AgentMessagePage.outline`. Every source the
daemon pages through `muster_proto::history` serves outlines, including the
SQLite index ADR 0153 added, which now stores each completed span and which
records start a turn (schema version 2). Orca keeps record pages.

**Work pages.** `AgentMessagesRequest.work_id` names a work record and
`work_hash` the span the client holds. The daemon returns that span's records,
newest first in pages of at most `tail` before `before_id`, or `reset` when the
span no longer matches.

**Live turns.** The watch still streams every record. The client keeps the
running turn in full. When a turn completes, the client collapses it with the
same shared function and keeps its records as that span's loaded work, so its
outline, and every proof built from it, match the daemon's. A turn whose start
the client has not loaded is not collapsed.

**The window.** Rust keeps ADR 0159's growing window, counted in turns: the
latest ten turns; Load earlier and Load newer add ten turns, at most 200
records a step; a browsed window holds at most 1,000 records. With record pages
(an older Mac, Orca), the same rules apply to records.

**Expanding.** A collapsed Worked group whose work is not loaded is a
`working` block with no children and `details = "unloaded"`. The shell keeps
expansion state as before; expanding such a block sends `AgentLoadWork` with
the block ID. Rust serves the work from memory, then the disk cache, then the
daemon, shows `loading` and then the children, or `failed` with an error the
shell offers to retry. A span longer than 2,000 records loads its newest 2,000,
and the group says how many earlier steps it leaves out.

**Caching.** Outline pages replace record pages in the recent and older page
caches under new keys. Loaded work is cached by session, work ID and span hash,
within the same 64 MiB budget, and survives reopening and offline use.

**Compatibility.** A page without `outline` (an older daemon, Orca) keeps the
client in record mode: no work records, no collapse, record pages. An older
client never sends `turns` and gets record pages as before.

## Alternatives

- Keep record pages and ADR 0159's window. Pages stay proportional to tool
  output that is collapsed on screen.
- Elide tool bodies but keep every record. Paging still counts thousands of
  records per turn, and the window problem remains.
- Decide completion from the daemon's runtime state instead of records. The
  outline would change with state, and client and daemon could disagree on
  proofs.
- Load work for every turn in the background. It would bring back most of the
  cost this decision removes; expanding is when the reader asks for it.

## Consequences

- Opening a session transfers, caches and projects ten turns of outline rather
  than 200 records, whatever their tool output.
- Expanding a Worked group that is not cached needs a round trip to the Mac,
  and offline it shows the failure until the Mac is reachable. Groups the phone
  saw live, or loaded before, open at once.
- A turn that completes while the phone is disconnected resets its cached page
  on the next catch-up, as any rewrite does.
- The protocol, the shared core and all three shells change. Swift and
  HarmonyOS bindings are regenerated; Android generates its own.

## Verification

Planned:

- Shared Rust: outline classification matches the presentation for completed,
  running, interrupted, compacted and card-bearing turns; turn pages and caps;
  proofs over outlines; work pages and stale spans; the stored outline in the
  SQLite index; local collapse matching the daemon's outline; window and cache
  behavior; record mode against an older server.
- Native: Apple tests and builds; HarmonyOS and Android source updates (no
  Android SDK on the build Mac).
- Installation and iPhone acceptance against the reported session.
