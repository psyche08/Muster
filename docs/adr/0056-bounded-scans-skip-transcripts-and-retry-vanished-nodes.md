# ADR 0056: Session scans skip transcripts; control scans retry a vanished node

- Date: 2026-10-27
- Status: Accepted
- Related: [0055](0055-clear-every-input-before-a-submission.md) (which left
  this failure class open), `docs/AGENT_INTERFACE.md`

## Context

ADR 0055 fixed the largest send failures in the three Macs' daemon logs and
deliberately left one class alone, because its evidence pointed at the
Accessibility walk rather than at the draft: a scan that stops early refuses
the send as "incomplete". Two different scans fail this way, for two different
reasons.

**The session scan hits the node ceiling once several conversations are
open.** To find the conversation's web area and prove no second view shows it,
`find_surface` walks the whole app. It also descended into every web area it
found, so every open conversation's transcript counted against
`ax::MAX_NODES` (4,096). On M4Pro at 2026-10-27 07:52 UTC, with five Codex
conversation views open, the walk reached 4,096 nodes in 363 ms — *after* it
had already found the target (`web_areas=5 target_found=true`) — and the send
was refused as "The native session scan was incomplete". The same tree had
grown over that morning from about 1,000 nodes to 2,600 and then past the
ceiling. Background attaches failed the same way (9 on M4Pro and 4 on the Mac
Mini in the last week). The Mini's session scans already sit at 3,125-3,167
nodes, 77% of the ceiling.

**The control scan stops when a node vanishes mid-walk.** `find_controls`
walks the target conversation with one attribute read per node and gives up on
the first read that fails. On the Mini at 2026-10-23 00:20 and 2026-10-25 12:14
UTC the draft was written and the send was refused as "The native control scan
was incomplete" 482 ms later — far under both the 2 s budget and the ceiling
(the Mini's complete control scans reach at most 2,910 nodes). Electron
rebuilds the composer and its controls around a write; a node destroyed
between being listed as a child and being read fails its read. The refusal did
not record why the walk stopped, so the cause had to be inferred from timing.

## Decision

Shared Rust owns both decisions in `muster_agent::native_scan`.

**A session scan does not enter a conversation's view.**
`descend_web_area(resolves)` is false for a web area whose own key resolves to
a conversation. `WebAreaKey` is read from the web area node's own `AXURL` and
`AXTitle`, never from its contents, so resolution is unchanged; the transcript
and composer are left to the control scan, which walks the target separately.
Web areas that resolve to nothing — the app shell, a home page — are still
entered, since they may hold conversation views. The platform walks with the
new `Element::walk_pruned` and `ax::Visit::{Descend, Skip, Stop}`;
`walk_report` keeps its behavior on top of it. Skipped nodes still count
towards the ceiling.

This never lets a send reach another conversation: the target is still matched
only by its own key, ambiguity is still refused among the views found, and the
control scan still requires exactly one composer in the chosen view. A view
nested inside another conversation's view would go unseen and refuse as not
found; no supported app has one, and before this change such a layout would
have been refused as ambiguous on every send.

**A control scan that stopped on a failed node read is walked again.**
`find_controls` now records why it stopped as `ScanStop::{NodeLimit,
TimeLimit, NodeRead(code)}`. `repeat_scan` allows up to `SCAN_ATTEMPTS` (3)
walks in total for a `NodeRead` stop, with `SCAN_RETRY_PAUSE` (100 ms) between
them, and repeats the whole surface scan so a replaced web area is resolved
again. A partial walk is never patched by skipping the failed node, because its
subtree may have held the composer or a second submit control. The node
ceiling and the time budget are not repeated: the same tree reaches the ceiling
again, and the budget is already spent.

Every refusal now names its stop — for example "The native control scan was
incomplete (node read failed (-25202)); not sent" or "The native session scan
was incomplete (node limit); not sent" — and the platform logs the walk's node
count, time and web-area count when a session scan stops.

## Alternatives

- **Raise `MAX_NODES`.** Only postpones the failure: the tree grows with every
  open conversation, and the session scan would get slower for every send to
  prove nothing it needs.
- **Skip a node whose read fails and keep walking.** Produces a "complete"
  scan of a tree that changed underneath it; the lost subtree could hold the
  composer or a second Send, so the uniqueness checks would be judged on
  partial evidence.
- **Retry inside each caller's own loop.** The same refusal reaches sends,
  answers, the composer clear, identity refreshes and attaches; one retry in
  the shared surface scan covers them all.
- **Prune transcripts inside the control scan as well.** The control scan's
  largest observed walk is 2,910 nodes, below the ceiling, and pruning there
  needs per-app knowledge of which subtree is the transcript. Left for when the
  evidence calls for it; its stops are now named in the logs, which will show
  it.

## Consequences

- With several conversations open, the session scan visits the app's chrome
  and each view's root instead of every transcript, so it no longer approaches
  the ceiling as conversations accumulate, and it is faster for every send.
- A transient vanished node costs at most two extra walks and 200 ms of pauses,
  inside the existing echo deadline, instead of a refused send.
- A genuinely huge single conversation can still reach the control scan's
  ceiling; it now refuses with "node limit" in its message instead of a bare
  "incomplete".
- The pause between walks blocks the calling thread, like the walk itself.

## Verification

- **Source and tests:** `cargo clippy --workspace --all-targets -- -D warnings`
  passes. New Shared Rust tests cover which stops are walked again and how many
  times, that only web areas resolving to nothing are entered, and the
  content-free stop wording. `muster-core`, `muster-agent`, `muster-platform`
  and `musterd` pass (496 tests); the full workspace run's result is recorded
  in the pull request.
- **Device evidence read:** daemon logs of the Mac Mini, M1Max and M4Pro through
  2026-10-27 10:57 UTC, as cited in Context.
- **Builds, signing, notarization, publication, installation:** not performed.
- **Runtime acceptance:** not performed. A send with five or more conversation
  views open, and a send whose control scan meets a vanished node, still need
  acceptance on an installed build.
