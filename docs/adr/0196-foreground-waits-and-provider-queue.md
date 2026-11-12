# ADR 0196: Keep foreground handoffs out of occupied provider queues

- Date: 2026-11-12
- Status: Accepted
- Related: Architecture §3.6.2; supersedes the in-queue foreground wait in [0190](0190-background-desktop-admission.md); [0046](0046-send-recovery-and-quiet-admission.md)

## Context

An uncovered request could spend 90 seconds waiting for a human to bring a
provider to the foreground, while owning the same provider queue that other
requests could wait only 30 seconds to acquire. Target diagnostics recorded
both timeouts. Counting ledger entries alone hides preparation failures;
counting every log line as a message overcounts retries of the same operation.

## Decision

Check for the exact visible session or eligible background creation form first.
If foreground navigation is necessary but unavailable, an operation already
owning the provider returns a conflict instead of waiting for a human. Keep
the managed creation's cancellable admission wait outside the provider queue.
Retain covered startup, input quiet, destination, draft and receipt checks.
Recheck foreground immediately before a navigation action. Never submit or
replay a request on a foreground change. Distinguish a locked, inactive or
unobservable console from a changed or unobservable process identity in
selection errors. Earlier logs cannot distinguish these causes.

## Alternatives

Increasing the queue timeout also increases latency for requests that could
already run. Activating another application could redirect local keystrokes.
Parking all callers for a later handoff requires carrying live caller and
console ownership across that wait; interactive requests instead receive a
specific refusal and can be initiated again when the desktop is ready.

## Consequences

A ready session can use the provider after a blocked request returns. A direct
request for a hidden session no longer completes automatically if the user
later foregrounds the app. Genuine drafts, physical input and an unavailable
console can still preclude delivery. Provider receipts do not measure time
to a model answer.

## Verification

Exercise hidden and visible sessions with another app in front, exact and
sidebar routes, covered and managed creation, input races and receipt
regressions. Repeat measurements on installed builds before claiming a
fleet success-rate or latency improvement.

Local verification: the native Rust selection/runtime filter passed 119 tests
(one existing ignored test); the daemon receipt integration test passed with
loopback enabled. The updated macOS adapter compiled as part of that test.
No signed build, install or physical-device run was performed for this decision.
