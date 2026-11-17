# ADR 0226: Task-local scopes are plain functions, not async fns

- Date: 2026-11-17
- Status: Accepted; source and tests verified
- Related: [ADR 0212](0212-forced-mask-tiles-and-session-launch.md), [ADR 0213](0213-bare-sidebar-badges-and-quiet-speculative-card-reads.md), [ADR 0221](0221-multica-workspace-folder-sessions.md), [ADR 0225](0225-multica-run-grant-closes-after-its-first-turn.md)

## Context

In a debug build, `cargo test -p musterd --lib scheduler::` aborts with
`has overflowed its stack`. The first test to fail is
`scheduler::tests::a_recorded_op_id_answers_its_record_and_never_runs_again`;
with it skipped, `a_done_adoption_answers_again_until_the_grant_ends` fails
next. A test thread has the default 2 MiB stack. ADRs 0212, 0213, 0221 and
0225 each met an overflow of this kind and ran their suites with
`RUST_MIN_STACK` raised to 8 or 16 MiB instead.

Caught in lldb at the guard page, the overflowing thread was 34 frames and
2.11 MB deep. Five frames held nearly all of it:

| Frame | Size |
| --- | ---: |
| the test function, holding the test body's future | 180,864 bytes |
| `poll` of the test body's async block | 1,273,824 bytes |
| `poll` of the test helper `scheduler::tests::operate` | 180,624 bytes |
| `poll` of `scheduler::call` (the test-only wrapper) | 179,584 bytes |
| `poll` of `scheduler::call_cancellable`, faulting in its prologue | at least 290,848 bytes |

An unoptimized build gives every temporary its own stack slot, so a `poll`
frame is a multiple of the futures it creates. The test body awaits a
scheduler call about fourteen times, each future about 90 KB. The bug is the
size of those futures, not the depth of the call chain.

The same overflow is not limited to test threads. On `origin/main`,
`tests/pair.rs` aborts on every run with `thread 'tokio-rt-worker' has
overflowed its stack`: an RPC served on a default 2 MiB runtime worker. There
lldb showed the `poll` frames of the RPC handler's async block (878,000 bytes),
of `WorkQueue::scope` (374,016 bytes) and of `handler::dispatch` (at least
454,688 bytes). That is the overflow ADR 0212 recorded in
`musterd::handler::dispatch`.

`-Zprint-type-sizes` (with `RUSTC_BOOTSTRAP=1`, on the musterd test build)
showed where the size comes from. Five helpers set a tokio task-local around a
future and were written as `async fn`:

```rust
pub async fn scope<T>(check: Arc<…>, future: impl Future<Output = T>) -> T {
    CHECK.scope(check, future).await
}
```

An `async fn` keeps its arguments in its state for as long as it runs, and
here it also keeps the `TaskLocalFuture` that the argument was moved into. The
compiler does not overlap the two, so each such wrapper doubles the future it
wraps. They nest: a ledgered native creation's 6.4 KB future becomes 12.9 KB in
`track_form`, 25.9 KB in `Navigation::scope`, and, after the navigation and
lease code around it, 41.8 KB in `Job::perform`. `control_guard::scope` then
doubles that to 83.6 KB for `scheduler::unlocked`. That is why `operate` and
`call_cancellable` were 86 and 88 KB. Outside the tests, `WorkQueue::scope`
doubles them again: the RPC request future at `handler.rs:288` was 187.7 KB,
and the `dispatch.rs` create and send tasks were 173 KB each.

## Decision

The five task-local wrappers are plain functions that return tokio's
`TaskLocalFuture` directly:

- `muster_agent::control_guard::scope`
- `muster_agent::control_guard::Delivery::scope`
- `muster_agent::control_guard::WorkQueue::scope`
- `muster_agent::native_navigation::restore::Navigation::scope`
- `muster_platform::macos::navigation_restore::track_form`

Each is `fn scope<F: Future>(…, future: F) -> TaskLocalFuture<V, F>`. The
returned future is the wrapped one plus the task-local value, so a wrapper no
longer doubles anything. Callers are unchanged: they already `.await` the
result, and none names the old `T` parameter.

Behaviour is unchanged where it matters. `LocalKey::scope` builds the future
without polling anything, as calling an `async fn` did. It sets the task-local
on every poll and restores it afterwards, and it drops a started inner future
inside the scope, as before. Two differences remain, and neither is observable
by a caller today:

- The value set in the scope (a clone of an `Arc` handle, or the check
  closure) is taken when the scope is created, not at its first poll. Every
  caller creates the scope and awaits it in the same expression.
- A scope dropped before its first poll now drops the inner future with the
  task-local set; the `async fn` dropped it without. No `Drop` in
  `muster-agent`, `muster-platform` or `musterd` reads one of these
  task-locals.

The test stack is not configured. No test, `.cargo/config.toml` or script sets
`RUST_MIN_STACK` for this.

## Alternatives

- **Raise the stack for the tests** (`RUST_MIN_STACK`, a `[env]` entry, or a
  runtime with bigger worker stacks per test). It hides the size instead of
  removing it, and every new layer of wrappers would need a bigger number.
  The task asked for this only as a last resort.
- **Box the large futures at a few call sites** (`Box::pin(unlocked(…))`, as
  `handler::dispatch` already does for side chats and archives). That moves a
  doubled future to the heap rather than shrinking it, needs a choice of site
  for every new path, and allocates on each call. It remains the right tool
  for a future that is large for its own reasons.
- **Fix `Navigation::with_window_scope` and `navigation_restore::run` too.**
  They also keep their `operation` argument while awaiting something that
  contains it, but they have logic around the await. After this change their
  futures are small enough that this costs only kilobytes, so they are left
  as they are.

## Consequences

- The futures on the scheduler, Locked Use, dispatch and RPC paths are a
  quarter to a seventh of their former size (see Verification). Each `poll`
  frame of a debug build shrinks with them, on test threads and on runtime
  workers alike. Release builds were not measured.
- The scheduler tests pass with the default 2 MiB test stack, and the whole
  `musterd` suite runs without `RUST_MIN_STACK`. Later ADRs need not record a
  raised stack for these suites.
- A new task-local wrapper should follow the same shape. An `async fn` that
  takes a future and awaits a value that owns it doubles that future.
- The 8 MiB runtime thread in `musterd`'s `main.rs` and the 8 MiB worker
  stacks in the `access` and `cli_archive` integration tests are unchanged;
  this ADR does not decide whether they are still needed.

## Verification

Source and tests, on an Apple Silicon Mac with rustc 1.99.1 (Homebrew),
debug builds, `RUST_MIN_STACK` unset:

- Reproduced on `origin/main` (76067b3d): `cargo test -p musterd --lib
  scheduler::` aborts in
  `a_recorded_op_id_answers_its_record_and_never_runs_again`. The frame sizes
  above come from lldb at that fault.
- Future sizes from `-Zprint-type-sizes`, before and after:

  | Future | Before | After |
  | --- | ---: | ---: |
  | RPC handler async block (`handler.rs:288`) | 187,720 | 27,936 |
  | `handler::dispatch` | 92,824 | 25,392 |
  | `dispatch.rs` create task (`dispatch.rs:153`) | 173,688 | 19,512 |
  | `scheduler::call_cancellable` | 88,808 | 21,376 |
  | `scheduler::operate` | 86,128 | 18,696 |
  | `scheduler::unlocked` | 83,664 | 16,232 |
  | `scheduler::Job::perform` | 41,752 | 16,136 |
  | `navigation_restore::run` for a native creation | 39,888 | 14,272 |

  The largest future in the `musterd` test build is now 27,936 bytes.
- With the change, all 20 scheduler tests pass. `cargo test -p musterd
  --no-fail-fast` passes every target: lib 163 passed and 3 ignored, and every
  integration test binary and the doc tests pass.
- Headroom: the whole `musterd` lib suite passes with `RUST_MIN_STACK` at
  1.25 MiB and 1.5 MiB; at 1 MiB a Locked Use test overflows. The scheduler
  tests pass at 1 MiB and overflow at 512 KiB. Before the change they needed
  more than 2.11 MB.
- `tests/pair.rs`, `create_invite_join_approve_call_and_remove`, fails
  intermittently with `timed out waiting for returning device awaiting
  approval` (`pair.rs:252`), where a removed device joins again. This is not
  caused by this change:
  - On `origin/main` with the default stack, the binary overflows on every run
    (6 of 6), so the test never reaches that point.
  - With `RUST_MIN_STACK=8388608`, run alternately, it failed at that same line
    in 3 of 6 runs on `origin/main` and 4 of 6 on this branch.
  - With the default stack on this branch it failed in 6 of 12 runs; the other
    two tests in the file always passed.
  It is left to its own fix.
- `cargo test -p muster-agent -p muster-platform -p muster-acp -p
  muster-app-core --no-fail-fast` passes: `muster-agent` lib 436 passed and 4
  ignored, `muster-app-core` lib 185 passed and 1 ignored, `muster-acp` 25,
  `muster-platform` 54 passed and 3 ignored, and every integration test.
- Strict Clippy (`--no-deps --tests -- -D warnings`) on `musterd`,
  `muster-agent`, `muster-platform`, `muster-acp` and `muster-app-core`;
  `cargo fmt --check` on the changed files (`muster-agent/src/questions.rs`
  differs from rustfmt on `origin/main` already and is left out);
  `scripts/check-adr.sh origin/main`; `scripts/check-public-content.py`.

Not applicable: no app build, signing, installation or device run. The change
is to how futures are laid out, not to what any operation does.
