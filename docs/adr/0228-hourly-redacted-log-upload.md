# ADR 0228: Desktops upload redacted log increments hourly, grouped by network

- Date: 2026-11-17
- Status: Accepted; source and tests verified, collector deployment pending
- Related: [SECURITY.md](../SECURITY.md) §6–§8, [MODULES.md](../MODULES.md),
  [deploy/relay](../../src/muster/deploy/relay/README.md),
  [deploy/logs](../../src/muster/deploy/logs/README.md)

## Context

Most failures worth investigating happen in a desktop's musterd: links that
do not come up, relay carriers that flap, native sessions that do not start.
Their evidence is in the log files under `muster_paths::log_dir()`
(`~/Library/Logs/Muster` on macOS): `musterd.log`, `app.log` and its one
rotation `app.log.1`, and `l4forwarder.log`. Today the only way to read them
is to ask the owner for the files, after the fact, often once the lines that
mattered have rotated away.

The requirement is an automatic upload with five properties:

1. The collector runs on `muster.linsheng.dev`, the relay host.
2. A device sends once an hour.
3. The collector keeps logs grouped by network.
4. Logs are redacted.
5. Uploads are incremental, so the same lines are not sent and stored again.

Constraints from the existing design:

- The relay is deliberately dumb ([MODULES.md](../MODULES.md) layer 2). Its
  public TCP and UDP port 9443 carry relay frames under ALPN `muster-relay/1`,
  not HTTP. Its frames are at most 1400 bytes.
- Device identity is a P-256 key, hardware-backed on Macs (`HwSigner`). A
  `DeviceId` is the SHA-256 of its public key.
- A network is named by its genesis hash (`NetworkId`), which every member
  holds in its roster.
- Phones embed the daemon with `listen = false` and write no log files.

## Decision

### Where the code lives

A new layer-2 crate, `muster-logs`, depends only on `muster-core`. It has two
halves.

The client half is linked by musterd with `default-features = false`:

- `collect` reads the log files from per-file cursors.
- `redact` rewrites the text.
- `wire` seals and opens uploads.
- `upload` schedules uploads and talks to the collector.

The `server` feature, which is on by default, adds the collector library
(`server`) and the `muster-logs` binary. CI's workspace `clippy` and `test`
build both halves. musterd's own builds (`-p musterd`, and the Apple targets
through `muster-ffi`) link only the client.

`musterd::log_upload` gives the uploader the daemon's log files and its state
file. It also supplies a closure that reads the uploader's identity afresh
for every upload: the device signer, the roster's network id, the device
kind, the version and commit, and the names to pseudonymize. The task starts
only in a daemon with `listen = true` whose kind is not a phone, and only
when a log directory exists. It ends once the daemon is closing.

### When

The first upload happens one hour after the last acknowledged upload (read
from the state file). It never comes sooner than a random 2–10 minutes after
start, so restarts do not upload at once and devices that start together do
not upload together. After that a device uploads every hour.

After a failure the uploader retries in 5 minutes, doubling the wait up to
the hour.

### What, incrementally

The state file `log-upload.json` sits beside the store with mode `0600`. It
holds one cursor per `(stream, generation)`. The stream is the file's base
name (`musterd`, `app`, `l4forwarder`). The generation is the file's inode,
so the rename that rotates `app.log` to `app.log.1` keeps its cursor: the old
generation is finished from `app.log.1`, and the new `app.log` starts at
offset 0. A file shorter than its cursor was truncated and starts over.

- **Whole lines only.** A line that is still being written waits for the
  next hour.
- **First sight of a file.** Only its last 256 KiB is sent, starting at a
  line start, so turning the feature on does not upload months of history.
- **Backlog cap.** At most 2 MiB of raw text per file goes in one upload.
  An older backlog beyond that is skipped. Its size travels in the chunk's
  `skipped` field, and the collector writes a one-line note in its place.
- **When cursors advance.** Only after the collector answers 2xx, or refuses
  the batch for good (400, 413, 415, 422). A transport error, a 5xx, a 403
  or a 429 leaves the cursors where they were, and the same range goes again
  later.

The collector also keeps a high-water mark per `(stream, generation)` and
device. It drops any chunk that ends at or before the mark. A device that
lost an acknowledgement, or restored an old state file, therefore stores no
second copy.

### Redaction, on the device, before compression

`Redactor` runs over each chunk's text before it is compressed and signed;
the collector never sees the original. Two kinds of value are handled.

Removed outright:

- PEM blocks, JWTs, API-key shapes and `Bearer` / `Basic` credentials;
- `key=value` and `"key": value` pairs whose key names a secret (token,
  secret, password, passphrase, API key, cookie, ticket, credential,
  authorization, private key, session id, with or without a prefix such as
  `access_` or `refresh_`);
- `user:password@` in URLs;
- email addresses;
- Chinese resident-ID and mobile numbers.

Replaced with a stable pseudonym `<class:8 hex>`:

- the home folder (rewritten to `~`);
- other `/Users/<name>`, `/home/<name>` and `C:\Users\<name>` user names;
- the names of the network's devices, the daemon's own name, the host name,
  the account name (`USER`, `USERNAME`, `LOGNAME`) and the home folder's
  base name;
- MAC addresses;
- public IPv4 addresses;
- global IPv6 addresses (2000::/3, outside the documentation prefix).

Private, loopback, link-local, CGNAT (100.64/10), benchmark (198.18/15,
which local proxies use for fake IPs) and documentation addresses are kept.
They locate a problem without identifying anyone outside the LAN.

A pseudonym is an HMAC-SHA256 of the value under a random 32-byte salt that
exists only in the device's state file. The same value keeps the same
pseudonym across that device's uploads, so lines can be correlated. The
collector cannot reverse a pseudonym or link it across devices.

### Wire format

One `POST` per upload, `Content-Type: application/x-muster-logs`, with this
body:

```text
"MLOG" | version u8 (1) | manifest_len u32 BE | manifest JSON
       | sig_len u16 BE | DER ECDSA signature | payload
```

The manifest carries:

- the device's public key;
- the network id (hex, or absent before joining);
- kind, version, commit and OS;
- `created_at` and a random nonce;
- one entry per chunk: stream, generation, `start`, `end`, `skipped`,
  compressed `len` and `raw_len`;
- the payload's SHA-256.

The payload is the chunks' gzip members, concatenated.

The signature is `sign_domain("MUSTER-LOGS-UPLOAD-V1", manifest JSON)` with
the device key, through `HwSigner`, so a Mac signs in the Secure Enclave. The
manifest carries the format version, and the payload is bound to the
signature by its digest in the manifest.

The collector checks, before it keeps anything:

- framing and limits: a 64 KiB manifest, 16 chunks, 4 MiB raw per chunk and
  a 16 MiB body;
- the signature, and derives the `DeviceId` from the key;
- the digest;
- that stream names are plain base names;
- that the chunk lengths cover the payload exactly;
- that every member decompresses to exactly its `raw_len`.

### Endpoint and transport

Devices send to `https://muster.linsheng.dev:9444/v1/logs`. The collector is
its own process beside the relay, on TCP 9444, because the relay's port does
not speak HTTP and the relay stays dumb.

The collector serves HTTP/1.1 over TLS with the relay's Let's Encrypt
certificate. Its unit reads the certificate through the `muster-relay` group
and is `PartOf=muster-relay.service`, so the restart that follows a renewal
restarts the collector too.

The uploader refuses any URL that is not https, except http to a loopback
address, which tests and a collector behind a local TLS proxy use.

### Storage and limits on the collector

```text
<root>/networks/<network hex | unjoined>/<device hex>/
    device.json          kind, version, commit, OS, first and last upload
    state.json           high-water marks, today's stored bytes
    <YYYY-MM-DD>/<stream>.log.gz
```

Each day file is a series of gzip members, appended and `fsync`ed, and reads
back as one text with `zcat`. Limits:

- 240 uploads an hour per source address;
- 12 an hour per device;
- 128 MiB of compressed logs per device per UTC day;
- created_at must be within 24 hours of the collector's clock.

Rate and quota refusals answer 429; the device keeps its cursors and tries
again an hour later. An hourly sweep deletes day directories older than the
retention, 30 days by default.

### Switching it off

Release builds upload to the default URL. Debug builds upload only when
given a URL. It can be switched off three ways:

- `musterd --no-log-upload`;
- `MUSTER_LOG_UPLOAD=off` (also `0`, `false` or `no`);
- `"enabled": false` in `log-upload.json`.

The state file is read before every upload, and a save keeps the flag the
owner wrote while an upload was out. `--log-upload-url <URL>` or
`MUSTER_LOG_UPLOAD=<URL>` points at another collector.

## Alternatives

- **Ingest through the relay.** This would add an HTTP-like ingest path,
  storage and quotas to the component whose job is to forward opaque frames.
  Uploads would also have to be split into 1400-byte frames. Rejected; the
  collector shares only the host and the certificate.
- **Redact on the collector.** Credentials and names would then leave the
  device and be stored before redaction. Rejected.
- **Upload whole files and deduplicate on the server.** That resends the
  same megabytes every hour. Byte cursors on the device, plus the
  collector's range marks as a backstop, send each line about once.
- **Deduplicate by line hash.** Repeated identical lines are meaningful in
  logs, and hashing every line costs more than comparing a range end.
- **A third-party log service (Sentry, Datadog, an OTLP endpoint).** It
  would add an external processor, an account and its own retention to the
  privacy policy. Rejected for a first version.
- **Prove membership of the named network.** The collector has no roster.
  Checking membership would mean reading mailbox entries from the relay or
  attaching a roster proof to every upload. Deferred; see Consequences.

## Consequences

- An operator can read the last hours of any desktop's logs, grouped by
  network then device, without asking the owner. Traffic is bounded at 2 MiB
  of raw text per file per hour, sent compressed.
- **Grouping is self-asserted.** The signature proves that the uploader holds
  the device key it names. It does not prove that the device belongs to the
  network it names. A device can file its logs under any network id. This is
  diagnostic storage readable only by the operator, not an access boundary.
  Rate limits and the daily quota bound what one key or one address can
  store. Listed in [SECURITY.md](../SECURITY.md) §8.
- **Redaction is pattern-based.** Free text that matches no pattern can still
  carry personal data: a name the device does not know, or text that a third
  party wrote into a log line. Existing log hygiene, such as Accessibility
  text logged as `<redacted:N chars>`, keeps conversation content out of the
  files. The privacy page now describes the upload, the redaction, the
  30-day retention and the opt-out.
- **Backlog loss.** While the collector is unreachable no cursor moves. If a
  file grows by more than 2 MiB in that time, the oldest part of the growth
  is skipped, and the collector notes it.
- **Builds.** `cargo build --workspace` unifies features, so musterd built
  that way also compiles the collector's dependencies. Release builds of
  musterd use `-p` and do not.
- **Manual prerequisites before a release ships with uploads on:**
  - build and install the collector on the relay host
    (`src/muster/deploy/logs/install.sh`);
  - open TCP 9444 in the host firewall and in any upstream port mapping;
  - publish the updated privacy page.

  Until the collector answers, desktops retry every hour and lose nothing
  but the backlog beyond the cap.

## Verification

Source and tests (macOS arm64, debug):

- `cargo test -p muster-logs`: 23 unit tests and 4 end-to-end tests pass.
  - Unit tests cover framing, signature and tamper refusal; every redaction
    class and lines left unchanged; cursors, rotation, truncation, the first
    tail and the backlog cap; schedule bounds and URL policy; UTC day
    arithmetic and the rate buckets.
  - `tests/end_to_end.rs` runs the collector on loopback and uploads through
    `upload_once` with a software key. It shows redacted text stored under
    `networks/<network>/<device>/<day>/musterd.log.gz` and under `unjoined`
    for a device in no network. Only appended lines go next. A half-written
    line waits for its end. A resent range is stored once. A 429 leaves the
    cursors in place. `"enabled": false` sends nothing. Garbage gets 400. The
    same upload works over TLS with a test certificate.
- `cargo test -p musterd`: the flag and environment precedence for the
  upload URL.
- The redaction was previewed locally with
  `cargo run -p muster-logs --example redact_preview` on a real
  4,000-line `musterd.log`. Home paths became `~`, device names became
  pseudonyms, private addresses stayed, and nothing was sent.

Not done in this change:

- the collector build and installation on `muster.linsheng.dev`;
- the TCP 9444 port mapping;
- publication of the privacy page;
- an upload from a released desktop build to the production collector.
