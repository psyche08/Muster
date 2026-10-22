# src

The implementation of [docs/ARCHITECTURE.md](../docs/ARCHITECTURE.md). Business
logic lives in Shared Rust; everything else is a thin platform layer
(AGENTS.md).

| Path | What it is |
|---|---|
| [`muster/`](muster/README.md) | The Rust workspace: identity, roster, links and relay (`muster-core`, `muster-net`, `muster-relay`), the services (`muster-agent`, `muster-terminal`, `muster-browser`), the desktop daemon `musterd`, the client core `muster-app-core` behind the C ABI `muster-ffi`, the `muster` CLI, OS adapters (`muster-platform`), and deployment files |
| [`native/`](native/README.md) | Platform shells (`apple/MusterApp` for macOS and iOS, `android/muster`, `harmonyos/muster`) and macOS OS components (`apple/MusterMacShim`, the root helper `apple/NetHelper`, the Locked Use plugin `apple/MusterLockedUse`) |
| [`browser/agenthalo-extension/`](browser/agenthalo-extension/README.md) | The Chrome extension adapter for Agent Halo browser control |

The Go agent and relay, the `src/rust` workspace and the AgentHalo apps were
removed on 2026-10-02 (architecture §8.5); `git show acf23be7:src/<path>`
recovers any of them.
