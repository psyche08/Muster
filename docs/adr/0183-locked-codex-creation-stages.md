# ADR 0183: Route a covered Codex creation before its first composer

- Date: 2026-11-11
- Status: Accepted; remote and local scheduler creation verified on a locked Mac
- Related: [0150](0150-remote-desktop-session-creation.md), [0154](0154-wait-for-slow-codex-windows.md), [0181](0181-native-create-model-and-effort.md)

## Context

With both Macs running 1.1.36, a member request and a local scheduler request
each opened a new Codex window and expired before submitting the first
message. A Release build with content-free stage logging identified the wait.
At 2.58 s the new window had already been identified and raised. The worker
waited for that window's initial composer until the 15 s Locked Use lease
expired. It never dispatched the directory route. Sampling the daemon could
itself interrupt the owner's heartbeat, so the timings below use no sampler.

## Decision

Only the caller that already owns a covered operation selects the fast path.
The member's native creation and the scheduler's locked execution share it.
After requesting one new window, retain the exact new AX element and raise it.
Dispatch the directory-only route once that element is still in the app's
window list, is its focused window, and exposes one Codex document. Do not
wait for the editor of the unrouted page. Unlocked native creation, including
the scheduler on an unlocked console, retains ADR 0154's composer wait.

Navigation cannot send the first message. Writing and sending still require
the retained window and routed form, the exact local project and canonical
directory, the expected composer value, and the existing input and delivery
guards. A missing window, failed route or expired lease is never a reason to
send again. The cover, heartbeat, lease duration and release-after-relock
rules are unchanged. Log elapsed monotonic time and fixed stage names only;
omit prompts, paths, window titles and AX values.

## Alternatives

Increasing the watchdog deadline would extend the unlock without making the
unrouted composer a useful prerequisite. Reusing an existing window could
displace a person's session. Sending the prompt in a URL would bypass the
form and draft readback. None is necessary.

## Consequences

The new window can initialize the requested project while its editor is
starting. A slow provider or user takeover can still stop an operation. This
is not an acceptance of every model, provider version, machine or error path.

## Verification

Two Macs first ran the identical notarized 1.1.36 daemon. For the test the
requester kept that binary; only the executor needed the notarized candidate
from commit `c1e87209`. Its provider was ChatGPT 26.1103.60933 in Codex mode.
The two synthetic tasks each used a new temporary directory and requested no
model or effort override.

| Source on the locked executor | Directory route | Form verified | Submit |
| --- | ---: | ---: | ---: |
| Paired Mac, member RPC | 2.86 s | 7.62 s | 7.92 s |
| Local scheduler, task-scoped grant | 3.20 s | 8.32 s | 8.61 s |

Each returned an applied receipt for a distinct session, with exactly one
matching session in its directory, one matching first user message (the
transcript adds a trailing newline) and the assistant's correct arithmetic
answer. Independent device-state reads observed locked → unlocked → locked
for each run. Both answers were read back after relock. The scheduler ledger
recorded the same session as applied and its test grant was revoked. These
tests exercised the scheduler RPC, not an external scheduler's own job queue.

The native platform unit suite passed (46 tests), as did the daemon suite
(143 passed, three ignored). Developer ID signing, Apple notarization,
stapling, Gatekeeper and the daemon source stamp were verified. No public
update or mobile distribution was part of this test.
