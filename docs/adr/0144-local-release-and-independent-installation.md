# ADR 0144: Local Apple release and independent installation jobs

- Date: 2026-11-06
- Status: Accepted; three Mac installations verified
- Related: ADR 0143; DESKTOP_UPDATES.md

## Context

The operator requests local compilation and notarization after the remote signing
key was inaccessible. Existing remote Macs can be reached through Muster, but
probed independent physical routes are unavailable. An SSH-driven installation
would lose its execution channel when the daemon stops.

## Decision

Build both Apple artifacts on the operator workstation from the clean main release
commit recorded in ADR 0143. Sign and notarize macOS using the existing local
Developer ID and Presence profile; sign iOS for registered devices. Keep the desktop feed private key on its current host and send
only the package and feed there for signing.

This supersedes only ADR 0143's physical-path-only installation decision. Prefer
an independent physical route. Otherwise stage and verify all inputs first, then
submit a one-shot job to the target user's launchd domain. The installer runs on
the target independently of SSH and records durable progress and results locally.
Confirm the job was accepted before leaving the connection. Never drive bundle
replacement directly inside a Vnet SSH session or replay a job whose result is
unknown. Reconnect for readback after the daemon returns; an unreachable target
remains unverified and requires independent recovery.

Preserve the Rust update lease, scheduler-idle, signing and identity gates.
Atomically exchange the verified candidate with the installed bundle and remove
the old program immediately; retain no program backup. This does not remove app
data. Normal Launch Services startup and bounded runtime checks remain required.

## Alternatives

Unlocking the remote Keychain automatically is prohibited; the operator selected
local signing. An SSH-bound script can be interrupted at daemon shutdown. Waiting
for a physical route is unnecessary when execution is owned by the target's
launchd, but remains required if job submission or durable logging cannot be
established. Retaining old programs conflicts with the operator's deployment rule.

## Consequences

The job survives loss of its delivery channel. A failed daemon startup can still
require physical recovery, and is not claimed as successful installation. Local
signing and remote feed signing have separate evidence; no private key is copied.

## Verification

Before execution verify exact source, signed artifacts and staged hashes. Record
launchd submission, per-host durable result, unchanged identity, installed version
and daemon health separately. Publication, iPhone installation and real-provider
latency acceptance remain separate. Execution evidence follows below.

### Execution evidence (2026-11-06)

Both remote user GUI launchd domains accepted independent one-shot jobs after
staged-package hash verification. Their durable completion records were read back
following daemon restart. The local Mac completed the same procedure. All three
jobs preserved device identity, verified the expected binary hash and main commit,
and restored relay/Vnet. Atomic replacement staged the candidate on the application
volume, avoiding cross-volume rename failures. The exchanged old program was
removed immediately. No SSH session owned the stop/replace/start sequence.

Local build, notarization, remote feed signature and public download verification
passed as recorded in ADR 0143. Physical iPhone installation and provider latency
acceptance remain pending; these are not established by launchd completion.
