# Muster direct peer paths

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](../../PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

Status: implemented (2026-09-17). Off by default; `v2.vnet.direct.enabled`
turns it on per agent.

## The problem

Every packet on the muster virtual network used to ride the rendezvous:

    utun → vnet frame → yamux stream → inner TLS 1.3 → WSS → relay → …

That path is correct and it is the worst one available whenever the two
machines can reach each other without it. Two Macs on the same desk sent their
packets to a host in another country and back. The relay is also a single
point of failure for something that does not need one: a LAN keeps working
when the internet does not, but the virtual network on it did not.

## What was added

A second link per peer, carried by QUIC directly between the two agents, and a
plane that keeps both and prefers the shorter one.

| path | when | transport |
| --- | --- | --- |
| `direct-lan` | both machines share a physical network | QUIC to a private address |
| `direct-punch` | different NATs, hole punched | QUIC to a public address |
| `relay` | always available while both agents are registered | the original WSS path |

Preference is `direct-lan > direct-punch > relay`, expressed once
(`PathKind.preference`) and read by the plane, the status document and the UI,
so they cannot disagree about which link is carrying traffic.

## What did NOT change: trust

A direct path is a **route**, never a permission. Every rule that governed a
relay-borne link governs a direct one, using the same code:

- The dialer pins the peer's inner SPKI (`innerchan.ClientTLS`). A handshake
  against any other key fails, so a redirected network cannot become the peer.
- The accepter authorizes the SPKI through its own ledger
  (`innerchan.ServerTLS`) — the same callback, the same `authorized_keys`
  model. A peer that has not been authorized is refused here for the same
  reason it is refused on the relay path.
- That ledger check is re-run for the **lifetime** of every direct link
  (`DirectEngine.guardLink`), so "revocation kills live sessions within two
  seconds" stays true of the whole agent and not only of the part that goes
  through the relay.
- The receiving plane still checks every decrypted packet's source against the
  address bound to that peer's SPKI. Anti-spoofing did not move.

The relay remains untrusted. It carries **signalling only** — candidate
addresses and punch tokens, inside the existing end-to-end encrypted session —
and it carries none of the packets on a direct path.

## Mechanism

### One socket

`internal/quicpeer` binds ONE UDP socket per agent and runs a `quic.Transport`
on it that both dials and accepts. The STUN request that learns the NAT
mapping, the punch packets that open it, and the QUIC handshake that uses it
must all leave from the same source port — a NAT mapping belongs to a
(source address, source port) pair, so a second socket gets a second mapping
and the reflected address means nothing. quic-go routes datagrams whose first
two bits are clear (RFC 9443) to `ReadNonQUICPacket`, which is what makes
sharing the socket safe.

The socket is pinned to the physical interface (`IP_BOUND_IF`) on the same
terms as the relay dial, so a transparent proxy owning the default route
cannot swallow it.

### Candidates

| source | origin | works without |
| --- | --- | --- |
| `static` | `v2.vnet.peers[].endpoints` | the relay, a control channel, a reflector |
| `local` | the peer's own interface addresses, advertised over the control channel | a reflector |
| `reflexive` | the peer's NAT mapping as a STUN reflector saw it | — |
| `peer-reflexive` | the source address an inbound punch packet arrived from | — |

Static endpoints are the reason a two-machine site keeps its virtual network
when its internet is down: they need nothing negotiated.

### Control channel

A long-lived stream inside the existing inner session
(`innerchan.OpPath`), carrying newline-delimited JSON: `offer`, `answer`,
`observed`, `disabled`. Both ends are symmetric. Sends are queued and never
block the read goroutine — two peers answering each other simultaneously would
otherwise deadlock until the write deadline.

### Punch

Punch packets are 21 bytes: a magic, a 16-byte token, one flag byte. The token
was exchanged inside the encrypted control channel and is re-issued on every
offer, so an observer sees an opaque datagram with no identity in it and an
off-path attacker cannot forge one a peer will act on. Acting on a forged one
would cost a failed handshake and nothing else.

Both sides punch for the whole round (a hole is only open in the direction
packets have already travelled); only the side chosen by the existing
`peerDialRole` tie-break (lower SPKI dials) opens QUIC, staggered by 300ms so
the peer's punch packets land first.

### Establishment is proven, not assumed

In TLS 1.3 — QUIC included — the client's handshake completes before the
server has verified it, so a ledger refusal arrives as a connection that dies
shortly after `Dial` returned successfully. A candidate is therefore only
recorded as reachable after the vnet stream's preamble round trip, which only
a peer that accepted us can complete.

### Both links stay up

The plane holds one slot per kind (`peerLinks`). The relay link is
deliberately kept alive underneath a working direct path: it carries the
signalling, and when the direct path dies the next packet simply takes it.
Failover costs a map lookup, not a handshake.

### Network changes

`internal/agentd/netwatch.go` compares an address-set fingerprint on a 15s
poll and, on darwin, whenever the kernel routing socket says something moved
(debounced 2s, since one Wi-Fi association emits a burst). On a real change
the engine bumps a generation counter, rebinds the socket — which drops every
direct link, because they were built on addresses that may no longer exist —
retires candidate evidence gathered on the old network, and re-probes. Traffic
rides the relay link throughout.

## Relay-side reflector

`musterd` can host a UDP STUN reflector (`rendezvous.reflector.addr`,
optional, off by default). It answers a Binding Request with the observed
source address and keeps no state, no identity and no association with the
rendezvous. It exists so devices need not tell a third party their public
address; a public STUN server works too and is a privacy decision, not a
security one — a wrong reflexive address costs connectivity, never
authorization.

## Observability

The daemon writes a JSON snapshot (`v2.vnet.direct.status_file`, 0600,
atomic) once a second. `muster-agent paths [--json]` reads it; the AgentHalo
Rust backend runs that command for `GET /muster/paths`; the macOS network page
renders it as the Connection panel.

The path is strictly one-way: nothing reads the file back to make a routing
decision, so a stale snapshot can mislabel a page and can never misroute a
packet. The envelope carries `available` explicitly, because "direct paths are
off here", "the agent is not running" and "here is the answer" look alike as
an empty list and mean entirely different things to whoever is trying to work
out why two machines are slow.

## Configuration

```yaml
# agent.yaml
v2:
  vnet:
    enabled: true
    direct:
      enabled: true
      listen: "0.0.0.0:7445"
      stun: ["muster.linsheng.dev:7444"]
      status_file: /var/lib/muster/paths.json
    peers:
      - spki: …
        address: 192.168.14.12
        peer: example/mac-c
        endpoints: ["192.0.2.21:7445"]   # optional, relay-free
```

```yaml
# musterd.yaml
rendezvous:
  reflector:
    addr: ":7444"
```

An `agent.yaml` written before this change resolves to exactly the relay-only
behaviour it had: no socket bound, nothing advertised, one code path.

## Deployment notes

- Open UDP 7445 (or the chosen `listen` port) between peers that should reach
  each other directly; a LAN usually needs nothing.
- Open UDP 7444 on the relay host if the reflector is enabled. It does not
  share the rendezvous TCP listener.
- Both peers must authorize each other's SPKI (`muster-agent authorize add`)
  exactly as for the relay path. That is not new; a direct path adds no way
  around it.
