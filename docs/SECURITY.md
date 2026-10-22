# SECURITY — Threat Model and Invariants

Implements the decisions in [ARCHITECTURE.md](ARCHITECTURE.md) §3.4, §3.5, and §7; see [PROTOCOL.md](PROTOCOL.md) for wire formats. Code lives in `src/muster/`. This document describes invariants of the new architecture. Public dual-carrier requirements were added on 2026-09-28; the two secure carriers, ACTIVATE, and full-frame nonce probes are implemented in the new Rust sources. Installation and public-network acceptance remain outstanding: neither the specification nor source code proves deployment. Legacy security modules are documented in `docs/history/`.

## 1. Assets and Objectives

| Asset | Protection Objective |
|---|---|
| Device private key (P-256, SEP / StrongBox / HUKS; software keys for development only) | Non-exportable; compromise of one device's key affects only that device |
| Membership roster (signed DAG) | Only active administrators may modify it; every participant can verify it independently; revocation is irreversible |
| Inter-device traffic (service streams, forwarding streams, vnet packets) | End-to-end encryption and authentication; Relay sees only ciphertext and routing information |
| Target-device capabilities (Agent control, terminal input, ports) | Available only to members granted the scope by the roster and allowed by local policy; high-risk operations require proof of presence |
| Local state (roster copy, ledger, policy) | Ambiguous mutations are not replayed after crashes; removed devices retain no old trust |

## 2. Adversaries

**In scope**:

- **Network attacker**: can observe, modify, replay, or drop packets on any link, including spoofing source addresses.
- **Malicious or compromised Relay**: has all network-attacker capabilities and knows registration status and traffic patterns for each DeviceId; can forge DELIVER, withhold mailbox contents, and report false reflected addresses.
- **Former member**: a formerly enrolled device holding its old roster copy and key, attempting access after removal.
- **Invitation holder**: someone who intercepts QR contents but is not the device the administrator intends to approve.
- **Access to a locked desktop**: a remote member requesting actions from musterd while the screen is locked (a supported product scenario; see section 5).

**Out of scope (decisions D3 and D2)**:

- Malicious software running under the same user account: it can read musterd's Unix socket, roster copy, and ledger, and is equivalent to the user.
- Compromise of the device OS or security chip itself.
- Migration from the old system: a clean cutover inherits no legacy trust state.

## 3. Roots of Trust

- **Inner device identity**: `DeviceId = SHA-256(SPKI DER)`, where SPKI contains the device's P-256 public key. A device certificate is only a self-signed key container: the verifier checks whether the peer SPKI hash is active in the roster (or equals the fixed pairing pin), not the DN, validity period, or certificate chain.
- **Public server identity**: TCP / TLS 1.3 and UDP / QUIC at `muster.linsheng.dev:9443` both use a Let's Encrypt certificate chain covering that domain. Clients perform normal WebPKI checks of the SAN hostname, validity period, and trusted chain; a DoH-resolved IP does not change SNI. The self-signed device-certificate verifier above must not be reused. This certificate authenticates only the Relay server, not inner hardware-device signatures, roster membership, or local authorization.
- The roster is a DAG of signed entries. Folding rules (`muster-core/src/roster.rs`): parents must already be accepted; the signer must be an active administrator in its causal past; Remove is permanent; Update uses last-writer-wins ordering by (lamport, hash); entries signed by a removed administrator outside the removal's causal past are invalid; VIP conflicts favor the smaller defining hash. Any insertion order yields the same state (`tests/properties.rs::fold_is_order_independent`).
- A `muster1:` invitation carries the network ID, administrator public key, Relay address, 16-byte secret, and expiration time. The joiner proves possession through an HMAC over the TLS exporter. Before approval, the administrator knows only a candidate record; approval is a human action (or an explicit unattended-server policy).

## 4. Invariants

Existing invariants list test or code locations. New objectives still require their corresponding acceptance checks; documentation does not replace testing.

| # | Invariant | Location |
|---|---|---|
| I1 | **Fail closed**: reject roster-read failures, missing policies, and signature-verification failures. | `Trust`/`authorize()`; `musterd::handler::authorize_request` |
| I2 | **Bounded parsing**: parsers never panic on arbitrary input and enforce size limits (Relay frame 1400, Envelope 8 MiB, bounded forwarding-stream hello). | `muster-core/tests/properties.rs`, `muster-net/tests/proto.rs`, `muster-proto/tests/framing.rs` |
| I3 | **Revocation ≤ 2 seconds**: once Remove is published, links to that device close within two seconds and it cannot reconnect. | `muster-net/tests/link.rs`; `scripts/vnet-netns-test.sh` |
| I4 | **No replay of ambiguous mutations**: mutations with `op_id` enter the ledger before side effects; crash-left `unknown` operations do not retry automatically; the same `op_id` with a different body conflicts. | `muster-store` Ledger; `musterd/tests/pair.rs` |
| I5 | **Relay is untrusted**: tampering, replay, and withholding can at most cause unavailability; Relay cannot join the roster or read plaintext. | `muster-net/tests/hostile_relay.rs` |
| I6 | **Members do not depend on the creator**: other members retain mutual access when the creator is offline. | `hostile_relay.rs::members_outlive_the_creator` |
| I13 | **Changes made while offline are retained**: even with no peer online, a device can fetch roster changes from the Relay mailbox and replenish entries missing from Relay. | `muster-net/tests/mailbox.rs` |
| I7 | **Display only provider-declared decisions**: approval options come from the provider (Codex `availableDecisions`, Claude `allow/deny/cancel`, or numbered onscreen choices), never invented by the UI. | Controllers in `muster-agent` |
| I8 | **No input broadcast**: iTerm2 `SendText` always uses `suppress_broadcast`. | `muster-terminal/src/iterm2.rs` |
| I9 | **Read back before and after writing**: the terminal or desktop editor must be empty before writing; confirm the echo before sending Return; an unknown result stops further input. | `muster-agent/src/iterm_cli.rs`, `muster-platform/src/macos/native_app.rs` |
| I10 | **Present user takes priority**: if the user's HID input count changes during a write, abort and mark the outcome unknown. | `muster-platform/src/macos/input.rs` |
| I11 | **One authorization point**: remote requests are decided once in `authorize_request`; service layers trust that decision. | `musterd/src/handler.rs` |
| I12 | **Removal means no network**: after local removal, `has_network=false`; discard the old roster before pairing again. | `musterd/src/network.rs` |
| I14 | **No public-carrier downgrade**: both TCP / UDP 9443 first verify domain TLS; certificate errors are never bypassed and never fall back to raw UDP / ws. | `muster-net/src/relay_tls.rs`; regressions must cover wrong hostname, expiration, untrusted chains, ALPN, and plaintext rejection |
| I15 | **Recovery does not replay business writes**: connection, registration, and the first full-frame probe finish successfully or fail within 3 seconds; healthy-standby switching and background recovery replace only the carrier, without resubmitting ambiguous operations. | Dual-carrier fault injection and OperationLedger regressions; public-network physical-device acceptance recorded separately |

## 5. Authorization Decisions

```
authorize(state, subject, permission, policy, target) =
  subject is an active member
  ∧ subject's scopes grant permission:
      Chat{p}   ← chats ∧ chats:p
      Agent{p}  ← (agents ∧ agents:p) ∨ legacy agent.view / agent.control (for view / control)
      Terminal  ← terminals ∨ legacy terminal.view / terminal.control
      others    ← matching scope (network.admin also requires role=admin; net matches port coverage)
  ∧ local policy allows permission (offer_agent covers Chats / Agents / browser; offer_terminal / open_ports / allow_locked_control)
  ∧ (while locked: permission is net or high-risk, or policy permits locked control)
  → high-risk permissions (locked.control, network.admin) additionally require proof of presence
```

- **Per-session provider decisions**: requests targeting an Agent session first require at least one Agents provider grant, then check that session's provider. If the session does not exist, only callers holding all providers continue to receive the service's own result; others receive NOT_FOUND. Lists and full pushes contain only visible providers' sessions. Open push streams recheck authorization before every event and end when authorization narrows.

- **Local principals** (Unix socket or embedded app) bypass scope checks: they are the user (section 2).
- **Proof of presence**: `Envelope.presence` is the caller's presence-key signature over `"MUSTER-PRESENCE-V1" || id || Request`. The presence public key is in the roster entry and is verified by the target. Phones sign only after local biometric authentication.
- **Forwarding streams and vnet packets** use the port form of the same function: `net:<proto>/<port>` scope plus local `open_ports`.

Temporarily unlocked Locked Use requires a narrower capability with independent domain separation; ordinary
`Envelope.presence` cannot be reused. `muster-core::locked_use` requires a target-issued nonce bound to the network, both endpoints'
DeviceIds, console, turn, and complete operation, consumed once within a 5–15-second monotonic deadline. During the active window,
authorization and local observations remain checked. The daemon uses exclusive console ownership; the native plugin's dedicated Unix socket
performs mutual UID and kernel audit-token code-signature checks. iOS/macOS App Core verifies challenge bindings
before requesting a Presence signature; mask/watchdog acknowledgments come from independent OS processes. See
[LOCKED_USE.md](LOCKED_USE.md); actual temporary OS unlocking still requires separate physical-device acceptance.

## 6. Boundaries

- **Outer Relay transport**: TCP / TLS length frames and UDP / QUIC DATAGRAMs first complete a WebPKI handshake with ALPN `muster-relay/1`. Missing certificates or mismatched private keys fail server startup, never opening a plaintext fallback port. Renewal atomically replaces a matching fullchain / key and updates both endpoints. Private keys are read only in the server certificate-loading path, never placed in clients, logs, or diagnostics.
- **Relay registration**: requires a signature over a Relay challenge (domain `MUSTER-RELAY-AUTH-V1`); forwarding routes only by DeviceId; QUIC payloads are opaque to Relay. Unregistered sources can trigger only HELLO/CHALLENGE, rate-limited by source address. Registered devices have frame and byte rate limits; registration and mailbox-key counts are bounded (`muster-relay/tests/limits.rs`). TCP / QUIC registrations for one device are isolated by carrier; standby registration and keepalive must not overwrite the active path. `ACTIVATE{id}` accepts only an authenticated route with a matching ID and confirms the return-path declaration with REFLECT. It changes neither roster, scopes, nor local policy and grants no business capability. Connection and registration share a 3-second total deadline. Shared Rust selects paths by health and RTT with hysteresis, switches immediately on failure, and probes failed paths in the background.
- **Mailbox**: each reader revalidates entries against roster rules; Relay withholding or forgery can at most prevent reads (`muster-net/tests/mailbox.rs`). Reads verify that `SHA-256(item)` equals the listed name.
- **Inner link**: quinn + rustls, ALPN `muster/1`, mutual certificates; nonmembers may open only the pairing service. Other service streams require explicit target ACK after Hello. QUIC bit greasing is disabled, GRO/GSO are absent, and DF is set to prevent Relay-frame misclassification.
- **Health-evidence boundary**: AUTH's small REFLECT does not prove that a link can carry business frames. PROBE (`0x0b`) and PROBE_ACK (`0x83`) are both exactly 1400 bytes, contain a matching u64 nonce, and strictly validate zero padding. Only replies with the correct nonce update health and RTT. The first probe shares the 3-second connection budget. Subsequent probes occur 5 seconds after each success with a 3-second reply deadline. ACTIVATE acknowledgment cannot impersonate a keepalive response.
- **Hole-punching addresses**: REFLECT establishes neither trusted identity nor the inner socket's NAT mapping. A reflected address from a separate outer QUIC socket or TCP cannot become an inner PeerLink public candidate. LAN direct connectivity remains available; WAN NAT requires same-socket probing or a verified correct binding.
- **vnet**: accepts packets only from that roster member's virtual IP (source validation); inbound traffic is filtered by scopes and `open_ports`, with connection tracking; unauthorized ports are silently dropped.
- **musterd Unix socket**: mode 0600, serving only the same user; `Envelope.target` forwarding is available only to local principals.
- **Controllers**: NativeApp first validates process code signature (`identifier "com.anthropic.claudefordesktop" and anchor apple generic and certificate leaf[subject.OU] = "Q6L2SF6YDW"`) and start time. The iTerm2 controller operates only on sessions referenced by Claude Code registration files (user-owned, not group/other-writable, single-link, ≤ 64 KiB).
- **App shells**: receive only ViewModels and send only Intents. Hardware-key and presence-key signatures use callbacks; private keys never leave the security chip.

## 7. Storage

- SQLite: identity (SEP blob or software key), roster entries, policies, kv, and ledger.
- Local storage encryption (deferred by D3): currently relies on OS user-directory permissions; software keys are for development only.

## 8. Known Gaps

1. Whether Electron accepts AX writes while locked (P0 ①); if not, the NativeApp channel falls back to read-only.
2. On 2026-09-28, both public TCP / TLS and UDP / QUIC endpoints moved to Let's Encrypt. Historical UDP without domain certificates and internal WSS checks describe only the old deployment. New client/server installation, public handshakes, negative certificate cases, renewal, failover, and real RPC require separate acceptance; see [NETWORK_DIAGNOSTICS.md](NETWORK_DIAGNOSTICS.md).
3. Mailbox roster entries are visible to Relay (device names, scopes, VIPs). These are metadata rather than content, but expose another layer beyond forwarded traffic; encrypting mailbox entries with a network key is future work.
4. iOS background push: Relay PUSH_REG and the `--push-command` hook, plus core's `SetPushToken` intent, are implemented. App-side APNs token acquisition and delivery to core are not wired yet. Tokens are opaque bytes to Relay, but it can correlate DeviceId and push token.
5. Codex Desktop's AX structure has not been probed; the NativeApp controller currently covers only Claude Desktop.
6. Relay rate limits count by DeviceId and source address and do not defend against distributed floods; that is an operational concern (upstream firewall), not solved by this protocol.
