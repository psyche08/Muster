# ADR 0105: Forward loopback-only member services with the l4forwarder addon

- Date: 2026-11-03
- Status: Accepted
- Related: [architecture/network.md](../architecture/network.md) (forwarding
  streams, vnet), PROTOCOL.md §10–11,
  [addons/l4forwarder](../../addons/l4forwarder/README.md)

## Context

A desktop member (Mac-B) runs a local HTTP/SOCKS proxy that listens only on
`127.0.0.1:7897`. Another member (Mac-A) should use it over Muster. Three facts
block that today:

- vnet carries IP packets addressed to Mac-B's virtual address. A socket bound
  only to loopback never receives them, so the kernel resets the connection.
  This happens even when the port is granted.
- Forwarding streams do reach a member's loopback port (`forward` service,
  PROTOCOL §10). Only the phone's embedded app core opens them, though. A
  desktop has no way to originate one.
- The owner does not want the proxy itself reconfigured to listen beyond
  loopback.

Several options were weighed and decided by the owner:

- **Add desktop-originated forwarding to musterd** (a local listener that
  opens forward streams). Drafted, then rejected before commit. The Muster core
  stays limited to networking and authorization; forwarding a particular
  service is not its job.
- **Add raw forwarding to an existing private HTTP dashboard project.**
  Rejected. That project's HTTP features are business code that will be split
  out, and a general forwarder does not belong there.

## Decision

1. **A standalone addon `addons/l4forwarder`.** It is its own Cargo package with
   its own `[workspace]` and lock file, outside `src/muster`, and depends on no
   Muster crate. It is not part of Shared Rust business logic and is not linked
   into any app.
2. **Rules forward one protocol from one address to one upstream.** TCP is
   copied byte for byte with half-close preserved. UDP keeps one connected
   upstream socket per client address and ends it after `udp_idle_secs`
   (default 60) without traffic in either direction. Upstream addresses are IP
   literals; there is no DNS and no protocol awareness.
3. **Listeners bind specific addresses only.** `0.0.0.0` and `::` are refused,
   so a rule written for a vnet address cannot also expose the upstream on the
   physical LAN. A listener that cannot bind (the vnet interface is not up, or
   the port is taken) retries every 2 seconds and never exits. TCP connections
   are owned per rule and survive a rebind.
4. **Muster stays the authority.** The addon makes no access decisions. Reaching
   a forwarder at a vnet address still needs the accessing member's
   `net:<proto>/<port>` scope and the forwarding member's open port, both
   enforced by the vnet packet filter.
5. **Bounded resources.** Each rule has a TCP connection limit (excess clients
   wait in the backlog) and a UDP session limit (datagrams from new clients are
   dropped and logged once). Both default to 1024. The upstream connect timeout
   defaults to 5 seconds.
6. **Deployment is a launchd user agent** (`dev.linsheng.l4forwarder`), set up
   by `deploy/install.sh`. The configuration lives outside the repository, at
   `~/.config/l4forwarder/config.json`. A hosted check workflow covers the
   addon separately from the Muster core suite.

## Alternatives

- Desktop forwarding streams in musterd: see Context. It would keep the
  service on loopback, but it puts per-service forwarding into the core.
- Reconfiguring the service to listen on the vnet address: ruled out by the
  owner, and it couples the service's start-up to the vnet interface.
- `ssh -L` from each accessing machine: TCP only, one long-lived session per
  client, and nothing restarts it.
- A general-purpose third-party forwarder (socat and similar): one process per
  rule and protocol, no retry while the vnet address is absent, and no session
  limits without extra supervision.

## Consequences

- A loopback-only service on a member becomes reachable from other members for
  TCP and UDP, without changing the service or the Muster core.
- Using it needs two Muster changes per port: the accessor's scope and the
  forwarder's open port. Both commands replace whole lists. A missing grant
  shows up as a silent timeout.
- The upstream sees the forwarder's address, not the client's vnet address.
- Ports below 1024 need root, which the user agent does not have.
- The proxy that motivated this change must still be configured on Mac-A
  (`http://<Mac-B vip>:7897`). Choosing that is the owner's decision.

## Verification

- Source and tests, local (macOS, Rust 1.98), on this branch:
  - `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are
    clean.
  - `cargo test` passes 4 configuration tests (including the shipped example)
    and 5 end-to-end loopback tests: TCP round trip with a 1 MiB payload and
    half-close; per-client UDP sessions and idle expiry; waiting for a taken
    port; an absent upstream closing the client and being found when back; the
    TCP limit. The suite passed five consecutive runs.
- Binary smoke test: the debug build forwarded TCP and UDP to local echo
  servers, `--check` listed the rules, an empty configuration exited 1, and
  SIGTERM exited 0.
- Hosted CI: Actions are disabled for this repository
  ([ADR 0016](0016-suspend-hosted-actions-budget.md)). The new workflow has not
  run.
- Not done in this change: no release build was installed, no launchd agent
  was loaded, no Muster scope or open port was changed, and nothing was tested
  over a real vnet path between two members. Device acceptance should cover:
  - the agent starting before the vnet interface exists and serving once it
    appears;
  - behaviour across a musterd restart, when the interface disappears briefly;
  - macOS Local Network privacy prompts, if any, for the agent;
  - UDP through vnet end to end.
