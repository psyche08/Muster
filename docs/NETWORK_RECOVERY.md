# Network recovery and Vnet ownership

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

See [ADR 0004](adr/0004-transport-recovery-and-stable-vnet-owner.md) for the
incident evidence, decision and acceptance limits.

## Ownership and recovery

`musterd` retains one `Arc<Node>`. Both automatic repair and `device_set_relay`
replace only its transport generation through the Node's accept/recovery task.
Vnet retains the same Node, datagram receiver, packet loops, flow state, TUN
and (on macOS) NetHelper connection. Service/roster subscriptions are not wired
again. A relay edit does not create another virtual interface.

The recovery owner responds to endpoint receive-driver termination, structural
local socket errors, or two missed local UDP receive challenges. A random
challenge goes to the socket's own local address every five seconds. It never
uses a remote peer's response to decide global health. Ordinary peer absence,
route errors, path MTU errors and relay carrier outages remain separate events.
The receive loop yields after 64 control frames so a flood cannot monopolize it.

Repair releases the retired socket even if callers retain old Quinn handles,
rejects late handshakes from old generations, and retries construction with
1, 2, 4, 8, 16, 32, then 60-second delays. A generation must remain alive for
60 seconds before that backoff resets. Network-change notifications cannot
skip the deadline; shutdown interrupts it. There is one repair owner and one
latched failure per socket, with no accumulating repair jobs. Persistent local
failure remains degraded with at most one reconstruction per minute.
Application requests are never replayed by transport recovery.

Vnet initialization is serialized. The macOS helper's blocking XPC open has a
three-second observation deadline; a timed-out request is retained and reused,
so another call cannot accumulate duplicate opens. An eventual result owns
its descriptor even after cancellation. Vnet publishes only if the daemon is
still running and its current signed address plan matches the interface.
Shutdown aborts/joins both packet tasks before releasing the descriptor/helper.
Signed address renumbering still requires the documented daemon restart; this
change does not silently renumber a live interface.

## Reading the logs

| Evidence | What it establishes |
| --- | --- |
| `relay ... authenticated` | One relay carrier registered; no proof of device RPC or Vnet |
| `presence ... Online`, `accepted ...` | Peer link evidence; not an OS virtual-interface test |
| `local UDP send/receive/writable: ...` | A structural local socket failure, with syscall phase and errno |
| `local UDP receive probe missed (n/2)` | Local receive-pump challenge failed, independent of remote peers |
| `transport generation ... failed/requested` | Repair cause and delay, including explicit relay edits |
| `transport generation ... rebuilt` | New local transport exists; relay and peer convergence are still pending |
| `Vnet datagram binding retained` | Same Vnet owner/channel; use packet checks to establish data delivery |
| `vnet helper open pending after 3s` | One outstanding XPC request; no duplicate open is launched |

A generic `quinn::endpoint I/O error: not connected` can also be emitted while
an intentionally retired relay endpoint releases its socket. Correlate it with
carrier and transport-generation logs before assigning it to the peer endpoint.

## Pre-release verification on 2026-10-21

Fault-injection tests use real Quinn endpoints, loopback relay servers, and
paired Unix datagram descriptors that exercise the macOS utun header adapter.
They cover receive errors, send-only errors, a silently stalled receive pump,
bind failure, shutdown during reconfiguration, retained old handles, repeated
recovery, same-relay and different-relay replacement, bidirectional RPC/IP
packets, failed duplicate Vnet attachment, and task/fd cleanup. An absent peer
and EHOSTUNREACH are observed across three local-probe intervals without any
transport reconstruction. No production fault was injected.

| Host | Installed version during baseline | RPC to both other Macs | Virtual IP ICMP to both other Macs | Isolated new-code recovery suite |
| --- | --- | --- | --- | --- |
| Mac-A | 1.0.70 | Passed | Passed | 8 passed |
| Mac-C | 1.0.73 | Passed | Passed | 8 passed |
| Mac-B | 1.0.69 | Passed | Passed | 8 passed |

Mac-A and Mac-C also logged in to Mac-B over `192.168.13.12` SSH. SSH to Mac-C
by bare IP reached authentication but rejected the offered key; SSH to Mac-A
reported connection refused. Those are not Vnet packet-loss evidence: the
independent six-direction virtual-IP ICMP checks passed.

The isolated tests use temporary ports, soft test identities and mock TUNs;
they do not exercise the installed NetHelper daemon or replace the running
application. The production baseline remains evidence for the old installed
builds only. Signing/notarization, publication, installation of this patch,
real NetHelper fault injection and post-install three-device acceptance have
not been performed. No existing terminal, native chat or daemon was restarted.

Source verification: 125 full-suite tests plus the delayed-helper-open/shutdown
regression passed (126 total; two existing live-environment tests ignored).
Strict Clippy, layer checks and the ADR gate passed. The committed source
`771564bd` produced a macOS Release daemon with SEP enabled; shared networking
also passed the iOS target check. Neither result is an app signing or deployment
acceptance result.


## Installed 1.0.74 (65) acceptance on 2026-10-21

The earlier baseline is superseded for installation status by this acceptance.
All three Macs now run the same notarized build from
`210013f54ad087ace876bba75694c1992847c8de`, containing PR #158. Device identities
were preserved. Signatures, stapled notarization, Gatekeeper, embedded commit
and daemon SHA-256 were independently verified on each installed bundle.

| Installed host | RPC to both other Macs | Real virtual IP ICMP to both other Macs | Runtime |
| --- | --- | --- | --- |
| Mac-A 1.0.74 (65) | Passed | Passed | Relay registered, Vnet active |
| Mac-C 1.0.74 (65) | Passed | Passed | Relay registered, Vnet active |
| Mac-B 1.0.74 (65) | Passed | Passed | Relay registered, Vnet active |

### Mac-B bounded I/O interruption

After acquiring the Rust desktop-update lease, only musterd was paused for
25 seconds (10:07:39–10:08:04 CST). An independent 35-second watchdog and the
parent's finally path could resume the same PID/start identity; neither
restarted the service. During the pause, a client RPC failed and virtual-IP
ICMP lost both packets. On resume, both peers returned Online at 10:08:04 and
relay QUIC reauthenticated at 10:08:05; TLS carried traffic in the meantime.
The subsequent six-direction RPC/ICMP matrix passed without a daemon restart.
PID 58399 and its 10:04:55 start time were unchanged through both experiments.

The management SSH connection also closed during this experiment. The remote
script did resume the process at the planned 25 seconds and persist its event
record, but its later stdout write failed before its own post-checks. Recovery
was therefore checked independently from fresh connections and device logs,
not inferred from the interrupted driver's exit status. Subsequent verification
used a detached driver with results written on the device.

This establishes recovery from a process network-I/O availability interruption.
It is not a recreation of the unproven the adjacent network extension/kernel trigger, nor an installed
socket-error injection: this pause caused peer/relay reconnect, not a global
transport-generation rebuild. Structural socket failures and a stalled receive
pump remain covered by the separate isolated fault-injection suite.

### Two consecutive transport replacements with real NetHelper/TUN

Under another update lease, two same-value relay changes rebuilt generations
1 → 2 → 3 at 10:08:54 and 10:09:03 CST. The logged delays were one and two
seconds; there was no concurrent duplicate repair. After each replacement,
Mac-B RPC and virtual-IP ICMP to both peers passed. The final independent
six-direction matrix also passed, and Mac-A/Mac-C logged in to Mac-B over
`192.168.13.12` SSH.

The daemon PID/start time, `utun7`, address `192.168.13.12/24`, MTU 1150 and one
`com.apple.net.utun_control` descriptor (unit 8) were retained. Logs showed
`Vnet datagram binding retained`; no second Vnet/helper interface was created.
This is actual installed NetHelper/TUN evidence, separate from mock-TUN tests.
General task/fd leak guarantees still come from regression tests; two live
replacements do not establish indefinite resource stability.

Mac-B's former bridge IP `192.0.2.3` was unaddressed at acceptance time.
Management used its reachable `192.0.2.235` address; no interface, the adjacent network extension or
security-component configuration was changed. Bare-IP SSH to Mac-A still
refuses connections and to Mac-C rejects the default key, while virtual ICMP
passes. SSH availability is not treated as equivalent to Vnet health.

Evidence: `build/releases/Apple-1.0.74-65/` contains installed verification,
`pause-client-probe.json`, `mac-b-pause.json`, `mac-b-rebuild.json`, correlated
logs and the independent before/after matrices. Public OTA was verified;
physical iPhone installation, native chat sends and the original kernel fault
were not exercised.
