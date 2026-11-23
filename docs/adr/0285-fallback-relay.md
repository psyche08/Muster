# ADR 0285. Reach a peer through an independent fallback relay

- Date: 2026-11-23
- Status: Proposed
- Related: [Network transport §3.5](../architecture/network.md), [ADR 0136](0136-settle-relay-carrier-switches.md), [ADR 0251](0251-windows-local-relay-settings.md), [ADR 0284](0284-basic-network-and-device-service-access.md)

## Context

The current client uses one relay endpoint. Desktop QUIC/UDP and TLS/TCP
carriers at that endpoint share its host, uplink and registration service.
Losing them together cuts off relay-dependent peers and rendezvous for new
links. A working direct PeerLink can still carry vnet and service traffic.
A relay records registrations, presence watches and mailbox items locally.
Merely changing one client's destination can leave its peer on the other relay.

## Decision

Deploy two independent muster-relay instances, each with a public name, its
own WebPKI key and certificate, and TCP and UDP reachability. Use separate
hosts and uplinks. Registration, forwarding and discovery on the fallback
must work while the primary is powered off. A device reachable only through
the affected vnet cannot be the bootstrap endpoint.

1. **Provision before failure.** Extend Shared Rust's single-endpoint setting
   to an ordered primary and optional fallback. Keep an existing installation's
   endpoint as primary. Persist both locally through the owner settings path,
   reject duplicate endpoints and keep a usable configuration on a failed
   edit. Provide both endpoints in the versioned invitation/bootstrap flow for
   new clients. Existing members can be provisioned independently. Track which
   members have actually registered on the fallback before declaring coverage.
2. **Make both ends reachable.** Desktops register on both relays concurrently
   and accept inbound packets on either. Keep the carrier selector and ACTIVATE
   state per relay. Select outbound relay **per peer**. Use the primary when
   it carries authenticated peer traffic. If either local relay health or peer
   reachability fails there, probe the peer through the healthy fallback even
   when the primary still answers its own probes. Confirm the peer on that
   path with a fresh end-to-end challenge. Associate replies with the receiving
   relay and registration generation so a delayed result cannot select a dead
   path. Keep validated direct links running throughout.
3. **Limit mobile power cost.** A foreground phone keeps one steady-state
   registration. On primary registration failure or an addressed desktop's
   path deadline, try the fallback within the same recovery cycle. Permit a
   temporary second registration for validation, then release the unused one.
   Desktop dual registration provides the rendezvous point. Foreground return
   and cold start use the saved pair even if the primary never answers.
4. **Switch without flapping.** Keep the existing three-second attempt bound
   and carrier backoff. Either relay can win a cold start without waiting for
   repeated primary retries. On a warm desktop with recent fallback readiness,
   target first successful peer RPC within ten seconds of primary loss. A
   healthy fallback remains selected through intermittent primary recovery.
   Before moving a peer back, require 60 seconds of continuous primary health
   and a successful peer round trip on the primary. Send each application
   operation once. Keep the inner QUIC connection where possible and preserve
   unknown outcomes when an application stream breaks.
5. **Scope evidence and preserve authorization.** Key ABSENT, PRESENT, WATCH
   and registration health by relay and generation. One relay's ABSENT can
   trigger an alternate probe. Declare a peer offline only with fresh negative
   evidence across all configured routes and no live peer evidence. Unknown
   or unreachable paths remain distinguishable from confirmed absence. Validate
   TLS name and chain independently for each endpoint. Both paths use the same
   roster identity, revocation, Basic/Advanced grants and target-local policy.
6. **Recover discovery.** Publish each eligible signed roster or sealed
   invitation mailbox item to both relays while valid, with independent
   acknowledgements and bounded retry. Read from either, deduplicate by content
   digest, and apply the existing signature, chain, expiry and invitation-use
   checks. Preserve revocation high-water state across restarts. Reconcile
   missing history from authenticated peers after an outage. A missing item or
   older snapshot on one relay must never roll back known membership. Only
   mailbox artifacts eligible for republication enter this retry path.
7. **Expose the actual path.** Rust reports each relay's registration, probe
   age and last failure, plus each peer's direct or relay path, last successful
   peer validation and switch reason. CLI and settings render this state.
   A green registration alone cannot establish peer recovery.

## Alternatives

DNS failover behind one name leaves existing registrations on different
servers until clients reconnect and caches expire. Two names on the same host
share its failure modes. Replicating all live connections or federating
datagrams between relays would add an inter-relay dependency. Dual desktop
registration lets independently operated relays remain usable in isolation.

## Consequences

The design requires a second public endpoint and upgraded clients. Desktop
registrations and probes consume additional capacity. A legacy client pinned
to the primary loses relay service during its outage. Two peers with neither
a direct path nor a mutually reachable relay remain partitioned. Background
mobile delivery and the time for a sleeping device to wake need separate
measurement. Restoring the network path alone cannot guarantee an existing
NFS mount or an interrupted application operation has recovered.

## Verification and rollout

| Fault or condition | Required observation |
|---|---|
| Stop the primary process or blackhole both transports | Two relay-only desktops reach each other through the fallback. Measure last good and first good RPC, all failures and the selected relay. Run a separate test with a healthy direct link and verify it stays up. |
| Block the primary for just one desktop | Its peer can still reach the primary. Both exchange fresh traffic through the fallback. An ABSENT on the primary does not mark the peer offline. |
| Power up with the primary down | Two restarted enrolled peers recover from saved configuration. A new supported client can discover an unexpired staged invitation on the fallback. |
| Foreground phone can reach primary, target desktop cannot | Phone reaches the desktop via fallback. Record switch time, registration count and background/foreground behavior. |
| Flap primary, then restore it | No repeated failback during the hold period. After stable peer validation, observe recovery in both directions. Count duplicate application writes. |
| Bad fallback certificate, both relays down, stale mailbox or revoked member | TLS and admission fail closed. Previously known revocations survive restart. Logs distinguish relay failure from peer absence. |
| Desktop vnet file service on a forced relay path | Read a known file, isolate the primary, then perform a fresh read with verified contents. Record any stall, reconnect or remount separately. |

First implement and exercise the matrix with two isolated relay processes
and software identities. Then provision an independent deployment, pilot two
desktops and a foreground phone, and distribute the endpoint pair to the
remaining members. Rollback selects the original single endpoint after
verifying its reachability. This proposal records the design. Implementation,
signed builds, deployment and real-device fault injection are pending.
