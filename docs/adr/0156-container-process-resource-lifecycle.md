# ADR 0156: Bound container exec resources and wrapper reconciliation

- Date: 2026-11-06
- Status: Accepted; source verified; patched runtime observed installed and the
  existing Multica container migrated to init (2026-11-16); provider end-to-end
  acceptance pending
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
- Hosted CI: PR #166 reports no checks; the repository permissions API confirms
  Actions is disabled under ADR 0016. No hosted run was dispatched or claimed
  to pass; local checks above are the validation evidence for this merge.
- Signing, installation, publication, existing-container migration and provider
  end-to-end acceptance have not been performed. A source merge does not claim
  that the currently installed runtime has changed or its accumulated heap has
  been released.

## Installation and migration evidence (2026-11-16)

Recorded on the operator's 16 GiB Mac. This section adds evidence; the
decision above is unchanged.

Installed runtime:

- Apple container reports 1.6.1 for the CLI and API server. The installed
  `container-runtime-linux` is signed by team `89LGY6BD53` rather than by
  Apple and was replaced on 2026-11-10, which indicates the operator's rebuilt
  runtime. Its build provenance against this patch was not re-derived here.
- Exec growth, measured with macOS `footprint` on the host runtime process of
  an idle Multica container: 7,697 KB before, 7,793 KB after 100 `/bin/true`
  execs and 7,921 KB after 200, about 1.1 KB per exec against about 78 KiB
  before the patch. After almost six hours of uptime and several hundred
  monitoring execs the footprint was 7.7 MB. This is consistent with the
  bounded exit-status cache. No plateau beyond 200 execs is claimed, and
  `probe_exec_resources.py` was not run.

Existing-container migration (README procedure, with one deviation):

- Keep-alive was not switched off. Instead, Muster.app was quit with SIGTERM,
  because it rejects the AppleScript quit event (-1708). The agent was then
  stopped with `launchctl bootout gui/$UID/dev.linsheng.musterd`, so nothing
  could restart the container. Reopening Muster.app re-registered the agent,
  which restarted the Multica daemon. The keep-alive setting was never changed.
- With no task active, the container was stopped and its writable layer was
  committed to a dated local rollback image. Both bind mounts, the read-write
  Multica data root and a read-only workspace mount, stayed on the host and
  were not modified.
- Two disposable 256 MiB containers ran from the rollback image with
  `sleep infinity` as PID 1 and no mounts. Each started six orphaned children:
  six zombies remained without init and none with `--init` (PID 1
  `.cz-init`). Both containers were removed automatically. The snapshot
  contained the daemon identity, Multica configuration, SSH material and the
  bridge wrapper.
- The image records Entrypoint `sleep` and Cmd `infinity`. Passing
  `sleep infinity` as the command runs `sleep sleep infinity`, and the
  container exits at once. Recreate it without a command.
- Before anything was deleted, an unstarted container created with the
  replacement flags was compared field by field with the saved
  `container inspect` output. Only the image and `useInit` differed. The
  init process, user, working directory, environment, CPU and memory, both
  mounts with their read-only flag, network and MTU, DNS, ports,
  capabilities, sysctls and platform all matched. That container and then
  the original were deleted, and the original name was recreated with
  `--init`.
- Afterwards, PID 1 was `.cz-init` and the hostname was unchanged. Six
  orphaned children in the live container left zero zombies; the old
  container had accumulated nine from earlier daemon restarts. The Multica
  daemon kept its identity and re-registered 15 runtimes with their previous
  IDs. Muster rewrote the bridge wrapper for the restarted agent's new port,
  on which the agent was listening, and the read-only mount rejected writes.
- Not done: no provider end-to-end Multica task has run since the migration.
  On an unlocked Mac, a Codex first turn waits for foreground routing or a
  lock, and neither was arranged. The rollback image is now the container's
  base image, so it must be kept while the container exists. For rollback,
  recreate the container from that image without `--init`.
