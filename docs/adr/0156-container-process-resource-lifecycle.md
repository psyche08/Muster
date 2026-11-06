# ADR 0156: Bound container exec resources and wrapper reconciliation

- Date: 2026-11-06
- Status: Accepted; source verified; installation pending
- Related: [ADR 0077](0077-multica-runtime-state-and-manual-upgrade.md),
  [ADR 0126](0126-multica-bridge-follows-the-container-gateway.md),
  [Multica architecture](../architecture/multica.md)

## Context

On a 16 GiB Mac, Apple container 1.6.1 retained about 1.7 GiB in its host
runtime after repeated execs. A heap summary contained over 23,000
LinuxProcess, exit waiter and HTTP/2 objects. Repeated no-op execs increased
the footprint by about 78 KiB each; guest memory was mostly reclaimable cache.
RuntimeService retains exited process objects and waiters, and ExitMonitor
retains completed task handles. Muster's 15-second maintenance pass repeatedly
installs unchanged wrappers. Containers created with `sleep infinity` as PID 1
also retain orphaned zombies because no init reaps them.

## Decision

1. Carry a version-pinned Apple container 1.6.1 source patch and reproducible
   validation instructions with the Multica deployment tools. Release heavy
   process and monitor state after exit, including failed cleanup paths.
   Resume existing waiters exactly once and keep only a bounded cache of recent
   exit statuses for late wait requests. Do not evict live waiters. Expired
   entries use the existing unknown-process result. This is infrastructure
   lifecycle code, not Muster product state in a platform shell.
2. Shared Rust caches successful wrapper reconciliation by container incarnation
   and configuration. A changed incarnation, bridge or provider destination
   invalidates it. Unchanged wrappers are rechecked at most every five minutes;
   failures do not populate the cache and retry on the next maintenance pass.
   The daemon health check and opted-in recovery retain their 15-second cadence.
   This supersedes ADR 0126's wrapper repair timing only; gateway recovery,
   credential checks and session destination pinning remain unchanged.
3. New managed containers use Apple's `--init` to reap orphaned children.
   Existing containers are not deleted, silently recreated or modified by an
   app update. Their operator migration must preserve the writable layer,
   mounts and configuration and retain a rollback image before replacement.

## Alternatives

- Lowering guest RAM does not bound the host runtime heap.
- Periodic forced restarts interrupt tasks and only hide resource retention.
- Never checking wrappers again loses recovery from drift. Checking every
  15 seconds creates unnecessary guest processes; a bounded five-minute
  reconciliation interval preserves eventual repair.
- Removing all exit records immediately races clients whose wait request
  arrives after a short process exits. Keeping all records grows without bound.
- Making the platform UI own recovery violates the Shared Rust boundary.

## Consequences

Steady-state maintenance keeps the health probe but avoids two wrapper execs
per pass. Provider binaries changed in place and guest wrapper drift are
reconciled within five minutes. The container patch needs a separately built
and signed runtime; merging Muster does not replace the installed runtime.
Existing containers require an attended maintenance window for init migration.
The patch does not change guest page-cache reclamation or lower its RAM limit.

## Verification

- Muster source/tests: `cargo test --locked -p musterd --lib multica` passed
  36 tests (one pre-existing ignored test); the eight bridge tests passed again
  after adding same-gateway data-root migration coverage. Tests cover 100
  unchanged maintenance passes without new wrapper execs, incarnation/provider
  changes, retry after failure, timed drift repair, unknown incarnation, config
  changes, and init in the actual installer command.
- Static checks: `cargo clippy --locked -p musterd --all-targets -- -D warnings`
  passed. The changed Rust files pass rustfmt; whole-workspace formatting has an
  unrelated pre-existing difference in `muster-agent/src/questions.rs`.
- Patch: verified clean application to the pinned upstream commit and byte-for-
  byte reproduction of the patched source/test files. Shell syntax and Python
  compilation checks passed.
- Patched Apple runtime: the debug `container-runtime-linux` executable compiled
  and all five `ProcessExitRegistryTests` passed (the monitor test covers four
  success/failure combinations). Tests cover early/late/concurrent waits,
  bounded exit history without evicting live waits, single callback invocation,
  monitor release, and stdin cancellation cleanup. HTTPS and SSH Git dependency
  fetches failed; verification used official archives of all 35 exact commits
  from upstream `Package.resolved`, with local dependency paths and an explicit
  Yams product reference in a temporary verification manifest. The runtime and
  test source files match this patch byte for byte. No release build or installed
  runtime memory-plateau acceptance is claimed.
- Runtime: an isolated 256 MiB Debian container with `--init` reaped eight
  orphaned test children, leaving zero zombies, and was automatically removed.
  The existing Multica container was not stopped or modified.
- Signing, installation, publication, existing-container migration and provider
  end-to-end acceptance have not been performed. A source merge does not claim
  that the currently installed runtime has changed or its accumulated heap has
  been released.
