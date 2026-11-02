# ADR 0080: The desktop session-list scan prunes conversation views

- Date: 2026-11-02
- Status: Accepted; source verified, device acceptance pending
- Related: [ADR 0056](0056-bounded-scans-skip-transcripts-and-retry-vanished-nodes.md),
  [ADR 0040](0040-desktop-navigation-identity.md),
  [ADR 0075](0075-claude-registry-entries-without-cli-session.md),
  [ADR 0076](0076-paired-release-1-1-6.md),
  [AGENT_INTERFACE](../AGENT_INTERFACE.md)

## Context

The three macOS daemon logs (M4Mini 1.1.6 build `0da52647`, M1Max 1.1.5
build `3ae46a2f`, M4Pro 1.1.7 daemon build `071e54de`) contain a remaining
class of refused sends that is not provider state: sending to a session that
is not the conversation currently open in Claude.app or ChatGPT.app.

On M4Mini at 2026-11-02 03:06:03Z, 03:06:08Z and 03:06:30Z three sends to
Claude Code session `59b3bc9a` stopped before the ledger:

```text
Agent send op="app-…" target="59b3bc9a-…": native preparation failed before
ledger elapsed_ms=3177: unavailable: session selection: desktop content is not ready
```

The AX diagnostics for the same attempts show why. Claude.app (pid 83467)
showed one *other* conversation (`ded32f16`) and the target title was unique;
the session scan completed (`web_areas=2`, walk `(287, "complete", 14)`), but
the sidebar scan reported

```text
navigation_areas=0 app_walk=(155, "unreadable_children", 11) (nodes, stop, ms)
```

ten times inside the 3 s read-only budget, once per rescan. Because
`navigation_areas == 0`, `prepare_readonly` treats the desktop as not ready
and no candidate or exact-id route is ever considered.

The cause is the breadth-first walk in `scan_sidebar`. Claude.app exposes two
web areas at the same depth: the app shell (which contains the sidebar rows)
and the open conversation. The walk visits the conversation's web area before
the shell's descendants; that node's `AXChildren` read fails (an invalid or
vanished Electron handle while the open transcript is rebuilt), and
`walk_pruned_report` stops the whole walk on the first unreadable child. The
sidebar is behind the point where the walk stopped, so no navigation area is
ever found. Pruning the conversation subtree makes the same walk complete and
reach the shell's rows; the failing node is never read.

ADR 0056 already decided that the session scan (`find_surface`) does not enter
a conversation's view, because the transcript is unnecessary and grows without
bound; `muster_agent::native_scan::descend_web_area` is the shared rule and has
unit tests. The separate app-level walk in `scan_sidebar` never applied it.

The same "0 navigation areas" shape also appears in older M1Max answer
failures (`Agent respond … cannot uniquely select the session in the desktop
sidebar (0 matches, 0 navigation areas, unique title=true)`); this change is
expected to help that class only where the missing sidebar is the reason.

## Decision

`scan_sidebar`'s app walk prunes an `AXWebArea` whose own key resolves to a
conversation, reusing `muster_agent::native_scan::descend_web_area`. The app
shell and any web area that resolves to nothing are still entered; rows are
still matched only by exact URL id or exact unique title, uniqueness is
unchanged, and the opened session identity is verified as before. Nothing is
pressed when the walk still stops early: `navigation_areas` stays 0 and the
send refuses with the existing readiness error. The AX trace now records a
`conversation_views=` count next to `navigation_areas=`.

## Alternatives

- **Retry the failed child read or patch a partial walk.** The outer read-only
  loop already rescanned ten times in 3 s; the stop was deterministic while
  the conversation view was open. ADR 0056 also forbids patching a partial
  walk by skipping the failed node, because its subtree could hold a control.
- **Raise the 3 s readiness budget or the node ceiling.** Neither limit
  stopped this walk: the budget was not exhausted (the last attempt still
  returned at ~3177 ms after ~10 scans) and the node count was far below
  `MAX_NODES`.
- **Read the open view's DOM to find the sidebar.** The sidebar is chrome and
  is not inside the conversation view in any supported app profile.

## Consequences

- A phone or desktop send to a session that is not the open conversation can
  find the sidebar again while another conversation view is open, instead of
  failing as "desktop content is not ready".
- If a chrome node is unreadable, the walk still stops and the send still
  fails closed.
- No new state is introduced; the pruning policy stays in Shared Rust and the
  platform walk only applies it.

## Verification

- **Source:** `cargo check -p muster-platform --all-targets` passes on macOS;
  `cargo clippy -p muster-platform --all-targets -- -D warnings` passes. The
  reused `descend_web_area` decision and its tests come from ADR 0056.
- **Device logs:** the three daemon logs read for this analysis are listed in
  Context. `session selection: desktop content is not ready` appears 13 times
  on M4Mini only (`1.1.4` 4, `1.1.5` 6, `1.1.6` 3); the other two Macs' send
  refusals predate the fixes on `main` and are not claimed here.
- **Not done:** a live send into a real conversation was not used as a test,
  and no post-install acceptance exists yet. Acceptance is an owner send to a
  session that is not the open one, with the installed build, while the same
  AX diagnostic shows `navigation_areas > 0` and no `unreadable_children`
  stop. Build, signing, notarization, publication and installation are
  recorded in the paired-release ADR for 1.1.8.
