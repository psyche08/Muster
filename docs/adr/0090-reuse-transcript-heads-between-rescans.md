# ADR 0090: Reuse transcript heads between catalog rescans

- Date: 2026-11-02
- Status: Accepted; implementation verified, installed acceptance pending
- Related: `muster-agent` catalog scan (`AgentService::rescan`),
  [ADR 0089](0089-paired-release-1-1-10.md)

## Context

The owner reported that the session list updates slowly. On M4Mini, musterd
used 45–60% of a core while idle in front of the list. A five-second `sample`
put most of that work in `AgentService::rescan`: `claude::scan` and
`codex::scan` call `viewer::head_records`, which parses up to 128 (Claude) or
64 (Codex) records of every transcript into `serde_json::Value`.

A rescan runs at most every 500 ms after any write under a provider's
transcript tree, and every five seconds otherwise. While any session is active
it therefore runs almost back to back. This Mac has 173 Claude sessions, 310
Claude subagent transcripts and 166 Codex rollouts. A release build of the
two scans took 1.5–2.5 s per pass:

| Pass | Claude (482 files) | Codex (166 files) |
| --- | --- | --- |
| 1 | 1.23 s | 0.28 s |
| 2 | 1.85 s | 0.28 s |
| 3 | 1.89 s | 0.57 s |

`ListChanged` is sent only after a scan completes. A changed title, a new
session or a new ordering therefore reached the app one to two and a half
seconds late. Scans also delayed open-transcript polling, which runs in the same
loop, and the parsing consumed CPU continuously.

Almost all of that work was repeated. A transcript's head is parsed again even
when the file's size and modification time have not changed since the last
pass.

## Decision

- `viewer::HeadCache<T>` maps each transcript path to its modification time,
  its size and the provider's parsed head. `get` returns the cached head when
  both the time and the size match, and reads the file otherwise. `finish` ends
  a pass and forgets every path that the pass did not see, so deleted or moved
  transcripts do not accumulate.
- `claude::scan` and `codex::scan` take a provider-owned `Heads` wrapper and use
  it in place of `Head::read`. `AgentService` keeps both wrappers behind one
  mutex and passes them to `scan_all` on the blocking pool. Overlapping rescans
  serialize on that mutex.
- Nothing else in a scan changes. Directory walks, `stat`, archive indexes,
  Claude Desktop relationships, tail recency and Claude rename lookups still run
  every pass. The catalog-level `updated_unix` and title reuse also stay as they
  were. The cache key is the same `(modified, size)` test that the catalog
  already trusts for `updated_unix`.

## Alternatives

- **Rescan less often.** Raising the 500 ms debounce or the five-second period
  would lower CPU, but it would slow the list further.
- **Parse heads into typed structs instead of `Value`.** This would make each
  parse cheaper, but it would still repeat the parse for every unchanged file.
  It can be layered on later if cold scans matter.
- **Incremental scans from watcher paths only.** These would avoid the
  directory walk as well, but FSEvents coalescing and missed events would then
  need their own reconciliation. The walk now costs a few milliseconds, so this
  complexity is not justified.

## Consequences

- A warm pass costs about 5 ms instead of about 2 s. The list therefore
  reflects a change within the existing 500 ms debounce, and musterd no longer
  parses JSON continuously while sessions are active.
- The first pass after daemon start still reads every head, which took about
  0.4 s here.
- Memory grows by one parsed head per transcript, made of short strings and
  flags. Only files seen by the latest pass are kept.
- A head that is rewritten in place to the same size with the same modification
  time would not be reread. The catalog already makes this assumption for
  recency. APFS stores modification times in nanoseconds.

## Verification

- **Profile (before).** A 5 s `sample` of the installed 1.1.9 musterd on M4Mini
  showed `AgentService::rescan` → `claude::scan`/`codex::scan` →
  `head_records` → `serde_json` as the dominant CPU stack. `top` showed musterd
  at 45–60% CPU.
- **Timing.** A temporary release-mode example ran both scans on this Mac's real
  homes, and was removed afterwards. Without the cache, each pass took
  1.5–2.5 s. With the cache, the cold pass took 0.36 s (Claude) and 0.08 s
  (Codex), and each warm pass took 2.6–3.1 ms (Claude) and 2.1–2.3 ms (Codex).
  The pass returned the same session counts (482 and 166).
- **Tests.** `viewer::tests::a_head_is_read_again_only_when_its_file_changed`
  checks the following:
  - an unchanged file is not reread;
  - a changed size or modification time causes a reread;
  - a path that a pass did not see is forgotten.

  The existing Claude and Codex scan tests pass a fresh cache.
  `cargo test -p muster-agent --lib` passed with 276 tests.
  `cargo test -p musterd` passed. `cargo clippy -p muster-agent --all-targets`
  reported nothing.
- **Pending.** The change takes effect when a release with this commit is
  installed. That release's ADR should record musterd CPU and list latency on
  the three Macs.
