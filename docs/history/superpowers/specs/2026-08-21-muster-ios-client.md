# muster iOS Client · Design Specification

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](../../../PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

> **2026-08-24 follow-up architecture note:** Phases 0–1.5 of this document remain the
> validation baseline for Apple SEP and the Muster transport path; Phase 2 onward
> has been superseded by the [AgentHalo Cross-Platform Architecture](../../architecture/agenthalo-cross-platform.md).
> AgentHalo is the main multiplatform product, Muster is an optional network layer,
> and Terminal connects through a Provider/Adapter abstraction rather than the
> direct SSH/SwiftTerm approach described here.

**Date:** 2026-08-21
**Status:** All spikes passed (simulator); design adversarially reviewed and corrected; implementation awaits real-device validation
**Related:** [muster CLAUDE.md](../../../../src/CLAUDE.md), [Hardware-Rooted Identity Specification](2026-08-13-private-tunnel-hardware-identity.md)

> **In one sentence**: The phone becomes a first-class muster device: SEP hardware identity,
> inner mTLS with mutual SPKI pinning, and authorization/revocation by the agent's ledger.
> The relay needs no changes and learns nothing.
>
> **The spike proved the entire chain works in Swift** (simulator + interoperability with
> muster’s own `internal/innerchan` code): SEP identity → self-signed container certificate →
> SecIdentity → mTLS 1.3 → handwritten yamux (interoperable with real hashicorp/yamux,
> including flow control/keepalive/reverse streams) → MST1 forward stream → swift-nio-ssh to sshd.
>
> **The only unverified environment is a real device**: this machine has no iPhone development
> certificate. See §Real-Device Validation Pending.

---

# muster iOS Client Design

## 0. One-Sentence Conclusion

**It is feasible, and cleaner than the macOS implementation**: the spike has proved the whole chain (SEP identity → relay WSS → inner mTLS+SPKI pin → yamux → MST1 → SSH/HTTP) works in Swift, against muster’s **actual production code** (`internal/innerchan`), without changing the relay or protocol or adding an op.

**The biggest risk is not a technical unknown: iOS background suspension is certain**. As soon as the app enters the background, it is suspended, sockets stop sending/receiving, and the agent’s `pingMaxMisses=2` × 30s ping kills the session within ~60s. This cannot be optimized away; the design must absorb it. Terminal continuity must be delegated to agent-side `tmux`; the app handles “reconnect on foreground + reattach.” Product expectations will keep failing unless this is accepted.

A secondary risk, and **the only technical link the spike has not proved**, is attaching the SEP `SecKey` to `NIOSSLCustomPrivateKey` to sign the inner TLS CertificateVerify. The API fits (see §8.1), and spike 1 has already proved a fallback path, so the risk is bounded, but it must be resolved on day one of Phase 1.

---

## 1. Architecture

### 1.1 Invariants (Mandatory for the App)

| Plane | muster rule | iOS app obligation |
|---|---|---|
| Rendezvous (`muster.linsheng.dev:8443`) | Blind forwarding only; holds no keys and performs no authentication | Outer layer sends only `GET /v2/connect/<user>/<device>` + standard WebPKI TLS; **no token, no client certificate**. The relay learns only that “two pipes were connected” |
| Inner mTLS | Mutual SPKI pinning; certificates are merely key containers, with no CA, chain validation, or validity-period semantics | Client pins agent SPKI and presents its own self-signed container certificate; **a pin mismatch never triggers automatic retrust** (consistent with `describeConnectError`) |
| Authorization | Agent-owned sealed ledger (`muster-agent authorize add/remove`); revocation takes effect within ≤2s | No app “credential” bypasses the ledger. Phone admission = possession of the SEP private key **and** that SPKI being in that Mac’s ledger |
| Ports | Agent’s `forwards:` allowlist determines reachable ports | App may request only ports present in `forwards`; requesting an absent port closes that stream (without affecting the session) |

### 1.2 Layers and Swift Components

Recommended pipeline: **use NIOTransportServices (Network.framework) for the outer Channel, then NIO throughout from inner TLS upward**.

```
┌─ MusterApp (SwiftUI) ──────────────────────────────────────────┐
│  AgentListView   TerminalScreen   WebScreen   IdentityScreen   │
└──────────────┬────────────────────┬───────────────┬────────────┘
               │                    │               │
┌──────────────┴────────────────────┴───────────────┴────────────┐
│ MusterKit (local SPM package, no UI)                           │
│                                                               │
│  Consumers/  SSHSession(NIOSSH)      LocalHTTPProxy(CONNECT)    │
│                    │                        │                 │
│              ┌─────┴────────────────────────┴─────┐           │
│  Session/    │ AgentSession                        │          │
│              │  openForward(port) -> Channel       │          │
│              │  Answer agent ping streams (required)│         │
│              └──────────────┬──────────────────────┘          │
│  Mux/        MST1Codec  +  YamuxMultiplexer → child Channel    │
│  Inner/      InnerChannel: NIOSSLClientHandler + SPKI pin      │
│  Identity/   SEPSigner: NIOSSLCustomPrivateKey ←── SecKey(SEP) │
│  Outer/      WSByteStream (WS binary frame ⇄ ByteBuffer)       │
│              RelayLink: NIOTSConnectionBootstrap + WebPKI TLS  │
│              + NIOHTTP1 upgrade + NIOWebSocket frame codec     │
│  Store/      AgentStore (agents / pins / services)             │
│  Control/    Reconnector (backoff + error classification)      │
└───────────────────────────────────────────────────────────────┘
```

Exact responsibilities:

- **`MusterIdentity`** (Security.framework; port spike 1’s `swift/Identity.swift` directly)
  `SecKeyCreateRandomKey(kSecAttrTokenIDSecureEnclave, kSecAttrIsPermanent, SecAccessControl(.privateKeyUsage, kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly))` → manually assemble DER SPKI → handwritten self-signed X.509 container certificate (v3, random 16B serial, `CN=muster-identity`, no SAN, critical `KeyUsage=digitalSignature`, 100-year validity, GeneralizedTime for `NotAfter`). Semantics match `innerchan.LoadOrCreateIdentity` 1:1: cache the certificate in Keychain and re-sign if SPKI does not match.
  Expose only three values: `spkiSHA256Hex` (64 hex, the argument to `authorize add --spki`), `fingerprint` (matching `enroll.Fingerprint`: first 10 bytes in base32, grouped by four characters), and `certificateDER`.
  **No need for macOS’s `muster-sep-helper` subprocess + CryptoKit blob.** iOS apps already have Keychain access; `internal/keysource`’s helper/blob mechanism has no counterpart on iOS and is unnecessary.

- **`SEPSigner: NIOSSLCustomPrivateKey`**
  `signatureAlgorithms = [.ecdsaSecp256R1Sha256]`; `sign(channel:algorithm:data:)` receives **unhashed plaintext**, so use `SecKeyCreateSignature(key, .ecdsaSignatureMessageX962SHA256, data)` (internally SHA-256, DER X9.62 output). Sign on a background queue, then hop the result back to `channel.eventLoop` (the protocol explicitly prohibits blocking in the callback).
  Declare only P-256/SHA-256, equivalent to `SupportedSignatureAlgorithms = [ECDSAWithP256AndSHA256]` in Go’s `ClientTLS`: replacing the signer does not change negotiation.

- **`RelayLink`** — `NIOTSConnectionBootstrap` + `NWProtocolTLS` (ordinary Let's Encrypt WebPKI outside, no pin and no client certificate) + `NIOHTTP1` + `NIOWebSocketClientUpgrader`. Choose NIOTS over `URLSessionWebSocketTask` because both SSH and the web proxy need **child Channels**; an all-NIO pipeline eliminates the spike’s two loopback bridges. NIOTS uses Network.framework underneath, retaining cellular/Wi-Fi switching and path-availability events.

- **`WSByteStream`** — Match `internal/tunnel/wsconn.go` precisely: **each write sends one complete binary message; concatenate inbound binary payloads into a byte stream; silently discard non-binary frames**. No extra framing; chunking is unconstrained. Spike 2 verified this byte-for-byte compatibility point.

- **`InnerChannel`** — `NIOSSLClientHandler`, `minimumTLSVersion = .tlsv13`, `maximumTLSVersion = .tlsv13`, `certificateChain = [containerCertificate]`, `privateKey = NIOSSLPrivateKey(customPrivateKey: SEPSigner)`.
  **`certificateVerification = .noHostnameVerification` (not `.none`)**: spike 2 found this security trap. `.none` makes BoringSSL use `SSL_VERIFY_NONE`, **ignoring** `.failed` from the custom verification callback. The resulting device silently lacks pinning, enabling exactly the MITM that v2 is designed to prevent.
  In the custom callback: take leaf → SHA-256(SubjectPublicKeyInfo) → constant-time pin comparison.

- **`YamuxMultiplexer` / `YamuxStreamChannel`** — The spike’s actor implementation is a probe, not a product (`Data` copies at each hop, one continuation per stream, no session-level write backpressure). Rewrite production code as a NIO handler + multiplexer (shaped like `NIOHTTP2` StreamChannel), consuming `ByteBuffer` directly to solve all three problems together.
  **Working code fixes the wire behavior; reproduce it rule by rule**: initial window 256 KiB / `MaxStreamWindowSize` 1 MiB; return credit when delta ≥ max/2; open streams with `WINDOW_UPDATE|SYN`, client uses odd IDs; FIN is a flag on window-update frames; **`typePing|SYN` on stream 0 must receive `typePing|ACK` echoing length** (otherwise Go returns `ErrKeepAliveTimeout`); **accept inbound streams** (the agent proactively opens `{"v":2,"op":"ping"}`).
  Do not implement the server role, `StreamOpenTimeout`, `Shrink`, or the ACK backlog semaphore: muster’s usage does not need them.

- **`MST1Codec`** — `"MST1"` + BE uint16 len + JSON, `maxHeaderJSON=4096`. Outbound: `{"v":1,"port":N}`. Inbound: `{"v":2,"op":"ping","nonce":S}` → reply on the same stream with `{"v":2,"op":"pong","nonce":S}`. **No pong means killing your own connection**: the agent’s `pingLoop` runs every 30s and closes the session after two consecutive misses.

- **`AgentSession`** — One inner session per agent; multiplex all SSH tabs and web requests over it. **Never open a relay pipe per terminal tab** (`MaxPipesPerAgent` limits relay pipes, and each pipe requires another SEP-signed handshake).

- **`Reconnector`** — Copy `cmd/muster/connect.go`: backoff `initial 1s / max 30s / jitter`; reset after `healthySession = 30s`; `rejectionGrace = 750ms` (TLS 1.3 client handshake returns before a ledger rejection arrives, appearing as instant session death or `SSLV3_ALERT_BAD_CERTIFICATE` on first read). Match all four `describeConnectError` classes exactly; **pin mismatch is always terminal and never retried**.
  Remember relay rate limiting: per-agent token bucket, burst 60 and 1/s; an uncontrolled reconnect loop receives 429.

- **`SSHSession`** — Install `NIOSSHHandler` directly in the yamux child-channel pipeline (swift-nio-ssh works on **any** Channel, not just sockets), with `PseudoTerminalRequest` + `ShellRequest`; `feed` output to SwiftTerm, write back `TerminalViewDelegate.send`, and send `window-change` on resizing/rotation.
  SSH authentication uses a **separate** key, optionally in SEP (`NIOSSHPrivateKey(secureEnclaveP256Key:)` exists), with its public key in the target Mac’s `~/.ssh/authorized_keys`. Do not confuse it with muster identity.

- **`LocalHTTPProxy`** — CONNECT proxy bound to `127.0.0.1:<ephemeral>` (spike’s `MiniHTTPProxy`, ~120 lines). Map `host:port` through a table to `(agent, agentPort)`, open a forward stream, then copy raw bytes bidirectionally, **without parsing HTTP**. Attach it to WebView via `WKWebsiteDataStore.proxyConfigurations` (iOS 17+, confirmed in SDK 26.5).
  Choose this over “loopback listener + URL rewriting” for correctness: rewriting collapses every service onto the same `127.0.0.1` origin, causing real bugs when services share cookie names. CONNECT preserves real hostnames and normal cookie / `Origin` / CORS / `localStorage` / absolute-link behavior. It is scoped to one data store rather than system-wide and needs no entitlement.
  `URLProtocol` is a dead end (WKWebView networking runs out of process and ignores registration); `WKURLSchemeHandler` cannot intercept `http`/`https`.

- **`AgentStore`** — One record per agent: `name("<user>/<device>")`, `pin(32B)`, `displayName`, `services: [{name, host, port}]`, `sshDefaults`. Persist as a JSON file in the app container, protected by `.completeUntilFirstUserAuthentication` (background reconnection needs readability after first unlock). Pins are public data; they need **integrity**, not secrecy. Anyone able to modify the app container already owns the app, so Keychain storage is unnecessary.

---

## 2. Enrollment Flow

### 2.1 Two Directions, Two Mechanisms (The Key Point)

Enrollment transfers information in **two independent directions**, with different ideal carriers:

**Direction A: phone SPKI → three Macs (authorization)**
After generating its SEP identity, the phone displays `spki_sha256` (64 hex) + fingerprint. The operator must run this on **each** Mac they want to access:

```
muster-agent authorize add --spki <64-hex> --name iphone
```

All three Macs use the **same** SPKI, so transfer it once and paste it three times.

**Recommended carrier: Universal Clipboard** (tap Copy on the phone, then ⌘V on the Mac). Zero code, zero friction. Alternatives: make the complete `authorize add` command copyable, or AirDrop a `.txt` file.

**QR is wrong in this direction**: Macs have no convenient scanner; scanning a phone screen with a Mac is much harder than copy/paste.

**Direction B: agent pin + name → phone (trust)**
This is the difficult direction: each agent has a 64-hex pin, and manually entering three is unacceptable.

**v1 answer: compile all three pins into app constants**, since they are already known invariants:

```
example/mac-a   example-identity-fingerprint
example/mac-b   example-identity-fingerprint
example/mac-c  example-identity-fingerprint
```

For a personal tool, distributing pins with the binary is a legitimate trust-distribution mechanism (equivalent to the `--pin` argument and stronger than TOFU). Add a “manually add agent” form (name + pasted pin) as a fallback.

**If a fourth agent is added later**, QR becomes useful in the direction **Mac displays, phone scans** (phone has a camera; Mac has a screen). Add `--qr` to `muster-agent id`, rendering a terminal QR with `muster://agent?name=example/xxx&pin=<hex>&relay=wss://muster.linsheng.dev:8443`. This is a convenience feature after Phase 4, not a requirement.

### 2.2 Explicitly No TOFU

Do not port interactive TOFU from `muster connect --trust-first` to the phone. There is no trusted “out-of-band fingerprint comparison” context on the phone, and pins can already ship with the app. **No app path** may automatically trust a new key after a pin mismatch.

### 2.3 How Revocation Appears

After `muster-agent authorize remove --fingerprint F`:

1. The agent’s per-session recheck (1s ledger cache + 1s recheck) kills active sessions within **≤2s**.
2. On the phone: yamux session dies → EOF on every SSH child channel (terminal shows “session closed”), and all in-flight web requests fail.
3. `Reconnector` retries: outer WSS establishes normally, inner TLS handshake “succeeds,” then instant death within `rejectionGrace` / `SSLV3_ALERT_BAD_CERTIFICATE` on first read → classify as **“the agent rejected this device’s key.”**
4. Show a dedicated UI banner (not a generic error), including local SPKI and the command to run on the Mac.
5. **Retry policy must distinguish two cases** (copy `connect.go`’s heuristic):
   - **Never connected** to this agent → stop retrying; show “Unauthorized; run authorize add.”
   - **Previously connected**, now dies instantly → continue with backoff. Agent restarts, transient ledger read failures (fail-closed kills the handshake), and relay instability are more common than real revocation. Real revocation keeps failing and keeps the banner visible.

### 2.4 Identity Destruction

- **Delete app = destroy identity** (iOS 10.3 onward clears Keychain on deletion; the spike tested only the simulator, where it did not clear. This real-device behavior difference must be confirmed; see §8).
- SEP keys **do not** sync through iCloud Keychain (device-bound); a new phone means a new identity and reauthorization on all three Macs.
- No backup/restore path, intentionally. Explain this clearly on IdentityScreen.
- The old SPKI left in the ledger is stale; remove it with `authorize remove`.

---

## 3. UI Shape

Four screens. This is a personal tool; avoid extras.

**① Agents (root List view)**
One row per agent: name, status dot (`connected` / `connecting` / `offline` / `rejected` / `pin-mismatch`), latest ping RTT.
Tap for agent details: a **Terminal** button and the agent’s service list (opens WebView).
Show error states in the row, not hidden in dialogs. `rejected` and `pin-mismatch` need distinct messages.

**② Terminal**
Fullscreen `SwiftTerm.TerminalView` (wrapped in UIViewRepresentable). Bottom accessory key bar: `Esc` `Tab` `Ctrl` `↑↓←→` `|` `~` `/`. Multiple sessions use a horizontal tab bar at the top, with no split panes.
**Make the default command a per-agent setting, defaulting to `tmux new -A -s muster`**: the only honest answer to iOS background suspension (§8.2).

**③ Web**
`WKWebView` with a proxied data store. Minimal address bar at the top (current service name, switchable). Back/forward/refresh. No bookmarks or multiple tabs; the service list serves as bookmarks.

**④ Identity / Settings**
- Local `spki_sha256` (64 hex, large monospace) + fingerprint + **Copy** button + complete `muster-agent authorize add --spki …` command (also copyable).
- Relay URL (editable, default `wss://muster.linsheng.dev:8443`).
- Edit agent list: name / pin / service table.
- **Regenerate identity** (destructive, second confirmation, explicitly warn that all three Macs need reauthorization).
- Recent log tail (connect/disconnect/rejection/pin events) for troubleshooting.

Out of scope: accounts, multiple users, notifications, iPad adaptation, widgets, file browser, SFTP.

---

## 4. Dependencies

### Third Party (All SPM)

| Dependency | License | Why required |
|---|---|---|
| `apple/swift-nio` | Apache-2.0 | Channel / ByteBuffer / pipeline / EventLoop: skeleton for the entire inner pipeline; `NIOHTTP1` + `NIOWebSocket` also handle outer WS handshake/framing |
| `apple/swift-nio-transport-services` | Apache-2.0 | Outer dialing through Network.framework (cellular/Wi-Fi path management, ATS, the correct iOS transport), producing a real Channel |
| `apple/swift-nio-ssl` | Apache-2.0 | **Irreplaceable**. Inner TLS must run over a byte pipe we own; Security.framework / Network.framework only provide TLS for connections they own, with no “TLS over these bytes” API. `NIOSSLClientHandler` consumes `ByteBuffer` and can sit on anything. Cost: bundled BoringSSL (a few MB) |
| `apple/swift-nio-ssh` (0.15.0) | Apache-2.0 | SSH client on any Channel, fitting the yamux child channel; libssh2 is C, requires manual lifecycle management, and still needs a Channel bridge |
| `migueldeicaza/SwiftTerm` | MIT | Only mature native terminal emulator. Transport-independent `TerminalView` (UIKit): `feed(byteArray:)` in, `TerminalViewDelegate.send` out, directly matching NIOSSH child channels. Other options wrap WKWebView + xterm.js, putting WebView in the hot path |

**Explicitly excluded**:
- `swift-crypto`: iOS includes CryptoKit; use system SHA-256 (the spike used it only for cross-platform support).
- **Any gomobile / Go xcframework**: the entire Swift stack is already proven; adding Go introduces a Go runtime and a second TLS stack, and Go cannot use `SecKey` directly (the reason macOS needs a helper subprocess).

### Handwritten

| Component | Source |
|---|---|
| `MusterIdentity` | Spike 1 `swift/Identity.swift` is essentially production-ready |
| `SEPSigner` (`NIOSSLCustomPrivateKey`) | New, ~40 lines, **only unverified component** |
| yamux (NIO handler + multiplexer) | Spike 2 `Yamux.swift` defines correct wire behavior, but needs rewriting over `ByteBuffer` for zero-copy + backpressure |
| `MST1Codec` | 65 lines, matching `internal/innerchan/codec.go` |
| `WSByteStream` | Matches the three rules in `internal/tunnel/wsconn.go` |
| `LocalHTTPProxy` | Spike 2 `MiniProxy.swift`, ~120 lines, already usable |
| `Reconnector` + error classification | Matches `cmd/muster/connect.go` |
| All UI | New |

---

## 5. Distribution

### Current State and Required Work

On this machine, `security find-identity -v -p codesigning` lists only two **Developer ID Application** certificates (Mac distribution), which cannot sign iOS device apps. An **Apple Development** certificate is required.

**The user must personally perform these steps** (I cannot do them on their behalf):

1. Xcode → Settings → Accounts, sign in with team **89LGY6BD53**.
2. Open the project; in Signing & Capabilities select **Automatically manage signing** and that team.
3. Connect the phone (or enable wireless debugging). Xcode automatically creates an Apple Development certificate (private key in the **login Keychain**) → registers the device UDID → generates a development provisioning profile → installs.
4. On first launch, trust the developer in Settings → General → VPN & Device Management on the phone.

**Keychain note**: step 3 writes to the login Keychain, which must be unlocked. Under the global rules, I will never run `security unlock-keychain` or any equivalent action; the user must complete this in their own Xcode session. A build error `errSecInternalComponent` is something to report, not bypass.

### Bundle ID: **Finalize in Phase 0**

Recommended: `dev.linsheng.muster.ios` (do not reuse the spike’s `.spike`).
Keychain access group: `89LGY6BD53.dev.linsheng.muster.ios`.
**Changing the bundle ID later orphans the SEP identity** (the access group changes and the key becomes inaccessible), requiring reenrollment. Decide now.

Only `application-identifier` and matching `keychain-access-groups` entitlements are needed. No Network Extension, App Groups, or ATS exceptions (inner TLS uses BoringSSL outside ATS; outer transport is standard HTTPS).

### Validity and Expiry

- For a **paid team**, Apple Development certificates and development provisioning profiles last **one year** (seven days applies to free personal teams, not here).
- Profiles bind a device UDID list; replacing the phone requires registering it and re-signing.
- **Expiry behavior**: the app refuses to launch (“no longer available”). **Fix**: open Xcode, connect the phone, and Run once; automatic signing renews the profile. Remember to do this annually.
- No OTA update channel: connect the phone and Run after every code change.
- **Optional alternative**: internal TestFlight testing (App Store Connect record + upload + builds expire after 90 days). Xcode development signing is more appropriate for a personal tool: one less Apple review step and no new upload every 90 days.

---

## 6. Phased Implementation

Every phase is independently demonstrable, with executable validation criteria rather than “looks right.”

### Phase 0 — Skeleton and Signing (Half a Day)

Create the project, finalize the bundle ID, resolve SPM dependencies, and run an empty app on a real device.

**Validation**: the empty app launches on the phone and displays a line of text. This also proves the signing path works; all later phases depend on it.

---

### Phase 1 — Identity + Connectivity + One Command (Minimal End-to-End Proof)

The goal is **proving the whole chain on a real device over cellular**, not a polished app. UI: one button and one `TextView`.

Work, in order:

**1a. Real-device SEP identity** (first, because it can invalidate assumptions)
Run `MusterIdentity` on the device and display SPKI hex.

*Validation*:
- `kSecAttrTokenID == "com.apple.setoken"`;
- Instrument one `SecKeyCreateSignature` call (expected <20ms; hundreds of ms would require reconsidering TLS handshake UX);
- Kill/reopen app → same key; reboot phone → same key;
- **Delete/reinstall app → does the key disappear?** (expected yes; it persists on the simulator, so only hardware can verify this);
- Also confirm `.userPresence` can be created on hardware, **but never use it for TLS signing**: Face ID on every reconnect guarantees background reconnect failure. TLS identity always uses `.privateKeyUsage` + `afterFirstUnlock`.

**1b. Prove `SEPSigner` offline** (the only unverified technical link; test the shortest loop first)
Do not use relay. Run the spike’s `go/innerpeer` (which imports the real `internal/innerchan`) as a bare TCP listener on the Mac; connect the phone over the same Wi-Fi and run inner mTLS.

*Validation*: Go logs `handshake OK: version=0x0304 cipher=0x1301`, with `client spki_sha256` matching the phone. Go completing TLS 1.3 means the phone’s CertificateVerify was signed by the SEP private key and verified: proof that SEP actually drives TLS client authentication.
**If this fails, immediately use the fallback in §8.1; do not pile on more code.**

**1c. Full path: relay → inner → yamux → SSH exec**
Target **`example/mac-b`**: its `forwards:` already has `22: "127.0.0.1:22"`, requiring **zero configuration changes**.
After connecting, open a `{"v":1,"port":22}` forward stream, exec `hostname; uname -a` through NIOSSH, and display results in TextView.

*Validation* (all four):
1. Run `muster-agent authorize add --spki <hex displayed on phone>` on mac-b, then **disable Wi-Fi and use cellular only**; app prints `Mac-B.local`.
2. Connect **before** authorization → app says “this agent rejected this device’s key,” includes the command, and **does not retry forever**.
3. Deliberately alter one pin character → PIN MISMATCH, terminal, no retry.
4. While connected, run `muster-agent authorize remove` on mac-b → app shows session disconnected **within 2 seconds**.

**1d. Check an environmental risk now**: verify cellular outbound access to `muster.linsheng.dev:8443` (non-443 ports may be blocked by carriers/corporate Wi-Fi; see §8.4). This is cheap and worth discovering early.

---

### Phase 2 — Terminal

Wire up SwiftTerm, PTY shell, accessory key bar, resize/rotation (`window-change`), and foreground reconnection.

*Validation*:
- Run `htop` and `vim` over cellular; colors, cursor, and scrolling work;
- Rotate the phone → remote `stty size` changes accordingly;
- **Lock for 60 seconds, then return** → observe and record actual behavior (expected: session died; foreground reconnects automatically; `tmux new -A -s muster` restores the original session). This is a mandatory core product acceptance test.

---

### Phase 3 — Web

`LocalHTTPProxy` + WKWebView + per-agent service table.

**Operational prerequisite** (not code): add every desired web service to `forwards:` in the corresponding agent’s `agent.yaml` and restart the agent. Today mac-a has only `8765` (agenthalo backend, still a Unix socket), and mac-b only `22`.

*Validation*:
- Load a service on mac-b through the proxy over cellular; page works;
- Open two different services (different hostnames) concurrently and verify cookie / `localStorage` isolation. This is why CONNECT was chosen over loopback rewriting; prove it experimentally;
- WebView refresh recovers after network loss/reconnection.

---

### Phase 4 — Three Agents + Polish

`AgentSession` manager (three concurrent sessions), Agents list status, IdentityScreen, pin editing, error banners, foreground/background policy (`beginBackgroundTask` buys ~30s to send yamux GoAway then close WS; the spike’s `goAwayAndClose` cancels before sending the close frame, so Go sees abnormal closure 1006; fix this), and log tail.

*Validation*: all three agents online simultaneously; terminal works on each; revoking one affects only it; killing an agent process changes its row to `offline` (relay returns 502), leaving the other two unaffected.

---

## 7. Honest Risks and Unverified Items

### 7.1 `NIOSSLCustomPrivateKey` + SEP `SecKey` — The Only Unverified Technical Link

The API fits; confidence is high but risk is nonzero:
- `data` in `sign(channel:algorithm:data:)` is **unhashed plaintext** (explicit in source comments), and `algorithm` will be `.ecdsaSecp256R1Sha256`;
- `SecKeyCreateSignature(key, .ecdsaSignatureMessageX962SHA256, data)` does exactly “SHA-256 + ECDSA + DER X9.62 output”;
- Callback runs on the event loop and **must not block**; hop SEP signing to another queue and back.

**If this does not work, spike 1 already proved a fallback**: replace inner TLS with Network.framework, using `sec_identity_create(SecIdentity)` + `sec_protocol_options_set_local_identity` + `set_verify_block` for pinning, over a custom `NWProtocolFramer` (spike 1 verified successful `TLS over PassThroughFramer over TCP` handshakes). Cost: handwrite outer WSS RFC 6455 frames in the framer, lose the NIO pipeline, and build another Channel bridge for SSH. **Feasible but expensive.** Risk is bounded, but settle it in Phase 1b.

Remember the Network.framework trap: a pin mismatch puts `NWConnection` in `.waiting(-9808)` rather than `.failed`; it treats this as retryable and reconnects forever. muster never retries authentication failures, so treat TLS errors in `.waiting` as terminal and `cancel()`.

### 7.2 iOS Background Suspension — A Fact, Not a Risk

Within seconds of backgrounding, the app is suspended and sockets stop being serviced. Agent ping every 30s × `pingMaxMisses=2` → **session is guaranteed to die within roughly 60 seconds**.

- **No legitimate mechanism lets a sideloaded app maintain arbitrary background TCP connections**. `NEAppPushProvider` needs the `app-push-provider` entitlement under `com.apple.developer.networking.networkextension` (Apple grants it by use case), and documentation says it works **only on Wi-Fi**, useless for a primarily cellular phone. VoIP background mode is deprecated (PushKit now mandates CallKit reporting).
- **Conclusion: design for reconnection, not residency.**
  - Automatically reconnect on foreground (with backoff);
  - **Delegate terminal continuity to `tmux`**: the only honest answer;
  - Use `beginBackgroundTask`’s ~30s for graceful shutdown (GoAway + WS close), not life support;
  - Web is request-oriented; reconnecting has little visible impact.
- **Unverified**: exact time from backgrounding to suspension, and whether NIOTS connections survive a brief background interval (e.g. checking a notification). Measure and record in Phase 2.

### 7.3 Real-Device SEP Behavior (Spike Tested Only on Simulator)

The simulator emulates SEP in software: same `isAvailable = true` and `tokenID`, but **no hardware non-exportability**. Three checks require hardware: actual hardware residency, acceptable signing latency, and **whether deleting the app removes its Keychain identity** (the item unexpectedly survives `simctl uninstall`; hardware is expected to behave oppositely). The last directly determines product semantics (delete app = identity destruction = reauthorize on all three Macs).

### 7.4 Cellular / NAT / Ports

- **`:8443` is nonstandard**. Some carriers, corporate Wi-Fi, and hotels block outbound traffic other than 443. The spike ran on a Mac (through a local HTTPS proxy to bypass Fake-IP hijacking); **cellular has not been tested**. Test in Phase 1d.
  If blocked, nginx already terminates TLS; adding a 443 server block using the same route as a fallback endpoint is almost free.
- **Cellular NAT rebinding / Wi-Fi↔cellular switching** silently breaks TCP. Relay’s `deviceWriteTimeout` reclaims half-open pipes; the phone recovers by reconnecting. NIOTS path-change events can trigger early reconnection instead of waiting for timeout.
- **Relay rate limit**: per-agent token bucket, burst 60 / 1 per second. Separate buckets for all three agents easily support normal reconnection, but a runaway tight loop gets 429; backoff must actually work.

### 7.5 `forwards:` Allowlist Is a Hard Constraint

The phone can reach only ports in the agent’s `forwards:`. Today: mac-a only `8765` (still a Unix socket), mac-b only `22`, mac-c unreadable (SSH refused; user must confirm that machine’s state). **Every newly reachable service requires editing `agent.yaml` and restarting the agent**. Operational, not code, but it directly determines whether Phase 3 can be demonstrated.

### 7.6 vnet (192.168.13.0/24) Is Unreachable

The phone cannot join the virtual network: that requires `NEPacketTunnelProvider` + Network Extension capability + a second target + per-packet IP stack, excessive for “one app accessing a few named services.” On iOS, entitlement documentation says `NEAppProxyProvider` transparent/app-proxy forms **require MDM-managed configuration** (**unverified: documentation, not experiment**).
Port forwarding covers SSH + web. Revisit Network Extension only if the whole phone must join 192.168.13.0/24 later.

### 7.7 Other Items

- **SwiftTerm is a one-person project** (MIT, active, used by real apps). Risk accepted: its interface is tiny (`feed` / `send`) and replaceable if necessary.
- **`WKWebsiteDataStore.proxyConfigurations`** successfully routed a real WKWebView through the proxy in the spike, but on Mac; revalidate hardware + real services in Phase 3.
- **Bundling BoringSSL** adds a few MB. Irrelevant for a personal tool.
- **Signing requires an unlocked login Keychain**; the user must unlock it personally (§5).
- **Annual profile renewal** is an unavoidable operational burden; document it in README.
- **`defaultRelay` in `cmd/muster/connect.go` still uses `:7443`**, while production uses `:8443`. Do not copy that constant into iOS.

---

# Adversarial Review (Findings and Corrections)

## Confirmed Parts (Briefly)

I checked every “proven” assertion against source. These are true and sufficient to start the project:

- **The inner protocol contract is correct**. The three `WSByteStream` rules match `internal/tunnel/wsconn.go` individually (one complete binary message per `Write`, concatenate inbound binary into bytes, silently discard non-binary); MST1 matches `internal/innerchan/codec.go`; yamux parameters (`KeepAliveInterval 30s` / `ConnectionWriteTimeout 10s` / `MaxStreamWindowSize 1<<20`, initial window `initialStreamWindow = 256KiB`) match `innerchan.yamuxConfig()` + `hashicorp/yamux@v0.1.2`.
- **The `.none` versus `.noHostnameVerification` trap is real**, explicitly documented upstream: `swift-nio-ssl/Sources/NIOSSL/SSLConnection.swift:178` — “if the verify mode is no-verification, this callback never gets called.” (`SSL_set_custom_verify` registers using the current verification mode.) The spike’s verify block is correct: mismatch → `.failed`, thrown error → `promise.fail`, **without** the classic true-on-error bug.
- **Relay needs no changes or new ops**. `handleConnect` (`internal/rendezvous/server.go`) has no authentication, just `validID` + token bucket + `MaxPipesPerAgent` + blind splicing; `/v2/connect/<user>/<device>` matches the design.
- **`NIOSSLCustomPrivateKey`’s API fits**: `data` in `sign(channel:algorithm:data:)` is unhashed plaintext (explicit comments at `CustomPrivateKey.swift:44-46`), `SignatureAlgorithm.ecdsaSecp256R1Sha256 = 0x0403` exists (`TLSConfiguration.swift:247`), and `SSL_(CTX_)set_private_key_method` is the actual installation path (`UnsafeKeyAndChainTarget.swift:116-122`).
- **The phone is treated exactly like other devices**: SEP-resident private key, ledger authorization, `VerifyConnection` only rejects without mutating state, revocation ≤2s (`defaultRecheckInterval = 1s` recheck loop in `agentd/v2.go`). No bypass.

The attack surfaces follow.

---

## CRITICAL

### C1. Wrong Keychain Accessibility Class, Contradicting the Design Itself

§1.2 / Phase 1a specifies `kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly`, justified by “background reconnection needs readability after first unlock.” But §7.2 firmly concludes **“iOS has no background reconnection; this is a fact, not a risk.”** The justification disappeared, but the class did not change.

The correct value is `kSecAttrAccessibleWhenUnlockedThisDeviceOnly`. Zero cost: TLS 1.3 signs only during handshakes, which happen only in the foreground; locking does not break established sessions (no new signature needed); Phase 4’s `beginBackgroundTask` graceful shutdown (GoAway + WS close) does not sign at all.

**Failure scenario**: the phone is stolen/seized while powered on and previously unlocked. With `AfterFirstUnlock`, the SEP private key remains **always available** even if the device is never unlocked again. Any code-execution path (jailbreak chain, future extension, forensic tool) can make it sign CertificateVerify and directly access all three Macs. With `WhenUnlocked`, the key is cryptographically unavailable while the screen is locked.

This is exactly the class choice the review requested. The design got it wrong with **pure loss and no benefit**. The `AgentStore` file’s `.completeUntilFirstUserAuthentication` has the same invalidated justification.

### C2. The CONNECT Proxy on 127.0.0.1 Is Unauthenticated and Open to the Whole Device

§1.2 / §4 binds `LocalHTTPProxy` to `127.0.0.1:<ephemeral>`, “raw byte copying, no HTTP parsing,” attached through `WKWebsiteDataStore.proxyConfigurations`. It says “scoped to one data store, not system-wide.” **That is true of WebView, not the socket.**

iOS loopback is not a per-app namespace. Any app on the phone can connect to `127.0.0.1:<port>`.

**Failure scenario**: the user installs any third-party app, even a utility containing an advertising SDK. It scans high loopback ports and sends `CONNECT mac-b.muster:80`, gaining a byte pipe through muster without the SEP key, without appearing in any ledger or `muster-agent authorize list`. The entire v2 trust model (“admission = possession of private key and SPKI in ledger”) is bypassed at this socket because the proxy already has an **authenticated** session behind it.

It is broader than muster itself: the ledger authorizes devices, `Ledger.Admit` records `Caps: {tunnel}` (`ledger.go:150`), and the forward path never checks Caps, so this intruder gets every port in `forwards:`.

**Required fix**: `Network.ProxyConfiguration` supports `applyCredential(username:password:)`. Generate random credentials at each launch; require valid `Proxy-Authorization` on every CONNECT and disconnect otherwise; close the listener when leaving the WebView screen; use a new port each time. This also rules out the “loopback listener + URL rewriting” alternative even more strongly. Cookie correctness was the stated reason for CONNECT; add this reason too.

---

## HIGH

### H3. The Spike Never Had BoringSSL Parse the Handwritten iOS Container Certificate

This is an **unnamed** unverified integration point, entangled with the only named one (SEPSigner).

- Spike 1: Swift handwritten DER → **Go `crypto/x509`**, PASS.
- Spike 2: **software-generated PEM** → `NIOSSLCertificate(file:format:.pem)`, PASS.
- Nobody tested: the iOS handwritten certificate (100-year validity, no SAN, GeneralizedTime `NotAfter`, only one critical KeyUsage) → `NIOSSLCertificate(bytes:format:.der)` → `NIOSSLContext`.

**Failure scenario**: Phase 1b handshake fails. Two variables (whether BoringSSL accepts the certificate and whether SEP signs correctly) get debugged as one bug. Worse, I checked that **nowhere in NIOSSL calls `SSL_CTX_check_private_key`** (`grep -rn check_private_key Sources/NIOSSL/` is empty). With custom keys, BoringSSL cannot verify that certificate and private key match, so any mismatch surfaces only as a vague handshake failure.

**Fix**: make “iOS mint → `NIOSSLCertificate(bytes:)` → successful `NIOSSLContext(configuration:)` construction” the **first independent assertion** in Phase 1b, before any network I/O.

### H2. The Design Replaces the Only Production-Tested Component and Loses System Proxy Support

§1.2 recommends `NIOTSConnectionBootstrap` + `NIOHTTP1` + `NIOWebSocketClientUpgrader` in place of `URLSessionWebSocketTask`.

But spike 2’s **only probe that reached the real mac-a agent** (nginx → musterd → register link → `innerchan.AgentSession`, receiving ServerHello) used URLSession. It worked because URLSession honors `connectionProxyDictionary`, essential on this machine because Fake-IP hijacks the resolver. `NWConnection`/NIOTS **does not read system proxy settings**.

**Failure scenarios**: (a) the author’s development machine can no longer run production probes, immediately losing regression validation; (b) on Wi-Fi with PAC / a corporate proxy, Safari opens `muster.linsheng.dev` but the app cannot, appearing as “muster is broken”; (c) ATS, happy-eyeballs, cellular↔Wi-Fi switching and related behavior move from inherited to reimplemented.

Another **unverified** detail was glossed over: relay’s `upgrader.CheckOrigin` rejects any request containing an `Origin` header (`server.go:145`). URLSession is proven to pass; `NIOWebSocketClientUpgrader` has not been tested.

### H1. yamux → NIO Child `Channel` Is the Largest Task, Yet Treated as a Footnote

§1.2/§4 describes “rewrite as a NIO handler + multiplexer (shaped like `NIOHTTP2` StreamChannel).” `HTTP2StreamChannel` is among `swift-nio-http2`’s hardest components: a complete `Channel` implementation handling autoRead, writability watermarks, `ChannelOptions`, half-closure events, and pipeline lifecycle. Mistakes cause deadlocks that look like network faults.

Meanwhile, the spike’s actor yamux has **wire behavior individually verified against real `hashicorp/yamux`** (1 MiB bidirectional flow control, ping ACK, inbound streams, FIN-on-window-update), and together with `TransportLoopback`/`YamuxGlue` already runs SSH over the full stack.

**Failure scenario**: the first weeks go into implementing `Channel`, rather than making the phone usable; Phase 1c slips indefinitely; alleged gains (zero-copy, backpressure) are **imperceptible** for an SSH terminal plus a few internal web pages.

**Recommendation**: keep actor yamux + loopback glue. Downgrade the NIO rewrite to an optimization triggered by measurable deficiencies (observed throughput/latency), not a prerequisite.

### H4. Phase 1c Requires Configuration Changes and Omits an Entire Credential Step

I checked mac-b: `forwards: 22: "127.0.0.1:22"` ✓, `sshd LISTENING on 127.0.0.1:22` ✓, **`PasswordAuthentication no`** ✓.

To exec `hostname` in Phase 1c, the phone’s SSH public key must first be added to mac-b’s `~/.ssh/authorized_keys`. The design never mentions this.

More importantly, this creates a **second credential the ledger cannot revoke**: `muster-agent authorize remove` kills the tunnel within two seconds, but that SSH key still works from the LAN, private-tunnel (today `ssh mac-b` uses 127.0.0.1:2222), or anywhere reaching port 22. “Revocation = one ledger edit” silently becomes “one ledger edit + edit `authorized_keys` on three machines.”

**Fix**: dedicated SSH key; restrict its `authorized_keys` line with `from="127.0.0.1"` (the tunnel target is 127.0.0.1:22, so sshd sees that source; this restriction is effective and significantly narrower); document “two-step revocation” in IdentityScreen and README rather than relying on memory.

### H5. The Design Has No SSH Host-Key Policy

The spike uses `PrintingHostKeyDelegate`, unconditionally calling `succeed(())` (`ssh-probe/main.swift:59-63`; its comment says “The spike accepts any host key”). swift-nio-ssh requires a delegate, and accept-all is the easiest implementation.

§2.2 devotes a whole section to “no TOFU on the phone, no automatic trust after pin mismatch,” then silently uses blind acceptance for SSH. The inconsistency shows an omission, not a considered tradeoff.

**Failure scenario**: barely defensible for target `127.0.0.1:22` (the agent’s host), but the design already includes per-agent `sshDefaults`, and `forwards:` can target other hosts (mac-a’s current entry is a Unix socket; vnet also has 192.168.13.0/24). The first non-loopback forward turns accept-all into a real MITM surface initiated by a compromised agent-side process.

**Fix**: fingerprint confirmation on first connection, store pin by (agent, port), and treat subsequent mismatch as terminal. Same discipline as muster pins.

### H6. Suspension Budget Is ~30–40 Seconds, Not 60, with Three Session Killers

§7.2 calculates 30s ping × `pingMaxMisses=2` ≈ 60s. Actual constraints (all source-verified):

| Mechanism | Location | Actual budget |
|---|---|---|
| **yamux keepalive** | `session.go:364-379` | `KeepAliveInterval=30s`, `Ping()` timeout `ConnectionWriteTimeout=10s`; **first failure calls `exitErr(ErrKeepAliveTimeout)`, with no miss counter** → **30–40s** |
| **relay `deviceWriteTimeout`** | `rendezvous/server.go:52` + `deadlineWriter` in `pipe.go` | Once the agent writes and the phone does not ACK, relay tears down the pipe after 30s → **30s** |
| MST1 ping | `innerchan/session.go` | 30s × 2 misses ≈ 65s → **never the bottleneck** |

**Failure scenario**: switch to 1Password to copy a token and return after 45 seconds; the session is dead and reconnecting + `tmux attach`. This feels very different from “about a minute.” Phase 2 acceptance must time and record 15/30/45/90s intervals. Reconnection must be fast and quiet enough to go unnoticed, or “delegate to tmux” fails experientially.

### H7. Phase 3 Needs `NSAllowsArbitraryLoadsInWebContent`, Despite the No-ATS-Exception Claim

§5 explicitly says “no ATS exceptions.” But WebView loads `http://mac-b.muster/...`, plaintext HTTP. iOS ATS governs WKWebView and blocks plaintext unless `NSAllowsArbitraryLoadsInWebContent` (or `NSAllowsArbitraryLoads`) is set.

Both spikes hid this: spike 1’s simulator app plist has `NSAllowsArbitraryLoads` (`spike/ios/identity/build/mkapp.sh:29`), and spike 2’s WKWebView test runs on **macOS**.

**Failure scenario**: the page does not load at the Phase 3 demo, with **nothing in proxy logs**: WebKit never sent CONNECT, making the failure invisible.

---

## MEDIUM

### M1. Enrollment Ignores an Existing muster Mechanism and Chooses the Worse Alternative

§2.1 recommends Universal Clipboard to transfer 64-hex SPKI to three Macs. But `agentd/v2.go:274-283` already records every unknown-key knock in the **pending log** (`Ledger.NotePending`, deduplicated by SPKI, with first/last-seen and count), and `muster-agent authorize list` prints:

```
PENDING (knocked, not authorized — `authorize add --fingerprint F` to admit):
```

`add --fingerprint` also accepts a **unique prefix** (`cmd/muster-agent/main.go:730,753` → `ResolvePendingFingerprint`).

The real flow is therefore: install app → connect → rejection → run `muster-agent authorize add --fingerprint AB3F --name iphone` on each Mac. **Nothing needs transferring**; the person checks only a four-character prefix.

Universal Clipboard instead requires both devices on the same iCloud account, Bluetooth enabled, and the same network, often unavailable for a phone “mostly on cellular, outside the network.”

**Impact on the plan** (the key point): Phase 1 is ordered backwards. The app must knock **before authorization**, and §2.3’s “never connected → stop retrying” harms first launch. Change to: knock once → dedicated banner “You are in mac-b’s pending list, fingerprint AB3F-…; run `muster-agent authorize add --fingerprint AB3F` on that machine” → Retry button.

### M2. Metadata Does Leak to Rendezvous, Contradicting §1.1

`URLSessionWebSocketTask` sends `User-Agent: <App>/<ver> CFNetwork/… Darwin/…` (the Go client does not), and by default attaches cookies from `HTTPCookieStorage.shared` and `URLCredentialStorage`. nginx records UA in access logs. The untrusted relay learns not just “two pipes connected,” but also “iOS, app version, OS build” on every connection.

Not fatal, but it violates a declared invariant. **Fix**: `URLSessionConfiguration.ephemeral`, `httpCookieStorage = nil`, `httpShouldSetCookies = false`, `urlCredentialStorage = nil`, and explicitly align `User-Agent` with Go. Same requirement if outer transport switches to NIO: deliberately choose headers rather than inherit defaults.

### M3. Stored Pins Can Silently Override Compile-Time Constants — Backdoor TOFU

§1.2 stores `AgentStore` as JSON in the app container because “pins are public; integrity matters, not secrecy; anyone modifying the container already owns the app.” On iOS this reasoning has a concrete gap: `Documents` / `Application Support` **are included in iCloud and encrypted local backups by default**. Pins leave in backups and return during restoration.

**Failure scenario**: a tampered or externally restored pin file overrides compile-time constants **without confirmation**. This is precisely the automatic retrust forbidden by §2.2. The MITM warning in `describeConnectError` never fires because there is no detectable mismatch: the pin already belongs to the attacker.

**Fix**: compile-time pins for the three known agents are authoritative and **cannot be overridden by files**; file entries may only **add** agents, with explicit confirmation equivalent to `--trust-first`; set `isExcludedFromBackup` on the container directory.

### M4. Pin Parsing and Comparison Need Fixed Rules

The spike compares lowercase hex **strings** with `==` (`inner-probe/main.swift:185`); `innerchan.ClientTLS` uses `subtle.ConstantTimeCompare` on `[32]byte` (`client.go:44`). `ClientTLS` also requires `len(rawCerts) == 1`; the spike takes `certs.first`, ignoring extras. Neither is exploitable (pins are public; the hashed certificate is the leaf), but the design claims “constant-time comparison” without specifying it.

**Fix during porting**: decode to exactly 32 bytes on load or reject; compare `Data`, not `String`; reject chain length ≠1; **every** thrown callback error calls `promise.fail`; never allow `succeed(.certificateVerified)` in a catch branch.

### M5. Three Persistent Sessions Are the Wrong Phone Default

Phase 4 keeps all three agents online. Each has a 30s yamux keepalive + 30s MST1 ping, bidirectionally: roughly one packet every five seconds across the fleet, preventing the cellular radio from idling. Merely opening the app drains power, and every suspension creates a threefold reconnect storm consuming three token buckets.

**Default to lazy connections**: connect only to the agent in use, close others after idle timeout, and show “offline (tap to connect)” in Agents instead of maintaining presence.

### M6. Two Omitted Facts Block First Hardware Installation

- **iOS 16+ requires Developer Mode under Settings → Privacy & Security**, with a device reboot, or development-signed apps refuse to launch. §5 never mentions it.
- **Apple Development certificates are limited to two per type per account**. At the limit, Xcode offers to revoke one; apps signed with it immediately stop working until re-signed.

One further clarification to §5: the `application-identifier` prefix is the team ID, so the Keychain access group (and SEP identity) binds to **team + bundle ID together**. The warning that changing bundle ID orphans identity must also apply to changing team.

---

## LOW

- **L1 `.waiting` semantics were applied in the wrong place**. §7.1 carries spike 1’s `.waiting(-9808)` trap into NIOTS, where outer transport is ordinary WebPKI TLS and that specific case does not occur. The actual issue: `NIOTSConnectionBootstrap` waits for an available path by default; airplane mode / captive portals produce a hanging connect rather than an error. Explicitly set connection timeout, disable wait-for-activity, and allow expensive/constrained paths, or cellular silently hangs in Low Data Mode.
- **L2 `SEPSigner` must be `Hashable` + `Sendable`**. `NIOSSLPrivateKey(customPrivateKey:)` requires `CustomKey: NIOSSLCustomPrivateKey & Hashable` (`SSLPrivateKey.swift:322`), and the protocol inherits `_NIOPreconcurrencySendable` (`CustomPrivateKey.swift:34`). A type holding `SecKey` satisfies neither automatically. Revise the “~40 lines” implementation estimate.
- **L3 Dependency risks are ranked incorrectly**. Only SwiftTerm is marked “one-person project; risk accepted.” In fact, `swift-nio-ssh` remains at **0.15.0**, not 1.0 after years, with no API stability guarantee and the thinnest maintenance of the five dependencies (NIO/NIOSSL/BoringSSL are the strongest). Still choose it, but do not classify it as “safe.”
- **L4 `SwiftTerm.feed` must run on the main thread**. NIO delivers on the event loop; `TerminalView` is UIKit. Every feed needs a hop, but one hop per read drops frames during `cat bigfile`; batch updates.
- **L5 “Regenerate identity” must actually delete it**. `SecItemDelete` both key and certificate items, or the old key remains and the ported “re-sign on SPKI mismatch” logic may resurrect a stale pair. Also explicitly require `authorize remove` for the old SPKI on all three machines: orphaned ledger entries are “long-term authorization for a key nobody controls,” the one genuinely important kind of ledger debris.
- **L6 The 7443 constants in the tree**. `defaultRelay` at `cmd/muster/connect.go:28` still uses `:7443`, as do `deploy/agent.yaml` and `README.md`; both production agents register at `:8443` (I checked `/opt/muster/etc/agent.yaml` on mac-a and mac-b). §7.7 already caught this; fix Go too rather than making iOS the only correct copy.

---

## Direct Answer on Scope Honesty

**Phase 1 is not a minimal end-to-end proof; it contains most of the app.** 1c requires WS client + inner TLS + yamux + MST1 + NIOSSH: the **entire transport stack** except UI, terminal rendering, and web proxy.

The risk profile is asymmetric: every 1c layer already worked in spike 2 on macOS with **software keys**, against the real `internal/innerchan`. The only new variable on hardware is **that key**. Bundling known and unknown pieces into one phase forfeits immediate fault isolation. §7.1 itself says “if this fails, immediately use the fallback; do not pile on code,” then places that step alongside five layers of new code in the same phase.

---

## Revised Recommendation: What to Do First

**Change two design choices now, while cheap:**
1. Use `kSecAttrAccessibleWhenUnlockedThisDeviceOnly` (C1).
2. Make compile-time pins authoritative and immune to stored-file overrides (M3).

**Phase 0 (half a day, unchanged)** — Create project, finalize bundle ID `dev.linsheng.muster.ios` and team, enable Developer Mode (reboot required), run empty app on hardware.

**Phase 1 — Prove only “this key can sign.” No relay, yamux, or SSH; UI is one label and one button.**

Three assertions, in order; stop if any fails:

1. **SEP key lifecycle**: mint, display `spki_sha256`, same key after app restart and phone reboot, instrument `SecKeyCreateSignature` latency, **delete/reinstall and confirm disappearance** (simulator cannot answer).
2. **BoringSSL accepts this certificate** (H3, new independent assertion): iOS mints container certificate → `NIOSSLCertificate(bytes:format:.der)` → successful `NIOSSLContext` using `NIOSSLPrivateKey(customPrivateKey: SEPSigner)`. **Zero network I/O.**
3. **SEP drives CertificateVerify**: direct bare TCP over LAN to `spike/ios/transport/go/innerpeer` (imports the real `internal/innerchan`). PASS: Go logs `handshake OK … cipher=0x1301` and `client spki_sha256` matches the phone.

Add two cheap negative paths here: one wrong pin character → `CERTIFICATE_VERIFY_FAILED`; SPKI absent from `-allow` → handshake “succeeds” then immediately dies with `SSLV3_ALERT_BAD_CERTIFICATE`.

**Phase 1.5 — Add the relay hop, still no SSH.**

Port the spike’s `URLSessionWebSocketTask` byte stream and **actor yamux** unchanged (no NIO rewrite), open a `{"v":1,"port":22}` forward stream, **read and display sshd’s banner**. Do this over **cellular**.

The banner is a perfect probe: no SSH credentials, yet it simultaneously proves carrier access to `:8443`, relay splicing, ledger admission, yamux interoperability, and MST1 correctness, with zero new abstractions.

Complete enrollment in the same phase (M1): launch **unauthorized**, confirm a pending entry in mac-b’s `muster-agent authorize list`, use a four-character prefix with `authorize add --fingerprint`, then Retry in the app and connect.

Cellular subcheck: if `:8443` is blocked, add a 443 nginx server block **now**, before stacking three more layers on top.

**Phase 2 — SSH + terminal.** First perform operational prerequisites: phone-specific SSH key, its mac-b `authorized_keys` line restricted by `from="127.0.0.1"`, documented two-step revocation (H4). Then NIOSSH on the existing yamux stream, **first-use host-key confirmation** (H5), SwiftTerm (batch feeds and hop to main thread). Add stopwatch acceptance tests: return after 15/30/45/90 seconds in background, record actual session death (expected 30–40s, H6), and confirm `tmux new -A -s muster` restores without noticeable disruption.

**Phase 3 — Web.** Proxy credentials from the first line of code (C2), set `NSAllowsArbitraryLoadsInWebContent` (H7), and run the correctly specified two-service cookie-isolation test.

**Phase 4 — Three agents (lazy connections, not persistent connections to all, M5), IdentityScreen, error banners, graceful shutdown.**

**Explicitly defer until measurements justify them**: rewriting yamux in NIO and switching outer transport to NIOTS. Both optimize proven functionality and are outside the critical path to a usable phone app.

---

## Corrections Implemented After Review (2026-08-21)

1. **C1 Keychain accessibility class** — Changed `kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly`
   to **`kSecAttrAccessibleWhenUnlockedThisDeviceOnly`** (`spike/ios/identity/swift/Identity.swift`
   updated). The original “needed for background reconnection” rationale was contradicted by
   §7.2 itself (iOS suspends the app; there is no background reconnection). TLS 1.3 signs only
   during foreground handshakes. The wrong choice was **pure loss, no benefit**: when a phone
   is taken after one unlock since boot, its SEP private key stays usable; any code-execution
   path can sign CertificateVerify and directly reach all three Macs.
2. **C2 Local CONNECT proxy requires credentials** — `LocalHTTPProxy` binds 127.0.0.1 but is
   accessible to all device processes. Require `Proxy-Authorization` from the first line of code,
   rather than adding it in Phase 3.

## Real-Device Validation Pending (No iPhone Development Certificate Here; User Action Required)

Sign in to team 89LGY6BD53 in Xcode → Settings → Accounts and let automatic signing generate an
Apple Development certificate and provisioning profile. Then revalidate three points on hardware
(the simulator cannot answer them):

- The `kSecAttrTokenIDSecureEnclave` key is actually hardware-resident, with acceptable signing latency;
- **Whether app deletion removes Keychain identity**: real devices clear it from iOS 10.3 onward,
  destroying identity and requiring `muster-agent authorize add` again. This product semantic must be confirmed;
- `.userPresence` is for approval only and **must never be applied to TLS identity**, or every handshake prompts Face ID.

## A Network.framework Trap the Implementation Must Handle

On pin mismatch, `NWConnection` enters **`.waiting(-9808)`**, not `.failed`; the framework treats it
as retryable and reconnects indefinitely. muster never retries authentication failures, so treat TLS
errors in `.waiting` as terminal and call `cancel()`.
