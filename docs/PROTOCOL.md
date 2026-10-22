# PROTOCOL — Wire Formats

Implements [ARCHITECTURE.md](ARCHITECTURE.md) §3.4, §3.5, and §3.7 in `src/muster/`. **2026-09-28 update**: public secure transports follow the new targets in §5.2, §6, and §7. Legacy raw UDP / WebSocket remains only in historical diagnostics or explicit tests, not as a production compatibility fallback. `relay_tls.rs`, `secure_relay.rs`, and the Relay server implement the dual carriers, ACTIVATE, and full-frame nonce probes below; source availability does not mean server/client installation or public-network acceptance is complete. This document specifies each layer byte by byte, sufficient to implement a second interoperable implementation or audit the first. All integers are big-endian unless stated otherwise.

| Layer | Code |
|---|---|
| Canonical encoding | `muster-core/src/canonical.rs` |
| Identity, roster, invitations, scopes | `muster-core/src/{identity,roster,invite,scope,policy}.rs` |
| TLS/QUIC, Relay frames, MagicSock | `muster-net/src/{tls,relay_tls,relay_proto,magicsock}.rs` |
| Service streams, pairing, gossip, forwarding, vnet | `muster-net/src/{stream,node,pair,gossip,forward,vnet}.rs` |
| Relay server | `muster-relay/src/lib.rs` |
| muster-rpc | `muster-proto/proto/muster.proto`、`muster-proto/src/lib.rs`、`muster-rpc/src/lib.rs` |

## 1. Canonical Encoding

All signed objects and Relay frames use the same notation:

| Notation | Bytes |
|---|---|
| `u8` / `u16` / `u32` / `u64` | Fixed-width, big-endian |
| `fixed(v)` | Raw bytes, no length prefix |
| `bytes(v)` | `[len u32][v]` |
| `str(s)` | `bytes(utf8)`; must be valid UTF-8 when read |
| `opt_bytes(v)` | `[0x00]` means absent; `[0x01][len u32][v]` means present; other tags are rejected |

Reader limits: reject any individual `bytes` value over 1 MiB; no trailing bytes may remain after reading (`finish()`). Errors are either `Truncated` or `Malformed`.

## 2. Identity

- Device keys are P-256. The wire public key is a 65-byte uncompressed point (first byte `0x04`), checked for curve membership when read.
- SPKI DER = fixed 26-byte prefix `30 59 30 13 06 07 2a 86 48 ce 3d 02 01 06 08 2a 86 48 ce 3d 03 01 07 03 42 00` + the 65-byte point, 91 bytes total.
- `DeviceId = SHA-256(SPKI DER)`, 32 bytes. Text form is lowercase hexadecimal; logs use the first 4 bytes (`short()`).
- Fingerprint: RFC 4648 base32 of the first 10 bytes of `DeviceId` (uppercase, no padding), 16 characters grouped 4-4-4-4 with `-`, for human comparison.
- All signatures use ECDSA-P256-SHA256 with DER encoding over messages that have not been prehashed.
- Domain-separated `sign_domain(domain, msg)` signs `domain_ascii || 0x00 || msg`. Roster, Relay authentication, and invitation proofs use distinct domains, so one signature cannot be valid in two contexts.

## 3. Roster

Constants: `ROSTER_DOMAIN = "MUSTER-ROSTER-V1"`, `ENTRY_VERSION = 1`.

### 3.1 DeviceRecord

```
[device_id 32][pubkey bytes(65)][presence_key opt_bytes(65)][name str]
[kind u8][role u8][vip_tag u8][vip 4 (only if vip_tag=1)][scope_count u32][scope str]*
```

- `kind`：0 Desktop、1 Phone、2 Server。`role`：0 Admin、1 Member。
- On read, `device_id` must equal `SHA-256(SPKI(pubkey))`.
- At most 4096 scopes, encoded in `Scope`'s derived `Ord` order (not string order): `browser.view < browser.control < agent.view < agent.control < terminal.view < terminal.control < locked.control < network.admin < net:… < chats < chats:<p> < agents < agents:<p> < terminals`. Within `net:`, protocol order is tcp < udp < any, then port None < Some(n); provider scopes of the same kind use provider byte order. Existing variants' relative order must not change; new variants may only be appended.
- Scope strings: `browser.view`, `browser.control`, `chats`, `chats:<provider>`, `agents`, `agents:<provider>`, `terminals`, `locked.control`, `network.admin`, `net:*` (any protocol/port), `net:tcp/22`, `net:udp/*`, `net:*/443`; legacy `agent.view`, `agent.control`, `terminal.view`, and `terminal.control` remain parseable. `<provider>` is 1–32 characters from `[a-z0-9-]`; unknown providers parse normally. The parser also accepts `net:*/*`, but re-encodes it as `net:*`, so non-Genesis entries using that spelling are rejected as noncanonical.
- Compatibility: older nodes do not recognize `chats` / `agents` / `terminals` or their provider strings and reject entries containing them and their descendants. Upgrade every device in the network before an administrator writes new scopes.

### 3.2 Entries

The hashed and signed `signed_bytes`:

```
[ver u8=1][network_id 32 (all zeros for Genesis)][parent_count u32][parent 32]*
[lamport u64][body_kind u8][body][signer 32]
```

| `body_kind` | body |
|---|---|
| 0 Genesis | `[name str][cidr_ip 4][prefix u8][creator DeviceRecord]` |
| 1 Add | `[DeviceRecord]` |
| 2 Update | `[device_id 32][name opt_bytes][role: 0 ∣ 1,u8][scopes: 0 ∣ 1,u32 count,str*][vip: 0 unchanged ∣ 1 clear ∣ 2,ip 4]` |
| 3 Remove | `[device_id 32]` |

- Wire form `Entry::encode()`: `[u32 len][signed_bytes][u32 len][signature DER]`.
- `EntryHash = SHA-256(signed_bytes)`; `NetworkId` = the Genesis `EntryHash` (computed with an all-zero network_id field).
- The signature covers `"MUSTER-ROSTER-V1" || 0x00 || signed_bytes`.
- Authors use `roster.heads()` (ascending) as parents; decoding permits at most 64 parents.
- Decoded non-Genesis entries must re-encode byte-for-byte identically to the wire bytes, or be rejected ("non-canonical entry encoding"). Genesis skips this check and ignores its wire `network_id` field.

### 3.3 Insertion and Folding

`Roster::insert` proceeds in order:

1. Already holding the same hash → `Duplicate`.
2. Any unknown parent → `MissingParents(list)`; the caller retries later.
3. Genesis requires an empty roster and no parents. Other entries require an existing Genesis, equal `network_id`, and at least one parent.
4. Validate against the folded `causal_past(parents)`: the signer is an active administrator (for Genesis, the creator); the signature is valid; Genesis prefix is in 8..=30; an Add device has never appeared in the past (removed IDs cannot be re-added); Update / Remove targets are currently active.
5. After insertion, `recompute()`; if the new entry is not in the accepted set, roll back with "entry was invalidated by the roster it joined".

`recompute()` (same state for any insertion order; see `tests/properties.rs`):

- Kahn topological order, with the ready set ascending by `(lamport, hash)`.
- Accept an entry if and only if every parent is accepted, and in the fold of `accepted ∩ causal_past(parents)` the signer is an active administrator and the signature is valid. Accept Genesis only when no entry has yet been accepted.
- Removed-administrator rule: for each accepted `Remove(X)`, exclude every entry signed by X outside `causal_past([remove])`, then repeat until stable.
- State = fold of the accepted set: Genesis sets network_id, name, and cidr, forcing the creator to Admin; Add inserts only if absent; Update applies only when `(lamport, hash)` exceeds the member's `last_update` (last writer wins), applies only `Some` fields, and updates `defined_by` when vip changes; Remove sets `removed = true` permanently.
- Resolve VIP conflicts after folding: sort active members sharing a VIP by `(defined_by, device_id)` ascending; the first retains it and the rest have `vip = None` in the state.

`free_vip`: no address is available for prefix ≥ 31. Otherwise select the smallest `base+offset` (offset 1 through hosts-2) unused by any member, including removed members. Authors assign automatically in Genesis and Add for nonphone records without a specified VIP, so the creator gets `.1`.

`heads()` returns held entry hashes not referenced as a parent by any entry (ascending; may include excluded entries); `next_lamport = max(lamport)+1`; `entries_after(peer_heads)` returns accepted entries outside those heads' causal past, in topological order.

## 4. Invitations

Constants: prefix `muster1:`, version 1, `PROOF_DOMAIN = "MUSTER-INVITE-PROOF-V1"`, `EXPORTER_LABEL = "muster-invite"`.

- Text: `"muster1:" + BASE64URL_NOPAD(body)`; trim leading/trailing whitespace when decoding.
- body：`[ver u8=1][network_id 32][network_name str][admin_key bytes(65)][relay str][secret 16][expires_unix u64]`。
- `admin_id` is not transmitted; derive it from the SPKI hash of `admin_key`. `expired(now)` means `now > expires_unix`; musterd defaults to a 600-second TTL.
- `invite_id = SHA-256(secret)`。
- Proof: `proof = HMAC-SHA256(key = secret, msg = "MUSTER-INVITE-PROOF-V1" || exporter)`, without a separator. `exporter` comes from `export_keying_material(32 bytes, label "muster-invite", empty context)` on the QUIC/TLS connection. Verification uses constant-time comparison and requires exactly 32 bytes.

## 5. TLS and QUIC

### 5.1 Inner PeerLink: Device Identity

- ALPN `muster/1`, SNI `muster`, TLS 1.3 only, ring provider; clients send no early data.
- Certificate: rcgen self-signed, `PKCS_ECDSA_P256_SHA256`, SAN dNSName `muster`, empty subject, freshly signed on each process start. Other fields are not checked.
- Mutual certificates; clients always send a certificate. Only `ECDSA_NISTP256_SHA256` is offered as a signature algorithm; TLS 1.2 signatures are rejected.
- Verifier: the peer must present exactly one certificate; derive its ID from the leaf SPKI. If pinned (joiners pin the invitation's `admin_id`), accept only `id == pin`; otherwise require an active member. Failing that, only the server may accept when `accept_unknown_clients` is true (an invitation is open). Validity period, issuer, and SAN are not checked.
- Transport parameters: 5-second keepalive, 30-second idle timeout, 4 MiB DATAGRAM send and receive buffers, `initial_mtu = 1350`, `grease_quic_bit(false)` (every QUIC packet's first byte has bit 0x40 set; section 7 relies on this).
- Connection deadline: 3 seconds; timeout is failure. Close codes: 0 `replaced` / `shutdown`, 1 `not a member` / `no identity`.

### 5.2 Outer Public Relay: Domain Identity

| Carrier | Public Endpoint | Wire Encapsulation |
|---|---|---|
| UDP / QUIC | `muster.linsheng.dev:9443/udp` | After a TLS 1.3 QUIC handshake, each QUIC DATAGRAM contains exactly one Relay frame; datagrams are not carried over QUIC streams |
| TCP / TLS | `muster.linsheng.dev:9443/tcp` | After a TLS 1.3 handshake: `[length u32][Relay frame]`; big-endian length in `1..=MAX_FRAME`, excluding the length field; validate length before allocating |

- Both endpoints use ALPN `muster-relay/1`, SNI `muster.linsheng.dev`, and the same Let's Encrypt fullchain / key. Clients perform normal WebPKI trusted-root, chain, validity, and hostname verification. No outer device-client certificate is required; device possession is still proved by signed registration in §6.
- Strictly separate from §5.1's device-certificate verifier: domain certificates do not prove membership, and self-signed device certificates cannot authenticate a public domain. A DoH-resolved IP determines only the connection address, not SNI or the verified hostname.
- One connection/handshake/registration/initial full-frame PROBE / PROBE_ACK attempt shares a 3-second deadline; certificate errors, ALPN errors, and timeouts fail. Never switch to raw UDP, plaintext WS, disabled certificate verification, or a silent legacy WSS fallback.
- Do not use 0-RTT for registration or side-effecting business requests. TCP and QUIC certificate renewal must update together; renewal deployment is complete only after new handshakes succeed on both endpoints.

## 6. Relay Frames

Constants: `RELAY_AUTH_DOMAIN = "MUSTER-RELAY-AUTH-V1"`, `MAX_FRAME = 1400` (Relay drops larger frames). Public carriers use QUIC DATAGRAM or TLS length frames from §5.2; legacy raw UDP / WebSocket encoding is not an automatic production fallback. Clients retain a healthy standby carrier and reconnect disconnected carriers with independent backoff.

| Type | Layout |
|---|---|
| `0x01 HELLO` | `[01][id 32]` |
| `0x02 AUTH` | `[02][id 32][key bytes(65)][nonce 32][sig bytes]` |
| `0x03 FWD` | `[03][dst 32][payload…]` (to end of frame, no length) |
| `0x04 DELIVER` | `[04][src 32][payload…]` |
| `0x05 MB_PUT` | `[05][key 32][item bytes]` |
| `0x06 MB_LIST` | `[06][key 32]` |
| `0x07 MB_GET` | `[07][key 32][hash 32]` |
| `0x08 KEEPALIVE` | `[08][id 32]` |
| `0x09 PUSH_REG` | `[09][id 32][kind u8][token bytes]` |
| `0x0a ACTIVATE` | `[0a][id 32]` |
| `0x0b PROBE` | `[0b][id 32][nonce u64][zero padding 1359]`, exactly 1400 bytes |
| `0x81 REFLECT` | `[81][addr]` |
| `0x82 CHALLENGE` | `[82][nonce 32]` |
| `0x83 PROBE_ACK` | `[83][nonce u64][zero padding 1391]`, exactly 1400 bytes |
| `0x85 MB_ACK` | `[85][key 32][hash 32]` |
| `0x86 MB_LISTING` | `[86][key 32][count u16][hash 32]*` |
| `0x87 MB_ITEM` | `[87][key 32][hash 32][item bytes]` |
| `0x8f ERROR` | `[8f][text str]` |

- `addr = [family u8: 4∣6][ip 4∣16][port u16]`. Trailing bytes are forbidden except for FWD / DELIVER.
- The first payload byte is its type: `0x20 PAYLOAD_QUIC` precedes a raw QUIC packet; `0x21 PAYLOAD_CANDIDATES` precedes `[count u16][addr]*`. Unknown types are ignored.
- PROBE / PROBE_ACK headers are 41 / 9 bytes respectively; nonce is big-endian u64 and all remaining bytes are zero. Reject frames not exactly 1400 bytes or with nonzero padding. These test maximum-frame reachability in both directions; small registration replies cannot replace MTU verification.
- AUTH signs `"MUSTER-RELAY-AUTH-V1" || 0x00 || nonce`; Relay also checks `SHA-256(SPKI(key)) == id`.

### 6.1 Relay Behavior

| Frame | Handling |
|---|---|
| HELLO | Generate a random 32-byte nonce, store it under the ID and carrier route, and reply CHALLENGE. Source-address rate limit applies (default 30 frames/second) |
| AUTH | Nonce is single-use and valid for 30 seconds. Success registers the device and an independent route for this secure carrier (QUIC connection or TCP / TLS channel; hot standby never overwrites the active route), returning `REFLECT(observed address)`. Failure returns `ERROR "authentication failed"`; capacity exhaustion (`--max-clients`, default 10000) returns `ERROR "relay full"` |
| KEEPALIVE | Refresh registration on the same route and reply REFLECT without changing the active return path; otherwise `ERROR "not registered"` |
| ACTIVATE | Only a previously authenticated/registered secure carrier whose registered ID matches the frame ID may declare itself the device's active return path, acknowledged with REFLECT. Reject unregistered declarations or impersonation of another ID. Does not change roster, scopes, or business authorization |
| PROBE | Handle only authenticated routes with matching registered IDs; refresh route liveness without taking over the active path, returning a 1400-byte PROBE_ACK with the same nonce |
| PUSH_REG | The sender's registered ID must equal the frame ID, token ≤ 512 bytes; store `(kind, token)` even after registration expires |
| FWD | Determine sender from the registered route (DELIVER `src` cannot be forged); silently drop unregistered senders or unknown `dst`. If `dst` is offline but has a push token, run `--push-command <id hex> <kind> <token hex>` at most once per device per `--push-interval-secs` (default 60) |
| MB_PUT | Sender must be registered (otherwise `ERROR "not registered"`); `hash = SHA-256(item)`, deduplicated by hash; at most `--mailbox-items` (default 512) per key, evicting the oldest; a new key beyond `--mailbox-keys` (default 10000) returns `ERROR "mailbox full"`; success returns MB_ACK |
| MB_LIST | Requires authenticated registration; returns hashes of the **latest** 40 entries for that key |
| MB_GET | Requires authenticated registration; returns MB_ITEM or `ERROR "no such item"` |

- Registrations expire after `--idle-secs` (default 90) without a frame; scan every 30 seconds.
- Each registered device has two token buckets, for frame count and forwarded bytes (defaults 3000 frames/second and 30 MB/second, with 2× burst capacity). Excess frames are silently dropped and counted in `rate_limited`.
- Both Relay TCP / UDP 9443 must map to secure endpoints. Source addresses, NAT, and certificate domains must match the actual paths; internal loopback verification is not proof of public reachability.
- Deployment: `src/muster/deploy/relay/`. Historical Caddy / nginx WSS configuration cannot replace the new TCP / TLS and UDP / QUIC listeners and firewall mappings.

## 7. MagicSock

**Public dual-carrier path-selection objective**: Shared Rust maintains authenticated registration and health observation for TCP / TLS and UDP / QUIC concurrently, selecting one active carrier by availability, RTT, and hysteresis. Healthy standby stays warm; active failure switches immediately; failed carriers are probed with background backoff and reenter selection only after sustained health is confirmed. Background retries do not extend the 3-second connection/registration deadline or keepalive response deadline. If both fail, report failed. Carrier changes affect only encrypted-datagram routing and never resubmit business writes; unknown outcomes remain unknown. Exact probe intervals and hysteresis parameters follow the implementation and fault-injection tests. Legacy raw-UDP timing does not describe the new dual-carrier state machine.

The inner-frame handling and direct-path mechanisms below remain in use; public registration and keepalive belong to the independent carrier state machine above:

Constants: `PING = 0x10`, `PONG = 0x11`, `CONTROL_LEN = 41`; `PING_INTERVAL 1.5 s`, `PUNCH_INTERVAL 400 ms`, `DIRECT_TIMEOUT 6 s`, `CANDIDATE_INTERVAL 2 s`. The driver ticks every 200 ms; mailbox replies wait 5 seconds.

**Return-path declaration**: whenever client health selection changes `(carrier, generation)`, send `ACTIVATE{id}` on that authenticated carrier. If its queue is full, retain the pending declaration and retry on the next selection tick. Selection ticks every 100 ms. Even unchanged selections repeat idempotent ACTIVATE every second so a lost QUIC control packet cannot strand the return path. The server acknowledges with REFLECT; this does not update probe RTT. Standby initial registration, re-registration, and keepalive never steal the active return path. Inbound-only traffic must not depend on the next business write to trigger server switching. This declaration selects only encrypted-datagram transport, without elevating device authorization or resubmitting business requests.

**Public registration**: after each secure-carrier handshake send HELLO, then AUTH immediately on CHALLENGE. After AUTH's REFLECT, send a 1400-byte PROBE and receive a same-sized PROBE_ACK with the matching nonce before becoming a healthy candidate. Connection, registration, and the first full-frame probe share a 3-second deadline. Subsequently send the next PROBE 5 seconds after each successful probe, compute RTT only from a matching-nonce PROBE_ACK, and enforce a 3-second reply deadline. Failed carriers leave selection and reconnect in the background; ACTIVATE's REFLECT cannot replace probe replies. REFLECT is an outer-transport observation, not a direct inner hole-punching candidate. Shared Rust retains the latest `push_token` in secure-carrier state; every new registration generation restores PUSH_REG from that cache, and token updates also update the cache, independently of UI or lossy REFLECT presentation events. Registration and keepalive must not duplicate business input. The legacy test-only raw UDP channel's 10-second keepalive and 3-second HELLO retry apply only to explicit tests.

**Relay path**: send `FWD{dst, 0x20 || quic_packet}`. For DELIVER with `0x20`, the QUIC packet begins at frame offset 34 and is delivered to quinn using the peer's virtual address. DELIVER from an unknown `src` adds that peer to the table.

**Candidate addresses**: all local non-IPv6, non-loopback interface addresses with the local port (include loopback when `allow_loopback`), plus `extra_candidates`. Add reflected addresses only when actually probed through the same inner PeerLink socket. REFLECT from a separate outer QUIC socket cannot enter inner public candidates; TCP reflected addresses likewise cannot support UDP hole punching. Retain LAN direct paths; WAN NAT requires same-socket probing or correct binding and separate acceptance. Every 2 seconds send `FWD{dst, 0x21 …}` to peers without direct paths, except in `relay_only` mode. On receipt, record changed lists and reset `last_ping`; reply with the local list if at least 2 seconds have elapsed since the previous send.

**PING/PONG** (sent directly to candidates, not through Relay): exactly 41 bytes `[kind][sender id 32][nonce u64]`. Nonce is microseconds since socket startup; PONG echoes it without validation. There is no magic number: a datagram is a control frame if it is not from Relay, is 41 bytes long, starts with 0x10/0x11, and bytes 1..33 are a known peer ID. With QUIC bit greasing disabled, a QUIC packet never starts with 0x10/0x11. PING/PONG itself is unauthenticated; TLS provides authentication. Receiving either sets `by_direct[from] = id`; PING also sends PONG.

**Direct-path selection**: with candidates but no direct path, PING every candidate every 400 ms. The first PONG sets `direct`; only PONG from the current direct address refreshes `last_pong`. Ignore PONG from other addresses (do not switch while the current path lives). On a direct path, PING every 1.5 seconds; if `last_pong` exceeds 6 seconds, clear the path, fall back to Relay, and restart candidate exchange. quinn sees only virtual addresses, making path switches invisible to it.

**Inbound filtering**: parse packets from the Relay address as Relay frames; otherwise check for control frames first, then check whether the source address is in `by_direct`. If so, deliver with that peer's virtual address; otherwise drop.

**Virtual addresses** (local only, never transmitted): IP `10.id[0].id[1].id[2]`, port `u16(id[3], id[4]) | 0x0400`.

**Socket**: DF set (Linux `IP_PMTUDISC_PROBE`, macOS `IP_DONTFRAG`); no GSO/GRO; `may_fragment = false`.

## 8. Service Streams

- Message frame `[len u32][bytes]`; writer limit `MAX_MESSAGE = 4 MiB`, reader limit supplied by the caller. Clean EOF before a length prefix ends the stream.
- Each bidirectional stream starts with Hello: `[HELLO_VERSION u8=1][service str][version u32]`, at most 256 bytes, with a 5-second receiver wait.
- The receiver then writes **one raw byte**: `0x00 ACK_OK` or `0x01 ACK_REFUSED` (ending the stream after refusal). The initiator waits 10 seconds; timeout is a "service accept" error and any other byte is a protocol error.
- Admission: anyone may open `pair`; nonmembers cannot open other services. Members may open `roster`, `forward`, and arbitrary names when an application handler is installed. Built-in services ignore `hello.version`; musterd's handler requires `service == "muster" && version == 1`, dropping mismatches immediately after ACK_OK.

## 9. Pairing and Gossip

**pair** (`SERVICE "pair"`, version 1, `APPROVAL_TIMEOUT 300 s`; states 0 PENDING, 1 ACCEPTED, 2 REJECTED).

Joiner:

1. Fail if the invitation has expired.
2. `connect_pinned(admin_id)` through the invitation's Relay.
3. Open a `pair` v1 stream.
4. Compute the exporter (label `muster-invite`, 32 bytes, empty context).
5. Send `[invite_id 32][proof 32][record bytes(DeviceRecord)]`.

Administrator:

1. Read the request (≤ 8192 bytes).
2. `record.device_id` must equal the TLS peer ID; otherwise fail without replying.
3. Look up the invitation by ID (first removing expired invitations); if absent, return `REJECTED "no such invite"`.
4. Verify proof using the exporter from this side; on failure return `REJECTED "bad proof"`.
5. Require an installed approver; send `PENDING` (empty reason, no entries).
6. Wait at most 300 seconds for approval; on timeout simply end the stream.

Response format: `[status u8][reason str][count u16]([entry bytes(Entry::encode)])*`.

- Approval (the record may be edited): create Add on the current heads (assign a VIP for nonphones if unspecified), insert, and publish. Invalidate the invitation (rejection does not invalidate it). Return `ACCEPTED` with all accepted entries in topological order. `node.adopt(peer, conn)` converts the connection to a member link and starts gossip.
- Rejection: `REJECTED reason`.
- After servicing, if no invitation remains open, set `accept_unknown_clients = false`.
- Joiner receives ACCEPTED: `Roster::from_entries` (any order), requiring `network_id == invite.network_id` and its own active membership, then replaces its roster with the received roster.
- Accept inbound nonmember connections only while an invitation is open; otherwise close with 1 `not a member`.

**Roster gossip** (`SERVICE "roster"`, version 1); each side symmetrically:

1. Write local heads: `[count u16][hash 32]*`.
2. Read the peer's heads.
3. Write `entries_after(their_heads)`: `[count u16]([u32 len][Entry::encode])*`.
4. Read and absorb peer entries, retrying those with `MissingParents`.

Read limit: 4 MiB. Runs once when adopting a member link (`adopt`). `push_entries` uses the same four messages but sends only new entries and ignores the peer reply; `announce` pushes to each live link, waiting at most 700 ms, then `enforce_membership` closes inactive members' links with 1 `not a member` (the enforcement point for revocation ≤ 2 seconds).

**Mailbox cold start** (`deposit` / `sync_mailbox` in `node.rs`):

- key = `NetworkId`; item = `Entry::encode()` (≤ `MAX_FRAME - 64` bytes). Relay names it by the item's SHA-256, distinct from `EntryHash`.
- Published entries are immediately sent with MB_PUT when registered.
- On registration and every 10 minutes thereafter: MB_LIST → MB_GET each hash absent locally, verify `SHA-256(item) == hash`, decode and absorb per section 3.3 → MB_PUT accepted entries missing from Relay.
- This retrieves changes made while offline even with no peer online (`tests/mailbox.rs`). The list contains only the latest 40 entries; older entries arrive through gossip. Mailbox entries are visible to Relay (names, scopes, VIPs), a known metadata leak (SECURITY.md §8).

## 10. Forwarding Streams

`SERVICE "forward"`, version 1.

- Request (≤ 64 bytes): `[proto u8 = 1 (tcp)][port u16]`; reject other protocols.
- Authorization `authorize_port(state, peer, policy, Tcp, port)`: the peer is active, holds a scope covering `net:tcp/<port>` (same protocol or any, port None or equal), local `open_ports` has a matching rule, and the target is unlocked. net scopes are not high-risk and require no proof of presence.
- Status message (length-prefixed, initiator reads ≤ 8 bytes): `[0x00]` accepted, after which the target connects to `127.0.0.1:<port>` and the stream carries bidirectional raw bytes; `[0x01]` refused; `[0x02]` local connection failed.
- Default policy: `open_ports = [tcp/22]`.

## 11. vnet

- QUIC DATAGRAM payload is a raw IPv4 packet with no prefix byte; enabled only when `NodeConfig.datagrams` is true.
- TUN MTU 1150 (`tls::VNET_MTU`): one DATAGRAM carries it even at QUIC's 1200-byte minimum path MTU. Local VIP address, netmask from cidr; musterd defaults to `192.168.14.0/24`.
- Parsing: version 4, `ihl ≥ 20`; protocol 6/17 ports at `ihl+0..4`; "SYN-only" means `tcp flags & 0x12 == 0x02`.
- Outbound: find an active member by destination VIP (absent → `dropped_unknown_dst`; skip own VIP); record outbound flow `(proto, local, remote)` (TCP/UDP only); failed `connect(target)` → `dropped_no_link`; oversized `send_datagram` → `dropped_too_large`, and, for a first fragment with Don't Fragment that is not itself an ICMP error, an ICMP type 3 code 4 written back to the TUN from the destination VIP, carrying the link's current `max_datagram_size()` (`sent_too_big`).
- Inbound, in order: `src` must equal the sender's active VIP (otherwise `dropped_spoofed`; nonmembers are also dropped here); `dst` must be local VIP; then `admit()`: allow ICMP; drop protocols other than 6/17; if a reverse key matches an outbound flow, allow and refresh; if an inbound flow matches and this is not SYN-only, allow and refresh; otherwise record an inbound flow and allow if `authorize_port(proto, dport) == Allow`; otherwise `dropped_unauthorized`.
- Flow entries expire after 120 seconds, cleaned lazily on inbound packets. Counters: `tx_packets`, `rx_packets`, and the `dropped_*` counters above.

## 12. muster-rpc

- Frame `[len u32][protobuf Envelope]`, `MAX_ENVELOPE = 8 MiB`; oversized or undecodable frames close the connection. Local Unix sockets and `muster` v1 service streams (after Hello + ACK) use the same framing, with the same 8 MiB stream limit.
- `Envelope`：`uint64 id = 1`；`oneof msg { Request = 2, Response = 3, Event = 4, Cancel = 5 }`；`bytes presence = 6`；`string target = 7`。
- Error codes: 0 UNKNOWN, 1 UNAUTHORIZED, 2 NOT_FOUND, 3 INVALID, 4 UNAVAILABLE, 5 CONFLICT, 6 TIMEOUT.
- Proof of presence: `presence` signs `"MUSTER-PRESENCE-V1" || id u64 || prost(Request)` (no NUL separator; not `sign_domain`), verified directly using the member's roster `presence_key`. Required only when `authorize` returns NeedsPresence, namely for high-risk scopes `locked.control` and `network.admin`.
- `target`: only local principals may set it (otherwise `UNAUTHORIZED "only the device's own app may address other devices"`). musterd opens a `muster` v1 stream to `DeviceId::from_hex(target)` and forwards the envelope with `target` cleared but `id` and `presence` retained. Events and Responses return with the same ID; Cancel with target is forwarded similarly; stream-opening failure returns UNAVAILABLE.
- Call semantics: Request opens call `id`, Events stream back, and Response ends it. Cancel gives the handler a 2-second grace period, then returns `TIMEOUT "cancelled"`; an empty Request returns `INVALID "empty request"`.
