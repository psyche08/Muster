# Deploying muster-relay

The production endpoint is **muster.linsheng.dev:9443**, on both TCP and UDP.
The relay itself terminates TLS 1.3 on TCP and QUIC/TLS 1.3 on UDP, with the
same Let's Encrypt full chain and matching private key. Both use ALPN
`muster-relay/1`. TCP carries a four-byte big-endian length plus one relay
frame; QUIC carries one relay frame in each DATAGRAM. This is not HTTPS or
WebSocket, and nginx/Caddy HTTP proxying must not occupy TCP 9443.

Clients validate the public certificate's chain, hostname and validity.
Signed device registration still runs inside each carrier, and the opaque
forwarded PeerLink packets retain their independent end-to-end device
certificate authentication and encryption. The relay holds its **public
server TLS key**, never a device or Presence key. Possessing the server key
does not grant member access or the ability to decrypt PeerLink traffic.

The client evaluates both carriers' handshake, signed registration,
keepalive responses and round-trip time. A failed or oversized QUIC path
can use TCP. Carriers for one device coexist: a standby keepalive or
reconnection does not replace the return route selected by business traffic.
Receive-only clients declare a new return carrier with authenticated
`ACTIVATE` (opcode 0x0a, device id), acknowledged by `REFLECT`.
Secure health checks use `PROBE` (0x0b, device id + u64 nonce) and matching
`PROBE_ACK` (0x83, nonce), preventing activation replies from being confused
with health/RTT measurements; a probe never changes return-path preference.
Both probe and acknowledgement are exactly 1400 bytes with strictly zero
padding, so success tests the maximum relay payload in both directions,
including before a newly registered carrier becomes healthy.
There is no automatic fallback to unauthenticated TLS or plaintext UDP.

## Installation and renewal

Only TCP/UDP 9443 needs to be publicly reachable. Since 80/443 are not
provided, obtain and renew the Let's Encrypt certificate with **DNS-01**
using the operator's DNS provider. The certificate must contain
`muster.linsheng.dev` (or a matching wildcard) in its SAN. Certificate
issuance/provider credentials are managed separately from this installer.

```sh
cargo build --release -p muster-relay
sudo CERT_DIR=/etc/letsencrypt/live/muster.linsheng.dev \
  deploy/relay/install.sh muster.linsheng.dev
```

Before installation, inspect existing listeners and the external port mapping.
An old `musterd` bound to `127.0.0.1:9443` may serve other nginx routes;
**do not stop it automatically**. (That is the legacy Go relay; its source
left this repository on 2026-10-02 and is recoverable with
`git show acf23be7:src/cmd/musterd/main.go`.) A wildcard `0.0.0.0:9443` TCP listener
would conflict with it. If the public forwarding maps to the host's ingress
address `192.0.2.42`, explicitly bind the new relay there on both transports:

```sh
sudo CERT_DIR=/etc/letsencrypt/live/muster.linsheng.dev \
  RELAY_LISTEN=192.0.2.42:9443 \
  deploy/relay/install.sh muster.linsheng.dev
```

Use the ingress IP confirmed on the actual host; the address above is an
example, not an automatically discovered value. An explicit `RELAY_LISTEN`
updates that field in `/etc/muster/relay.env`; without it an existing setting
is preserved. The installer does not stop unrelated services or change
nginx/router mappings. Verify TCP and UDP forwarding target this exact IP.

The installer validates hostname, current expiry, chain/key parsing and key
matching before replacing the program. It installs the unprivileged
systemd service, copies the certificate and key into a root-owned generation
under `/etc/muster/relay-tls`, and opens **both** firewall transports on 9443.
The private key is mode 0640, readable only by root and the relay group;
Let's Encrypt's original directory permissions need not be widened.
Existing operator rate limits and push configuration are preserved.

The installer also installs `/etc/letsencrypt/renewal-hooks/deploy/50-muster-relay`.
This hook runs only when Certbot's `RENEWED_LINEAGE` exactly matches the
configured certificate directory. Renewals of unrelated certificates are ignored.
Re-running installation replaces the same hook, so it does not add duplicate
restarts. Existing nginx/coturn hooks are left in place.

For an already installed relay, add or update just this hook without reinstalling
the service, copying keys, or restarting it. For the wildcard certificate used
on `linsheng.dev`, run:

```sh
sudo deploy/relay/install-certbot-hook.sh \
  muster.linsheng.dev /etc/letsencrypt/live/linsheng.dev
```

The second argument must be Certbot's exact absolute lineage path, without a
trailing slash; it need not have the same name as the relay hostname. Append
`--print` to preview the generated hook without root access or filesystem
changes. Hook installation validates the current certificate's hostname and
expiry; it does not execute a renewal. On a matching renewal the hook invokes
`/opt/muster/bin/muster-relay-renew-cert` with those two fixed arguments.

That existing renewal helper checks the renewed files, stages a complete pair, atomically switches
one directory symlink, and restarts the relay. The restart briefly disconnects
clients, which must reconnect. This version does not implement SIGHUP or
in-process certificate reload; changing PEM files alone does not activate
new certificates. A failed restart restores the previous selection when
available. Old copied key generations are removed after successful rotation.
Do not run a generic hook against unrelated certificate lineages.

`--check-config --cert <fullchain.pem> --key <privkey.pem>` validates parsing
and key matching without binding a socket; it is not a live hostname,
public trust-chain, firewall or renewal test. Installation and loopback
regressions do not establish production reachability. Verify the deployed
certificate and **signed registration/keepalive on each transport**, then
network-loss failover, from actual clients.

For public data-plane verification without existing device keys or pairing state:

```sh
cargo run -p muster-net --example relay_probe -- muster.linsheng.dev:9443 --require-both
cargo run -p muster-net --example relay_transfer_probe -- muster.linsheng.dev:9443
```

The first probe verifies both carrier registrations. The second creates two
ephemeral peers with an in-memory roster, forces relay-only routing, waits for
peer connection arbitration to settle, then checks exact 256 KiB echoes in both
directions. It does not retry application operations or alter server settings.
Its selected carriers follow the production policy; it does not force a separate
throughput run on each carrier. Device lifecycle recovery remains a separate check.

## Migration and explicit local compatibility

The old UDP frame listener uses a different wire protocol. A secure server
upgrade requires compatible clients; coordinate that rollout instead of
silently retaining a public plaintext endpoint. The WebSocket carrier and its
nginx/Caddy examples were removed on 2026-10-02; a daemon that still
remembers a `wss://host` relay uses the secure relay at `host:9443`.

For local tests only:

```sh
muster-relay --insecure-legacy --udp 127.0.0.1:9443
```

The CLI rejects non-loopback addresses in this mode. The default production
mode requires `--cert` and `--key`; invalid credentials fail startup.

## Limits

| flag | default | meaning |
|---|---|---|
| `--listen` | `0.0.0.0:9443` | shared TCP/TLS and UDP/QUIC bind address |
| `--max-clients` | 10000 | maximum registered devices; up to four authenticated routes per device and four carrier connections per device budget |
| `--client-pps` | 3000 | frames per second per registered device, shared by its carriers, burst 2x |
| `--client-bps` | 30 MB/s | forwarded bytes per second per device, burst 2x |
| `--source-pps` | 30 | HELLO/AUTH per second per source socket address |
| `--mailbox-items` | 512 | items per mailbox key, oldest evicted |
| `--mailbox-keys` | 10000 | distinct mailbox keys |
| `--idle-secs` | 90 | idle signed registrations expire |
| `--absent-grace-secs` | 10 | a device unregistered for less is re-registering; carriers that sent `FEATURES` get `ABSENT` for it only after this (the relay start counts as a departure) |

Handshake and initial registration frame waits are bounded. TCP frame lengths
must be 1–1400 bytes; outbound connection queues hold at most 256 frames.
Challenges are bound to both device and carrier, expire after 30 seconds,
and cannot be consumed by replaying them on the other carrier. Mailbox reads
and writes require registration. Logs include `clients`, `mailbox_keys`,
`fwd`, `fwd_bytes`, `dropped`, `auth_failures`, `rate_limited`, `pushes`.

## Push hook

A registered phone may send `PUSH_REG` with its opaque APNs/FCM token. If a
member forwards to an offline device, `--push-command` receives
`<device id hex> <kind> <token hex>` at most once per
`--push-interval-secs` (default 600) for that device, including across the
device's re-registrations, which repeat `PUSH_REG` on every carrier. iOS
throttles background pushes past a few an hour and then delays or drops them,
so a shorter interval spends the wake-ups a phone would need. The hook owns
the provider integration (deploy/apns); the relay does not interpret provider
credentials.
