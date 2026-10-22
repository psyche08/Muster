# AgentHalo Cross-Platform Architecture

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](../../PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

> Security specification: all new remote authorization, pairing, federation, management transport,
> and service scope implementations must follow [Muster Security Protocol V2](./muster-security-protocol-v2.md).
> Existing V1/ABI 1.19 wiring does not constitute production security completion and cannot bypass
> V2 acceptance gates for SAS, activation envelopes, quorum, anti-rollback, MST2, and revoke-disconnect.

**Status:** Phases one and two are implemented. Phase three’s iTerm2 Adapter and Ghostty
control-only Adapter have code and synthetic tests; phase four’s Rust `MusterTransport`
request layer, Apple Native connector callback ABI, and Swift driver are implemented.
Initial macOS and iOS SwiftUI product shells are implemented in this repository without
migrating or reusing the historical AgentHalo UI. Rust `agenthalo-backend` implements a private
UDS listener, provider registry, and bounded HTTP parser, launched on demand by the macOS
app as a signed helper. The Codex Provider ports the old `remote-coding` local rollout
discovery/preview path for read-only history browsing; AgentHalo-owned app-server
create/send/interrupt and the approval/question bridge are complete; Desktop owners remain
read-only. The Claude Provider retains read-only local transcript discovery/preview and adds
production logical-session creation, default plain-text input, native identity ownership, and
bounded preview. The native process starts only on first input. One tool decision, structured
answer, independent interruption of a confirmed original input, and durable receipts are wired
into production. Recovery preparation for existing native transcripts and logical sessions that
never received input, plus confirmation of the next input, are wired up. Recovery with execution
history but missing transcript, interruption before input confirmation, full execution parameters,
and actual validation on both ends remain incomplete; see “Claude Production Logical Sessions
and First-Input Lifecycle” and “Claude Production Tool Decisions and Receipts.” Apple macOS/device/
simulator staticlibs, a three-slice XCFramework, unsigned Xcode linking, and Simulator ABI
round-trip validation have passed. There is historical runtime evidence for macOS app builds,
model tests, strict signing, actual launch, Codex approval-deny E2E, and iTerm2 live session
list/snapshot/text input (Automation/TCC evidence needs revalidation); real relay/agent E2E
has not been accepted. Android has its first Kotlin/JNI library, arm64/x86_64 Rust artifacts,
and AAR build; device runtime, Keystore, and secure-session connector await implementation/acceptance.

iOS now opens AgentHalo instead of the Muster Phase 1 tools page, with remote providers/sessions,
conversation preview, conditional watchers, provider-gated mutations, and in-session approval/
structured-question cards. Muster/SEP pages remain as Network diagnostics. Actual Simulator
launch and generic Simulator/device unsigned builds have passed.

## Product Boundaries

### Shared Semantics for Provider Execution Options

Shared Rust preflight and backend Provider validation jointly constrain create/send parameters.
Claude currently advertises only `provider-default`, using its native default approval policy;
this is neither an operating-system sandbox nor an alias for Codex automatic review/full-access
policies. Unsupported model, reasoning-effort, and sandbox overrides are explicitly rejected.
Old signed logs retain their original structure and bytes; tighter validation for new requests
must not delete or rewrite historical failed requests.

Mac and HarmonyOS show only options actually supplied by the Provider; HarmonyOS filters create
entry points by target device and isolates late callbacks. Runtime fields may be displayed, but
models/permissions absent from the options must not become overrides for the next request.
Rust projects Codex approval presentation from explicit policy and reviewer identity; unknown
combinations are never inferred to mean automatic approval. This wiring is not complete Provider
parameter parity, signed installation, or acceptance of actual execution on both ends.

### Recovery of Claude Logical Sessions That Never Received Input

Explicit recovery rescans native directories and archive/hidden state. With no transcript, Shared
accepts only the original durable create record and requires no input/control history whatsoever;
successful or canceled input cannot count as “never sent.” Legacy unscoped records block this branch;
if a create record exists but its input log is missing, do not automatically recreate an empty log.
Recovery preserves the original UUID and canonical working directory, saves a recovery-preparation
receipt first, and starts native creation only on the next explicit input. Do not invoke native
resume against a nonexistent transcript or obtain native control from HTTP success.

Historical logical sessions are displayed through an empty `session_id` and the original UUID in
`transcript_id`, preventing either client from treating history rows as currently writable sessions.
This route does not imply a real transcript exists. Mac recovery entry points and HarmonyOS Shared
presentation therefore reuse explicit recovery without platform business decisions. Sessions with
input/control history but missing transcripts, or entries with pending same-process operations,
cannot use the never-sent branch.

### Explicit Recovery of Native Sessions Closed in the Same Process

The SDK separately records actual `wait` confirmation that its former child process ended; it does
not infer this from closed flags, cleanup state, or an empty process slot. While holding the session
map lock, the controller verifies there are no borrowers of the old entry and that original input/
control operations and their retained candidates completed or were canceled before a new explicit
recovery intent may replace it. The registry uses nonblocking locking to avoid deadlock with control
requests’ opposite lock order. Recovery preserves the original UUID, logical title, and working
directory without consuming another session slot. Parse failure retains the old entry; append failure
retains the exact new candidate. A preparation receipt still does not establish native identity;
only the next explicit input launches a new native process.

Closed rows expose only the recovery route, not a currently writable session ID. Duplicate checks
for native request IDs also bind the original client object, preventing completed approvals from
an old process from intercepting identically named requests in a new process. Queries for old
operations still return only old receipts. Unknown/retryable original operations, missing evidence,
historical-capacity governance, and real two-ended recovery acceptance remain separate work.

### In-Memory Lifecycle of Completed Control Results

Rust centrally manages Claude production control receipts: release business retention entries and
observed native tool results only after the exact original request’s completed receipt is durable.
Retain them if synchronization fails, the original operation is still running, or the native result
has not appeared. The SDK releases result payloads but retains bounded per-generation tool-ID reuse
protection; changing a request ID cannot attribute late old results to a new call. Outer reclamation
does not detach an interrupt from its original input. This does not delete durable logs or remove
the long-term bounds on native input history, request-reuse records, and persistent logs; corresponding
product governance and actual two-ended acceptance remain pending.

Claude native input likewise separates pending retention from completed evidence. Release strong
references from the tracking table when preparing later input only after the exact acceptance
receipt is durable, the native final result is observed, and no interrupt is pending. Original opaque
input/receipt handles independently hold evidence. Retained old evidence still checks duplicate
result consistency. Evidence contents are allocated separately and actually reclaimed when the last
handle is released, rather than retained in weak-reference registration allocations. Each generation
keeps at most 4096 weak-reference records of retired input UUIDs and forbids UUID reuse. Ignore late
old events only when they are known retired and have no consumers; they cannot confirm or complete
current input. The 128 pending-input limit and long-term durable logs remain bounded; when full,
preserve original evidence and report an explicit error instead of clearing history.

### Two-Layer Cancellation of Proven Undelivered Input

Claude cancellation requires proof from Rust’s retained original request or original durable
undelivered record. Missing history, HTTP errors, unknown outcomes, and native acceptance confirmations
are not cancellation grounds. Under the native send lock, an active original object first disables
sending, then appends and exactly confirms the cancellation record. Synchronization failure preserves
the disabled-send state; retries may only finish that original cancellation. Release retention and
native records only after durable confirmation. Never reuse original operation IDs/input UUIDs or
delete complete durable history.

Local and authenticated remote cancellation endpoints obtain inner cancellation credentials that
cannot be constructed from JSON, verify bound original operation ID/type/exact request bytes, then
report cancellation to the existing Shared client. Preserve the outer original failed/unknown log.
If a cancellation response is lost, the original append is later confirmed, or the process restarts,
restore the UI result through read-only cancellation credentials in the same namespace. Missing,
successfully completed, or mismatching outer operations cannot be used to cancel another operation.
Cancellation of possibly delivered input remains unsupported.

### Exact Result Recovery After a Saved Error

Status queries for six local/authenticated-remote write operations support “outer error already
saved, inner durable receipt later available.” Rust verifies both the outer namespace/original
canonical request and inner original operation ID/type/exact request bytes. Preserve the outer
error log; read-only queries generate recovery responses from durable inner opaque receipts,
including after restart. Pending or unknown outer records retain the existing durable-completion
path. Missing, canceled, retryable, already-successful, or proven-undelivered attachment-expiry
terminal records cannot be rewritten. Without an exact inner receipt, retain the original error.
Acceptance receipts do not prove model success, completed tool side effects, or current session
ownership. Queries neither resend requests nor start native processes, and do not replace ownership
persistence and current authorization checks required for subsequent explicit operations.

### Main Product and Infrastructure Decisions

AgentHalo is the user-facing main product and the only desktop/mobile entry point on macOS, iOS,
Android, HarmonyOS, and Windows. Platform order follows the current product plan; do not build
separate Muster GUIs for individual platforms.

Muster is AgentHalo’s underlying network platform, not a peer main app. It owns hardware identity,
device discovery/authorization, encrypted virtual networking, routing, and remote-service exposure,
and is embedded, launched, and managed by AgentHalo. Terminal is an AgentHalo feature; Agent
Providers such as Codex and Claude are also its business capabilities. Computer Use and Locked Use
are higher-level AgentHalo capabilities and do not own the network layer or platform UI.

The repository provides one foundation and four higher-level capabilities:

1. **Muster Network**: establishes device identity using platform hardware such as SEP, Android
   Keystore, and Windows CNG/TPM, and provides discovery, authorization, revocation, encrypted
   virtual networking, routing, and service forwarding;
2. **Agent**: interacts with Codex, Claude, and other agents through Provider Adapters;
3. **Terminal**: views/operates terminals through iTerm2, Ghostty, ConPTY, PTY, and other Adapters;
4. **Computer Use**: captures screens and performs pointer, keyboard, and application interaction
   within explicit platform authorization and Adapter capabilities;
5. **Locked Use**: limited operations while a device is locked, with independent capabilities,
   security policies, and audit boundaries. Ordinary Computer Use availability must not imply
   locked-screen operations are available.

Dependency direction is fixed: `AgentHalo → Agent/Terminal/Computer Use/Locked Use → Muster Network`.
All remote higher-level capabilities discover devices and establish authenticated connections through
Muster; no independent SSH bypass, raw TCP, or separate device-registration system. Desktop clients
with local services and platform Adapters may use local UDS/Adapters for Agent, Terminal, and Computer
Use. These desktop-local capabilities remain usable while Muster/relay is offline, with remote network
state explicitly shown.

iOS, Android, and HarmonyOS are clients of remote device capabilities through Muster. They have no
offline local capabilities, no mobile-local AgentHalo backend, and must not provide or pretend to
provide local Providers, Terminal, Computer Use, or Locked Use. Mobile Agent/Terminal/Computer Use/
Locked Use depend on Muster discovery, authentication, and encrypted connections. If Muster, relay,
or the target is unavailable, the shell and Device/Network pages remain accessible, but affected
capabilities explicitly show offline/unavailable and all mutations fail closed. Locked Use additionally
requires the target platform’s declared locked-screen capability and security prerequisites.

Although not the main app, Muster is a first-class AgentHalo page: users inspect network/devices,
add/authorize/revoke/delete devices, and diagnose issues. Headless servers/operations environments
retain `muster-agent` and `muster`/`musterctl` CLI tools. These are AgentHalo network nodes and
management tools, not another terminal desktop product.

Brand/ownership boundaries: AgentHalo owns windows, navigation, sessions, composers, settings,
notifications, upgrades, and cross-platform experience. Muster names protocols, network core,
daemons, helpers, CLIs, and network diagnostic data. Platform layers implement only hardware keys,
lifecycle, permissions, and native Adapters; Rust Core owns shared protocols, routing, and session semantics.

`~/Developer/Projects/AgentHalo` is a retired historical project, not a source tree, reference UI,
or API contract for the new product. Build the new macOS app from scratch in this repository.
Unless explicitly decided otherwise, do not copy its three-column console, navigation, page models,
or build targets.

```text
Native AgentHalo Apps
        │ Agent / Terminal / Computer Use / Locked Use / Network
Rust AgentHaloClientCore
        │ AgentHaloTransport
        ├── LocalUDSTransport (desktop local capability only)
        └── MusterTransport (all remote capability; mobile capability path)
                    │
              target AgentHalo UDS
                    │
AgentHalo Core ── TerminalProvider Registry
                    ├── iTerm2Adapter
                    ├── GhosttyAdapter
                    ├── ConPTYAdapter
                    └── PTYAdapter
```

## Rust and Native Responsibilities

Shared Rust owns:

- AgentHalo API and session data models;
- Transport, error classification, and prevention of ambiguous replay;
- Terminal Provider protocol and capability negotiation;
- snapshot/delta, cursor, input sequence/ACK;
- Muster cross-platform protocol state machines;
- Cross-platform consistency tests and a stable FFI ABI.

Native Platform Layer owns:

- Swift/SwiftUI, Kotlin/Compose, and Windows native UI;
- Apple SEP/Keychain, Android Keystore, Windows CNG/TPM;
- Platform Adapters such as iTerm2, Ghostty, and ConPTY;
- App lifecycle, permissions, main-thread handling, and platform-error conversion.

Rust holds no platform UI objects and exposes no async runtime objects across FFI. Native signer
callbacks inject hardware signing into the Rust Muster state machine.

## AgentHalo API and Local Transport

`agenthalo-client` depends only on `AgentHaloTransport`; the same typed API runs over local UDS
or optional Muster transport. Current coverage includes health, provider discovery, live/native
session merging, preview, task history, pending approval, scoped status, session create/resume,
durable prompt, interrupt, approval, structured-question mutations, and Muster authorized-device
management. Stable C ABI 1.6.0 exposes these Local UDS operations to native shells. Native decodes
response JSON without copying socket HTTP, query encoding, session merge, or mutation replay rules.

Preview envelopes contain provider-native `sig`. Native polling sends it back as optional `if_sig`.
If Codex rollout or Claude transcript size/mtime is unchanged, the Provider returns
`not_modified=true` before parsing JSONL, avoiding rescanning long transcripts or transferring full
message arrays. Legacy `agenthalo_local_preview` and HTTP clients omitting `if_sig` retain full
responses. `agenthalo_local_preview_if_changed` is additive in ABI 1.5.0; ABI 1.6.0 adds the first
typed Muster health/providers/sessions/preview/create/send/interrupt operations for iOS; ABI 1.7.0
adds tasks, pending approval, scoped status, resume, approval, and structured questions. ABI 1.8.0
adds Muster device list/authorize/revoke/delete for native shells. macOS provides management UI in
the main AgentHalo app; later platforms reuse the typed API, not the old standalone `MusterDevices`
shell. ABI 1.9.0 adds provider execution options and create/send with sandbox/model/reasoning
settings; old entry points retain defaults. iOS does not duplicate query encoding, live/native
session merging, or mutation replay; it supplies only authenticated raw Muster streams to Rust.
Approval/question uses `ReplayPolicy::Never`; UI displays only Provider-returned
`available_decisions` for that request and fails closed if absent.

Codex changed-preview also keeps bounded incremental parse state for at most eight rollouts:
byte offset, first `turn_context` boundary, incomplete tool-call associations, and parsed messages.
Appends to the same inode read only new JSONL bytes; inode replacement/truncation discards cache
and rebuilds fully. Exact previews for discovered sessions directly `stat` cached paths instead of
rescanning the sessions directory on the discovery coalescing cycle. Release-helper benchmarks on
a real ~34 MB rollout: initial fill ~0.19 seconds; unchanged and appended reads both below `time(1)`’s
0.01-second display precision. Before optimization: warm read ~0.06 seconds, appended read after
directory refresh ~0.13 seconds.

Apple Native facade: `src/native/apple/AgentHaloCoreClient`. It always schedules synchronous C ABI
calls on background detached tasks, preserving FFI status, HTTP status, transport error kind, and
retry disposition. It returns only JSON `Data`, holding no SwiftUI/product model. Production macOS/
iOS apps can reuse their own `Decodable` models while sharing Rust network semantics.

`agenthalo-transport-local` remains HTTP/1.1-over-Unix-socket compatible with the existing backend:

- Socket paths must be absolute;
- Request targets forbid whitespace, control characters, and fragments;
- Explicit limits on request/response bodies and response headers;
- Finite read/write timeouts; no indefinite blocking;
- Reject conflicting Content-Length, truncated bodies, and unsupported transfer encoding;
- Safe reads may retry with backoff; `Never`/sequenced mutations return `AmbiguousMutation` when
  the response is lost after sending. Only mutations durably deduplicated by backend operation ID
  may declare `StableOperation`.

This repository’s `agenthalo-backend` is the new cross-platform local-daemon baseline. It registers
Providers, routes AgentHalo API, and listens on a current-user-private UDS: no group/other permissions
on the parent directory, socket mode `0600`; active sockets are never replaced, stale sockets recover
safely, and ordinary file paths fail closed. macOS first connects to an existing endpoint and launches
the bundled signed helper only when the default endpoint is offline. Startup completion requires an
actually connectable socket, not path existence. One disconnected/malformed client cannot terminate
the daemon. It does not launch Muster; only explicit remote transport uses Muster.

Muster network management is a protected AgentHalo feature. Backend accesses the target host’s
sealed ledger through shell-free `muster-agent authorize ... --json`, exposing
`GET /muster/devices` and `POST /muster/devices/add|revoke|delete`. macOS requires device-owner
authentication before showing the ledger. Add approves a pending fingerprint or explicit SPKI;
revoke retains an audit tombstone and terminates matching live connections; pending records may
be deleted directly, while permanent deletion requires prior revocation. Device mutations do not
yet declare stable replay: response loss after sending is ambiguous and must not auto-retry.
Upgrade to `StableOperation` only after integrating a durable operation ledger.

## Muster Service Directory and Cross-Device Routing

Every platform uses the same remote data path. Clients connect independently to each discovered
device and aggregate sessions locally:

```text
Muster device/service discovery
  ├─ mac-a  ──Muster tunnel:8765──> AgentHalo backend
  ├─ mac-b  ──Muster tunnel:8765──> AgentHalo backend
  └─ mac-c ──Muster tunnel:8765──> AgentHalo backend
                    ↓
        Client aggregates locally and groups by device
```

Muster must provide a separate service directory. Neither the authorization roster nor static
`agent.yaml -> v2.vnet.peers` is a discovery result. The roster answers only “who may connect”; it
neither indicates AgentHalo service availability nor guarantees complete dialing details. Each
service-directory device record returns at least:

- Stable `device_id`, preferably SPKI-derived rather than a display name or array index;
- Device display name;
- Relay peer;
- SPKI pin;
- `agenthalo` service declaration and port `8765`;
- Online/reachability state and service capability version.

### Owner Pairing and Automatic Discovery of All Devices

“Pair once, automatically discover every device belonging to this owner” belongs to **Muster
Network**, not independent business discovery in AgentHalo, Terminal, Computer Use, or Locked Use.
Every client consumes one owner-directory contract. Business layers declare/filter their service
IDs and perform application health/capability probes once transport is ready.

Initial pairing must create an owner-level trust anchor, not merely save a guessable username.
At minimum it includes stable `owner_id`, directory-signing `key_id`, and owner-directory P-256
public key. Human-readable names such as `example` or relay namespaces are only for display/routing,
never sufficient authorization to read a directory or trust a device. Subsequent directory snapshots
are end-to-end signed by that owner key; relay only stores/forwards signed envelopes and cannot
replace SPKI/services/capabilities while retaining client acceptance.

Pair using a short-lived, single-use pairing bundle (QR, same-device confirmation, or equivalent
explicit out-of-band flow), containing at least that trust anchor, directory endpoint, `invitation_id`,
challenge, and an unguessable one-time pairing code. Never include a long-lived directory read
capability. After hardware-SPKI challenge proof, mobile receives only an inactive candidate read
capability; relay activates it only after the owner compares short codes on both ends and signs a
client grant for the matching subject. Show owner/key fingerprints and require explicit confirmation;
expiry, repeated redemption, or inconsistent confirmation fails closed. Read capability protects
directory privacy and relay access, not a substitute for owner signatures. It must never appear in
signed snapshots, logs, public username queries, or device display names. Store redeemed credentials
in platform secure storage: Apple Keychain, Android Keystore-wrapped storage, Windows DPAPI,
and HarmonyOS hardware-key/secure storage. Read-credential rotation must preserve `owner_id`.
Owner signing-key rotation requires a transition signed by the old key or explicit re-pairing;
relay must never replace the trust anchor unilaterally.

Directory snapshots contain at least:

- `owner_id`, monotonically increasing `revision`, issue time, and expiry;
- Each device’s SPKI-derived `device_id`, display name, relay peer, relay URL, and agent SPKI pin;
- Published services, each with stable service ID, forward port, and nonempty versioned capability
  version string; business adapters may impose stricter version formats for their services;
- Owner signature and corresponding `key_id`.

Signature input uses versioned, cross-language deterministic encoding, never arbitrary JSON
serialization. v1 uses fixed field order, UTF-8 bytes, big-endian lengths/integers, devices sorted by
`device_id`, services sorted by `(service_id, port)`, and `MUSTER-OWNER-DIRECTORY-V1` domain
separation. Every client rejects:

- Owner/key mismatch, bad signature, expired or not-yet-valid snapshots;
- Revision rollback;
- Same-revision directory forks with different contents;
- Duplicate devices/services, `device_id` inconsistent with SPKI, non-`wss` relay URLs, or invalid ports;
- Unsigned records sourced only from relay online tables, authorization rosters, or static peer config.

Muster Network owns signature verification, encrypted caching of the last verified snapshot,
incremental/subscription sync, revision/diff, and peer reconciliation. One owner snapshot may publish
`agenthalo`, `terminal`, `computer-use`, and `locked-use` services; ordinary Computer Use records do
not imply Locked Use. AgentHalo extracts only `service_id=agenthalo`, establishes per-device
transport, probes `8765 /healthz`, and aggregates sessions.

Offline devices remain in the directory. Relay online state is only an unsigned short-lived display
hint, never identity evidence. Muster reconciliation cancels a matching supervisor only when an
owner explicitly removes the device/service. Temporary offline state, failed directory requests,
or snapshot expiry must not silently turn last-verified devices into Local or trust a new pin.
Without an initial owner trust anchor, enter explicit pairing/bootstrap only; never expose public
“enumerate all devices by username” APIs.

Shared implementation boundary: Muster Core exposes verified versioned snapshot/delta/status APIs
with C/JNI/platform bindings. iOS, Android, HarmonyOS, Windows, and macOS supply only platform
keys/storage, lifecycle, and UI consumption; they must not copy signing encodings/revision rules
or maintain separate device tables.

#### Owner Directory Sync Lifecycle Cross-Platform Contract

After owner pairing, automatic discovery must do more than read one snapshot at pairing completion.
Muster Network creates an independent, cancelable directory-sync lifecycle for each activated remote
authority pairing, using the verified trust anchor, credential-free HTTPS directory endpoint, `odr_`
read capability in secure platform storage, and last verified revision/ETag/envelope. `odr_` may exist
only in secure storage and borrowed request callbacks, never in status events, logs, business models,
or persisted plaintext snapshots.

Shared Muster Core owns the sync state machine and centrally handles:

- Restoring the last verified snapshot at app startup/foreground and immediately publishing all its devices as `stale` or cached;
- Cancelable long polling with `GET /v2/owner-directory/snapshot?since_revision=N&wait_seconds=30`
  and `If-None-Match`; apply 200 responses only through shared verifier/rotation/revision rules; continue subscribing on 304;
- Independent per-owner generation/sequence, revision/ETag, `unpaired/syncing/ready/offline/stale/
  unauthorized/protocol_error/cancelled` states, error classification, and capped exponential backoff
  with jitter; one owner’s failure cannot affect others or other established peers;
- Retaining last-verified devices as stale/offline on network failure or temporary absence of a relay
  snapshot; never clear devices, fall back to Local/bootstrap pins, or trust unsigned online tables.
  `unauthorized` and signature/rotation/protocol errors are terminal: stop retries and require
  reauthorization or explicit recovery;
- Emitting removal deltas only for explicit device/service removal in an owner-signed snapshot;
  verified new snapshots drive the same Muster peer-network reconciliation, not a second connector/retry supervisor;
- Idempotent reconciliation/cancellation on pairing, owner-set changes, foreground/background changes,
  read-capability revocation, and app shutdown. Repeated start/reconcile must not create duplicate
  pollers/connectors or orphan tasks.

Native Platform Layer supplies only actual HTTPS requests/cancellation, secure hardware-storage I/O,
and lifecycle entry points, consuming copied shared versioned snapshot/delta/status events. AgentHalo
adapters select only `service_id=agenthalo` on port `8765`, update Device/Network and session groups,
and perform `/healthz` after shared peer transport is ready. Never implement separate directory
backoff/revision/credential state machines in Swift/Kotlin/ArkTS/C#.

Cross-platform acceptance must prove: one pairing discovers all currently signed owner devices without
developer bootstrap records; later publications appear automatically; owner-removed devices disappear;
offline groups remain; app restart restores secure cache and resubscribes; 304/initial offline/late relay
startup recover; terminal authorization errors do not retry; lifecycle/shutdown create no duplicate
requests. Separately record relay deployment, owner-snapshot publication, platform signing/installation,
real device discovery, per-peer transport, 8765 health, and session read/write. Mock snapshots or ABI
unit tests cannot replace any layer.

#### Federated Directory Across Multiple Terminal Authorities

A terminal with Agent or Terminal capability and a platform hardware identity may be an authority.
Eligibility comes from capability and explicit local authorization, not OS name. This applies to macOS,
Windows, and future Linux terminals with equivalent security; pure mobile remote clients are consumers
by default. Each authority has its own `owner_id`, non-exportable hardware-protected owner key (e.g.
SEP/TPM), independent `odp_` publisher capability, and existing relay owner-directory record. Terminals
must not share `owner_id`, owner private key, or `odp_`. Single-owner current-key/rotation/monotonic
revision semantics are not a multiwriter protocol; shared keys or alternating rotation cannot simulate
multiple authorities.

Each authority publishes only self-authoritative snapshots, containing only its own
`device_id=spki:<sha256>` and actually declared Agent, Terminal, Computer Use, Locked Use, and other
services. The global device map is the union of `LocalAuthoritySource` and all verified remote
owner-directory snapshots, not a full map republished by one terminal. On source conflicts, the local
self-authoritative record wins; remote observations cannot change another device’s SPKI, service
endpoint, or routing identity. Conflicting pins/identities for one `device_id` quarantine the conflicting
source and fail closed; never overwrite by modification time or display name.

Relay adds no global federation directory or authority-set API. Reuse existing `create-owner`/`odp_`,
publish, single-use invitation/redeem/approve, per-owner `odr_`, ETag/revision/rotation, and cancelable
long polling. An authority neither subscribes to its own directory nor needs its own `odr_`; its local
record comes from `LocalAuthoritySource`. It stores secure `owner_id -> trust anchor/endpoint/odr_`
pairings only for other authorities, running one sync owner per remote owner on the same shared peer
network. N authorities thus form an explicitly paired federation; N×(N-1) directed read relationships
are acceptable at personal-device scale, with credentials independently revocable per owner.

To add authority B, B creates hardware identity/owner key locally and registers an existing relay owner
record. A and B perform the existing invitation/grant flow in both directions, each obtaining `odr_`
to read the other, then publish self-only snapshots. To add mobile, the inviting authority completes
its own pairing and coordinates other online authorities through authenticated Muster management
service so each performs its own invitation/grant; mobile stores multiple owner pairings. Offline
authorities remain `pending` and resume when online; partial unreachability must not silently classify
devices as Local. A mobile device lacking Agent/Terminal authority capability does not become an
authority merely by holding multiple `odr_` credentials.

Muster Network/Shared Core manages membership; relay does not infer it. Shared maintains an encrypted,
reconcilable signed operation log binding at least `authority_set_id`, `operation_id`, `issuer_owner_id`,
`subject_owner_id`, `add/revoke/restore`, epoch, issue time, and signature, synchronized over authenticated
Muster links. Adding an authority requires explicit local authentication on both an existing authority
and the new terminal. Revocation normally needs `min(2, N)` valid authorities, with 1/1 for single-device
bootstrap and a strongly warned, locally authenticated emergency recovery entry. After revocation,
every platform immediately stops new connections/mutations, closes existing sessions, and removes or
seals pairing/cache. UI retains an explicit Revoked group; surviving authorities revoke capabilities
they issued through existing relay APIs. Returning offline terminals reconcile the signed log.

Shared Core owns authority sets, multi-owner pairing collections, self-only snapshot validation,
source-aware aggregation, signed-operation reconciliation, and the single peer network. Platform
layers supply SEP/TPM/Keystore, HTTPS/Muster transport, local authentication, and native UI. macOS
is the first reference consumer; iOS, Android, HarmonyOS, and Windows adopt the same contract in
their platform tasks without copying revision/conflict/membership state machines. Migration may read
old multi-device single-owner snapshots but must label them legacy. Upgraded self-authoritative
sources take precedence; retire legacy only after all eligible terminals migrate to independent owners.

#### Authority Orchestration Production Contract

`authority_federation_create(input_json)` validates an already assembled state; it is not complete
product orchestration. Production Swift/Kotlin/ArkTS/C# must not generate authority-set IDs, assemble
signed operation logs, allocate directory revisions, or construct publication snapshots. Shared Core
also owns a long-lived `AuthorityOrchestrator`, managing one atomically persisted encrypted object:
local authority identity reference, authority-set signed log, remote pairings, client grants/revocations,
pending/committed self-publication revisions, and last signed envelope. Platform callbacks only return
actual local `MusterDirectoryDevice`, sign in hardware, briefly borrow `odp_/odr_` for Core-directed
requests, perform system local authentication, and save Core-provided opaque protected state.

ABI 1.17 adds `agenthalo_authority_orchestrator_federation_input`: it exports a credential-free
federation seed only from verified protected state after explicit bootstrap, containing exactly the
Core-generated `authority_set_id`, local self-authoritative source, and signed epoch-1 operation.
Platforms may only encrypt/save it unchanged and pass it to the federation owner, never reconstruct
its fields. A local-only seed establishes authority identity, not remote federation availability.
Migration retains legacy peers until the first federation projection containing a verified remote
authority source atomically takes over. Later remote pairings/grants/operations are appended by the
same orchestrator management lifecycle, not platform edits to seed JSON.

ABI 1.18 combines the local authority transaction and full federation document into one encrypted,
crash-consistent protected state, adding `agenthalo_authority_orchestrator_reconcile_federation_input`.
It accepts only complete credential-free documents transported by authenticated
`muster.authority-management`. Shared Core revalidates all authority signatures, pairing/keyring,
and client grants/revocations, rejecting operation-log rollback/forks, deletion of verified state,
or replacement of local hardware identity. Expose atomically to the federation owner only after
successful persistence; platforms must not split persistence or update UI/peers first. ABI 1.18
completes only safe apply/restore boundaries. SAS ceremonies, local authentication on both ends,
operation-signature collection, and real Muster management transport remain later stages of the same
Shared Core lifecycle; direct reconcile calls or handwritten JSON cannot replace them.

Every eligible terminal uses the same state machine:

```text
uninitialized
  -> local_identity_ready
  -> relay_activation_pending
  -> self_publishing
  -> active
  -> degraded_offline | terminal_auth
  -> revoked
```

- `local_identity_ready` derives from the running local Muster agent’s hardware identity; `device_id`
  exactly matches its certificate SPKI. AgentHalo capability adapters explicitly register local service
  inventory and validate forwards; never infer it from remote peer tables, rosters, sessions, or display names;
- Shared Core constructs/verifies the first authority’s epoch-1 `add(self)`, later add/revoke/restore,
  operation IDs, epochs, and quorum. Platforms hardware-sign Core canonical bytes only after explicit local authentication;
- Relay activation is a deployment/owner-management action. Existing `create-owner` returns `odp_`
  once; platform immediately encrypts it with an independent hardware-protected key and stores it in
  secure system storage. Manual `odp_` entry in Developer diagnostics is not a normal product flow;
  internal deployment may use a one-time, non-loggable activation handoff;
- Shared Core generates self-only snapshots from `LocalAuthoritySource`, persisting pending revision
  and the full envelope before PUT. Retries replay exactly the same envelope; commit revision only
  after success. Crashes, retries, or clock rollback therefore cannot cause rollback/equivocation.
  Normal refresh produces the next revision only on service/identity changes or expiry-window arrival;
- Authority federation and owner-directory sync share one peer network. Keep `transport_ready` separate
  from remote `agenthalo:8765 service_ready`. Temporary directory/relay/service failures enter
  degraded/offline while retaining verified devices, never reverting to Local or deleting session cache.
  Only terminal pin/unauthorized/revoked/protocol errors stop retries;
- Legacy `agent.yaml -> v2.vnet.peers` may provide temporary connections only during
  `uninitialized/relay_activation_pending`, explicitly marked `Legacy migration`. Stop legacy connectors
  immediately upon atomic takeover by the first valid federation projection. Once migration completion
  is durable, never auto-fallback and let two sources connect the same device.

Authority join/revoke and operation-log/pairing/grant propagation use authenticated Muster
`muster.authority-management`, not SSH, raw TCP, new relay global APIs, or AgentHalo UI bypasses.
Messages bind authority-set ID, both hardware device IDs, nonce, generation, and expiry. To add an
authority, show the same SAS on both ends and perform system local authentication on each before
committing signed add. Incomplete quorum or offline authorities remain pending and resume idempotently.
On committed revocation, Shared Core first closes existing connections and fails target routes/mutations
closed, then asynchronously asks surviving owners to revoke issued read/client grants through existing
relay capabilities. Management service cannot read owner private keys or place `odp_/odr_` in messages,
events, or logs.

UI consumes only structured orchestration events: phase, authority-set epoch, local owner, each
authority’s membership/sync/grant state, publication revision, pending ceremony, conflicting/revoked
devices. Normal “Add trusted terminal” and “Add mobile device” start here. For mobile, the current
authority coordinates each active authority’s existing invitation/grant flow; offline owners stay
pending, and the current owner cannot sign for them.

Beyond peer-lifecycle tests, acceptance covers bootstrap crash recovery, byte-identical same-revision
retries, no revision decrease on clock rollback, new revision on inventory change, two-authority SAS/
local authentication at both ends, pending without quorum, resumption after offline authority returns,
immediate disconnection on revoke, no fallback after legacy takeover, and no plaintext `odp_/odr_`
or private keys in any logs/events/disk scans.

Acceptance uses at least three authorities plus one mobile client: independent publication aggregated
into one device map; one offline authority does not affect Local/other remotes; repeated reconciliation
creates no duplicate pollers/connectors; mobile receives multi-owner pairings; conflicting sources do
not contaminate healthy ones; revocation immediately blocks connections while retaining Revoked display;
no `odp_`, owner private key, or `odr_` appears in plaintext logs/snapshots/events/disk. SSH, static
rosters, shared keys, or new privileged relay APIs are not substitutes.

### Mobile Hardware Identity and Cross-Device Authorization Interaction Contract

“Discover owner devices” and “allow this phone to use them” are adjacent but separate security actions.
Owner-directory trust anchors/read capabilities permit only reading/verifying the signed directory.
Connecting requires a separate revocable AgentHalo client grant. Seeing devices does not imply permission
to connect, and authorization rosters are not service directories. The normal flow is “invite on a trusted
device, scan on the new phone, approve once on the trusted device,” not SPKI copying, JSON editing,
or per-device commands.

On first entry, AgentHalo automatically creates/reuses a hardware-protected, non-exportable P-256
identity, without requiring users to understand keys/fingerprints. iOS uses Secure Enclave with
`WhenUnlockedThisDeviceOnly`; Android prefers StrongBox/TEE-backed Keystore; HarmonyOS uses HUKS.
If hardware residency cannot be established, show the true degraded/unavailable state; never silently
create exportable software keys. Uninstall, device replacement, or identity loss means a new device
requiring re-pairing; never restore the old private key from cloud backup. SPKI/full fingerprints/
low-level diagnostics may appear in developer details, not as the primary CTA.

A trusted desktop AgentHalo device starts “Add mobile device” from Device/Network. Shared Rust creates
a single-use scanning invitation valid for exactly two minutes. Normal UI shows only a QR code: no
display/copy/share of payloads, deep links, `pairing_code`, or trust-anchor JSON, and no manual-entry
path. For unavailable cameras/accessibility, use accessible system scanning controls or restart the
QR flow on another trusted device; never fall back to copied keys, short codes, clipboard, CLI, or
username lookup. Existing deep-link encoding ABI is legacy compatibility only, outside the unified
QR contract; new entry points must neither call it nor expose it as an alternative CTA.

QR contains only Shared Rust-encoded canonical, bounded, credential-free Pairing Presentation V2 JSON.
It binds `schema_version=2`, `purpose=muster.pairing.presentation.v2`, `audience=agenthalo`, exact
directory origin, and signed Pairing Transcript V2, excluding publisher bearers, owner-directory read
credentials, long-lived private keys, and all other capabilities. Platforms must not parse/rebuild
JSON, alter canonical bytes, or construct their own QR. Shared scan entry accepts only QR JSON,
not deep links, checking QR/JSON size, canonical encoding, duplicate fields, version, purpose/audience,
directory origin, owner signature, two-minute expiry, and transcript substitution before returning
an opaque handle. Downgrades, origin substitution, oversize/duplicate payloads, and expiry fail closed.
Existing accepted owner anchors use normal verified-presentation flow; a fresh install receives only
nonserializable, non-authoritative `VerifiedCandidatePresentationV2`. The QR’s self-declared owner-key
signature proves internal consistency only; it cannot authorize owner-directory, Grant, Activation,
MST2/readiness, or be extracted by a platform for raw owner-anchor acceptance.

After scanning, the phone proves challenge possession using its newly generated hardware private key.
The owner-side Rust invitation handle accepts only the first successfully verified proof, marking
the invitation redeemed and closing further proof input before returning SAS. Identical or different
replays all fail closed. Since QR contains no bearer, this burn is the Rust-owned owner invitation’s
single-redemption terminal state. ABI 1.31 `PAIRING_TRANSPORT` permits platforms only to execute
Shared-provided opaque HTTPS requests and return bounded responses unchanged for Shared verification.
Platforms cannot parse/reassemble proof/delivery or mark transport/device-add/remote mutations complete
from HTTP 2xx, relay burn receipts, or delivery ACKs.

Both devices show the same short verification code derived from `invitation_id`, owner identity,
phone SPKI, and challenge, along with phone name, platform, invitation source, and remaining lifetime.
The user confirms the match and approves on the trusted device. Names are editable display metadata;
identity remains bound to hardware SPKI. Approval synchronously requires system local owner
authentication (Touch ID, Face ID, or equivalent). Shared ABI approval requires a successful platform
authentication callback; it receives no payload, anchor, key, or pairing identity, and biometrics never
enter Muster. Authentication denial/unavailability, SAS mismatch, cancellation, phase expiry, or
exhausted attempts terminate and burn the invitation. QR’s two-minute expiry governs only first
proof redemption. Immediately after success, Shared burns QR/proof inbox and creates a nonserializable
approval handle bound to the exact terminal request/session. Approval has its own five-minute deadline
starting at redemption and cannot reopen proof reception. After owner authentication and signed
acceptance, Shared starts a separate one-minute owner-delivery, consumer-commit, and ACK deadline.
Platforms must not consult QR expiry for Approve/ACK or collapse delivery/ACK/signing/persistence
failures into `invitationExpired`.

A fresh-install candidate becomes an accepted owner anchor only after the phone hardware-signs Rust
consumer-confirmation bytes, both sides confirm the same SAS, the owner locally authenticates and
hardware-signs Rust acceptance bytes, and the phone atomically saves the complete protected chain,
rereads actual bytes, and commits with SHA-256. Persistence/hash mismatch, cancellation, expiry, or
replay never creates trust. After consumer SAS confirmation, Shared creates/verifies an opaque
`FirstPairTransportRequestV2` binding both signed proof and hardware-signed consumer confirmation.
Platforms cannot extract invitation/recipient from QR/URL/display projections/JSON to assemble
receipts or acceptance. ABI 1.32 revalidates the exact locally hardware-signed consumer request in
Rust and upgrades it to a role-scoped, process-local opaque verified handle. This proves request
binding only within the consumer session and cannot be consumed by owner redeem/approve APIs.
Owner accepts only Rust-verified owner-role request handles from relay poll batches, so consumer
self-verification cannot bypass relay’s first-redeem/burn state machine.

ABI 1.32 does not yet provide production-ready delivery completion. Next, a Rust-owned composite
journal must stage exact FirstPair, Grant ledger, and Activation candidate states together before
any trust is runtime-visible, binding delivery/session/request hashes, three protected-state hashes,
a stable ACK idempotency key, `prepared/ack_pending/acked` phases, and crash restoration. Platforms
may only atomically persist Shared’s exact bytes and return readback hashes; they must not supplement
authority context or parse QR/relay bodies/display projections/UI JSON and feed reconstructions back
to Shared. Until this journal and attack tests ship, ABI 1.31 completion is not a safe integration
path for new platforms; device-add and remote mutations remain blocked.

Approval pages explicitly show scope. Recommended default: “current and future devices belonging
to this owner that publish AgentHalo,” granting only `agenthalo.remote-client`; users may instead
select current devices. Terminal, Computer Use, Locked Use, device management, and authorization
management each require explicit scope. None is implied by `agenthalo.remote-client`, and ordinary
Computer Use especially must not grant Locked Use. Owner authorities issue versioned end-to-end
signed cross-device grants binding at least:

- `grant_id`, `owner_id`, subject `device_id` and SPKI;
- Target scope (owner current/future or stable device-ID set);
- Explicit service/capability scopes;
- `issued_at`, optional expiry, monotonic revision, issuer `key_id`, and signature;
- Revocation state or a stable identifier referenced by a later higher-revision revocation statement.

Muster Network owns invitation redemption, proof of possession, grant verification/cache/distribution/
reconciliation, and revocation propagation. Targets project verified grants into their own sealed
authorization state; AgentHalo UI does not edit each local ledger. Offline targets remain “awaiting
authorization” and reconcile on return; one failure does not affect others. If the directory is readable
but grants are not effective yet, mobile may list discovered devices immediately, distinguishing
“awaiting authorization,” “device offline,” “service not started,” “ready,” and terminal authentication
failure. Directory-sync success is not connection success.

Completion reports actual layers, e.g. “3 devices discovered, 2 available, 1 offline awaiting
authorization,” with per-device details and retries for retryable phases. Replay, expiry, code mismatch,
owner/key mismatch, changed subject SPKI, revoked grants, and terminal authentication errors fail
closed. Any trusted Device/Network page may revoke a mobile device, stopping new connections,
closing existing sessions, and revoking directory-read/client grants. The phone’s hardware key may
remain, but reuse requires re-pairing.

Mobile scanning handles camera denial, VoiceOver, dynamic type, and edge-back gestures. No fallback
to short codes, deep links, payload sharing, manual entry, or forced key copying. During redemption/
approval mutations, system Back and left-edge back gestures use the same cancellation/confirmation
protection, never leaving partially completed redemption in the background. Muster Core exposes a
testable state machine:
`identity_ready -> invitation_scanned -> verification_required -> awaiting_owner_approval ->
grant_syncing -> ready/partially_ready/failed`. Platforms implement native UI/secure storage only,
not independent wire formats.

Relay online state is only discovery/UI information, never a replacement for end-to-end verification.
Each device must independently pass mTLS/SPKI verification, Muster tunnel establishment, and AgentHalo
health probe before its service is reachable. One device’s failure cannot block discovery, connection,
or session reads for others.

All sessions and subsequent requests use this routing identity:

```text
(device_id, provider_id, native_session_id)
```

`preview`, `send`, `interrupt`, `approval`, `question`, provider options, and runtime status route back
to the owning device; do not merge by title/ID without ownership. After provider/session switches,
reload model, reasoning, permission/sandbox, workspace permission, context usage, and quota from the
owning device. Never retain Local or previous-device values. Unreachable devices retain groups and
last-known sessions, explicitly offline; one discovery/dial/health failure must not remove them.

Last-known remote sessions are sensitive offline data and require encrypted storage. Apple uses a
SEP-generated non-exportable P-256 key and ECIES authenticated encryption over the whole cache.
Disk stores ciphertext only; Keychain stores SEP key references/metadata, not exportable symmetric
master keys. SEP/Keychain/authentication/decryption unavailability fails closed, with no software-key
or plaintext fallback. Cache only minimal session-list metadata, never transcripts, prompts, or tool output.

New sessions require an explicit target selection first. Desktop Local is a real target, but UI must
not silently fall back to it when none is selected or selection is remote. Mobile has no Local target.

Each remote connection creates only a private local `AgentHalo/run/peers/<device_id>.sock` Unix
socket with `0600`, never localhost TCP. Remote service must explicitly publish
`8765: unix:.../AgentHalo/run/backend.sock`; installing/launching the remote app does not publish it.

## Remote Peer Lifecycle Cross-Platform Contract

Remote-peer connection lifecycle belongs to **Muster Network**, not AgentHalo pages, WorkspaceModel,
or platform ViewModels. This applies to macOS, iOS, Android, HarmonyOS, and Windows. Platforms may
use persistent helpers, native transport libraries, or lifecycle-constrained connectors, but must
not duplicate distinct reconnection algorithms in each product UI.

Muster Network maintains an independent supervisor per peer, responsible for:

- Handling retryable failures from before the first connection, not only after a prior successful connection;
- Capped exponential backoff for relay unreachability, temporarily offline agents, disconnects, and
  transient network changes; baseline `initial=1s`, `max=30s`, `jitter=true`, resetting after a stable session;
- Classifying pin mismatch, revoked identities, and definite protocol/configuration errors as terminal,
  with no automatic retrust or retry;
- Peer isolation: one failure/exit/backoff cannot restart, clear, or block healthy peers;
- Reconciling peer additions/removals/identity changes from the service directory; cancel a removed
  peer’s supervisor, and prevent late old-generation events from overwriting current state;
- Stable private listener/UDS lifecycle: temporary transport failure or an unstarted upper service must
  not cause socket deletion after one failed app probe; keep UDS in a private directory with `0600`;
- Cancelably stopping retries on app shutdown, mobile non-runnable lifecycle phases, or config removal;
  close sessions/listeners/streams and release Process, FileHandle, Task/coroutine resources, leaving
  no orphan connectors or duplicate connections.

Muster publishes ordered structured states with `peer_id` and generation; UI must not parse logs.
Common states include `discovering`, `connecting`, `transport_ready`, `backoff`, `offline`,
`terminal_auth_error`, and `stopped`. `backoff` includes a displayable reason category and next retry
time, never keys, tokens, or private certificate contents. Duplicate events must be consumed idempotently.

Keep `transport_ready` separate from `service_ready`:

- `transport_ready`: Muster has authenticated relay/peer transport and may open allowlisted forward streams;
- `service_ready`: remote AgentHalo backend’s `8765` forward passed application `/healthz`, permitting
  provider/session reads.

AgentHalo subscribes to Muster status, probes `service_ready` after `transport_ready`, updates device
offline/connecting/available state, and refreshes sessions. Late backend startup, one `/healthz` timeout,
or transient session-API failure affects only that peer’s app state. The upper layer must not terminate
its Muster supervisor, delete stable listeners, or implement its own network backoff. Mutations fail
closed until `service_ready`; local capabilities and healthy peers remain available.

Shared Rust/C ABI 1.13 owns lifecycle: per-peer supervisors, generation/sequence, structured events,
automatic initial/runtime reconnection, `1s -> 30s + jitter` backoff, terminal authentication classification,
descriptor reconciliation, cancelable shutdown, and export of ready peers through existing typed
AgentHalo connectors. Platforms implement one real WebSocket, hardware-identity mTLS, SPKI pin,
yamux session, and raw-stream callback, with no retry loop outside callbacks. The old one-shot
60 × 100ms (~6-second) health window permanently misses late-starting backends and cannot define
transport or service readiness.

The macOS reference consumer uses ABI 1.13: thin descriptor reconciliation every 30 seconds and
shared-event consumption by generation/sequence. Its callback uses bundled
`muster connect --supervisor-child` for one authenticated transport attempt, returning failures/
disconnections directly to Rust for classification/retry. Swift no longer restarts at fixed intervals
after CLI exit or kills transport because 8765 `/healthz` is not ready. Independent continuous app
probes update devices/sessions dynamically. Stable typed connectors belong to Rust network; helper
raw UDS is an attempt-private bridge in a private directory with `0600`, cleaned up with its session.
Production service-directory subscription and real multi-device relay E2E still require separate
acceptance. iOS consumes the same ABI 1.13 supervisor/events/connectors with one Native Platform
Layer secure-transport callback; app service probes remain independent. Android and HarmonyOS have
ABI 1.13 bindings but explicitly fail closed for production secure-transport ownership. Windows is
not implemented. Do not report these platform gaps as completion across all platforms.

Acceptance covers initial offline followed by availability beyond the old six-second window, recovery
after an established connection breaks, ready transport with late 8765 service startup, peer-failure
isolation, no retries for terminal auth, no duplicate connectors on repeated reconcile, and no restart
after removal/shutdown. Record code, deterministic tests, signing/installation, actual runtime, UI
state, and real relay E2E separately per platform. Passing one layer does not validate another.

### Current Implementation Gaps (2026-08-29)

- macOS integrates shared ABI 1.14 owner-directory sync, ABI 1.15 federation verifier, and the same
  peer supervisor. Native callbacks briefly borrow `odr_` from Keychain/SEP; last-verified caches use
  Secure Enclave encryption; publishers use shared snapshot-signing bytes and SEP owner keys.
  Swift only consumes verified events, projects `agenthalo:8765`, and performs service probes/UI.
  Release app and embedded components passed ad-hoc signing verification but were not installed or
  launched. Current wiring remains a single `active-owner` pairing/sync and publisher integration;
  multi-authority pairing collection is absent, federation only reads preexisting opaque input, and
  management service is missing. A fresh install without relay activation therefore remains on
  legacy peers, and Add mobile is not production-ready. Manual endpoint/`odp_` Developer forms are
  internal diagnostics only and must be replaced with secure activation handoff, not final product UX.
  Shared `muster-transport` implements domain models for hardware-signed bootstrap drafts and
  crash-replayable two-phase self-publication. ABI 1.16 adds a long-lived orchestrator: only explicit
  bootstrap invokes local authentication; background `start` never prompts. Core generates authority-set/
  operation IDs and canonical bytes, verifies hardware signatures, requires encrypted opaque-state
  persistence, and persists the complete staged envelope before PUT. Initial/runtime offline failures
  use capped backoff, same-revision retries are exact, revision commits only after success, and next
  revisions occur only at expiry windows or local service-descriptor changes. Callback responses have
  explicit release ownership; shutdown cancels in-flight PUT and joins the worker. macOS callbacks are
  wired: bundled `muster local-device` supplies local descriptors; bootstrap local authentication,
  SEP owner signing, independent Secure Enclave sealed state, exact-envelope PUT with briefly borrowed
  Keychain `odp_`, cancellation, and response ownership work. UI no longer duplicates bootstrap prompts.
  ABI 1.17 exports exact federation seeds from Core protected authority state; macOS only persists them
  in an independent Secure Enclave sealed store. Local-only seeds do not incorrectly retire legacy
  peers; takeover requires a verified remote source. Orchestration events are not fully projected into
  Device/Network, remote pairings/grants/operations are not in the same orchestrator management
  transaction, and activation handoff/management service remain absent. Code/unit-test success thus
  does not mean automatic app activation. macOS binds ABI 1.28 QR-only invitations: a prepared owner
  derives a credential-free HTTPS origin from local Muster identity and creates a two-minute single-use
  QR. Regenerate after expiry; old 15-minute transcripts fail closed. The app renders Shared canonical
  QR JSON directly; product entry no longer creates V1 deep links/short codes or calls `/pairings`.
  ABI 1.33 supplies opaque completion plans, exact action/readback, restart restoration using only
  consumer protected state+journal+three-store readback, stable ACK retries, and phase-gated component
  handles. The protected journal retains exact signed requests/deliveries without depending on relay
  replay after terminal burn. Shared validates Activation quorum and the exact paired-owner Grant
  internally from staged FirstPair; platforms neither need nor may first export an anchor to construct
  verified Grant/Activation. Platforms follow journal order; ABI 1.32 legacy completion returns
  unavailable, and platforms cannot merge Keychain/file/database states themselves. Apple’s ABI 1.45
  thin adapter is wired in source: macOS proof-only relay responses show SAS/QR only via Shared
  invitation preview; iOS startup restore never invokes LocalAuthentication, explicit confirmation
  creates one `LAContext`, and typed next steps drive checkpoint, ACK, owner-directory, and independent
  composite Keychain readback. Successful checkpoints, Acked journals, and readback persist long term
  to reconstruct live Shared handles on ordinary restart. Simulator/Swift package tests do not replace
  artifacts, signing, installation, relay deployment, or real multi-device E2E. Until that evidence
  exists, do not claim production acceptance or enable remote mutations. `REMOTE_LISTENER` remains
  unavailable; PairingTransport completion cannot open 8765, service publication, sessions, or remote
  mutations. Local authorities need no self-`odr_`; “relay must issue owner-Mac self-`odr_`” is no longer
  a production prerequisite. Real multi-owner publish/subscription, peer transport, 8765, and session
  E2E remain incomplete;
- iOS removed the compile-time three-device/SPKI table and switched to the shared ABI 1.15 consumer federation manager, supplying only
  `local_consumer_device_id=spki:<sha256>`. Shared Core verifies complete federation input, persisted with
  `WhenUnlockedThisDeviceOnly` Keychain. Native callbacks still briefly borrow `odr_` by `owner_id` only to execute
  Core-specified HTTPS long polling, retaining response memory until `release_fetch_response`. Rust owns the authority set,
  multiple ABI 1.14 child sync owners, source conflict/revocation, peer reconciliation, and lifecycle within one `peer_network`.
  Swift removed per-owner handle orchestration and owner unions; it consumes atomic federation events, projects
  `agenthalo:8765`, constructs connector views, and performs service probes/UI. Platforms cannot safely fabricate migration
  of old ABI 1.14 pairing without signed authority-set operations; these fail closed pending complete signed input delivered
  through authenticated management. Four targeted ABI 1.15 tests and all 93 Simulator tests (three skipped) passed.
  Development-signed build and paired-device installation passed, but iPhone lock blocked launch; no launch/UI evidence
  for the ABI 1.15 package was obtained. Mirroring UI, actual production-relay authority-set/pairing/grant propagation,
  per-peer transport, 8765 health, and session read/write E2E remain incomplete; multi-authority automatic discovery is not production-ready;
  production owner-directory authorization accepts only Security V2 / PairingTransportV2 GrantV2. Legacy Grant V1
  ABI, Relay writes, platform bindings, persisted fields, and consumer-activation fallback were removed. Relay state
  with nonempty legacy grant ledgers fails closed; old mobile storage must be cleared before re-pairing. Production Relay
  configuration/deployment, real authority-set/pairing/grant propagation, multi-device publish/subscription, and E2E
  must finish before automatic discovery can be called available;
- Android binds shared ABI 1.14 owner-directory sync with Keystore-protected opaque caches, cancellable
  HTTPS callbacks, structured state projections, the same repository/peer reconciliation, and foreground/background lifecycle.
  It still fails closed until secure transport ownership connects; real relay/8765/session E2E is unaccepted;
- macOS routes `providerOptions` by owning device and requires explicit reachable-target selection for new sessions;
  actual multi-device E2E must still verify model, reasoning, permission, and workspace permission stay synchronized
  with the original remote session;
- HarmonyOS connects ABI 1.14 resolver, bundle validator, peer lifecycle, and owner-directory sync using
  HUKS callback-only `odr_` borrowing, opaque caches, and the same peer reconciliation. Signed HAP build/install passed,
  but device lock blocked this round's launch acceptance. Secure peer transport still fails closed; actual relay/grants,
  per-device 8765 health, and session read/write remain unaccepted, so this is not multi-device E2E;

### Baseline commit and mac-b migration blocker (2026-08-28)

- Baseline commit `a93df1b` was not pushed and its workspace was clean at that checkpoint. Recheck later workspace state;
  this record is not current live state;
- mac-b migration stopped safely; old `/opt/muster` was still running normally and fully retained;
- macOS denied directory creation at `~/Library/Application Support/AgentHalo-Muster` through SSH, a one-shot LaunchAgent,
  and Finder. Before continuing, allow one directory creation in mac-b's interactive GUI environment, then validate
  and switch atomically. Do not stop/delete old `/opt/muster` before that.

## Device session-list contract

AgentHalo session lists on macOS, iOS, Android, HarmonyOS, and Windows group by device with consistent progressive
presentation and collapse behavior. Screen-size differences must not cause all historical sessions of a device to render at once.

Every Agent Provider first normalizes time to provider-neutral `lastUpdatedAt`: the last actual change in session content,
metadata, or actionable state, not catalog-read, refresh, reconnect, projection-receipt, or UI-render time. Canonical values
are integer UTC Unix epoch microseconds, preserving source and original precision. Offsetless input without an explicit
provider time-zone contract is missing. Sources in order: authoritative provider update, provider event, host-confirmed
mutation/activity, constrained file mtime, then `createdAt` fallback. Never fabricate missing values.

Default order is descending `lastUpdatedAt`, known before missing. Ties use persisted descending `activityOrdinal`,
descending `createdAt`, then ascending canonical UTF-8 bytes of `(device_id, provider_id, native_session_id)`,
preventing jumps on refresh or input permutations. Shared Rust `AgentHaloSessionProjection` implements this comparator
through bindings for every client. It belongs to AgentHalo domain, not Muster network core. Swift/Kotlin/ArkTS/UI
must not independently parse timestamps, guess locales, or duplicate sorting.

- Fixed projection pipeline: normalize/dedup → visibility/search filter → shared sort → stable device grouping →
  per-device limit. Each device initially shows at most its first five sorted visible sessions;
- If more exist, show `Show more` at that group's end. Each click adds five for that device only,
  without expanding others or re-requesting/resetting their page sizes;
- Search, account/configuration, data-source identity, or sort-spec changes reset affected device limits to five.
  Ordinary incremental refresh and same-authority reconnect must not collapse expanded lists;
- Device headers are disclosure controls. Clicking hides that device's provider headings, session rows, and `Show more`;
  another click expands them. Collapse changes presentation only: it does not deselect sessions, close conversations,
  or clear loaded session/page state;
- Collapse state uses stable `device_id`; transient peer outages/refresh must not apply it to another device.
  Remove that device's collapse/pagination state when it is removed;
- Headers show expanded/collapsed indication, total session count, and reachability, with equivalent keyboard,
  screen-reader, and automation semantics. Session-row clicks only select sessions, never toggle device collapse;
- Phones, tablets, narrow columns, and desktop sidebars share this behavior. Layout may vary, but initial five,
  five-per-expansion, and device-header collapse semantics must not diverge.

Live updates use keyed diffs by stable session identity. Sort-key changes may move rows deterministically, but selection/
scroll anchors never bind array indices. New rows entering the prefix preserve the visible limit; never blindly append
new pages to old arrays. Remote `Show more` recomputes `limit + 5` against `collection_revision`; changed revisions
return new projections rather than old-offset duplication/omission. Reconnect re-enumeration reuses same-device pagination/
collapse state without writing reconnect time into `lastUpdatedAt`.

Terminal permits versioned optional sort specs (creation time, title, host/device, activity); every sort finally falls back
to the default comparator, with unknown values returning to recency. Store Terminal preferences in nonsensitive native client
preferences scoped by `owner_profile_id + client_installation_id + terminal_workspace`. Retain across App restart,
without default cross-device Muster sync. New installs, Owner switches, and corrupt values return to recency. Preferences
change Terminal projection queries only, not the no-override default for all Providers.

Acceptance covers at least: more than five sessions on one device, independent `Show more` across devices, page-size retention
through collapse/expand, collapsing the selected session's device without closing its conversation, search reset, stable device
identity through offline refresh, removed-device cleanup, and touch/mouse/keyboard/accessibility entry points. Unit/property/
cross-platform golden tests additionally verify comparator total order, permutation invariance, nil/precision/time zones,
and duplicate/omission-free device pagination. Live checks cover equal timestamps, missing-to-known time, clock rollback,
new-session insertion, sort changes, reconnect, and revision changes. Record code tests, installed execution, and actual UI acceptance separately.

### Session and conversation performance contract

macOS, iOS, Android, HarmonyOS, and Windows must separate owner-directory, workspace, session-catalog, and selected-conversation
refresh into stable projections. High-frequency preview/stream events must not synchronously rebuild every device, session,
or historical message. Native frameworks may differ but share these constraints:

- Device/provider/session/turn/message/interaction rows use protocol-stable identities, never array indices, current order,
  or refresh-batch-generated IDs;
- Ordinary directory/workspace refresh replaces only changed devices/session rows, preserving unchanged devices' pagination,
  collapse, selection, and scrolling. Do not repeatedly compute sort/search normalization/grouping in one render/body/composition;
- Long lists/conversations use native virtualization. Offscreen rows must not keep parsing Markdown, formatting dates,
  probing attachments, animating, or rebuilding accessibility trees;
- Cache/incrementally update conversations by stable turn/message revision. Streaming usually recomputes only the final turn;
  unchanged historical turns reuse projections and accessibility identity;
- Completed-message Markdown/block/AttributedString parsing uses bounded caches. Streaming text follows a fast path that
  avoids full Markdown parsing per token, switching to final rich text on completion;
- Reuse time parsers/date formatters/other expensive formatters, never recreate them in row getters or render hot paths;
- Follow-latest/jump-to-bottom has one cancellable coalescible pending task. New previews replace it, not accumulate delayed
  `scrollTo` calls. Stop automatic follow when users leave the bottom; keep explicit jump available;
- Transport `not_modified` emits no equivalent UI state. Apply suffix/delta only for exact owning route/base revision;
  otherwise preserve transcript and request compatible snapshots. Never concatenate across devices/sessions;
- Backgrounding, session switching, page exit, and shutdown cancel preview, formatting/parsing, follow-scroll, and health
  tasks; restoration must not create duplicate watchers, pollers, or connectors.

Deterministic tests measure at least: 30 consecutive tail-stream updates with over 40 historical turns recompute only the tail;
unchanged row identities persist; Markdown caches hit and remain bounded; repeated follow requests execute once; search/ordinary
refresh preserves other-device pagination; wrong base revisions cannot pollute transcripts. Performance unit tests, simulator/device
traces, signed installation, and real-Provider long-conversation acceptance remain separate. “Tests did not time out” is not rendering/main-thread evidence.

## Mobile back-navigation contract

All non-root AgentHalo pages on iOS, Android, and HarmonyOS support a rightward swipe from the left screen edge to the prior
navigation-stack page. Gesture, system Back, and navigation-bar Back use the same navigation action, not three divergent histories
or a recreated home page masquerading as Back. macOS does not implement this mobile edge gesture.

- At a root without a predecessor, edge swipes must not accidentally exit App, clear workspace, or duplicate roots;
- Back restores previous device/session selection, scroll position, and loaded state. Unsaved input or active mutations
  use the same confirmation/back-blocking rules; gestures cannot bypass protections;
- iOS prefers system interactive pop; Android aligns with system Back/predictive-back lifecycle and explicitly supports
  the left-edge gesture; HarmonyOS aligns Navigation/NavPathStack and system Back lifecycle;
- Horizontal scrollers, attachments, and draggable controls must not consume the left-edge Back region. Only controls
  explicitly requiring continuous horizontal operations may compete within their hit area, retaining a discoverable Back button;
- Back gestures share button accessibility semantics and follow platform navigation direction on phones/tablets,
  portrait/landscape, and RTL/LTR languages.

Each mobile platform verifies consecutive edge-back through at least two levels, root boundaries, prior-page state retention,
horizontal-scroll conflicts, unsaved-composer/active-mutation protection, and equivalent system/button/gesture results.
Code, simulator tests, signed installation, physical gestures, and full business E2E remain separate and non-substitutable.

The first Codex Provider Adapter references old `remote-coding` without copying its Go server. Rust directly scans bounded
`rollout-*.jsonl` under `sessions` and `archived_sessions` in `CODEX_HOME` (default `~/.codex`), merging user titles from
`session_index.jsonl` while keeping UUIDs opaque native identities. Scans do not follow symlinks and bound file count,
recursion depth, and JSONL-record size. Subagent rollouts stay out of user lists but remain previewable by exact UUID.
Previews read local rollout only, parsing user/assistant text, tool calls/results, and public reasoning events without
launching app-server or taking Codex Desktop ownership. Read-only capabilities explicitly declare
`sessions/preview=true`, `create_session/send/interrupt=false`.

Provider previews also return optional `runtime`: sandbox mode, model, reasoning effort, current context tokens/window,
and actual quota windows. Codex reads `turn_context`/latest `token_count`; Claude reads transcript/native model/usage events
and its separate native permission mode, never treating `permissionMode` as verified OS sandboxing. Apple composers render
only actual fields, labeling 300/10080-minute windows 5h/Weekly without inventing missing quotas. Codex sandbox/model/reasoning
are also compact pickers: models/supported efforts come from current app-server `model/list`, sandbox from current schema.
New sessions pass model/sandbox to `thread/start`; every turn passes all three as sticky overrides to `turn/start`.
Providers without parameter execution, including Claude, show actual values without fake editability. Even basic Claude
text control neither ignores nor accepts unimplemented parameter overrides.

The second slice implements a native Rust Codex app-server JSONL RPC client, permitting create/send/interrupt only for
AgentHalo-owned threads. It launches `codex app-server --stdio` without a shell, negotiates `initialize/initialized`, and
implements bounded `thread/list`, `thread/start`, `turn/start`, and `turn/interrupt`. It tracks authoritative thread status/
turn ID, rejects second sends on active turns before writing, and fails interrupt closed without a known turn ID. Command execution,
file changes, and structured-user-input requests enter bounded pending inboxes and respond using exact request/thread identities.
Additional permissions, MCP elicitation, and unknown requests receive explicit protocol errors, never automatic approval.
Both `thread/start` and `turn/start` explicitly set `approvalsReviewer=user`, preventing host-global `auto_review` from approving
outside AgentHalo UI. Typed clients preserve provider `available_decisions`, so SwiftUI shows only supported choices.
`--diagnose-codex-app-server` performs only initialize/thread-list for local diagnostics without creating threads or prompts.

Controls now use Rust's typed `ProviderControlOperation`. For resume, approval, structured answers, and interrupts with operation IDs,
Codex persists exact request fingerprints before dispatch and original results after success. Remote IDs are namespaced by authenticated
device; local operations retain an independent UDS namespace. Control bodies are not rewritten as JSON. Completed identical IDs return
original results without redispatch; failures not provably undelivered remain unknown and never replay automatically.
Local/remote status queries use exact durable receipts to repair existing outer pending/unknown records, still checking original
request, operation kind, namespace, and authorization scope. Missing records, old bare IDs, and mismatched evidence cannot recover.
Receipts neither restore old-generation ownership nor prove post-approval tool completion. Missing Provider replies, unpersisted
results, and legacy receipt-less operations still need recovery mechanisms. These are source/software contracts, not real bilateral
signed-install acceptance; see the [complete gap checklist](../testing/agenthalo-remote-completion-checklist.md).

The shared backend operation log now confirms retained append candidates, avoiding duplicate transitions after a write succeeds
but sync/readback fails. Records are bounded before encoding, then retain exact candidates and committed lengths. Recovery checks
new file bytes are an exact candidate prefix, fills only the missing suffix, and updates the in-memory index only after disk sync
and exact non-cursor-moving readback. Writes confirm prior candidates before evaluating transitions; native workers may persist
known original results only, without recalling Provider. Read-only status/receipt queries neither repair appends nor report
unconfirmed candidates complete. Length/prefix conflicts, extra bytes, and contradictory outcomes preserve evidence and block;
never truncate/delete logs for recovery.

Encoding and reading enforce 4 MiB per record before copying. Appends obey the loader's 100,000-record capacity, rejecting
new records at capacity rather than creating unreloadable history. Restart accepts only complete lines, valid original transitions,
and confirmed sync. If a candidate died with the process leaving a partial tail, opening still fails; partial JSON cannot reconstruct
results. Software fault injection validates this common-log fix, not hardware power loss, historical compaction, corrupt-log repair,
or Claude durable-receipt wiring. Protected-state logs and native Owner registries are unchanged.

Provider CLI discovery does not depend on Finder's incomplete `PATH`. Search explicit `AGENTHALO_CODEX_CLI` /
`AGENTHALO_CLAUDE_CLI`, current `PATH`, known embedded Desktop CLIs, then Homebrew/common user directories;
accept only absolute regular executable files. Codex supports `ChatGPT.app/Contents/Resources/codex` and the corresponding
standalone `Codex.app` path. Claude checks only explicitly named `claude-code`/`claude` resources under
`Claude.app/Contents/Resources`, never mistaking GUI `Contents/MacOS/Claude` for Claude Code CLI. Log the selected
absolute CLI path at backend startup to distinguish Desktop bundles from Homebrew fallbacks.

Claude Provider directly scans each project's top-level UUID JSONL under `CLAUDE_CONFIG_DIR/projects` (default
`~/.claude/projects`). Duplicate session IDs retain the newest file; sidechains stay out of lists. Titles fall back through
custom title, AI title, summary, then first user text. Preview parses user/assistant text, thinking, tool use/results,
correlating by tool ID. Claude Desktop's presence does not imply controllable CLI, so this baseline declares only
`sessions/native_sessions/preview=true`, with create/send/interrupt/approval false.

Claude write integration begins with Rust bidirectional `stream-json` transport, using frame/request correlation from the
[official Anthropic SDK control protocol](https://raw.githubusercontent.com/anthropics/claude-agent-sdk-python/main/src/claude_agent_sdk/_internal/query.py).
`ClaudeSdkClient` owns a separate child process and correlates responses with a process-random prefix plus increasing IDs.
Tool requests use single-use opaque handles bound to their original process; cross-process, cancelled, or consumed handles cannot respond.
Provably undelivered writes, attempted writes of unknown outcome, and definitive responses produce distinct results.
Only definite zero-write preserves tool requests for explicit retry; unknown writes never replay automatically. Pipe writes are
nonblocking/deadline-bounded, with bounded queues, frames, and history. Corrupt frames, duplicate JSON keys, or capacity overflow
close only this process, never scan/kill users' existing Claude processes. Error/debug formatting does not echo raw events/CLI errors.

New `--diagnose-claude-sdk` performs a prompt-free handshake only, using `--bare`, `--safe-mode`, disabled tools, empty
`mcpServers`, no user/project settings, and no session persistence. It does not run automatically at ordinary backend startup
or read/write AgentHalo pairing/operation logs. Output reports protocol initialization and declared model count only, explicitly
`session_control_ready=false`, `prompt_sent=false`. Real local Claude Code 2.1.263 handshake passed, without establishing
account/model calls, session control, tool execution, or mobile availability. The first attempt lacked `mcpServers` in empty MCP
configuration and was rejected; the correction passed. Mock tests cannot replace this check.

At this transport-foundation checkpoint, Claude Provider ownership, creation, sending, approvals, structured answers, interrupts,
durable receipts/recovery, and bilateral UI remain unwired; read-only capability declarations remain unchanged. Tool disabling and
no persistence restrict diagnostic processes, not full product control, and do not close non-Codex feature gaps.

Claude transport adds opaque original-input handles and native acknowledgment credentials: prepare exact UUID, session ID,
body digest, and send bytes before explicit dispatch. Unattempted input can retry; attempted unknown input queries original evidence
without resending. SDK handshake, pipe writes, or ordinary user events cannot mint acknowledgments. Require matching native
`system/init` and `isReplay=true` user acknowledgment, checking message UUID, session ID, role, root-message ownership, and exact text.
String and single-text-block lossless representations may correspond to the same body.

Final results separately correlate by `user_message_uuid`; if `user_message_uuids` is also present, it must contain only that
same original UUID. Old input results cannot finish new input. Reject missing original UUIDs, mixed batch UUIDs, conflicting fields,
and contradictory final outcomes for one input. `subtype=success` does not necessarily mean execution succeeded: actual Claude Code
2.1.263 API errors also contain `is_error=true`, which must return explicit failure. Input acknowledgment proves only native acceptance;
each final result does not prove all background tasks completed. Neither establishes current ownership, replaces durable logs,
or can be reconstructed from serialized data.

Real CLI regressions use an isolated local fake API, fake test credentials, temporary configuration, disabled tools, and no persistence,
actually exercising Rust preparation/send, native acknowledgment, failure results, and non-replaying queries without real models.
Tests require explicit installed executable selection via `AGENTHALO_TEST_CLAUDE_CLI`, skip separately by default, and must be explicitly
run for release evidence. Each process retains at most 128 input identities; text is at most 64 KiB. Capacity rejects new input rather
than forgetting unknown evidence. At this native baseline, original-session create/resume, production launch policy, tool approvals,
cross-restart persistence/recovery, and both UIs remain incomplete. Current production wiring follows; limited capability exposure
must not be read as complete Claude functionality.

#### Claude production logical sessions and first-input lifecycle

Production backend startup opens exclusive Claude creation logs, input/control logs, and ownership registries without launching CLI,
querying accounts, or invoking models. Configuration failure preserves read-only discovery. Current Provider integrates logical
creation, default plain-text sends, exact-receipt recovery, in-process session lists, and native-event previews. Tool approval/answer
receipts and separate interrupts of acknowledged original input follow below. Explicit resume preparation for sessions with native
transcripts and next-input acknowledgment are connected; empty logical-session restoration, attachments, and execution parameters
remain incomplete. Unsupported parameters cannot be ignored.

Creation means Rust owns a durably confirmed new-session launch intent with a separate UUID, empty native ID,
`native_confirmed=false`, and `prepared` state. It sends no artificial prompt, starts no native process, and registers no native ownership.
Failed creation appends retain the same UUID/candidate receipt; queries or explicit original-request retries can only finish that
candidate. Restart after losing it cannot allocate a replacement UUID. Unconfirmed creation cannot receive first input.

Only the first explicit send launches and reuses the associated CLI process. Return `accepted` only after exact native input
acknowledgment is persisted, then save ownership using the current process's native identity. Later inputs require ownership persistence
for existing native identity; failures recover the original candidate. A closed process never restarts automatically on another send.
Before creating native processes, check pending-session isolation retained from input/control logs at startup; blocked requests must
not launch CLI first. Old durable creation/send receipts return historical outcomes only, without rebuilding current process entries
or control rights. Native/empty logical resume boundaries follow; confirmed-creation durable directories are display-only,
and normal listing never resumes sessions automatically.

Retain at most 32 logical sessions without deleting records at capacity. One starting/failed session must not hide others.
Native state comes from input trackers/identities, not platform inference. Previews reuse the existing parser; native-event components
mark missing history with `history_gap`. Existing bilateral formats do not carry it, so production Provider rereads real persisted native
transcripts on gaps, explicitly failing if unavailable rather than silently truncating. Real CLI tests recover first-turn input from disk
after eviction by a 160-chunk long reply. Continuous chunk assembly, concurrent disk updates, and full-history pagination remain pending.
Controller shutdown waits in parallel for each owned process within five seconds, using existing forced-close fallback only on its own
processes; unconfirmed shutdown records errors, never pretends normal persistence.

`ClaudePromptOperations` connects exact native input acknowledgment to an exclusive operation log. It neither starts CLI,
creates/resumes sessions, nor grants control. Callers establish authorization/ownership first and independently own native process
lifecycle. It currently accepts plain-text sends matching exact original request bytes only; attachments and model/approval/sandbox/
reasoning overrides explicitly fail rather than being ignored.

Persist intent before calling original-input handles; return `status=accepted` only after native acknowledgment persistence.
This means input accepted, not successful model execution. Original calls may also obtain native input credentials, but disk caches,
completed receipts after restart, and another process's recovery provide historical responses only, never native credentials/control.
The same process retains original handles after persistence failure/timeout. Explicit recovery saves only the original acknowledgment
without native send calls. Ordinary durable-receipt queries are read-only, neither repairing disk nor retiring in-memory evidence.
Explicit recovery of records confirmed by another write path releases their completed retained entries, avoiding stale pending blockers.

Retrying the original UUID or explicit cancellation requires the original worker to prove native send was never called or SDK to
confirm no write attempt. Cancellation persists; unknown, absent, or accepted input cannot masquerade as undelivered. A pending item
blocks other operation IDs for its session, even with another native process. Opening logs with pending/unknown records whose handles
are gone blocks new sends by durably bound session, without guessing retries. New Claude input uses version 2 operation records:
extract Provider and normalized UUID from exact requests, persisting ownership/fingerprint but not prompts. Ownership/candidate
confirm in one append/sync/readback; later transitions retain the same ownership. Missing ownership, changed sessions on transition,
unknown Providers, or wrong versions reject loading. Ordinary unscoped records remain version 1; old pending records block file-wide
without inventing ownership from current requests. Proven-undelivered legacy retries keep original format/history. Old readers reject
version 2 rather than ignoring new fields and appending incompatible records. Shared ABI is unchanged.

Restart isolation snapshots restrict pending input only and grant no control. Scoped pending input no longer blocks other sessions,
but its original session cannot bypass through changed operation IDs, native processes, or UUID case. In-process retained entries
use the same UUID isolation. Historical success/explicit cancellation do not block new input; normalized ownership does not relax
exact-byte checks for existing IDs. Unconfirmed appends make read-only isolation queries fail, without implicit repair/ignored candidates.
This does not restore native evidence across restart: original pending input still needs native proof. Production logs are fixed,
with isolation checked before process creation; changing files or launching CLI first cannot bypass it.

This component is connected to controlled sends/receipt queries in production `ClaudeProvider`. Native control still needs independent
current-process identity; cached responses do not restore rights. Isolated real CLI 2.1.263 regressions cover native acknowledgment,
actual disk persistence, model API failure, original-result reads after restart, and no resend, using fake API/test credentials,
not real models or bilateral acceptance.

Approvals, structured answers, and interrupts use version 3 control records, extracting Claude session scope from exact typed bodies.
Operation IDs still derive from authenticated routes; scope normalization does not alter fingerprints. Intent/scope persist in one
append and remain unchanged through candidate confirmation/transitions. Do not store questions, answers, or tool arguments.
Version 3 accepts only those three controls; version 2 only input. Version mismatch, missing/replaced scopes, invalid sessions,
or unknown fields reject loading without changing the original file.

Controllers retain pending-scope snapshots when opening logs and check before native-process launch/new control preparation.
Unknown new-format controls block only their session; old unscoped pending controls block the whole log. Later requests cannot
invent scope or delete old evidence. Completed controls, including native rejection, do not block; exact old receipts remain readable
without current ownership. Snapshots restrict, never authorize recovery or restore missing native proofs. Confirmed logical creations
now project from exclusive creation logs into durable historical directories, preserving original UUID/title/canonical working directory
after restart even if the directory no longer exists. Historical rows have `requires_restore`, `live=false`, empty native IDs,
and create no launch intents, ownership, or current-generation rights. Reads accept only valid completed creations, rejecting duplicate
logical identities, invalid metadata, and non-creation records. Pending appends fail the read rather than confirming writes through
display or serving old caches as current. Display caches version by confirmed-record count and never write disk.

Historical directories use existing production `/native_sessions` pagination/Shared client merging, preferring real native transcripts.
Archive indexes and hidden-transcript identifiers also exclude rows, preventing logical-history resurrection after restart. Derived
history is limited to 20,000 rows/16 MiB and merged catalogs to 20,000 rows; overflow fails explicitly rather than truncating.
Preview without native transcripts explicitly requires restoration, never fabricates empty content. Historical working directories
confer no file/Git rights. Current-process controlled lists remain separate. File/Git reads use separate `workspace_session_catalog`;
Claude builds it only from actual native discovery. Other Providers retain existing reads, without inferring new eligibility from historical rows.

Explicit restoration of existing real native transcripts is wired to production `/resume_native_session`, requiring an operation ID,
exact typed body, original UUID, nonarchived discovery, and valid working directory. Never use latest-session, forks, or new UUIDs.
Exclusive `resumptions.jsonl` persists original preparation results; `state=prepared`, `native_confirmed=false` means the request
is saved, not native identity confirmed. This step starts no CLI, sends no artificial prompt, and registers no native ownership.
Only subsequent explicit real input launches `--resume=<original UUID>`; matching native identity/input acknowledgment must persist
before current-generation ownership. Restored previews read the full native transcript, not just this process's new events.

Resume-intent scope derives from original typed requests using version 4, preserving old input/decision formats. Failed candidate
appends retain preparation entries without enabling send; original-receipt queries or explicit original requests can complete the exact
candidate, never allocate another session. After restart, reading successful old receipts does not rebuild entries; new explicit resume
is required. Pending resume blocks its session only; missing candidates, existing preparations, or old pending inputs/controls cannot
be bypassed through new operation IDs. Creation/send/resume share the per-process 32-session limit, without replacing closed entries
or erasing pending evidence.

Backend explicitly declares resume capability. HarmonyOS projections no longer hardcode Codex-only support, but still require
authenticated reads, current Grant, and actual Provider capability. Shared mutation preflight/backend atomic checks remain; UI declarations
confer no rights. Both interfaces explain saved original requests versus native confirmation, never inferring current control from receipts.
Empty logical-session restoration, re-restoring closed same-process entries, complete bilateral execution-setting mapping, full history
lifecycle, and old pending-evidence migration remain incomplete. ABI is unchanged; native-library rebuild/signed-install acceptance remain pending.

Native Claude launch adds immutable `ClaudeSdkSessionLaunch`: new sessions preallocate exact UUIDs; resumes specify original UUIDs
and canonical directories, never recent sessions or implicit forks. `spawn_session` only starts an independent subprocess and performs
SDK handshake. Before first input, actual CLI returns `session_state=idle` without native session initialization; this is not business
creation success or ownership.

Only original-process `system/init` matching launch session/directory exactly yields opaque `ClaudeSdkSessionIdentity`. It binds the
original process generation; stopped/closing states or another same-UUID process cannot use old identity. Native identity is not business
ownership, input acknowledgment, or current tool authorization, and cannot be recreated from disk/display JSON. Launch errors retain only
the first static content-free diagnostic and wake initialization waiters, without exposing raw CLI errors, prompts, or account data.

Launch explicitly selects native permission mode, defaulting to no permission bypass. Arguments disable implicit settings, hooks,
plugins, MCP, and other customization; CLI still enforces administrator-managed policy. This is not OS sandboxing or general mapping
of approval/sandbox parameters to native policy. Production does not use `--bare` or disable persistence. Callers still establish
business authorization, ownership, and execution policy. Isolated tests additionally use `--bare`, disabled tools, and fake credentials,
which do not establish actual account/tool/policy acceptance.

`shutdown` is an explicit App lifecycle action: stop accepting writes, send input EOF to originally launched children, retain output
reading, wait at most five seconds, then terminate/reap only module-owned children as needed. Output ending during closure must not
prematurely terminate native saving; forced `close` may abort waiting. Never find/kill other CLIs. One turn result does not automatically
close the process because native background tasks may continue. Normal exit is not proof all effects/business records persisted;
unknown input must still never replay.

Isolated actual-CLI tests found immediate forced termination after a fixed success reply left only a startup marker, without user/assistant
transcripts, and original-UUID resume then failed. Waiting for normal exit and verifying saved transcripts allowed exact new-process resume
and next-input acceptance; old identity/input credentials still cannot cross processes. Replies come from local fake API streams, not real
model success. At that checkpoint business creation/resume logs, production Provider, runtime projections, tool controls, and bilateral UI
remain unwired; continued-event reception follows, with the checkpoint's read-only Provider declarations unchanged.

Bound native sessions now enable `--include-partial-messages` and no longer send display events to the diagnostic 16-entry queue.
Readers first enforce exact session/input/result checks, then retain bounded display history: at most 128 items and 8 MiB raw frames.
`session_messages` reads at most 64 at once; cursors are non-reconstructible and process-bound, and repeat reads are side-effect-free.
Eviction explicitly returns `history_gap`, never full-transcript claims; production wiring must refresh transcripts. Retained content
after closure is stale display only, with no current rights.

`pending_session_controls` retrieves actual unanswered handles from the original request table, independently of display pagination.
Slow UI/eviction must not lose approvals, input acknowledgments, or final results. Multiple readers share original request state;
responses still check original process, cancellation, and consumption. String IDs create no credentials. Unknown request types remain
pending without automatic approval; existing 32 pending-request/4096 seen-ID limits remain, with no automatic old-request deletion.
This native layer does not execute business approval policy or directly map raw events to authorized business state.

Normal shutdown also confirms reader arrival at actual EOF within the same deadline. Previously children could exit while readers
lagged behind pipe output, and immediate closure truncated data. Regressions block display reads while actual subprocesses output/exit,
reproducing/fixing the race. Unconfirmed output drain returns explicit errors and retains the first static diagnostic. Normal exit codes
still do not prove durable business receipts/effect completion. Real isolated CLI tests stream 160 fake-API chunks without UI reads,
verifying gaps, final results, native persistence, and exact new-process resume; no real models/tools or bilateral UI acceptance.

Rust `ClaudeSdkInterrupt` binds native interrupts to current native identity and original input acknowledgment.
The [official SDK interrupt request](https://github.com/anthropics/claude-agent-sdk-python/blob/main/src/claude_agent_sdk/_internal/query.py)
contains no input ID, so before dispatch, recheck the input is still active under its lock and register the interrupt. Do not dispatch
next input before native reply. Input completion, timeout, or dropped handles do not unblock it. Reopening pending interrupts returns
the same request and queries exact late replies without replacement interrupts. Explicit retry of the same handle requires proof of zero
bytes written and a fresh active-input check. Preserve native success/rejection by exact control ID; success acknowledges only the native
command, not stopped tools, finished input, or durable business receipts. Cross-process handles are invalid; original handles after closure
are query-only. Production ownership/control logs/exact receipts connect below; interrupts before input acknowledgment and cross-restart
pending-native-evidence recovery are unsupported. Isolated actual CLI tests verify pending Write cancellation, stale approval invalidation,
no file creation, and next-turn completion in the same process, using temporary directories/fake API only, not real models or bilateral acceptance.

Rust-owned `ClaudeSdkToolInteraction` can be created only from current `ClaudeSdkSessionIdentity` and original pending control requests.
Parsing accepts representable `can_use_tool` only. Unknown/corrupt requests retain pending evidence without fabricated replacement approvals.
Display projections do not authorize; replies check original process, request-table membership, cancellation/consumption, and original input.
Copied IDs grant no control. Allow once returns exact original input; deny and deny-and-stop map to native responses. No permanent-permission
suggestions or `updatedPermissions` writes. Successful response methods confirm transport writes only, not native execution or durable
receipts; production Provider uses the separate original-operation-log/result-confirmation layer below.

`AskUserQuestion` associates answers with original question text, bounds question/option/length counts, and checks completeness,
uniqueness, single/multiple selection, and free text. Reject prefilled answers, extra/missing/duplicate answers, and unknown options.
Only validated answers are added to original input, leaving other fields unchanged. Native HTML previews never enter executable rendering;
platforms cannot infer authorization from display fields. This component connects to production Provider through the decision layer below.
Generic bilateral forms support these inputs, but real Claude operations after matching signed installations remain unaccepted.

#### Claude production tool decisions and receipts

`ClaudeControl` obtains pending requests only from native sessions with confirmed ownership in the current process. Allow once,
deny, cancel, and structured answers require operation IDs and exact bodies matching typed requests; permanent approval is unsupported.
Answers require structured arrays; any accompanying legacy answer map must agree and cannot overwrite original questions. Requests
without native `tool_use_id` display nonactionable explanations, without inferred substitute call identities.

Rust `ClaudeSdkToolResponse` retains original process, control request, session, and tool-call identity. Preparation does not write;
native dispatch occurs at most once, with explicit same-handle retry only after proven zero write. Send state/result witnesses share
a lock so zero-write cleanup and native observation cannot interleave into stale evidence. The lock does not span pipe writes, allowing
readers to receive results arriving during writes. `controls.jsonl` records the original operation before dispatch. Successful writing
is not business completion. Independent of display queues, witnesses receive `tool_result` for exact native session/tool call or exact
original-request cancellation. Return `native_response_observed` only after original-result durable confirmation, distinguishing tool
errors from request cancellation. This does not prove the decision caused that result, model success, or all effects/background tasks completed.

Timeouts, unknown writes, and result-sync failures retain original evidence/candidate. Read-only queries can complete original receipts
without resending. Concurrent submissions, changed bodies, and new operation IDs cannot replace original requests. Completed receipts
survive restart but restore no current native identity; pending operations without original witnesses cannot replay. Production control
records and response witnesses per native process each cap at 128, with no automatic reclamation yet. Display caps at 256 items/4 MiB;
overflow errors rather than trimming tool arguments and still allowing approval. Long-term capacity/full pagination remain pending.

Real isolated CLI regressions submit fixed Write allow/deny/cancel and question single/multiple/free-text/cancel through production HTTP,
checking native outcomes and original receipts after restart without replay. They use local fake API, temporary settings, and OS-constrained
test files only. Existing-transcript resume wiring is described above; empty logical resume, execution parameters, attachments,
cross-restart pending evidence, and full bilateral acceptance remain incomplete. This section changes no ABI, platform code, or Relay.

Separate `/interrupt` controls reuse the same production logs/receipt recovery. Rust `prepare_active_interrupt` captures acknowledged
input or the original input bound to pending interrupts from native trackers, then rechecks under the original input lock. Turn changes
between reads reject rather than selecting the next turn. `NativeResponse` retains either tool-response or interrupt handles.
Persisted original-command success/rejection returns `native_interrupt_acknowledged` / `native_interrupt_rejected`; existing bilateral
failure checks handle rejection `ok=false`. Command acknowledgment still does not prove original input finished or all tool effects completed.

Original operation ID/body bind to the first captured input. Timeout, log-sync failure, or spontaneous original-input completion
cannot redirect retries to a newer input. Pending interrupts block next sends until exact native replies; read-only receipt queries
send no commands. Completed receipts remain readable after restart without control authority. Currently the same original input cannot
be interrupted again with another operation ID, including after native rejection; explicit retry semantics/capacity governance remain pending.
Production HTTP actual-CLI isolation tests cover interrupt, tool cancellation, next-turn continuation, and no restart replay.
No real accounts/models, signed installation, or joint Mac/HarmonyOS acceptance were performed.

Shared inbox/mobile projections preserve `multiSelect`. Legacy projections missing it remain single-select-compatible; wrong types
are not silently converted. Question keys may be original text up to 4096 bytes, including newlines, not short-route-ID display limits.
Mobile projections tolerate old absent titles/descriptions, but any invalid selection flag/question structure leaves the whole group
nonactionable rather than dropping one question and permitting submission.

Mac/HarmonyOS forms retain original question keys, arrays of selected options, and separate free text, without converting free text into
options or flattening multiple choices. Switching single/free input and checking/unchecking multiple options only collect user input;
Shared/Provider original-request checks decide final validity. Mac rebuilds forms by full card identity; HarmonyOS drafts isolate by
Owner UI generation, device, Provider, session, request, and complete question definition. Late results clean only their original submission.
These UI keys are not business identities/authorization handles and cannot bypass authenticated routes, pending operations, or durable receipts.

Real fixed-tool CLI tests cannot use `--bare`, which disables tools. A separate macOS fixture uses `sandbox-exec` to deny user-directory
reads, Mach-service queries, external networking, and writes outside test directories, retaining only local fake API, temporary settings,
and fixed Write/question tools. It accesses no real accounts/models. Fake API matches tool replies by fixed user prompts, never assumes
the first initialization request is a user message. Tests separately verify no file before approval, exact content after approval, native
tool errors after denial, and actual single/multiple/free-text answers. Final success text alone is not tool-execution evidence.
This test boundary supplies no product OS sandbox and does not replace installed Provider/hardware/network/bilateral UI acceptance.

Codex/Claude filter archived sessions before generating `native_sessions`, applying list limits afterward. Cross-platform Rust clients
and SwiftUI retain defensive filtering for old backends. Archive data is not deleted, and hiding lists does not change exact-ID preview access.

HTTP mutations are protected by private `0600` append-only operation ledgers and owner registries. Ledgers persist SHA-256 request
fingerprints only, never prompts. Before Provider writes call `sync_data(pending)`, then `sync_data(completed)` on success. Completed
same-ID operations return cached results; different fingerprints return 409. `pending/unknown` remains ambiguous across backend restarts,
without execution again; only app-server proof of non-delivery permits matching retries. `X-AgentHalo-Operation-ID`, when present,
must match JSON body.

Owner registries generate a fresh generation each backend/app-server startup. Send/interrupt requires current-generation `thread/start`
or explicit native-identity-verified `thread/resume` plus successfully persisted ownership. Rollout-discovered Desktop/CLI/Codex Desktop
threads never implicitly switch writers. Prior-generation ownership cannot restore writes after restart; explicit native resume is still
required. AgentHalo creation title/cwd persists as bounded display-only owner-record metadata, keeping user titles after restart without
restoring ownership/writes. Capabilities open only after private state and app-server both start successfully; otherwise retain read-only
rollout with a degradation reason. Approval capability opens only for the available private control generation.

The registry holds a nonblocking exclusive write lock. Install release guards only after acquisition; destruction releases it without
waiting for nonauthoritative duplicate descriptors. Each claim retains the full append candidate, checks actual length/exact prefix,
fills missing suffix only, and registers current-generation ownership after sync/exact readback. Failed candidates are never replaced.
Ordinary metadata/current-ownership/session-list queries remain read-only; complete candidate bytes on disk do not grant ownership early.
Writes may confirm original candidates but cannot replay native creation/resume; this registry is not a business-operation completion receipt.

Production Codex explicit resume first calls `confirm_retained_claim`. If native resume succeeded in this process but ownership saving
failed, only identical session/title/directory may confirm the candidate and continue, without another `thread/resume`. Identical confirmed
current-process ownership reuses directly. Other-generation metadata or complete-but-unconfirmed old records after restart cannot take this
shortcut; native protocol must complete again. Changed candidate parameters cause neither disk writes nor native resume. Validation uses
production CodexControl, actual sockets, and a software app-server peer.

Loading reads bounded record boundaries and rejects complete JSON without terminating newline, oversized frames, unknown fields,
and same-generation metadata forks. Legacy metadata-less formats and identical historical duplicates remain compatible; reads do not clean up.
Writes also enforce 100,000-record capacity. Partial tails after candidate loss preserve evidence and block; long-term governance and installed
power-loss recovery remain incomplete. File locks coordinate only cooperating writers, not independent hardware rollback resistance,
and do not change protected pairing state.

macOS Workspace selects newly created sessions directly. Independent bounded watchers follow preview for every selected Codex/Claude
session. After send returns `active`, refresh live session state separately until `idle` or timeout. Conditional `sig/if_sig` previews
avoid re-decoding/replacing unchanged transcripts. Active turns expose Interrupt in the top bar; authoritative `turn/interrupt` is followed
by polling to idle. Rollout previews hide app-server-injected developer/user bootstrap before the first `turn_context` (e.g. AGENTS/environment),
avoiding presenting runtime instructions as user conversations. Older rollouts without `turn_context` retain prior parsing.

Real isolated E2E verified create → send → idle → preview, with minimal replies persisted after roughly three seconds. Same operation ID/
request returned original-task cache; different requests returned 409. A second backend generation's send to the same thread returned 409
before ledger writes, leaving its ledger empty. Actual Interrupt returned 200, session returned idle, and target reply never appeared.
The test created a clearly labeled `AgentHalo E2E 2026-08-26` Codex historical thread. Real approval E2E also passed: signed Release created
an owned thread entering `waiting approval`; cards offered only provider-declared `Deny & Stop` and `Allow Once`. Selecting `cancel`
removed the card and returned idle; the target `touch` file existed neither before nor after denial.

Real Provider sessions created during development/acceptance must have explicit E2E/test titles and record
`device_id + provider_id + native_session_id`. Tests end with archive and reverse verification: absent from active lists and Provider-marked
archived. Failed archive permits only “functional steps passed, cleanup failed,” not complete acceptance. Cleanup archives only, never deletes
transcripts. Pure in-memory fixtures, temporary JSON, and unit data never entering real Providers are exempt.

Windows local transport will implement the same `AgentHaloTransport` contract with Named Pipes in Native Platform Layer,
not simulated Unix-socket paths.

## Rust MusterTransport

`muster-transport` now contains concrete `MusterTransport`: it gets an authenticated yamux child stream without headers from
`MusterForwardConnector`, writes existing Go/Swift wire `MST1 + BE uint16 + {"v":1,"port":N}`, then the same AgentHalo HTTP/1.1 request.
Target `port` still maps through agent `forwards:` allowlists to TCP or `unix:/path/to/backend.sock`; no allowlist bypass is added.

Rust owns request/path/body/header bounds, strict MST1 v1 encoding/decoding, HTTP-response checks, and replay classification.
`Never`/`SequencedStream` return `AmbiguousMutation` if ACK is lost after HTTP bytes may have arrived. Failure after MST1 header only,
before HTTP starts, does not count as execution. Mutations with stable `operation_id` retain backoff retries.

`MusterForwardConnector` uses versioned C callback tables to Platform Layer. Native establishes/maintains outer WebSocket,
inner TLS 1.3/SPKI pin, hardware identity, and yamux sessions, returning fresh raw child streams without MST1 headers per request.
Read/write/open callbacks are bounded synchronous ABI, callable only from Rust background workers. Native cannot await the UI/main
thread already waiting on Rust requests. Rust closes each successful nonempty stream handle exactly once.

Apple driver reuses physically verified `MusterConnector` with new `openRawStream()`; Phase 1.5 `openForward()` remains only a banner
baseline. Swift no longer writes MST1/HTTP for AgentHalo requests. Open preserves pin mismatch/unauthorized/timeout types; after HTTP
starts, Rust replay policy still classifies I/O failures as `AmbiguousMutation`. Synthetic Rust callback tests cover wire order, pin
terminality, close ownership, and ambiguity. Swift 6 iOS tests exercise actual static libraries through Swift → Rust → Native callbacks →
Rust response parser. Xcode completed unsigned Simulator and generic iOS-device links; these do not establish signing, installation,
physical SEP residency, or relay/real AgentHalo E2E.

## Terminal Provider

Stable Terminal session identity:

```text
device_id + provider_id + native_session_id
```

Do not mix it with Claude/Codex logical sessions or treat native IDs as file paths. Adapters explicitly declare list/create/snapshot/
delta/input/resize/close capabilities; clients cannot infer Ghostty behavior from iTerm2.

Input/output must declare content semantics; text input is not arbitrary PTY bytes:

- `RawInput`: arbitrary VT/PTY bytes;
- `TextInput`: vendor Unicode text-input interface;
- `VtByteStream`: incrementally replayable raw terminal output;
- `RenderedTextSnapshots`: vendor-rendered UTF-8 text snapshots.
- `ControlOnly`: session discovery/control without UI output.

Grid size is optional metadata, populated only if vendors expose rows/columns. Never fabricate unknown dimensions as `0×0` or
`24×80`. Generic create/close contracts cannot include vendor-specific profiles or force-close parameters.

Discrete mutations such as create/close use stable `operation_id`. Continuous input uses `stream_id + monotonic sequence`:

- Same sequence + same bytes returns Duplicate ACK without execution again;
- Same sequence + different bytes fails closed;
- Sequence gaps fail closed;
- Lost ACK is ambiguous mutation; clients cannot blindly resend;
- Reconnect restores output by cursor before allowing new input.

Adapters declare lock-screen capability as `Unsupported`, `ExistingSessionOnly`, or `PrepareAppRequired`. GUI-launching adapters obey
`prepare_app → launch → operate → cleanup/relock`, never launch before authorization.

## iTerm2 Native Adapter

The first macOS adapter lives in `src/native/apple/`, entirely in-process native Swift. Pinned `api.proto` generates SwiftProtobuf types;
SwiftNIO performs WebSocket over verified iTerm2 UDS. Dispatcher owns correlation/notifications/transactions/limits; adapter enumerates/
creates/closes sessions, snapshots scrollback, sends text, and resizes. `AgentHalo.app` ships/invokes no Python runtime, PyPI packages,
or external helpers.

iTerm2 exposes rendered-text reads and Unicode sends, so this adapter declares only `RenderedTextSnapshots + TextInput`, not
`VtByteStream`, `RawInput`, or `IncrementalOutput`. Current UI reads snapshots by revision; incremental capability requires connected,
verified screen-update cursor/delta semantics first.

Input uses bounded `stream_id + sequence` deduplication, rejecting old sequences outside the window without re-execution.
Create/close `operation_id` dedup and input ACKs are valid only in the current connection epoch. Mutations after timeout, disconnect,
or epoch changes are ambiguous: close, re-enumerate/snapshot, and leave decisions to upper layers without automatic replay.

iTerm2 API/cookies are normally user-configured. External apps request one-time credentials with AppleScript `request cookie and key`,
so real connections are Automation/TCC-controlled. “Only connects to authenticated Unix socket” does not mean “sends no Apple Events.”
The implementation neither launches iTerm2, simulates clicks, disables cookie requirements, nor enables “Allow all apps to connect.”
Synthetic tests establish native protocol contracts only. Actual acceptance needs user-enabled API, explicit AgentHalo Connect, TCC,
session enumeration, reads, and input. Historical internal App listed seven real sessions and refreshed input/snapshots, but
`authorization_attempted=false` came only from historical runtime diagnostics that did not connect to the API; it cannot prove actual
connections avoided Automation. Create/close/resize still lack live acceptance. Final native architecture:
[`agenthalo-terminal-native-iterm2.md`](agenthalo-terminal-native-iterm2.md).

The old bridge/runtime builder and Python-only tests/packaging targets are removed. Full Swift WebSocket + SwiftProtobuf is the sole
product path; schema legal gates and real iTerm2 E2E remain as described in the native architecture.

## Ghostty Adapter capability boundaries

Ghostty 1.3 macOS AppleScript dictionaries offer stable terminal IDs, window/tab/terminal enumeration, window/tab/split creation,
text input, and closure, protected by Automation/TCC. Current official `Ghostty.sdef` has no screen/scrollback, rows/columns, or resize API.

The repository implements a native ScriptingBridge control-only adapter with bounded operation history, `stream_id + sequence`
deduplication, stable continuity tokens, and explicit user-authorization gates. It cannot provide Terminal UI content:

- May declare `ListSessions`, `CreateSession`, `TextInput`, `CloseSession`;
- Cannot declare `Snapshot`, `Scrollback`, `IncrementalOutput`, `Resize`;
- Does not guess terminal-ID/screen correspondence through Accessibility APIs;
- Must not mark Ghostty a fully usable Provider before official output APIs or verifiable AX mappings exist.

Local inspection of actual Ghostty 1.3.1 sdef, bundle ID `com.mitchellh.ghostty`, matches official documented capabilities. Code uses
a minimal stable sdef subset without `osascript`, simulated keyboard input, or automatically launching Ghostty to execute commands.

## macOS Automation gating

Apple Platform Layer uses `AEDeterminePermissionToAutomateTarget`:

- Ordinary preflight fixes `askUserIfNeeded=false`, never prompting;
- Check only `core/getd`, matching actual ScriptingBridge reads, not wildcard class/ID. Wildcards may return `noErr`
  without a real TCC grant;
- Asynchronously request permission only after explicit “Connect Ghostty” selection;
- External iTerm2 cookie/key requests use target-specific Apple Events, only after explicit Connect/Reconnect.
  App launch, background refresh, and remote requests never request them;
- Map `-1744` to awaiting authorization, `-1743` to denied, `-600` to target not running;
- Authorization must not block the main thread.

Signed AgentHalo macOS targets enabling Ghostty require `NSAppleEventsUsageDescription`. Hardened Runtime requires
`com.apple.security.automation.apple-events`. With App Sandbox, separately evaluate target scripting-access-group entitlements,
without broad System Events control permission.

## Migration plan

1. Rust domain, Terminal contract, FakeAdapter, and C ABI. (Complete)
2. AgentHalo API client and local UDS transport. (Complete)
3. iTerm2Adapter and Ghostty control-only Adapter (code/synthetic tests complete; real signed-internal iTerm2
   list/snapshot/text-input E2E passed, create/close/resize pending). Full Ghostty output waits for supported APIs.
4. Rust MusterTransport request layer (MST1 + AgentHalo HTTP + replay contract). (Complete)
5. macOS AgentHalo main App: new SwiftUI shell/workspace, Terminal feature, Rust core, embedded local-backend lifecycle,
   and iTerm2/Ghostty adapters connected; historical AgentHalo three-column console not migrated. Build/model tests/ad-hoc signing/
   actual launch complete. Codex rollout discovery/preview passed real local-history acceptance. AgentHalo-owned app-server
   create/send/interrupt, durable dedup, and second-owner protection are connected; Desktop owners stay read-only. Real approval-denial
   E2E passed. Claude transcript discovery/preview complete, owned control transport not yet implemented at this checkpoint.
   Historical signed internal iTerm2 session list/snapshot/text-input has runtime evidence, but Automation/TCC evidence is incomplete
   and requires reacceptance under native architecture. Remote-relay live E2E remains pending.
6. iOS AgentHalo: original `MusterIOS` target retains fixed bundle identity, with AgentHalo product entry. Provider/session lists,
   archived filtering, previews, roughly 850 ms conditional watchers, prompt/create/interrupt gates, and persistent relay/pin/forward
   settings are implemented; SEP/Phase 1–1.5 pages move to Network diagnostics. Rust ABI 1.10 typed Muster covers all shared client
   operations. Approvals/questions appear in selected conversations, failing closed by capability/`available_decisions`. iOS is a
   Muster-only remote client without local backend. Relay/target unavailability retains App and Device/Network pages but disables
   all remote capabilities/mutations. Compile-time three-device/SPKI tables are removed; independent directory records create
   per-device tunnels/health probes and aggregate device-grouped sessions, routing all operations to owners. Offline groups/caches
   remain. Creation explicitly selects online remote devices, with no Local option. Swift 6 app-module 76 tests, Xcode Simulator builds,
   signed device builds/install/launch, and M1 Max actual-relay reads/directed create/send/stream/interrupt passed. Real Muster service-directory
   sync and multiple-device aggregation remain pending. All-device automatic discovery after owner pairing must come from shared Muster
   Network signed directories, not iOS username/authorization-roster enumeration. Physical approval/question, attachments, and Terminal E2E remain pending.
7. Android AgentHalo. (Kotlin/JNI core, arm64-v8a/x86_64 bridges, and AAR completed early; pause expansion until iOS finishes,
   then implement device smoke tests, Keystore, Muster connector, Compose UI, and Terminal Adapter.)
8. Windows AgentHalo: Native Platform Layer, Named Pipe transport, and ConPTY Adapter.
9. Retire overlapping Swift networking only after real E2E on at least two platforms.

Existing Muster iOS Phase 0–1.5 remains the SEP/mTLS/relay/yamux/MST1 validation baseline;
this architecture supersedes its SSH/SwiftTerm Phase 2.
