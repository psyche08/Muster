# muster — the rebuilt Rust core

Target: [docs/ARCHITECTURE.md](../../docs/ARCHITECTURE.md). This workspace is
the implementation; each crate maps to §3.9 of the architecture. The legacy
tree it replaced was removed on 2026-10-02 (architecture §8.5).

The product integration stack (D12, 2026-09-28) is:

```text
Codex Desktop | Claude Desktop | Catdesk Desktop | iTerm2          | Ghostty
Agent Halo (Accessibility + Locked Use)          | iTerm2 adapter  | Ghostty adapter
Muster Networking + Hardware Security
```

Agent Halo and the two terminal adapters are peer integrations composed in
`musterd`, sharing hardware identity, authorization and networking. Agent Halo
is a logical product boundary, not a revival of the removed `agenthalo-*` crates.
All business state and adapter semantics remain in Shared Rust; native code
only renders the client UI or executes OS-specific effects. Muster's mobile
apps are clients, not Agent / Terminal service providers.

Codex / Claude / CatDesk NativeApp profiles and the Rust iTerm2 client exist
here. CatDesk has its own viewer (the app's SQLite store, read-only) and reads
live status from the app's command socket; its device acceptance is pending
(see `docs/history/2026-10-02-catdesk-adaptation.md`). Ghostty's control-only
adapter existed only in the removed Swift tree (recover it with
`git show acf23be7:src/native/apple/AgentHaloTerminalAdapters/…`): its business
logic must move into `muster-terminal`, with only ScriptingBridge calls in
`MusterMacShim`. The new Ghostty path is not implemented. Capability and device
acceptance are tracked in architecture §5 / §8.

| Crate | What it is | Verified on Linux |
|---|---|---|
| `muster-core` | Device identity (SHA-256 of the P-256 SPKI), signed roster DAG, scopes, local policy, the one `authorize()`, QR invites, the desktop app registry (`apps`) | unit tests |
| `muster-net` | The link: quinn + rustls with the hardware signer, MagicSock (direct vs relay per datagram, hole punching, path migration), service streams, roster gossip, pairing, forward streams, vnet over QUIC DATAGRAM with the packet filter | loopback relay → pair → gossip → forward → direct path → revoke ≤ 2 s; vnet needs root (`--ignored`) |
| `muster-relay` | Rust relay: signed registration, forwarding by id, mailbox, reflection over QUIC DATAGRAM and TLS length frames on one port; plain UDP on loopback for tests | used by every integration test |
| `muster-apns` | APNs provider behind the relay's `--push-command` (`muster-apns-push`): silent wake-ups over HTTP/2 on :2197, sandbox fallback; deploy/apns | socket + fallback tests; `probe` against APNs |
| `muster-proto` | `muster.proto` (muster-rpc: requests, events, presence, target) and `app.proto` (Intent / Effect / ViewModel) | build |
| `muster-store` | SQLite: identity, roster, policy, kv, the OperationLedger (applied / duplicate / conflict / unknown) | unit tests |
| `muster-rpc` | Envelopes over any byte stream (Unix socket, service stream): concurrent calls, events, cancel, presence proofs, forwarding to a member (`Envelope.target`) | unit tests |
| `muster-terminal` | iTerm2 API client (line deltas, capture, guarded send), VT grid, SSH client authenticating with the device key (russh) | grid and signature units; SSH end to end against a russh server |
| `muster-agent` | Codex / Claude transcript viewers and the CatDesk store viewer with change push, CatDesk socket runtime, Accessibility-only desktop controllers, PendingInteraction, desktop Chat-mode parsing (`desktop_chat`), app profiles (`profiles/*.toml` compiled by `build.rs` into `profile`: each app's identity, process, session source, controls and capabilities as data; [docs/APP_PROFILES.md](../../docs/APP_PROFILES.md)) | viewers, service, missing-AX rejection, chat parser fixtures, profiles against recorded AX trees (`tests/fixtures/ax`), profile ↔ registry agreement; real AX acceptance is separate |
| `musterd` | The device daemon: identity, roster, link, Unix socket, the `muster` service, Device / Network / Terminal / Agent services, ledger-backed mutations, the agent bridges | two daemons pair through a relay, remote calls with scope and policy enforcement, proxying, ledger replay, removal |
| `muster-browser` | Agent Halo browser adapter: the Chrome native-messaging host, a typed local client, AX projection | bridge integration test |
| `muster-paths` | Where a user's daemon keeps state and logs (`$MUSTER_DIR` overrides); a leaf crate so the CLI finds the socket without linking the daemon | build |
| `muster-cli` | `muster` command over the socket, `--target` reaches members through the daemon | build |
| `muster-app-core` | The shells' core: intents in, effects and view models out; embeds the daemon on phones, uses musterd on desktops; presence-gated admin, chat, terminals, in-app SSH | desktop core + phone core end to end |
| `muster-ffi` | Nine C functions and `include/muster.h` | build (staticlib, cdylib) |
| `muster-platform` | OS-only pieces behind traits: macOS lock state, HID input counter, code signing, Accessibility control of the Codex / Claude / CatDesk desktop apps, Secure Enclave signer, NetHelper XPC client | Linux defaults only; device evidence is recorded separately below |

Public relay `host:port` now selects certificate-verified TCP/TLS and UDP/QUIC
on the same port (9443). This is a coordinated server/client protocol change;
see [deployment](deploy/relay/README.md). Loopback legacy transports are test
fixtures only. PeerLink hardware identity and authorization remain independent.

## Run it

```bash
cd src/muster
cargo test --workspace                       # everything that runs on Linux
cargo run -p muster-relay -- --listen 0.0.0.0:9443 --cert /path/fullchain.pem --key /path/privkey.pem
cargo run -p musterd -- --relay muster.linsheng.dev:9443 --name mac-a   # state: musterd::paths (macOS: ~/Library/Application Support/Muster)
cargo run -p muster-cli -- create home        # then: invite, pending, approve <device>, status
cargo run -p muster-cli -- --target <device> agents
```

`musterd --vnet` brings up the TUN (root on Linux; the NetHelper on macOS).
`sudo scripts/vnet-netns-test.sh` runs the vnet end to end on one Linux host:
two daemons in two network namespaces pair through a relay, TCP flows between
their virtual IPs on the granted port only, and revocation cuts it.
`scripts/check-apple.sh` type-checks the macOS-only code without an SDK.
`scripts/check-layers.py` checks the crate dependency graph against the layer
map in [docs/MODULES.md](../../docs/MODULES.md) (CI runs it).
`musterd --terminal` enables the iTerm2 service where iTerm2's API socket exists.

## Shells

The app shells that link `muster-ffi` live in `src/native/` (Apple, Android,
HarmonyOS); see `src/native/README.md`.

## Verification boundaries

- The Linux checks above do not exercise `muster-platform/src/macos`.
  Subsequent macOS / iPhone acceptance is recorded in
  [LOCKED_USE.md](../../docs/LOCKED_USE.md) and
  [AGENT_INTERFACE.md](../../docs/AGENT_INTERFACE.md). Codex normal-path
  Locked Use passed; other apps and fault scenarios require separate checks.
- The Secure Enclave signer is behind the `sep` feature of `muster-platform`
  and uses the Apple signing shim built by `muster-platform/build.rs`
  (`cargo build -p musterd --features sep` on macOS). Without the feature a macOS musterd keeps a software
  key, which is what CI's macOS job builds and tests.
- The NativeApp (Accessibility) controller and locked-screen operation are
  scoped by [architecture §5](../../docs/ARCHITECTURE.md#5-migration-roadmap-strangler-pattern-delete-code-every-phase) and
  [P0-validation.md](../../docs/P0-validation.md). No provider CLI process or
  terminal-agent fallback exists; open the session in the desktop app first.
- Encryption at rest for the store (D3) is deferred.
