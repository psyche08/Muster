# ADR 0060: Recover native Multica runtimes across release upgrades

- Date: 2026-10-29
- Status: Proposed; transport recovery installed, Codex creation acceptance blocked
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

Device acceptance also exposed an incorrect terminal notification: the Codex
adapter emitted a `final_answer` item even when native submission was rejected.
One scheduler run recorded completion with no reply despite a rejected local
operation. Emit a final answer only after EndTurn; failed turns emit a terminal
error notification before their failed completion. Cancellation has no final
answer. Do not depend on the consumer reconciling a misleading success signal
with a later error. Test both immediate submission rejection and failure after
partial output, as well as cancellation and successful completion.

Keep process identity and HID generation checks unchanged, but distinguish
their rejection reasons so operator interference is not mistaken for a process
restart. Never retry a rejected/ambiguous native submission automatically.

## Consequences

Native routing remains Rust-owned. Persistent wrappers must be reconciled when
the daemon restarts; provider availability must not be inferred from the
scheduler heartbeat alone. Live desktop acceptance requires an available,
unlocked console and dedicated test conversations.

## Verification

Source, package, installation and runtime evidence are recorded separately
below. Codex creation and repeated-directory receipts remain a landing gate.

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
- Agent service integration passed 15 tests with one environment case ignored.
- The terminal-notification correction passed all 22 ACP library tests,
  including immediate rejection, partial-output failure, cancellation and
  success. Strict Clippy passed for the ACP/platform libraries and tests.
- ADR, whitespace and public-content checks passed (735 tracked text files).

### Build and targeted installation

Candidate `4d5abcd2` was built and signed as desktop 1.1.3 (93). Notarization
`ee78d587-2fb1-4305-8f0c-9b66efea4ccb` was Accepted; stapling, Gatekeeper,
signature, Presence and release-provenance checks passed. The first package's
SHA-256 was `1339390ae82dec58168b1a4cd890596b8b691dae8384ffdf773cb429a29d3a0f`.

The follow-up `cf5bb557` includes corrected terminal notifications and distinct
process/input rejection messages. It was rebuilt and signed with the same
unpublished candidate version. Notarization
`3109eaf6-e8b1-4d30-84a0-59b7079a512e` was Accepted with the same package gates.
The replacement package's SHA-256 is
`11d0335fbb787192d142d059f530f9c07d24d751fd312de85da2daaf5f759ce9`.
Both installations required an idle scheduler and Rust desktop update lease.
The designated desktop now reports `cf5bb557`; identity and fingerprint were
preserved, Accessibility and Relay recovered, and the native bridge restarted
with reconciled wrappers. No Keychain unlock operation was used.

All three Apple FFI targets built. This was a targeted desktop repair: no
public macOS or iOS feed, GitHub Release or TestFlight distribution changed.

### Runtime and provider acceptance

- Installed initialization completed in 93 ms. A deliberately oversized
  synthetic request was rejected by daemon validation before any AX action;
  the actual protocol emitted `turn/started`, terminal `error`, and failed
  `turn/completed`, no final answer, then exited cleanly when stdin closed.
- A dedicated Claude scheduler task returned its exact synthetic marker.
  This is an actual native provider reply, independently of initialization.
- Six independent Codex tasks produced no accepted provider reply. Three
  stopped on process/input protection; contemporaneous HID counters confirmed
  mouse/keyboard activity. Three stopped before directory navigation because
  the new window's composer was not published within 25 seconds, including
  two cases in the operator-confirmed quiet interval.
- One rejected task was incorrectly marked completed by the scheduler with
  no text on the first candidate. This prompted `cf5bb557`; it is explicitly
  excluded from successful acceptance. No failed/ambiguous task was replayed.
- Read-only AX observation found the fresh default ChatGPT shell at 17 nodes
  with no composer while the app's own main-frame/ready-to-show logs reported
  about 0.5 seconds. A read-only hit test did not resolve the missing form.
  Installed application code queues navigation messages until web contents
  are ready. Whether allowing earlier directory navigation resolves this
  mismatch is an unverified hypothesis, not an accepted change to the guard.
- A separate directory-only calibration script is prepared but has not run;
  it awaits a new quiet interval after the agreed five-minute window ended.
  Existing drafts/windows are preserved. Failed test issues were cancelled
  with no new run; the verified Claude issue was marked done. Dedicated test
  agents and local evidence remain available for continued acceptance.

PR 60 remains a draft and is not landed. Codex creation, two same-basename
directory receipts and final test-artifact cleanup remain unresolved. The
installed transport/error correction is not a claim of complete Codex recovery.
