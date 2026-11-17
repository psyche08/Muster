# Deploying muster-logs

`muster-logs` is the collector for desktop log uploads
([ADR 0228](../../../../docs/adr/0228-hourly-redacted-log-upload.md)).
Release builds of musterd send to
**`https://muster.linsheng.dev:9444/v1/logs`**, once an hour. It runs on the
relay host as its own service, on TCP 9444, beside the relay on 9443. The
relay's port speaks relay frames, not HTTP, and the relay stays a dumb
forwarder.

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

On the relay host, after `deploy/relay/install.sh` has installed the relay
and its certificate:

```bash
cargo build --release -p muster-logs
```

```bash
sudo CARGO_TARGET_DIR="$CARGO_TARGET_DIR" deploy/logs/install.sh
```

The installer:

1. checks the certificate and key with `muster-logs check-config`;
2. creates the `muster-logs` system user;
3. installs `/opt/muster/bin/muster-logs`, `/etc/muster/logs.env` (once; an
   operator's edits are kept) and the systemd unit;
4. restarts the service;
5. opens 9444/tcp in firewalld when it runs.

An upstream router or cloud firewall needs TCP 9444 forwarded or allowed by
hand.

The unit reads the relay's certificate through the `muster-relay` group. It
is `PartOf=muster-relay.service`, so the restart after a certificate renewal
restarts the collector with the new certificate. Its only writable path is
`/var/lib/muster-logs` (mode 0700).

## Limits and retention (`/etc/muster/logs.env`)

| Setting | Default | Meaning |
|---|---|---|
| `LOGS_LISTEN` | `0.0.0.0:9444` | HTTPS listener |
| `LOGS_RETENTION_DAYS` | `30` | day directories older than this are deleted hourly; the privacy page promises 30 |
| `LOGS_DEVICE_DAILY_MIB` | `128` | compressed MiB one device may add per UTC day |

Fixed in code:

- 12 uploads an hour per device;
- 240 an hour per source address;
- a 16 MiB body;
- `created_at` within 24 hours of the server clock;
- 256 concurrent connections.

A refusal for rate or quota answers 429. The device keeps its place and tries
again the next hour.

## Check

```bash
curl -sS https://muster.linsheng.dev:9444/healthz
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
- An owner switches uploads off with any of: `"enabled": false` in
  `log-upload.json` in the Muster data folder, `musterd --no-log-upload`, or
  `MUSTER_LOG_UPLOAD=off`.
- To preview locally what redaction does to a log file, without sending
  anything:

```bash
cargo run -p muster-logs --example redact_preview -- ~/Library/Logs/Muster/musterd.log
```
