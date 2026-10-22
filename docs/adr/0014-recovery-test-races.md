# ADR 0014: Fix the recovery tests' own races

- Date: 2026-10-22
- Status: Accepted
- Related: [ADR 0013](0013-ci-must-be-askable.md) is why these became visible;
  [ADR 0004](0004-transport-recovery-and-stable-vnet-owner.md) owns the recovery
  behaviour under test; [ADR 0011](0011-linux-ci-artifact-budget.md) already
  settled the Swift half of the same red CI

## Context

With Actions re-enabled ([ADR 0013](0013-ci-must-be-askable.md)), `muster-net`'s
recovery tests failed in two different places, neither in production code and
neither caused by the change that exposed them:

- `fail()` read `old.endpoint.local_addr()` *after* calling
  `fail_next_receive_for_test()`. The receive loop can observe the injected
  failure and release that socket immediately, so asking it for its own address
  returns `NotConnected` — the recovery working, reported as a fault. This is
  what failed on the Linux runner in
  `shutdown_during_rebind_never_resurrects_transport`.
- `echo()` made a single `open` attempt right after a transport rebuild. While
  both sides converge, a connection that is present can still be the one being
  retired, so that attempt can lose its stream with `connection lost`. This
  failed locally in `send_only_socket_failure_recovers_without_receive_error`
  under the parallel suite, while passing three times out of three alone.

Both are timing assumptions in the test helpers, not claims about the product.
They are flakes: the same code went green on `main` in run 39708067342 without
any change, which is exactly why a named cause matters more than a green run.

The Swift half of that red CI — `swift test` failing on Linux because
`MarkdownUI` pulled SwiftUI into the test graph — is **not** addressed here.
[ADR 0011](0011-linux-ci-artifact-budget.md) had already decided it, by giving
that product dependency a platform condition and keeping the Linux core tests.
An earlier draft of this record proposed moving `swift test` to the macOS job
instead; that would have contradicted an accepted decision and lost the Linux
coverage, and it was dropped.

## Decision

The recovery helpers assert convergence, so they stop depending on a single
moment:

- `fail()` reads the endpoint address before injecting the fault. The address is
  only needed to deliver the wake packet, and reading it first cannot race with
  the teardown the fault triggers.
- `echo()` retries a lost attempt inside the existing `CONNECT_TIMEOUT`, and
  fails immediately on any other error. The claim under test is unchanged: an
  RPC must succeed after recovery, within the deadline.

## Alternatives

- **Run the net tests single-threaded** (`--test-threads=1`). Rejected: the
  product faces concurrent sockets, and the tests exercising recovery are the
  ones that should run under contention. It would hide the helper bugs rather
  than fix them.
- **Mark the two tests `#[ignore]`.** Rejected: they cover the recovery path ADR
  0004 exists for, and an ignored red test is a lie with extra steps.
- **Leave them, since `main` went green anyway.** Rejected: a flake that passes
  today fails in the middle of someone else's landing tomorrow, and the cause
  was identified, not guessed.

## Consequences

- `echo()`'s retry is bounded by `CONNECT_TIMEOUT` and accepts only a lost or
  closed connection. A regression where every attempt is lost still fails. One
  where the first attempt is reliably lost and the second succeeds would now
  pass silently: that is the intended tolerance, and it is the convergence model
  the surrounding `wait()` helpers already use.
- `fail()`'s change removes a dependency on teardown order, so the test no
  longer cares how quickly recovery reclaims the socket.

## Verification

- `cargo test -p muster-net --lib` five times in a row before merging `main`:
  50 passed, 0 failed each time. The same configuration failed before the change.
- Not established: that these tests are now free of every race. Five green runs
  plus a named cause is evidence, not proof, and the Linux runner's timing is
  its own.
