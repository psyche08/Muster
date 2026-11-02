# Identity and Trust

Part of the [Muster architecture](../ARCHITECTURE.md). Section numbers are shared across all parts, so a reference such as §3.6.2 means the same section wherever it appears.

## 3.4 Identity and Trust: One Model

**Keys** (all nonexportable hardware P-256):

| Key | Purpose | Platform implementation |
|---|---|---|
| Device Key | Mutual QUIC / TLS handshake authentication; admin roster-entry signing; directly used as the phone SSH client key (`ecdsa-sha2-nistp256`, decided) | SEP / StrongBox (fallback to TEE) / HUKS / TPM |
| Presence Key (optional, one per device) | Biometric ACL; signs high-risk operation intents: locked control, adding/removing members | As above, with `.biometryCurrentSet` / `setUserAuthenticationRequired` |
| Storage Key | ECIES wrapping of the local database DEK (reusing the existing sealbox approach) | SEP key agreement / Keystore |

`DeviceId = SHA-256(SPKI)`; display uses a short base32 fingerprint (reusing `enroll.Fingerprint`).

Presence Key signatures replace `LAContext` booleans. Targets verify cryptographic proof rather than platform-supplied `authenticated=true`. This implements AGENTS.md's prohibition on substituting native authentication booleans.

**Network = signed membership roster (Roster)**

```
NetworkId = SHA-256(Genesis entry)

Entry {
  network_id, parents: [EntryHash], lamport: u64,
  body: Genesis{name, vnet_cidr, creator: DeviceRecord}
      | Add{DeviceRecord}
      | Update{device_id, name?, role?, scopes?}
      | Remove{device_id}
  signer: DeviceId, sig: ECDSA-P256(domain="MUSTER-ROSTER-V1", canonical(entry))
}
DeviceRecord { device_id, spki, presence_spki?, name, kind: desktop|phone|server,
               role: admin|member, vip: IPv4?, scopes: Set<Scope> }
```

Roster rules:

- **The entry set only grows (2P-set)**. Roster state is a deterministic fold of valid entries:
  - Remove wins permanently; rejoining requires a new key;
  - Update selects the maximum `(lamport, hash)`;
  - An entry is valid only if its signer is an admin in its causal past;
  - Entries signed by a removed admin are invalid unless they are in the Remove entry's causal past, preventing stolen admin keys from backfilling entries.
- **Virtual addresses**: Genesis defines `vnet_cidr`, default `192.168.13.0/24`. An admin can sign `Readdress` for an existing network, atomically updating the subnet and all desktop/server addresses while preserving network identity, member keys, roles, and permissions. Restart desktop daemons afterward to apply the interface. The admin signing Add assigns desktop/server `vip`; phones have no vnet and an empty `vip`. If two admins concurrently assign the same address, the lower entry hash wins, and a subsequent Update readdresses the other device.
  - This eliminates manually maintained mappings in each machine's `agent.yaml`.
- **No expiry or renewal**, so creator unavailability affects no one (R2).
- **Admin is a role, not a single point of failure.** The creator is an admin in Genesis and may promote other desktops. Post-enrollment permission governance allows any admin to sign Update / Remove.
- **Distribution**:
  - Exchange roster heads on every PeerLink establishment and fetch missing entries;
  - Any member can upload entries to the Relay mailbox (key = NetworkId), fetched on cold start;
  - Relay may withhold entries (DoS), but cannot forge them.
- **Revocation**: On Remove, immediately close all links, service streams, and forwarding streams for that device and clear its vnet connection-tracking entries. For inbound connections, synchronize with the Relay mailbox first if the roster was last synchronized more than N seconds ago.

Desktop Presence may also be approved after pairing using an admin-signed
binding anchored to an empty DeviceUpdate epoch. This retains the v1 roster
wire format and existing identity/address; targets verify the binding against
the accepted epoch in Shared Rust. See [ADR 0006](../adr/0006-desktop-presence-and-console-routing.md)
and [Presence enrollment](../PRESENCE_ENROLLMENT.md).

**Scopes** (roster member-level permissions: what this device may do to other devices as an accessor; since 2026-09-29 admins configure each device under Chats / Agents / Terminals):

| Module | Scope | Capability |
|---|---|---|
| Chats | `chats` | Device-level switch; when off, all providers below are ineffective, but selected providers remain stored |
| | `chats:<provider>` | Ordinary desktop-App chats (`chatgpt`, `claude`, not their coding Agents). Service not yet implemented; authorization comes first |
| Agents | `agents` | Device-level switch, as above |
| | `agents:<provider>` | Agent sessions for that provider: viewing, preview, files, git, sending, approval, interruption (`codex`, `claude`, `catdesk`) |
| | `locked.control` | Device-level switch for control while locked; requires **both** target-local policy and a Presence signature. Also covers locked Chrome operations and cover |
| Terminals | `terminals` | Device-level switch: view/input all terminal sessions, create, close, resize |
| Other | `browser.view` / `browser.control` | Read/control Chrome pages |
| | `net:<proto>/<port>` | Target ports accessible to the member, e.g. `net:tcp/22` (SSH), `net:tcp/5900` (VNC / Screen Sharing), `net:*` (all). **Desktop vnet filtering and phone forwarding streams share this permission** |
| | `network.admin` | Sign roster entries (equivalent to role=admin) |

- Providers are stored as text (`[a-z0-9-]{1,32}`). Unknown providers still parse and sort consistently, but match no sessions; adding providers does not require upgrading the whole network first.
- Agent requests are evaluated against their session's provider; lists and full push updates are provider-filtered, and narrowed authorization ends existing push streams on the next event. Locked Use continuously checks the provider during challenge and execution.
- Legacy scopes `agent.view`, `agent.control`, `terminal.view`, and `terminal.control` remain parseable and retain their original all-provider meaning, but are no longer offered. Management lists them as "Earlier grants" and can remove them when saving new settings.
- `muster-app-core` projects groups, labels, summaries, and enrollment defaults (all three modules and providers on; locked control and other items off) into `AccessView`; platforms only render it.

**Target-local policy** (local configuration, not part of the network):

- Services offered locally;
- Ports exposed to the network (e.g. only 22 and 5900), filtering both inbound vnet packets and forwarding streams;
- Whether locked control is permitted;
- AX-controllable App allowlist: Codex, Claude, and Catdesk only after its independent profile and identity verification are complete.

One authorization function is used by service streams, forwarding streams, and vnet packet filtering:

```
allowed = roster.active(subject)
        && roster.scopes(subject) grant permission   // module providers require their switch
        && local_policy.allows(permission, target_state)
        && (!permission.high_risk || presence_proof.verify(subject, op))
```

`net:` authorization establishes only which device is connecting and whether it can reach that port. Application authentication, such as SSH keys and VNC passwords, still applies normally.

**Pairing**:

- **QR (primary)**: An admin displays `{network_id, admin_device_id, admin_spki, relay_url, invite_secret(128bit), expiry}`.
  1. The new device generates hardware keys, connects to the admin through Relay, and pins the peer using `admin_spki` from the QR.
  2. It proves possession of the invitation with `HMAC(invite_secret, TLS exporter)`.
  3. The admin UI shows Add 'xxx's iPhone' (phone), permissions: … (also assigned virtual IP for desktops); the user confirms with the Presence Key, then signs Add.
  4. The new device receives the complete roster; pairing finishes.
  
  The camera is itself a trusted out-of-band channel; **no SAS comparison is needed**.
- **No camera (desktop-to-desktop)**: Six-digit short code + PAKE (CPace / SPAKE2), also without SAS.

**Concepts entirely replaced by this model**:

- owner authority / owner registration / owner directory；
- GrantV2 / grant revocation；
- federation / authority orchestrator / authority pairing issuance；
- activation / freshness / sealed head / runtime readiness / protected state；
- first pair bootstrap / consumer enrollment / member enrollment / member listener bootstrap；
- device network distribution / enrollment / management / mobile join；
- network certificate / remote certificate (checkpoint, consumer, exchange, owner driver) / short certificate / TLS identity lifecycle / TLS broker;
- interaction binding ledger (replaced by operation ledger + Presence signatures);
- Go SPKI ledger and vnet mappings in `agent.yaml`;
- Single-network guard (one roster per device; simultaneous owner and consumer does not exist conceptually).
