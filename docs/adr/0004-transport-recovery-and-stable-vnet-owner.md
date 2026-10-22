# ADR 0004: Recover transports under a stable Node and Vnet owner

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](../PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

- Date: 2026-10-21
- Status: Accepted
- Related: ARCHITECTURE §3.5; ADR 0002 (relay carrier probes)

## Context

Mac-B 1.0.69 reported QUIC `I/O error: not connected` on October 20 at
21:43 CST. Relay authentication later succeeded while peer RPC timed out.
Replacing Node restored RPC but not virtual-IP SSH; restarting musterd at
08:42 CST on October 21 restored both. the adjacent network extension reconfiguration is only a temporal
correlation. The historical log does not identify the failing syscall or endpoint. A fresh
read of Mac-B logs places the quinn::endpoint error immediately after relay QUIC
closure and TLS selection; relay endpoints also emit this on intentional
retirement. It cannot establish that the device endpoint died. Send-error and
stalled-pump tests reproduce recovery gaps, not the historical external trigger.

Current Quinn propagates receive failures through EndpointDriver, whose Drop
wakes accept. In contrast, connection send/writable failures can end only a
ConnectionDriver. MagicSock also falls back to relay after direct send errors.
Therefore waiting exclusively for accept(None) misses local socket failures
reported on the send path. Existing tests injected receive failures only.

## Decision

PlainUdp latches structural local socket failures (ENOTCONN, EBADF, ENOTSOCK)
from send/writable operations independently of Quinn. Node's single recovery
owner selects this signal alongside accept completion. Relay unavailability,
peer timeouts, ICMP errors and path MTU failures do not raise this signal.
A random local UDP challenge every five seconds checks the actual receive pump.
Two consecutive misses trigger repair even if Quinn returns neither data nor
errors. This adds 0.2 local packets/second and is independent of peer presence.
The control-frame receive loop yields after 64 packets to avoid starvation.
Recovery retires the complete socket/endpoint generation and uses exponential,
capped backoff, including when newly created generations fail immediately.
Network-change notifications must not bypass the minimum rebuild interval.

Keep Node identity, service subscriptions and its datagram channel stable.
Changing relay requests a transport generation replacement through the same
recovery owner, instead of replacing Arc<Node>. Vnet retains its Node, packet
loops, flow filter, TUN descriptor and NetHelper connection throughout recovery.
No second reader, TUN or XPC helper is created. Reconfiguration is serialized;
shutdown cancels pending recovery and cannot publish a replacement afterward.
Vnet startup is serialized and publication checks shutdown/address state after
asynchronous helper acquisition. A helper open is observed with a three-second
deadline but retained across timeouts, so retries cannot spawn duplicate XPC
opens; late results own/drop their descriptor. Shutdown awaits aborted packet tasks before
releasing the helper when possible.

## Alternatives

Service restart hides the defect and interrupts unrelated work. Peer timeout
thresholds mistake offline peers for global failure. Reopening Vnet on every
transport replacement introduces duplicate routes and helper/descriptor races;
a stable Rust owner avoids that lifecycle transition entirely.

## Consequences

A rebuild closes failed transport connections but never replays application
mutations. Relay registration, peer convergence and retained Vnet binding are
separate diagnostic events, not interchangeable proof of recovery. Existing
kernel flows can resume when peers reconnect, subject to their own timeouts.
A persistently broken local socket remains degraded with bounded retry rate.

## Verification

Implemented: fault-injected send and receive failures, repeated generations, bind
failure/shutdown races, ordinary peer/relay failures without global recovery,
and actual QUIC DATAGRAM traffic through paired mock utun descriptors in both
directions after automatic and explicit replacement. Check task/fd cleanup.
The eight recovery regressions passed on Mac-A, Mac-C and Mac-B in isolated
processes. Different-relay replacement is additionally checked using a second
real relay and the same Vnet descriptors. Full suite, build and gate results are
recorded below when complete. An existing presence test counted an in-flight
cancelled handshake tail packet as a fresh dial; it now waits for a bounded
quiet window before asserting no further dialing, without relaxing the verdict.

See [NETWORK_RECOVERY.md](../NETWORK_RECOVERY.md) for the separate six-direction
RPC/VIP baseline matrix and the precise installed versions. These are old-build
baseline checks, not acceptance of a new installation. No production service
restart, release, signing/notarization or new installation was performed.

### Completed source verification

- `cargo test -p muster-net -p musterd`: 125 passed, 2 existing live-environment
  tests ignored. The added delayed-helper-open/shutdown test separately passed
  (126 tests total). It reuses one injected pending job through two observation
  timeouts, races a final attempt with shutdown, and verifies no Vnet/helper is
  published; it never opens a real system interface.
- The eight transport/Vnet recovery tests passed independently on all three
  Macs. The additional different-relay case also passed on all three.
- Strict Clippy (`-p muster-net -p musterd --all-targets -- -D warnings`),
  16-crate layer check and whitespace checks passed.
- `cargo check --target aarch64-apple-ios -p muster-net --no-default-features`
  passed. This is shared Rust target validation, not an iOS app archive.
- macOS Release `musterd --features sep` built; a final build from the source
  commit and the ADR gate are recorded in the follow-up evidence commit.

### Final build and gate

- Source commit: `771564bd068ac65fe43ec3c6c1505a4a1e5fe63c`.
- Clean-source macOS `cargo build --locked --release -p musterd --features sep`
  succeeded. Binary SHA-256:
  `92d67ba9867b9ab19fa84bfc511461a98b9f9aac096f2b9d322fdbc0074647a6`.
- `scripts/check-adr.sh origin/main` passed for ADR 0004. Final Clippy, layer
  checks and whitespace checks also passed. No app signing/notarization or
  installation is implied by the executable build.


### 1.0.74 (65) release and installed-device verification plan

Publish the existing recovery decision as macOS and iOS 1.0.74 (65). Build from
this clean source commit, notarize/staple the macOS portable bundle, publish both
platforms to the documented server and verify public artifact hashes/versions.
Install on Mac-A, Mac-C and Mac-B using the Rust update lease; preserve device
identity and application state. Do not bypass an active protected session.

On Mac-B, separately verify bounded loss of daemon network I/O without service
restart, reconnection, explicit same-relay transport replacement, RPC and real
virtual-interface traffic. A temporary process I/O pause is a controlled
availability fault, not a reproduction of the unproven the adjacent network extension/kernel trigger.
Actual socket-error and stalled-receive-pump injection remains the isolated
regression suite; do not add a production fault-injection backdoor. Keep a
bounded independent resume watchdog for any process pause. Record installed
acceptance separately from mock-TUN/source tests and public OTA availability.


### Completed 1.0.74 (65) release acceptance

Both Apple builds succeeded from clean commit `210013f54ad087ace876bba75694c1992847c8de`.
Apple accepted notarization `1073b2bd-86bb-414a-826a-bde519d3ffbd`; the installed
bundles on all three Macs passed signature, staple and Gatekeeper checks.
Public macOS/iOS downloads matched local hashes and version/build metadata.
See [NETWORK_RECOVERY.md](../NETWORK_RECOVERY.md) for installed-device evidence:
all six RPC and virtual-IP paths passed after a bounded Mac-B I/O pause, then
again after two real transport replacements retaining the same NetHelper/TUN.
Neither experiment restarted musterd. The pause is peer-reconnect evidence;
actual socket-failure auto-rebuild remains separately covered by isolated tests.

Deployment exposed a foreground-dependent startup on Mac-C: `open -g` left
the app running without the daemon socket; normal `open /Applications/Muster.app`
triggered the existing registration flow and restored the daemon. Correct the
installer procedure with one bounded normal-open fallback before reporting
startup failure, rather than forcing launchd or bypassing protection. This is
an installer workflow correction, not a claim that GUI background lifecycle
behavior was changed in the released binary. Do not replace an already updated
bundle again merely to complete verification.

No new networking defect was observed in these bounded installed checks. The
source-test, build, publication, installation and physical-device limitations
remain explicitly separated in the release report.

### Fault-fixture readiness follow-up (2026-10-22)

Run 39701314578 failed the first echo in the fatal-socket test, before any
fault injection or transport rebuild. The log shows both initial member
accepts; a connection can still be replaced by simultaneous-dial selection.
Five isolated Mac-A runs passed, so that does not prove the CI race absent.
Make only this pre-fault fixture readiness echo a bounded ten-second probe.
Its payload is a read-only synthetic echo, never a provider send. Keep the
post-fault bidirectional RPC assertions, generation checks, resource retirement
and stale-connection rejection unchanged and without replay. This changes test
setup, not production retry or authorization behavior.
