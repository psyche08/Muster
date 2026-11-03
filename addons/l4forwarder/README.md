# l4forwarder

A small TCP/UDP port forwarder, kept as an addon beside Muster
([ADR 0105](../../docs/adr/0105-l4forwarder-addon.md)). Each rule listens on
one address and forwards one protocol to one upstream address: TCP byte for
byte, UDP datagram for datagram.

Its intended use is a service on a Muster member that listens only on the
member's loopback, such as a local HTTP/SOCKS proxy on `127.0.0.1:7897`. vnet
delivers packets to the member's virtual address, so a loopback-only service is
out of reach. l4forwarder on that member listens at its vnet address and
forwards to the loopback port, without reconfiguring the service.

l4forwarder does not depend on Muster and makes no access decisions. Who may
reach the vnet address and port stays Muster's decision:

- The accessing member needs a `net:<proto>/<port>` scope, which an admin
  grants with `muster scopes <device> <all scopes…>`.
- The forwarding member must open the port with `muster policy --open-port …`.

Both commands **replace** the whole list, so pass the existing entries too.
Without either grant, vnet silently drops the packets and clients time out.

## Configuration

```json
{
  "rules": [
    { "proto": "tcp", "listen": "192.168.13.13:7897", "upstream": "127.0.0.1:7897" },
    { "proto": "udp", "listen": "192.168.13.13:7897", "upstream": "127.0.0.1:7897" }
  ]
}
```

| Field | Default | Meaning |
|---|---|---|
| `rules[].proto` | required | `tcp` or `udp`; list both to forward both |
| `rules[].listen` | required | A specific address and port. Wildcards (`0.0.0.0`, `::`) are refused because they would expose the upstream on every network, including the physical LAN |
| `rules[].upstream` | required | Address and port to forward to; an IP literal, no DNS |
| `connect_timeout_secs` | `5` | Time allowed for the upstream TCP connection |
| `udp_idle_secs` | `60` | A UDP client without traffic in either direction for this long loses its upstream socket. Its next datagram starts a new one |
| `max_tcp_connections` | `1024` | Per TCP rule. Further clients wait in the listen backlog |
| `max_udp_sessions` | `1024` | Per UDP rule. Datagrams from further clients are dropped |

## Behaviour

- **Waiting for the address.** If a rule cannot bind (the vnet interface is
  not up yet, or the port is taken), it retries every 2 seconds. It logs each
  new reason once and never exits.
- **TCP.** Each connection gets its own upstream connection. Half-closes pass
  through. An upstream that refuses or does not answer closes the client.
- **UDP.** Each client address gets its own connected upstream socket, so
  replies go back to the right client. ICMP "port unreachable" from the
  upstream does not end a session, because the upstream may come back.
- **Sources.** The upstream sees connections from the forwarder, not from the
  original client. Logs at the upstream lose the client's vnet address.
- **Logs.** Start-up and errors go to stderr (`RUST_LOG=debug` for
  per-connection detail).

## Build and test

```bash
cargo test
cargo build --release
target/release/l4forwarder --config config.json --check   # validate and list the rules
```

## Install as a launchd agent (macOS)

```bash
deploy/install.sh path/to/config.json   # build, check, install, (re)load
deploy/install.sh                       # upgrade, keeping the installed configuration
```

| | Location |
|---|---|
| Binary | `~/.local/bin/l4forwarder` |
| Configuration | `~/.config/l4forwarder/config.json` |
| Log | `~/Library/Logs/l4forwarder.log` |
| Agent | `dev.linsheng.l4forwarder` |

```bash
launchctl print gui/$(id -u)/dev.linsheng.l4forwarder
launchctl kickstart -k gui/$(id -u)/dev.linsheng.l4forwarder   # restart after a configuration change
launchctl bootout gui/$(id -u)/dev.linsheng.l4forwarder        # stop
```

The agent runs as the logged-in user, so ports below 1024 are not available
to it.
