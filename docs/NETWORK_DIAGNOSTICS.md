# Relay Access Diagnostics and Dual-Transport Acceptance (2026-09-28)

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

This record distinguishes resolution, public ingress, relay registration, and real RPC. A successful individual probe does not constitute end-to-end acceptance.

**Subsequent scope correction (2026-09-28)**: The user confirmed that the domain exposes only public port 9443. In the historical checks below,
unreachable port 443 is no longer a deployment issue to fix. The latest [9443 and Key investigation](history/2026-09-28-relay-9443-key-diagnosis.md)
records that the raw UDP keepalive failure at the time was unrelated to nginx's domain certificate. This is a **diagnosis of the old implementation**, not a decision to continue using UDP without domain certificates. The root cause of intermittent production-client disconnections remains unresolved.

## New Target: Domain Certificates for Both TCP and UDP on 9443

The user subsequently explicitly required `muster.linsheng.dev:9443` to provide both TCP and UDP, selecting between them based on network conditions. The target protocol is:

| Layer | Requirement |
|---|---|
| TCP 9443 | TLS 1.3, ALPN `muster-relay/1`, with u32 big-endian length-prefixed Relay frames inside the encrypted channel |
| UDP 9443 | QUIC / TLS 1.3, the same ALPN, and Relay frames carried in QUIC DATAGRAMs |
| Public server identity | Both endpoints load the same Let's Encrypt fullchain / key covering `muster.linsheng.dev`; clients verify the hostname, validity period, and trusted chain |
| Device identity | The inner PeerLink continues to use hardware device keys, the roster, and local authorization; domain certificates grant no membership permissions |
| Resolution and routing | Shared Rust centralizes DoH; each path independently connects, registers, maintains keepalives, and measures RTT; a 3-second total deadline covers connection, registration, and the first full-frame probe; a healthy standby takes over immediately; failed paths are probed in the background with backoff; after recovery, RTT-based selection uses hysteresis |
| Writes and fallback | Path changes must not replay Agent input or other business writes; no automatic fallback to raw UDP, plaintext ws, legacy WSS, or connections without certificate verification |

The source implements `relay_tls`, `secure_relay`, dual secure endpoints, ACTIVATE, and full-frame nonce probes. Tests, signed builds, installation, and public-network acceptance remain separate states; this protocol update awaits deployment. The new source status recorded here does not mean the new version is deployed. Registration successes and RPC records below must be interpreted according to their dates and the old protocol.

### Public Deployment Constraints

- TCP and UDP listeners on the same port number do not conflict, but host firewalls and upstream NAT mappings must separately target the actual new secure endpoints. Historical internal nginx WSS success or a successful TCP three-way handshake does not prove the new TCP 9443 protocol works; UDP reachability does not prove QUIC handshakes or hostname verification succeed.
- The certificate SAN must cover the hostname, with a fullchain and matching private key. After renewal, atomically replace the files and reload or restart both endpoints together, then make fresh external handshakes against each to verify the new certificate; reloading nginx alone is insufficient.
- When only public port 9443 is open, ACME HTTP-01 or TLS-ALPN-01 cannot be moved to 9443: they require ports 80 and 443 respectively. Use DNS-01 or the existing centralized issuance / secure distribution process with least-privilege credentials. Diagnostics inspect only public certificate metadata and must not read or print private keys. See [Let's Encrypt challenge types](https://letsencrypt.org/docs/challenge-types/).
- DoH A / AAAA results must still match the actual service addresses and listening capabilities; IPv4 success does not prove IPv6 reachability. Opening 443 is not a prerequisite for this task, and old 443 failures must not remain on the repair list.
- The outer QUIC currently uses a separate socket. Its REFLECT reports only the outer NAT mapping and cannot serve as a public hole-punching candidate for the inner PeerLink; the same applies to TCP reflected addresses. Keep LAN direct connections; WAN NAT hole punching requires same-socket probing or correct binding, followed by separate acceptance testing.

### Acceptance Method and Completion Boundaries

1. **Certificates and handshakes**: From a real external network, complete SNI, ALPN, WebPKI verification, and signed registration separately over TCP / TLS and UDP / QUIC 9443. TCP may use a TLS probe supporting TLS 1.3; UDP requires a QUIC client, not ordinary UDP echo or browser HTTP/3 in place of the custom ALPN. Record SAN, issuer, validity period, and public fingerprint without outputting private keys.
2. **Negative cases**: Reject incorrect hostnames, untrusted chains, expired certificates, incorrect ALPN, and plaintext protocols. Raw UDP / ws must not automatically restore connectivity. Reject oversized TCP frames before allocation; truncated frames and disconnections must not hang the read loop indefinitely.
3. **Routing**: Use local fault injection or a dedicated test network to block UDP, TCP, and both separately, and add latency, loss, and jitter. Verify the 3-second connection/registration and keepalive-response deadlines, healthy standby takeover, failed state when both paths fail, background recovery, and stable hysteresis. Keep both paths registered and verify that initial standby registration, recovery, and keepalives do not steal the active route. Check selection every 100 ms; send authenticated ACTIVATE after a change, repeat it idempotently every 1 second, and confirm it through server REFLECT; keep retrying after queue saturation. This REFLECT does not count as probe RTT. Cover path changes with inbound-only requests, reassertion across generations, and restoration of the latest PUSH_REG after server restart.
4. **Real operations**: Run bidirectional read-only RPC with authorized production devices and record the actual path. Separately use a side-effect-free fixture to verify that switching does not resubmit business requests. Successful direct RPC does not replace relay-only acceptance; short probes do not replace sustained registration and foreground/background or network-change acceptance.
5. **Renewal and MTU**: After certificate updates, establish new TCP and QUIC connections and verify the same new certificate on both. Low-MTU and large-transfer tests must verify TCP selection when outer QUIC DATAGRAM capacity is insufficient, without silently dropping packets. After AUTH, complete bidirectional 1400-byte PROBE / PROBE_ACK frames (41 / 9-byte headers respectively, with strictly zero-filled remaining bytes) before marking the path healthy. Subsequently probe 5 seconds after each success, with a 3-second deadline for a matching-nonce ACK. Cover small packets succeeding while maximum-size frames are blackholed, incorrect nonces, incorrect lengths, and nonzero padding to prevent false health reports. Update failures must not result in plaintext listeners.

## 2026-10-08 Peer Path Check

Read-only `muster status` on all three Macs reported Mac-A ↔ Mac-B and
Mac-A ↔ Mac-C as `relay`; Mac-B ↔ Mac-C was `direct`, about 3 ms.
The roster and remote hardware check identify the mini as Mac-C (Apple M4),
not M1Mini. Candidate logs contain private LAN addresses for these Macs.
The secure carrier's separate socket has no usable public PeerLink candidate
(see Public Deployment Constraints above), so this does not establish WAN
UDP hole punching. A `relay` label describes the peer path, not whether the
outer carrier currently uses UDP/QUIC or TCP/TLS. Mac-A logs show successful
UDP registration as well as 3-second keepalive failures and TLS fallback.

Member connection direction is owned by `muster-net::Node` and the signed
roster: desktop/server targets can be dialed; phone targets are excluded
from automatic reconnect and explicit member connects. Phones can initiate
desktop/server links and receive replies. Existing links continue carrying
roster and heartbeat control traffic. Phones reject application/forwarding
services even if a handler was registered. Initial invite pairing remains
available; member pinning cannot bypass the connection policy.

Repeat the transport regression with:

```sh
cd src/muster
cargo test -p muster-net --test link
```

This source change does not add WAN NAT traversal or establish deployment
on the Macs/phones. Repeat read-only CLI status checks after installation;
test physical-phone requests and desktop replies separately.

## Historically Confirmed Findings

- Mac-C's system resolver returned `198.18.2.71`; Muster's DoH returned
  `203.0.113.10` and `2001:db8::10`.
  DoH was deployed on all three Macs and the iPhone; real-address resolution and relay registration were observed on each.
  Mac-C was restored from a temporary fixed IPv4 address to the `muster.linsheng.dev:9443` configuration.
- All three Macs timed out during TLS when connecting directly to `203.0.113.10:443`; TLS 1.2 controls also timed out.
  On the server, access to `127.0.0.1:443` with hostname verification retained and a correct WebSocket handshake returned 101.
  Ordinary HTTP/HEAD returned 502 because the WebSocket upstream did not accept those requests; that alone does not establish an nginx failure.
- When the server accessed its own public IPv4:443, the certificate name was `192.0.2.2`, which did not match the domain.
  This establishes that the hairpin path differed from the local nginx ingress; whether public port mapping, the router management endpoint,
  or NAT hairpin behavior caused it remains to be checked. It does not establish that every external request reached the same wrong endpoint.
- The published DNS AAAA was absent from the relay host's current global IPv6 addresses; nginx and the UDP relay
  listened only on IPv4. That AAAA cannot be treated as a verified working IPv6 relay endpoint.
- The relay service had run continuously since September 26 without a systemd restart. Recent logs showed zero authentication failures and rate-limit events,
  but iPhone/Mac-A registrations expired and some forwarded traffic was dropped; real RPC also encountered 3-second connection timeouts.
- On the same Mac-A, an independent temporary-identity UDP probe registered in 37 ms, with three keepalives at 12/17/12 ms;
  an empty-roster control using the production `Node` remained Registered for 40 seconds. Passing a short probe does not establish
  stability of the production client with real peer connections, App lifecycle, and platform tasks.

## Reproducible Historical Diagnostics

The commands below target the protocol and tools in use at the time. The new protocol requires matching updated probes; old probe results must not be recorded as TLS / QUIC dual-transport acceptance. Run in `src/muster`:

```sh
cargo run -p muster-net --example doh_probe -- muster.linsheng.dev
cargo run -p muster-net --example relay_probe -- muster.linsheng.dev
cargo run -p muster-net --example relay_probe -- muster.linsheng.dev --node
```

`relay_probe` generates only temporary in-memory keys and creates diagnostic relay registrations that expire naturally. It joins no
membership network and reads no production keys or pairing data. By default it checks three 10-second keepalives; `--node` exercises the actual
client state machine for 40 seconds. Neither changes system DNS, proxies, routers, or server configuration.

## Source Fixes and Reproducible Regressions in This Round

- Direct QUIC discovers an MTU of 1452 bytes, but the relay's 1400-byte frames also require
  34 bytes of encapsulation. Oversized packets were dropped when switching from direct to relay. Path MTU discovery is now capped at
  1366. The regression first transfers 256 KiB so direct-path discovery completes, then switches the same connection to relay and verifies
  256 KiB in both directions. The old code measured `current_mtu=1452` and failed this regression; it passes after the fix.
- Real iPhone logs at `2026-09-28T03:57:20Z` showed Quinn's
  `I/O error: Socket is not connected (os error 57)`. Only `network changed` fired afterward,
  which does not restart an exited endpoint driver. Shared Rust's single accept loop now
  rebuilds the entire UDP socket/QUIC endpoint, retaining the Node handle, identity, roster, handlers, and latest push token.
  Failed rebuilds back off; public network operations return unavailable during reconstruction; intentional shutdown does not revive the endpoint.
- Old connections, streams, and pollers may retain socket references indefinitely, so retirement explicitly releases the UDP file descriptor
  and cancels background WebSocket tasks. Connection adoption and pending pairing approvals are checked against the transport generation
  to prevent old handshakes or late approvals from contaminating the new endpoint. Normal shutdown closes first, waits at most 3 seconds for idle, then retires,
  avoiding an indefinite `wait_idle` after Quinn driver failure.
- Fault-injection regressions use real UDP/QUIC/relay: inject ENOTCONN, occupy the target port to make rebinding fail,
  release it, rebuild on the original port, and verify bidirectional RPC; retain old connection/endpoint handles to verify the port can still be released.
  Additional cases cover canceling rebuild on shutdown, rejecting old pairing approvals, and exiting WS tasks while socket handles remain held.
  Remote old connections may need the 3-second idle timeout; existing duplicate rules reject new connections during that interval.
  This arbitration behavior remains unchanged in this round; tests wait for both endpoints to converge before verifying RPC.

## Additional Production-Client Observations

The original Mac-A process's relay-reflected address changed at `2026-09-28T03:59:43Z` from
`198.51.100.37:15776` to `198.51.100.24:47547`; keepalives and real Mac-to-Mac info RPC
then recovered. No proxy, system DNS, or routing configuration changed during this process. This supports a change in the external egress path,
but does not establish a particular proxy component as the cause. Independent UDP, mixed direct/relay, and multinode QUIC
controls also passed, but do not replace production-client acceptance across all four devices.

The iPhone's CoreDevice connection subsequently became unavailable; installation and device acceptance of the recovery patch above were not yet complete.
Source regressions, signed artifacts, installation on each device, and real RPC must be recorded separately.

## Unresolved Issues

Public port 443 is outside this service's scope. AAAA deployment consistency and intermittent production-client registration expiry still lack verified root causes; the new secure TCP / UDP 9443 transports require the separate acceptance method above.
DoH removes dependence on system DNS/Fake-IP; it does not solve port mapping, TLS path, or UDP lifecycle issues.

## Delivery Verification in This Round

- `a473a38` contains endpoint recovery, the MTU cap, and generation/resource cleanup fixes; `c009fa5` contains
  the Session Return Locked Use trigger fix. Network and relay regressions: 31 passed, 1 existing
  root/dual-namespace test skipped. Consumer regressions: 15 passed; Core: 45 passed; plugin installation policy: 2
  passed. Clippy reported no warnings for all targets in the selected crates.
- macOS and iOS Release builds and signature verification passed; no notarization was performed this round. Mac-A, Mac-C, and mac-b
  received the same version, with daemon SHA-256
  `3ecd51f4759ff4d0cf3c3d339bcbcc578b2bc43d9ab7c5e0dc5241cfc3233b92`，
  on all three. Pairing and data were retained, and the high-frequency diagnostic-log version was replaced.
- Three rounds of `muster --target DEVICE_ID info` across all six directions between three Macs, 20 seconds apart,
  succeeded 18/18 times with correct responding device IDs. Observed state: relay registered, Mac peers direct.
  This verifies production direct RPC and registration observations, not public-relay bulk throughput or long-term stability.
- The iPhone remained CoreDevice unavailable. The new version was built and signed but not installed, and foreground/background or network-change
  recovery and four-device RPC acceptance were not complete. The AAAA issue remained unresolved as above; 443 is not a current service requirement.

### Follow-up: Actual Installation and RPC After the iPhone Reconnected

After the user confirmed the iPhone was ready, CoreDevice changed from unavailable to available (paired).
The previous Release iOS artifact was rigorously verified, installed, and launched with bundle id `dev.linsheng.muster`.
The device identity remained `958afe55`, with pairing data retained. Logs at 12:42 confirmed DoH resolution to the relay IPv4 address,
followed by registered state.

From approximately `2026-09-28T04:45:04Z` to `04:46:57Z`, real DeviceInfo RPC from the three Macs to the iPhone
succeeded 9/9 times across three rounds, with 0 errors over an observation window of at least 105 seconds. The phone and Macs' relays
were registered in each round, and all phone links were direct. Final-round CLI times were Mac-A 53 ms, Mac-C 170 ms,
and mac-b 287 ms (the two remote measurements include SSH). Raw second- and third-round outputs are at
`/private/tmp/muster-phone-rpc-acceptance.jsonl`。

This completes iOS installation and RPC verification during a normal operating window for this round. No real-device ENOTCONN was injected,
and foreground/background or network-change recovery acceptance remains incomplete. The AAAA issue remains unresolved; 443 is not a current service requirement.
