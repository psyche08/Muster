# P0 Physical-device Validation Plan

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

Covers phase P0 in section 5 of the architecture ([migration.md](architecture/migration.md)). Each of four checks answers one feasibility question about the new architecture, ends with **pass / partial pass / fail**, and records the chosen fallback. At the end of P0, update any content in ARCHITECTURE section 3 invalidated by the results.

> **2026-10-02:** The four POC tools named below (`axprobe`, `hwquic`, `relaypoc`, `itermrs`) were deleted from `src/poc/`. Their proven code lives in production crates (`muster-platform`, `MusterMacShim`, `muster-net`, `muster-terminal`). The original sources remain in Git history: `git show acf23be7:src/poc/<name>/…`. Later acceptance is recorded in [LOCKED_USE.md](LOCKED_USE.md), [AGENT_INTERFACE.md](AGENT_INTERFACE.md), and [NETWORK_DIAGNOSTICS.md](NETWORK_DIAGNOSTICS.md), not in the empty results table below.

## General Conventions

- **POC code goes in `src/poc/<name>/`**, as independent Cargo / Swift projects outside the main workspace and its clippy pedantic gate. At the start of P1, either delete it or clean it up as the skeleton of a production crate.
- Spend no more than 3 working days on each POC; finish all four within two weeks. Recommended order: **POC-1 and POC-4 first** (highest product risk, least code), then POC-2 and POC-3 in parallel.
- Devices: `mac-a` (target / creator), `mac-c` (second Mac), and iPhone; POC-3 also needs a non-home network (a phone hotspot suffices).
- Every locked-screen test must distinguish and record three states:
  1. `Ctrl-Cmd-Q` screen lock (session remains on the console, `CGSSessionScreenIsLocked = true`);
  2. Display sleep with password required on wake;
  3. Fast user switching to the login window (`kCGSSessionOnConsoleKey = false`) — **expected to fail**; this confirms musterd must distinguish this state in lock detection.
- Record results in the table at the end of this document, not a separate document.

---

## POC-1: AX Control of Electron Desktop Apps While Locked

**Questions**

1. Can a LaunchAgent process read the AX trees of Claude Desktop / Codex Desktop while the screen is locked?
2. Can it write `AXValue` to the composer and send with `AXPress` while locked, with **actual frontend receipt** (AX readback alone is insufficient)?
3. How does each of the three lock states behave?

**Risk**: both apps use Electron. Chromium exposes the full AX tree only after `AXManualAccessibility`. There is no public guarantee that `AXValue` on an invisible window triggers React onChange or that `AXPress` works without a visible window. Existing code (`src/rust/crates/agenthalo-backend/src/macos_claude_native_input.rs`) rejects `!unlocked || !focused` and has never been tested while locked.

**Tool**: `src/poc/axprobe/` (written; see its README; Swift, not yet compiled on macOS, so the first `./build.sh` may require compilation fixes).

```
axprobe --dump  --app <bundle-id>                # Print role, label, title, and visibility of AXTextArea / AXButton nodes
axprobe --check                                  # AXIsProcessTrustedWithOptions(prompt) + three CGSessionCopyCurrentDictionary keys
axprobe --delay 40 --app <bundle-id> --text <marker> --send --log <path>
```

`--send` follows the existing Rust sequence: validate process signature (`com.anthropic.claudefordesktop` / `com.openai.codex`) → `AXUIElementSetAttributeValue(app, "AXManualAccessibility", true)` → find the `AXTextArea` labeled `Prompt` and `AXButton` labeled/titled `Send` under `AXWebArea` (discover Codex control names with `--dump` while unlocked) → confirm empty draft → write `AXValue` → compare readback → `AXPress`. Log lock state and `CGEventSourceCounterForEventType` before and after each step.

**Procedure**

1. Sign `axprobe` with Developer ID, fixed identifier `dev.linsheng.muster.axprobe`, and place it in `~/Library/Application Support/muster-poc/`.
2. Run `axprobe --check` while unlocked and enable it in System Settings → Privacy & Security → Accessibility. Run `--dump` for each app to confirm control discovery; record Codex control names.
3. Create a new "No folder" test session in Claude Desktop with an empty composer; prepare an equivalent test session in Codex.
4. Write `dev.linsheng.muster.axprobe.plist` (`ProgramArguments` uses the second command, with `--delay 40`), run `launchctl bootstrap gui/$(id -u) <plist>`, immediately lock with `Ctrl-Cmd-Q`, and unlock after 60 seconds.
5. Inspect logs and evidence. **Independent evidence, in priority order**:
   - A user message containing the marker in the session record (first confirm whether Claude Desktop Code sessions persist under `~/.claude/projects/`; for Codex, inspect `~/.codex/sessions/`);
   - The marker in AX readback of the conversation after unlocking;
   - Screenshots.
   `AXPress` returning success alone is not a pass.
6. Repeat steps 4–5 for lock states 2 and 3; repeat everything for Codex.
7. Also record whether typing a password while locked changes `CGEventSourceCounterForEventType`. If it does, musterd's user-input generation check while locked must use only the delta after unlocking.

**Criteria**

| Result | Condition | Consequence |
|---|---|---|
| Pass | Both Claude and Codex accept writes and sends in states 1 and 2, with independent evidence | Implement NativeApp per ARCHITECTURE §3.6.2 |
| Partial pass | Tree and draft reads work, but writing/sending does not; or only one app works | Affected apps' NativeApp channel is read-only while locked; control uses Owned / iTerm2; product wording states this |
| Fail | AX tree cannot be read while locked | Locked control uses only Owned / iTerm2; viewing is unaffected (viewer reads files) |

**Data to write back to ARCHITECTURE**: actual `kCGSSessionOnConsoleKey`, `kCGSessionLoginDoneKey`, and `CGSSessionScreenIsLocked` values in all three states, as the basis for musterd's lock detection.

---

## POC-2: Hardware-signed QUIC Handshakes

**Questions**

1. Can rustls 0.23's custom `SigningKey` use SEP for TLS 1.3 `CertificateVerify`?
2. Can Swift SEP code link as a **static library** into a Rust executable and sign inside a LaunchAgent while locked?
3. Can an iPhone client use SEP to complete a QUIC handshake with a Mac?
4. What is the handshake latency?

**Risk**: the existing implementation has never signed in-process (Go uses a helper subprocess; Rust uses an XPC broker). Linking static Swift into Rust requires explicitly linking the Swift runtime; incorrect SEP key access-control flags prevent signing while locked.

**Tool**: `src/poc/hwquic/` (written; Linux software-key tests passed mutually pinned handshakes and incorrect-pin rejection; SEP, static linking, and iOS have not run on physical devices).

- `sepsign/`: Swift package exporting three `@_cdecl` functions: `sepsign_create(out_blob)`, `sepsign_public_key(blob, out_spki_der)`, and `sepsign_sign(blob, msg, len, out_der_sig)`. Copy the existing helper's access control: `kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly` + `.privateKeyUsage` (`src/cmd/muster-sep-helper/main.swift:174`), without `userPresence`.
- `hwquic`（Rust）：`quinn 0.11` + `rustls 0.23`（ring）+ `rcgen`。
  - Certificate: `rcgen::KeyPair::from_remote(Box<dyn RemoteKeyPair>)`, using SEP to self-sign a certificate containing only SPKI;
  - Signing: implement `rustls::sign::SigningKey` (`ECDSA_NISTP256_SHA256`); `Signer::sign` calls `sepsign_sign` (CryptoKit applies SHA-256 internally and emits DER, matching rustls expectations);
  - Verification: custom `ServerCertVerifier` / `ClientCertVerifier` checks only that the leaf SPKI SHA-256 equals the command-line pin and that `verify_tls13_signature` accepts CertificateVerify; ignore validity period and DN;
  - Subcommands: `serve --pin <peer-spki>`, `connect <addr> --pin <peer-spki> --rounds 20`.
- `build.rs`: `swiftc -parse-as-library -emit-library -static` produces `libsepsign.a`; link `-L/usr/lib/swift -lswiftCore -lswiftFoundation -lswiftCryptoKit -framework Security`.

**Procedure**

1. mac-a: run `hwquic serve` as a LaunchAgent (plist in `~/Library/LaunchAgents/`).
2. mac-c: `hwquic connect mac-a:port --rounds 20`. Record each handshake duration minus RTT.
3. Lock mac-a (states 1 and 2); connect from mac-c for another 20 rounds.
4. Reverse direction: serve on mac-c, connect from mac-a.
5. iOS: build `libhwquic.a` with `cargo build --target aarch64-apple-ios`; write a one-button SwiftUI app whose `hw_sign` callback calls `SecKeyCreateSignature(.ecdsaSignatureMessageX962SHA256)` on a `kSecAttrTokenIDSecureEnclave` key; connect to mac-a over LAN.
6. Optional (required before P4): Android JNI calls `Signature.getInstance("SHA256withECDSA")` with a StrongBox key; record whether the device has StrongBox. HarmonyOS uses HUKS `sign` (ECC P-256 / SHA-256). At minimum confirm that the Rust targets compile.

**Criteria**

| Result | Condition |
|---|---|
| Pass | Mac ↔ Mac handshakes work both ways in both lock states; iPhone → Mac succeeds; 1-RTT handshake; each SEP signature < 50 ms |
| Partial pass | Static linking fails but `.dylib` works → use a dynamic library in bundle `Frameworks/`; architecture unchanged |
| Fail | SEP signing fails while locked → first check access-control flags; if it still fails, redesign Device Key and **escalate to an architectural decision** |

**Record**: rcgen / rustls / quinn versions; whether `RemoteKeyPair` works correctly with the ring provider; the complete static-link command.

---

## POC-3: Relay Datagram Forwarding, Hole Punching, and vnet over DATAGRAM

**Questions**

1. Can a QUIC connection switch between direct and Relay paths without disconnecting?
2. Does hole punching work between home NAT and a phone hotspot?
3. Can QUIC DATAGRAM carry inner IP packets with MTU 1280? What is actual SSH/VNC usability on direct and Relay paths?
4. Does WebSocket datagram fallback work when UDP is blocked?

**Risk**: existing vnet uses reliable yamux streams. DATAGRAM is constrained by QUIC path MTU: quinn starts at 1200 and uses PMTUD up to 1452. A 1280-byte inner packet plus headers needs roughly 1300 bytes of datagram capacity; incomplete PMTUD or small path MTU yields `DatagramTooLarge`. Path switching requires either QUIC migration or lower-layer routing with stable upper-layer addresses (Tailscale magicsock's approach), neither previously implemented here.

**Tool**: `src/poc/relaypoc/` (written; Linux loopback passed Relay connection → hole punching → live path switching, WebSocket fallback, and one-way TUN; see its README for three preliminary-run issues and fixes), with two Rust executables:

- `relay`: tokio UDP + TCP/WebSocket (`tokio-tungstenite`). Clients register 32-byte IDs (random and unsigned in this POC); datagrams prefix a destination ID, replaced by the source ID when Relay forwards; one message per WS frame.
- `peer`: quinn `Endpoint::new_with_abstract_socket` wraps a custom `AsyncUdpSocket` ("magicsock") exposing a fixed virtual address per peer. It maintains candidates (local interfaces + STUN-reflected addresses), probes periodically, switches sends to direct after successful probing, and falls back to Relay on failure. quinn always sees the same peer address, so QUIC migration is not triggered. Copy candidate and punching timing from `src/internal/agentd/direct*.go`.
- `peer --tun 192.168.14.x/24`: create utun with the `tun` crate (POC uses `sudo`; production receives an fd from NetHelper), read packets → `send_datagram`, receive → write utun. Until PMTUD provides sufficient `max_datagram_size()`, drop and count packets rather than fragmenting them.

**Procedure**

1. Deploy `relay` on linsheng.dev using a new port (for example 9443), leaving existing musterd untouched.
2. LAN: start `peer --tun` on mac-a and mac-c. Run `ping -c 100`, `iperf3`, `ssh 192.168.14.x`, and Screen Sharing `vnc://192.168.14.x`. Record RTT, throughput, and subjective VNC smoothness.
3. Force Relay: use `pfctl` on mac-c to drop UDP from mac-a's LAN address. Repeat step 2, then remove the rule and confirm the **SSH session survives** switching back to direct.
4. NAT: connect mac-c to a phone hotspot. Inspect candidates, STUN results, and first direct-packet arrival time in logs; repeat step 2.
5. MTU: record post-PMTUD `max_datagram_size()` and discovery duration on both paths. `ping -D -s 1252 192.168.14.x` (1280 − 28) must work; use SSH to `cat` a 10 MB file.
6. WS fallback: block UDP to Relay on mac-c; confirm WS connection and interactive SSH, and record latency.
7. Revocation rehearsal: delete one ID's Relay registration and confirm the peer notices within 2 seconds (baseline for P1's revocation deadline).

**Criteria**

| Result | Condition |
|---|---|
| Pass | No disconnection on path switches; home NAT ↔ hotspot hole punching works; both UDP paths have `max_datagram_size ≥ 1300`; SSH interactive and VNC usable through Relay; WS fallback works |
| Partial pass | PMTUD ceiling too low → lower vnet MTU to 1200 (update ARCHITECTURE §3.5); hole punching fails → temporarily use Relay-only for that pair, retaining candidates; WS fallback latency high → accept and document it |
| Fail | magicsock unstable → use QUIC migration (`rebind` + server-observed address changes), repeat step 3; if still failing, escalate to an architectural decision |

**Record**: RTT and iperf3 throughput on all three paths; PMTUD results; hole-punching latency; VNC bandwidth usage on Relay.

**Two conclusions from the preliminary Linux run** (physical devices need only reconfirm):

- Before PMTUD completes (about 200 ms on loopback), `max_datagram` is only 1162, dropping 1280-byte inner packets; afterward it reaches 1382. Production should either set `initial_mtu` near 1350 on known-good paths or buffer vnet packets for the first few hundred milliseconds.
- The 34-byte Relay header is outside QUIC and also accompanies PMTUD probes, so the ceiling automatically decreases to fit the path. A 1500-MTU path should yield `max_datagram` around 1370, still ≥ 1284.

---

## POC-4: Connecting to iTerm2 from Rust in musterd

**Questions**

1. Can a LaunchAgent retrieve a cookie through Apple Events and show an Automation prompt? Does authorization survive restarts and re-signing?
2. Can Rust WebSocket over UDS + protobuf complete the iTerm2 handshake?
3. What is `NOTIFY_ON_SCREEN_UPDATE` latency, and do notifications continue while locked?
4. What are notification frequency and `GetBuffer` duration under high-volume output, to determine coalescing policy?
5. Does Rust `SendText` with `suppress_broadcast` reach only the target session?

**Risk**: the existing iTerm2 client runs in the app process with windows and user interaction; TCC Automation is authorized under the app identity. Moving to a windowless LaunchAgent changes whether a prompt appears and which code identity it names, determining whether D6 (Automation authorization) is viable.

**Tool**: `src/poc/itermrs/` (written; Linux compilation and aarch64-apple-darwin type checking passed; Swift shim and iTerm2 handshake have not run on macOS), a Rust executable:

- `prost-build` compiles the existing vendored `src/native/apple/AgentHaloTerminalAdapters/Sources/AgentHaloIterm2Protocol/Vendor/api.proto` (proto2).
- `tokio-tungstenite::client_async` over `UnixStream` (`~/Library/Application Support/iTerm2/private/socket`), copying existing Swift handshake headers (`Iterm2WebSocketHandshake.swift:88-96`): `Host: localhost`, `Origin: http://localhost`, `Sec-WebSocket-Protocol` (existing subprotocol constant), `x-iterm2-library-version`, `x-iterm2-advisory-name`, `x-iterm2-disable-auth-ui: true`, `x-iterm2-cookie`, `x-iterm2-key`.
- Test both cookie acquisition paths to determine TCC attribution:
  - `--cookie-via osascript`: subprocess `osascript -e 'tell application "iTerm2" to request cookie and key for app named "musterd-poc"'`;
  - `--cookie-via shim`: a 20-line Swift `@_cdecl` function runs the same script in-process via `NSAppleScript` (static linking, using the POC-2 approach).
- Subcommands: `list`, `watch` (subscribe to `NOTIFY_ON_SCREEN_UPDATE` / `NEW_SESSION` / `TERMINATE_SESSION` / `LAYOUT_CHANGE`; timestamp each update, `GetBuffer` for the visible region with styles, record duration), `send <session> <text>`, `history <session> <range>`.

**Procedure**

1. iTerm2 → Settings → General → Magic → Enable Python API (already required by the existing adapter).
2. Developer ID-sign `itermrs` (identifier `dev.linsheng.muster.itermrs`), write a plist, `launchctl bootstrap gui/$(id -u)`, command `itermrs --cookie-via shim list`. While unlocked, expect "itermrs wants to control iTerm2"; allow it. Record whether the prompt appears and the displayed name.
3. `tccutil reset AppleEvents dev.linsheng.muster.itermrs`, then repeat with `--cookie-via osascript`; record attribution differences.
4. Re-sign with the same certificate after changing binary content and rerun: no prompt expected. Rerun with a different identifier: a prompt is expected. These establish the precise meaning of stable signing identity.
5. `watch`: type in iTerm2 and compare `date +%s.%N` output with log timestamps to measure typing → notification latency.
6. Before locking (states 1 and 2), run `while sleep 1; do date; done` in iTerm2. Notifications and changing `GetBuffer` contents should continue while locked.
7. High frequency: `yes | head -200000`, and `cat` a large log. Measure notifications/second and p50 / p99 `GetBuffer` time (80×50, with styles). Use these to choose the coalescing window (expected 16–33 ms).
8. `send`: send `echo AH_P0_$(date +%s)` to a chosen session with `suppress_broadcast=true`, while a separate session has broadcast enabled; confirm only the target receives it.
9. `history`: fetch the latest 200 scrollback lines and record duration.

**Criteria**

| Result | Condition |
|---|---|
| Pass | LaunchAgent retrieves cookies after one authorization; typing → notification < 50 ms; notifications continue while locked; styled full-screen `GetBuffer` p99 < 20 ms; `suppress_broadcast` works |
| Partial pass | Prompt appears only for a bundled agent registered with `SMAppService` → deploy musterd as a bundled helper (already planned); slow `GetBuffer` → request line ranges and lengthen coalescing window |
| Fail | Neither path obtains Automation authorization → **revisit D6** (fall back to allowing all apps to connect) |

**Record**: prompt attribution name, latency distributions, chosen coalescing window, and notification count while locked.

---

## Results

| POC | Date | Devices / OS Versions | Result | Key Measurements | Chosen Fallback | ARCHITECTURE Changes |
|---|---|---|---|---|---|---|
| 1 Locked AX | | | | | | |
| 2 Hardware-signed QUIC | | | | | | |
| 3 Relay / Hole Punching / DATAGRAM | | | | | | |
| 4 Rust to iTerm2 | | | | | | |

P0 exit criteria: all four rows completed; ARCHITECTURE section 3 updated from results; code in `src/poc/` either deleted or moved into production crates in P1's first commit.
