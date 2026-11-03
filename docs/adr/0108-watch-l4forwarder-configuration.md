# ADR 0108: Watch the l4forwarder configuration instead of polling it

- Date: 2026-11-03
- Status: Accepted; source verified, release pending
- Related: supersedes the 5-second poll in decision 2 of
  [ADR 0107](0107-l4forwarder-in-muster-app.md); the rest of ADR 0107 stands.

## Context

ADR 0107 has musterd supervise the bundled l4forwarder according to
`l4forwarder.json` in its state directory. musterd read that file every
5 seconds. The owner asked for a better mechanism than a fixed poll. The poll
wakes the daemon for nothing while the file is unchanged, and a change can
take up to 5 seconds to apply: 3.7 seconds in the end-to-end run.

muster-agent already watches provider transcript trees with `notify` and
polls only where the platform offers no watcher.

## Decision

1. **A file-system watch reports changes.** musterd watches the state
   directory, non-recursively, through `notify` (FSEvents on macOS, inotify on
   Linux).
   - **What wakes it.** An event naming `l4forwarder.json`, a watcher error, or
     an overflow that may have hidden one wakes the supervisor. It wakes
     through a `tokio::sync::Notify`, which turns a burst of events into one
     wake-up.
   - **Why the directory.** An atomic save replaces the file, which would end
     a watch placed on the file itself.
2. **Reads wait for the writer.** After a wake-up the supervisor waits 100 ms
   before it reads, because one save can be several events. As before, it
   acts only when the contents differ from what it last read.
3. **Restarts are timers.** A pending start, whether a restart backoff or a
   retry of a missing program, is a deadline the loop sleeps until. Nothing
   else wakes it while the file and the helper are unchanged.
4. **Polling is only a fallback.** If no watcher can be created or the
   directory cannot be watched, musterd logs that it is polling and reads the
   file every 5 seconds, as before.

## Alternatives

- **Watching the file itself.** An editor's atomic save, or `mv` into place,
  would leave the watch on a deleted file.
- **An explicit reload command** (`muster` RPC or a signal). It adds a step
  the user can forget, and the file stays the single source of truth anyway.
- **A shorter poll.** It is faster but wakes the daemon even more often for
  nothing.

## Consequences

- A configuration change applies at once. In the end-to-end run, forwarding
  worked 0.6 s after the write, against 3.7 s with the poll.
- The state directory also holds the store's write-ahead log, so the watch
  sees frequent unrelated events. The callback filters them by file name
  before anything wakes.
- A non-atomic save can still be read half-written if it takes longer than
  100 ms. That costs one extra restart, and the final contents then win.

## Verification

- Source and tests (macOS, Rust 1.98):
  - The `l4forwarder` module has 6 tests. The new one checks that a write and
    a removal are each acted on within 1 second, which only the watch can
    achieve (the fallback poll is 5 s). The earlier five pass unchanged with
    the poll removed. All six passed six consecutive runs.
  - The full `cargo test -p musterd` suite: 130 passed, 3 ignored (existing
    ignores). Every daemon test now also creates a watcher.
  - `cargo fmt`, `clippy -D warnings` and `scripts/check-layers.py` are
    clean. `notify` was already a workspace dependency; `Cargo.lock` only
    gains it in musterd's list.
- End to end, local: a debug musterd with an isolated state directory and the
  real addon beside it.
  - Forwarding worked 0.6 s after the configuration was written.
  - No fallback was logged.
  - `kill -9` of musterd still ended the helper 0.06 s later.
- Not done: a release, installation, and hosted CI (Actions are disabled,
  [ADR 0016](0016-suspend-hosted-actions-budget.md)).
