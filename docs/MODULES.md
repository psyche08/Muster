# Module Map: Layers, Crates, and Where Business Logic Lives

Status: source review of `src/muster` on 2026-10-04. The target architecture
is [ARCHITECTURE.md](ARCHITECTURE.md); this document is the map of its
implementation: which layer each crate belongs to, what each layer may depend
on, which modules carry product state, and which product knowledge is data
rather than code. `src/muster/scripts/check-layers.py` enforces §2 in CI.

## 1. Layers

Product integration has three layers (architecture §0): desktop applications
→ Agent Halo and the terminal adapters → Muster Networking + Hardware
Security. The Rust workspace realizes them as six crate layers. Each layer
depends only on the layers below it, with the two exceptions named in §2.

```text
┌─ 6 Clients ──────────────────────────────────────────────────────────────┐
│ muster-app-core (Intent → State → ViewModel / Effect)   muster-ffi (C ABI) │
├─ 5 Composition ───────────────────────────────────────────────────────────┤
│ musterd (the device daemon)                   muster-cli / muster-acp   │
├─ 4 OS adapters ───────────────────────────────────────────────────────────┤
│ muster-platform (macOS: AX, lock state, SEP, NetHelper XPC, Locked Use)    │
├─ 3 Services (the integration layer) ──────────────────────────────────────┤
│ muster-agent (Agent Halo)   muster-terminal (iTerm2 / SSH)   muster-browser│
│ muster-web (the one-origin web window)                                     │
├─ 2 Infrastructure ────────────────────────────────────────────────────────┤
│ muster-store   muster-rpc   muster-net   muster-relay   muster-apns        │
├─ 1 Foundation (no I/O, no runtime) ───────────────────────────────────────┤
│ muster-core (trust model, app registry)   muster-proto (schemas)   muster-paths │
└───────────────────────────────────────────────────────────────────────────┘
```

| Layer | Crates | Owns | Never contains |
|---|---|---|---|
| 1 Foundation | `muster-core`, `muster-proto`, `muster-paths` | Device identity, the signed roster and its fold, scopes, local policy, the one `authorize()`, Locked Use proofs, the desktop app registry (`apps`); protobuf schemas; state-directory paths | I/O, async, platform code, a dependency on any other workspace crate |
| 2 Infrastructure | `muster-store`, `muster-rpc`, `muster-net`, `muster-relay`, `muster-apns` | SQLite state and the OperationLedger; muster-rpc envelopes over any byte stream; the QUIC link, paths, gossip, pairing, forwarding, vnet; the dumb relay; the APNs provider | Product semantics (what an agent session or a terminal is), authorization decisions beyond what `muster-core` exports |
| 3 Services | `muster-agent`, `muster-terminal`, `muster-browser`, `muster-web` | Agent Halo: viewers, controllers, Chat mode, question cards, queues, receipts, **app profiles**; the iTerm2 client, VT grid, SSH client; the Chrome native host; the web window onto one foreign origin (ADR 0049) | OS calls (no Accessibility, no Apple Events, no XPC); knowledge of the daemon or the shells |
| 4 OS adapters | `muster-platform` | Traits for what only the OS provides, and their macOS implementations: Accessibility driving of the profiled apps, lock state, input counter, code signing, Secure Enclave signing, NetHelper XPC, Locked Use console helpers | Business decisions: it executes a profile's strategies and the engine's checks, it does not choose them; a dependency on `musterd` or `muster-app-core` |
| 5 Composition | `musterd`, `muster-cli`, `muster-acp` | The daemon composes identity, roster, link, services, authorization and the Locked Use actor; the CLI and its ACP runtime adapter speak muster-rpc to it | Logic that another layer should own (a provider branch belongs in a profile, a protocol rule in `muster-core` / `muster-net`) |
| 6 Clients | `muster-app-core`, `muster-ffi` | One reducer per feature, ViewModel projections, subscriptions, in-app SSH; nine C functions | Anything a shell could reconstruct business state from: the shell receives display-only ViewModels and sends Intents |

## 2. Dependency Rules

`scripts/check-layers.py` reads every crate's `[dependencies]` and
`[target.*.dependencies]` (dev-dependencies are test fixtures and do not
count) and fails CI when a crate depends on one the map does not allow.

```text
muster-core      → (none)                     muster-proto → (none)     muster-paths → (none)
muster-store     → core                       muster-rpc   → proto, core
muster-net       → core                       muster-relay → core, net  muster-apns → (none)
muster-agent     → proto, core                muster-terminal → proto, core
muster-browser   → paths                      muster-web   → (none)
muster-platform  → proto, agent, store, core
musterd          → browser, web, core, net, proto, store, rpc, terminal, agent, platform, paths, acp
muster-acp       → proto, rpc
muster-cli       → proto, rpc, paths, browser, acp
muster-app-core  → agent, proto, rpc, musterd, terminal, core, net
muster-ffi       → app-core, terminal
```

Two edges cross the simple layer order on purpose:

- `muster-app-core → musterd`: phones embed the daemon in the app process
  (architecture §3.3); the desktop app talks to a separate `musterd` over
  its socket through the same code.
- `muster-platform → muster-agent`: the OS adapter implements the service's
  `Controller` trait and evaluates the service's profile selectors over live
  elements. The service never depends back on the adapter.

Adding a dependency means editing the map in the script and this table in
the same change, so the direction stays a decision rather than an accident.

## 3. Where the Product State Machines Are

AGENTS.md requires every product state machine, protocol decision and
authorization decision to live in Shared Rust. This is where each one is:

| State or decision | Module | Notes |
|---|---|---|
| Roster fold, membership, revocation | `muster-core::roster` | Order-independent fold, property-tested |
| Authorization (`authorize`, `authorize_port`) | `muster-core::policy` | One function for service streams, forwarding streams and vnet packets |
| Locked Use proofs, TTL, nonce binding | `muster-core::locked_use` | Verified by the target; the platform only reports facts |
| Locked Use lifecycle (prepare, execute, relock, cancel) | `musterd::locked_use` (`mod.rs` owner API, `actor.rs` state machine) | Accepted on devices; see LOCKED_USE.md |
| Link paths, carriers, health, hysteresis | `muster-net::magicsock`, `secure_relay`, `node` | Phones use the lighter relay-only profile |
| Pairing, gossip, forwarding, vnet filtering | `muster-net::pair`, `gossip`, `forward`, `vnet` | |
| Operation idempotency | `muster-store` OperationLedger, `musterd::handler::ledgered` | Applied / duplicate / conflict / unknown |
| Agent session catalog, viewers, change push | `muster-agent` (`lib.rs` service; `jsonl` engine over the profiles' record grammar with `codex` / `claude` / `deepseek` hooks, reading DeepSeek Harness's Zstandard-framed logs through `transcript`; `catdesk` store reader; `deepseek_app`, DeepSeek Harness driven through Muster's plugin, `deploy/deepseek-harness`) | |
| Session control through Accessibility | `muster-agent::controller` trait; `muster-platform::macos::native_app`, `native_chat` | Engine checks: process signature and start time, input generation, exact echo, one press per attempt, receipts |
| Desktop Chat mode parsing | `muster-agent::desktop_chat` | Interprets AX snapshots; the platform presses what it is given |
| Question cards, queued prompts, send receipts | `muster-agent::questions`, `queue`, `receipt`, `unconfirmed` | |
| Terminal rows, deltas, VT state, SSH | `muster-terminal` | iTerm2 and SSH sessions share the grid |
| Request admission (scope × policy × presence) | `musterd::handler` (`access_for`, `authorize_request`, `admit`) | Local principal by socket uid, remote by roster DeviceId |
| Client state, retries, merging, ViewModels | `muster-app-core::state`, `update/*`, `render` | One module per feature in `update/`; shells never retry or merge |

## 4. Product Knowledge as Data (the Upper Layer's DSL)

The three desktop apps differ mostly in data: where sessions are read, what
the controls look like, what the app can do. That knowledge is written once
per app as a **profile** in `src/muster/crates/muster-agent/profiles/`
(TOML, schema in [APP_PROFILES.md](APP_PROFILES.md)) and compiled by the
crate's `build.rs` into the statics of `muster_agent::profile`. The engine
is shared; a profile can only narrow what it allows.

| Profile part | Describes | Read by |
|---|---|---|
| `[app]` | Provider (joins `muster_core::apps`), executable, which process shows windows, how a web area names a session, where history is read, the cross-process lock name | `musterd::agent_bridge` (scan, resolver, queues, locks), `muster-platform` (process lookup) |
| `[capabilities]` | send, respond, interrupt, files, model options; questions, queue and chat follow from the tables present | `musterd::handler` rejects an undeclared operation before any lock or ledger entry; `muster-app-core::render` projects `can_*` into `ChatView` |
| `[source]` | The record grammar of a JSONL source: state directory, timestamp and cwd fields, skipped records, bootstrap, message envelope and roles, text and media blocks, block numbering, tool calls and results, turn start and end | `muster-agent::jsonl` (the one transcript engine), `receipt`, `recency` |
| `[controls]` | Composer, send, stop, approval selectors; drafting strategy; session list strategy; queue and question-card controls | `muster-platform::macos::native_app`, `native_queue`, `native_questions`; `desktop_chat` over snapshots |
| `[chat]` | Chat mode: area, URL shape, mode switch, history rows, send / stop / new chat / dialog controls | `muster-agent::desktop_chat`, `muster-platform::macos::native_chat` |
| `[cloud]` | The provider's cloud sessions: API strategy, origin, label, poll cadence, where the CLI keeps its credentials, what a cloud session offers | `muster-agent::cloud` (the two strategies), `musterd::handler` (cloud operations), `muster-app-core::render` (label, `can_*`) |
| `vocabulary.toml` | Label lists shared by profiles (`$stop`, `$approve`, …) | Every profile |

What is still code per app (architecture §3.6.4): session discovery and
head metadata (`codex.rs`, `claude.rs`, `deepseek.rs`), the four named hooks of each JSONL
source (text cleaning, title records, usage, Claude's prompt queue), the
CatDesk store reader (`catdesk.rs`), question-card protocols
(`questions.rs`), Chat mode's model and effort pickers, and the two resolver
implementations the `session_key` strategies name, and the two cloud
strategies `[cloud].api` names (`cloud.rs`).

Other closed sets the shells render but never decide: the `Intent` /
`Effect` / `ViewModel` schema (`muster-proto/proto/app.proto`), the scopes
grammar (`muster-core::scope`), and the app registry (`muster-core::apps`).

## 5. Adding a Desktop App

1. Register its identity in `muster_core::apps` (provider, labels, bundle id,
   signing team) and add the provider to the join defaults if it should be
   granted by default.
2. Write `profiles/<provider>.toml` and add the provider to the `APPS` list
   in `muster-agent/build.rs`. The build fails on an unknown field, an
   unknown strategy or an unresolved `$label`.
3. Record its idle, running and (if any) question-card trees with
   `muster debug ax-dump`, redact them with `scripts/redact-ax-dump.py`, and
   prove the selectors in `profile.rs` tests against the recordings.
4. Give it a session source (a viewer) if neither `jsonl` nor `sqlite`
   describes it, and a resolver if none of the `session_key` strategies does.
5. Accept each declared capability on a device (LOCKED_USE.md).

No enum needs a new variant for steps 1–3: the daemon's queues, locks,
resolvers and capability checks are built from `profile::APPS`.

## 6. Size, for Orientation

Rust source on 2026-10-04 (tests included): about 67K lines across 16 crates.
The largest files are the places where a new behavior most likely belongs
and where a split pays first:

| File | Lines | What it is |
|---|---|---|
| `muster-agent/src/desktop_chat/` | 2.2K | Chat-mode snapshot interpretation, one submodule each: `tree`, `app`, `mode`, `history`, `pickers`, `conversation`, `transcript`; `mod.rs` re-exports them and holds the tests |
| `muster-platform/src/macos/native_app.rs` | 1.7K | The Accessibility controller engine |
| `muster-platform/src/macos/native_chat.rs` | 1.7K | The Chat-mode driver |
| `musterd/src/handler.rs` | 1.7K | Request admission and dispatch |
| `muster-agent/src/lib.rs` | 1.5K | The agent service |
| `muster-net/src/node.rs`, `magicsock.rs` | 1.5K, 1.3K | The link and its paths |
| `muster-core/src/roster.rs` | 1.5K | The roster fold |

Shells: the Apple SwiftUI package is about 4K handwritten lines plus 16K
generated protobuf; Android about 1.4K (stale, P4); HarmonyOS builds and
runs (see `src/native/README.md`).
