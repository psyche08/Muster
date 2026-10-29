# ADR 0060: Recover native Multica runtimes across release upgrades

- Date: 2026-10-29
- Status: Proposed; implementation and device acceptance pending
- Related: ADR 0053; native runtime candidate PR 52; ADR 0059

## Context

A previously deployed native-runtime candidate left provider wrappers and
runtime registrations on a desktop. A subsequent mainline installation lacked
that implementation and ignored its runtime_bridge configuration field. The
scheduler continued advertising Codex online through daemon heartbeats while
the wrapper referenced an absent ephemeral bridge listener. One task failed
with connection refused and another remained in initialization until the
scheduler's thirty-minute idle watchdog. The container also lacked Git.

## Decision

Integrate the existing Shared Rust native runtime implementation with current
mainline fixes rather than restoring an older application or silently switching
the selected desktop task to CLI. Preserve native identity, directory, draft,
console, credential and receipt guards. Restore bridge supervision and refresh
provider wrappers against the current listener. Fail unavailable launch paths
promptly, and validate the provider protocol independently of daemon liveness.
Keep existing storage, identities and login state. Repair the missing container
Git dependency without changing the selected runtime or replaying failed tasks.

Target only the affected desktop for installation. Public macOS/iOS releases
remain unchanged. Record any uncompleted provider acceptance before landing;
source checks or an initialize reply alone are not an end-to-end task receipt.

## Alternatives

Changing the wrapper to the legacy ACP listener does not implement Codex
app-server framing. Extending the idle watchdog delays the same failure.
Restoring the old candidate would discard later mainline correctness fixes.
Disabling the selected runtime would prevent misleading availability but would
not restore the requested native execution capability.

The inherited shell framing wrapper also used a foreground `header + cat |
socat` pipeline. When the transport fails or reaches EOF while the scheduler
keeps stdin open, the producer can keep the wrapper alive. Own the FIFO,
transport and stdin relay explicitly, bound TCP connect to five seconds and
reap the relay when transport exits. Do not impose a global idle timeout on
legitimate provider work. These are byte transport/process cleanup mechanics;
provider routing, authorization and framing validation remain in Shared Rust.
Existing-container Install must provision Git, SSH and socat as new Install does.
Add a release fingerprint for the native bridge so future packages cannot
silently omit it again. Preserve the candidate ADR history under 0061–0063 to
avoid collisions with mainline 0055–0057.

## Consequences

Native routing remains Rust-owned. Persistent wrappers must be reconciled when
the daemon restarts; provider availability must not be inferred from the
scheduler heartbeat alone. Live desktop acceptance requires an available,
unlocked console and dedicated test conversations.

## Verification

Pending: merge review, regression tests, release provenance, signed build,
target installation, restart recovery, bounded protocol initialization and
actual native task receipts. Do not report these as completed until observed.

### Source verification

- The integrated ACP, agent, platform and daemon library suites passed 385 tests
  with six environment/operator cases ignored (excluding a nested-test duplicate).
- After adding the saved-config regression, the Multica subset passed 12 tests
  with one manual case ignored. Strict Clippy passed for the affected libraries
  and tests.
- The old framing script reproduced an open-stdin hang after peer EOF. Both
  connection-refusal and EOF tests pass with the new process cleanup.
- The target container's missing Git and SSH client packages were installed;
  Git reports 2.39.5. No task was replayed and no provider receipt is claimed.
- Build, signing, installation, restart and provider acceptance remain pending.
