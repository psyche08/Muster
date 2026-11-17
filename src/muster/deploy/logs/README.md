# Deploying muster-logs

`muster-logs` is the collector for desktop log uploads
([ADR 0228](../../../../docs/adr/0228-hourly-redacted-log-upload.md)).
Release builds of musterd send to
**`https://muster-logs.linsheng.dev:8443/v1/logs`**, once an hour
([ADR 0229](../../../../docs/adr/0229-log-collector-behind-nginx-on-8443.md)).

```text
device ──TLS──▶ public 8443 ─▶ host 443: nginx, SNI muster-logs.linsheng.dev
                                  └─ http ─▶ 127.0.0.1:9180 muster-logs serve --plain
```

The public 8443 already reaches this host's nginx for linsheng.dev, so the
collector needs no port of its own: nginx chooses its server by name,
terminates TLS, and passes the upload to the collector on loopback with the
client's address in `X-Real-IP`.

## What arrives

Each upload is one `POST` of a signed envelope (`application/x-muster-logs`).
It holds what the device's log files gained since its last acknowledged
upload: whole lines only, at most 2 MiB of raw text per file. The text is
already redacted on the device:

- credentials, tokens, keys and email addresses are removed;
- device names, account names, user paths, MAC addresses and public IP
  addresses are replaced with per-device pseudonyms.

The collector:

- verifies the device signature, the payload digest and every chunk's
  length;
- drops ranges it already holds;
- appends the rest as gzip members.

## Layout

```text
/var/lib/muster-logs/networks/<network id | unjoined>/<device id>/
    device.json                 kind, version, commit, OS, first/last upload
    state.json                  per-file high-water marks, today's bytes
    <YYYY-MM-DD>/musterd.log.gz app.log.gz  l4forwarder.log.gz
```

Read a day with `zcat` (the members concatenate). The network id is the hex
genesis hash a member sees in its roster. The device id is the hex SHA-256 of
the device's public key.

```bash
sudo zcat /var/lib/muster-logs/networks/<network>/<device>/2026-11-17/musterd.log.gz | less
```

The network id is what the device claims. The signature proves the device
key, not membership of that network.

## Install

Prerequisites, by hand:

1. A DNS record for `muster-logs.linsheng.dev` pointing at the same public
   address as `linsheng.dev`.
2. A certificate covering `muster-logs.linsheng.dev`: its own Let's Encrypt
   lineage, an added name on the site's certificate, or a wildcard.

On the host that runs nginx for the public 8443:

```bash
cargo build --release -p muster-logs
```

```bash
sudo CARGO_TARGET_DIR="$CARGO_TARGET_DIR" deploy/logs/install.sh
```

The installer:

1. creates the `muster-logs` system user;
2. installs `/opt/muster/bin/muster-logs`, `/etc/muster/logs.env` (once; an
   operator's edits are kept) and the systemd unit;
3. restarts the service and waits for `http://127.0.0.1:9180/healthz`.

It does not touch nginx. Add the server block from `nginx-muster-logs.conf`
where nginx keeps its sites, set its certificate paths, and reload:

```bash
sudo nginx -t
```

```bash
sudo systemctl reload nginx
```

Keep the body limit at 16 MiB and the `X-Real-IP` header: the collector
counts its per-address limit by that header. It trusts the header because it
listens on loopback only (`--plain` refuses any other address), and the unit
also denies it every non-loopback address. Its only writable path is
`/var/lib/muster-logs` (mode 0700). nginx reloads after a certificate
renewal as it does for its other sites; the collector holds no certificate.

No firewall or router change is needed: nothing new listens publicly.

## Limits and retention (`/etc/muster/logs.env`)

| Setting | Default | Meaning |
|---|---|---|
| `LOGS_LISTEN` | `127.0.0.1:9180` | plain HTTP listener for nginx; loopback only |
| `LOGS_RETENTION_DAYS` | `30` | day directories older than this are deleted hourly; the privacy page promises 30 |
| `LOGS_DEVICE_DAILY_MIB` | `128` | compressed MiB one device may add per UTC day |

Fixed in code:

- 12 uploads an hour per device;
- 240 an hour per client address (`X-Real-IP` from nginx);
- a 16 MiB body;
- `created_at` within 24 hours of the server clock;
- 256 concurrent connections.

A refusal for rate or quota answers 429. The device keeps its place and tries
again the next hour.

## Check

```bash
curl -sS https://muster-logs.linsheng.dev:8443/healthz
```

```bash
journalctl -u muster-logs -n 50 --no-pager
```

`refused upload: …` lines at info level name the reason, for example a bad
signature (403) or a malformed body (400). Storage errors are warnings.

## Devices

- Release desktops upload by default. Phones never upload.
- Debug builds upload only with `--log-upload-url <URL>` or
  `MUSTER_LOG_UPLOAD=<URL>`.
- `muster-logs serve --listen 0.0.0.0:<port> --cert … --key …` still serves
  TLS directly, for a host without nginx.
- An owner switches uploads off with any of: `"enabled": false` in
  `log-upload.json` in the Muster data folder, `musterd --no-log-upload`, or
  `MUSTER_LOG_UPLOAD=off`.
- To preview locally what redaction does to a log file, without sending
  anything:

```bash
cargo run -p muster-logs --example redact_preview -- ~/Library/Logs/Muster/musterd.log
```
