# ADR 0229: The log collector sits behind nginx as muster-logs.linsheng.dev:8443

- Date: 2026-11-17
- Status: Accepted; source and tests verified, collector deployment pending
- Related: [ADR 0228](0228-hourly-redacted-log-upload.md),
  [deploy/logs](../../src/muster/deploy/logs/README.md),
  [private tunnel spec](../history/superpowers/specs/2026-08-13-private-tunnel-hardware-identity.md)
  (§3, risk 20)

## Context

ADR 0228 put the log collector on its own public port,
`https://muster.linsheng.dev:9444/v1/logs`, with the relay's certificate.
That port did not exist yet. Opening it meant three things: a router mapping,
a host firewall rule, and reachability from every client network.

The private tunnel spec records how this host is reached:

- Public 443 and 80 serve the router's management UI.
- Public 8443 maps to this host's 443, where nginx serves linsheng.dev (the
  download site) and ntfy by server name.
- Each new external port is a new reachability dependency for clients behind
  NAT-only gateways. Multiplexing by SNI over an already proven port avoids
  that dependency.

The 8443 path already carries the download site that every desktop reaches,
and nginx there already chooses servers by name.

The owner asked to put the collector on 8443 under its own name,
`muster-logs.linsheng.dev`, instead.

## Decision

- **Default URL.** Release builds send to
  `https://muster-logs.linsheng.dev:8443/v1/logs`
  (`muster_logs::wire::DEFAULT_URL`). Everything else in ADR 0228 stays as
  it was: the signed envelope, device-side redaction, cursors, the schedule,
  storage, limits and the off switches.
- **nginx terminates TLS.** A new nginx server for `muster-logs.linsheng.dev`
  (`deploy/logs/nginx-muster-logs.conf`):
  - listens on the host's 443 beside the existing sites and serves its own
    certificate for that name;
  - passes `POST /v1/logs` and `GET /healthz` to `127.0.0.1:9180`, and
    answers everything else with 404;
  - allows a 16 MiB body (the envelope's own limit) and sets `X-Real-IP` to
    the client address.

  This is TLS termination, not SNI passthrough: the spec rejected nginx
  `stream` passthrough on this port.
- **The collector listens on loopback.** It runs as
  `muster-logs serve --plain --listen 127.0.0.1:9180`. `--plain` still
  refuses any non-loopback address, and the unit adds `IPAddressDeny=any`
  with `IPAddressAllow=localhost`. It no longer reads the relay's
  certificate, joins the `muster-relay` group, or follows the relay's
  restarts.
- **The per-address limit counts the proxied client.** Behind the proxy
  every connection comes from 127.0.0.1. A plain listener therefore takes
  the client address from `X-Real-IP`, which nginx overwrites on every
  request. Only processes on the host can reach a loopback listener, so the
  header is nginx's. The limit of 240 uploads an hour per address now
  applies behind the proxy. Before this change, plain mode turned it off.
  A TLS listener still uses the TCP peer and ignores the header.
- **Direct TLS stays.** `muster-logs serve --listen 0.0.0.0:<port> --cert …
  --key …` still works for a host without nginx. It is not the deployed
  layout.

## Alternatives

- **Keep TCP 9444.** This needs a new router mapping and firewall rule, and
  a reachability check from each client network, for no gain over a port
  every desktop already reaches.
- **SNI passthrough to the collector's own TLS.** nginx would need a `stream`
  listener on the same 443 as its `http` servers. The spec records that this
  passes `nginx -t` but fails at runtime.
- **A path under linsheng.dev** (`https://linsheng.dev:8443/muster/logs`).
  This would reuse the site's certificate and DNS name. But it mixes an
  ingest API into the download site's server and its cache headers, and the
  ingest cannot be moved later without moving the site. A name of its own
  costs one DNS record and one certificate name.
- **Trust `X-Forwarded-For`.** Its leftmost entry is client-supplied when
  nginx appends rather than replaces. `X-Real-IP` set from `$remote_addr`
  holds exactly the address nginx saw.

## Consequences

- Nothing new listens publicly. The collector depends on nginx and on the
  public 8443 mapping, as the download site already does.
- Manual prerequisites, all on the operator's side:
  - a DNS record for `muster-logs.linsheng.dev` pointing at the 8443 public
    address;
  - a certificate covering that name;
  - installing the nginx server block and reloading nginx.
- nginx's own access log records each upload's client address and size,
  as it does for the site's other requests.
- ADR 0228's endpoint and deployment sections are superseded by this record;
  its status line says so.

## Verification

Source and tests (macOS arm64, debug):

- `cargo test -p muster-logs`: 23 unit and 5 end-to-end tests pass.
  - The new end-to-end test runs a plain collector on loopback with a limit
    of one upload per address. It sends with `X-Real-IP` set as nginx would:
    a second upload from the same address gets 429, and one from another
    address is stored.
- `cargo clippy -p muster-logs --no-deps --tests --examples -- -D warnings`:
  clean.
- `sh -n deploy/logs/install.sh`: the installer parses.

Not done:

- the DNS record and certificate for `muster-logs.linsheng.dev`;
- the nginx server block on the host, and `nginx -t`;
- the collector installation;
- an upload from a released desktop through 8443.
