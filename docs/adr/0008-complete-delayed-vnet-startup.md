# ADR 0008: Complete delayed Vnet startup independently of callers

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](../PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

- Date: 2026-10-21
- Status: Accepted
- Related: ADR 0004 (stable Node and Vnet ownership)

## Context

Mac-A 1.0.77 logged a NetHelper open timeout at 13:33:45 CST and continued
reporting Vnet down. At 14:00 the daemon already owned a utun descriptor and
its virtual route existed. The saved blocking task result had no automatic
consumer after the three-second observation deadline. This explains the stuck
startup; it does not identify why the original helper call exceeded the deadline.

## Decision

Give each daemon one tracked Vnet startup task. Concurrent callers observe the
same completion signal for at most three seconds. That deadline does not stop
the task: it awaits the one blocking XPC open and immediately completes Vnet
publication when the result arrives, including address and closing checks.
A failed attempt is logged and completes; no timer retries or service restarts
are introduced. A later explicit startup request may make a new attempt.
Shutdown closes admission, cancels and joins the startup task before releasing
Vnet and the helper. A non-cancellable XPC job owns its eventual result so late
file descriptors are dropped even after its async observer is cancelled.

## Alternatives

Polling maybe_start_vnet would add retry rate and lifetime ambiguity. Waiting
indefinitely in daemon startup would prevent RPC availability. Restarting the
service hides the missing continuation and interrupts unrelated activity.

## Consequences and verification

RPC readiness remains independent of NetHelper latency. Distinguish pending,
helper completion, Vnet publication and shutdown in logs. Inject a delayed open
and concurrent observers to verify automatic consumption without another API
call, one open, failure completion and shutdown rejection of late results.
Keep isolated tests separate from the notarized 1.0.78 (69) installation and
three-device RPC/virtual-IP acceptance. iOS publishes together; physical iPhone
installation remains a separate gate.

### Source verification

All 51 musterd library tests and strict all-target Clippy passed. The new tests
cover delayed success after the observer deadline, concurrent observers sharing
one task, delayed failure consumption without another request, and shutdown
before late success. Mock Unix datagram descriptors confirm publication and
closure; no system TUN or route is opened by the tests.

The pending diagnostic retains the historical release fingerprint while adding
the background-completion action. Release verification rejected its initial
rewording; retain the regression guard rather than bypassing it.

### Installed 1.0.78 evidence

Source `a299568898ccb9699b5edad4381f3b5f736ffdf0` produced both Apple releases.
All three Macs installed and ran the notarized build. The four helper-lifecycle
regressions also passed separately in isolated processes on Mac-C and Mac-B.

On Mac-A at 14:22:08 CST, the real helper exceeded the observation deadline:
`vnet helper open pending after 3s` was followed by `vnet helper open completed
after 3.25696575s for 192.168.13.11`, then both Vnet-up messages in the same second.
No second startup request or extra service restart was issued to finish it.
This directly exercises delayed completion in the installed app, independently
of the installation restart itself. Six directed RPC and virtual-IP ping paths
passed across all three Macs. Four SSH paths targeting Mac-C/Mac-B passed.
The two paths targeting Mac-A returned connection refused; localhost port 22
also refused and launchd reported no com.openssh.sshd service. Remote Login
configuration was not changed. These SSH paths are not reported as passed.

Mac-B's `stapler validate` online CloudKit lookup failed with DNS error -1003.
The exact publicly verified ZIP hash, strict signature and native Gatekeeper
`Notarized Developer ID` assessment passed on Mac-B before installation; the
installer retained these gates and used that native notarization assessment
for this DNS failure. It did not disable a security component or alter DNS.
Mac-A/Mac-C and public artifact staple validation passed normally.
