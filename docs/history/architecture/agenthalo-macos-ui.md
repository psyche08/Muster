# AgentHalo macOS UI

## Reference and ownership

The new macOS app is implemented in this repository. Its interaction reference is the former
`remote-coding` Web workspace (`remote-agent/static/index.html`), not the abandoned native
AgentHalo SwiftUI project. The reference informs hierarchy and interaction density; the new app
uses native SwiftUI components and owns a separate product model.

## Workspace hierarchy

```text
AgentHalo window
├── Sidebar (about 300 pt)
│   ├── Brand, settings and connection state
│   ├── Workspace mode: Agent / Terminal / Computer Use / Locked Use / Network
│   ├── Device
│   ├── Provider or Terminal Adapter chips
│   ├── Session search and New Session
│   └── Session cards
└── Main workspace
    ├── Active session toolbar and state
    ├── Open-session tabs
    ├── Approval/question inbox when present
    ├── Conversation, tool output, terminal snapshot, or remote-control canvas
    ├── Usage/status strip
    └── Fixed composer with attach/send/queue/insert/stop actions
```

The Agent workspace is the default and primary product. Terminal is a child workspace reached
through the mode picker; it reuses provider/session selection but does not turn AgentHalo into a
terminal-only app.

The Device/Network workspace is also a first-class AgentHalo surface. It presents the bundled Muster
network's device discovery, authorization, revocation, deletion, route and connectivity diagnostics,
but it does not introduce a separate Muster desktop product. Remote Agent and Terminal sessions use
this shared device/network identity and connection graph; local sessions continue through their local
UDS or platform adapters when the remote network is unavailable.

The current candidate redesign is specified in [cross-platform Devices page](agenthalo-device-page.md) and
[desktop Devices page](../design/agenthalo-desktop-devices.md): network overview, actionable blockers,
local device, one member list, and device details with explicit permission direction and scope.
It separates network governance from serving Agent/Terminal capabilities. Shared Rust owns
identity grouping, usability and action eligibility; macOS only renders and performs OS callbacks.
This design is not implemented. The prior per-source label UI and its test evidence are preserved
in the [historical implementation record](../design/history/agenthalo-device-page-2026-09-21-v1.md).

The owner-pairing details below describe the existing/legacy flow and its historical ABI gates;
they do not define the new member-level permission UI. Current decoupling progress is recorded in
[network authorization and member services](agenthalo-network-authority.md). Existing authentication gates remain in force.

The trusted-device side of mobile authorization lives in this Device/Network surface. Its primary
action is **Add mobile device**: macOS renders the short-lived, one-time Pairing Presentation V2 only
as a QR code. The normal product flow does not display, copy, share or accept the raw payload, deep
link, pairing code or trust-anchor JSON. After the phone proves possession of its hardware identity,
both devices show the transcript-derived SAS. The approval sheet shows the phone name/platform and
requires the user to compare that SAS, then requires `LAContext.deviceOwnerAuthentication` before
submitting the grant. Expired, cancelled and already redeemed invitations are not reusable.
The default selection is current and future devices owned by the paired owner with only AgentHalo
remote-client access. Selected-device targeting is explicit; Terminal, Computer Use, Locked Use,
device management and authorization management are separate opt-in capabilities. Locked Use is
never inferred from ordinary remote-client or Computer Use access.

The UI consumes an injected typed projection of the Shared Muster PairingTransportV2 state. Shared
Rust creates the signed transcript, enforces the QR redemption lifetime, burns the invitation after
the first valid proof, owns approval and owner-delivery deadlines, and returns only canonical
credential-free QR bytes and opaque handles. Swift passes those exact bytes to Core Image, supplies
local-auth and Secure Enclave callbacks, and never creates a deep link, pairing code, manual payload,
trust-anchor export, legacy grant, or consumer-activation fallback. GrantV2 issuance, verification,
revocation, persistence, and delivery remain Shared-owned. A non-exportable Secure Enclave P-256 key
signs owner requests. The relay
publisher capability is encrypted with a distinct Secure Enclave key and only the ciphertext-bearing
record is retained in a `WhenUnlockedThisDeviceOnly` Keychain item. Secure Enclave keys use
CryptoKit's opaque `dataRepresentation` persistence under
`~/Library/Application Support/AgentHalo/SecurityKeys` with `0700/0600` permissions. The blob is
bound to the creating Secure Enclave and is not private-key material; copying it to another Mac cannot
open or use the key. Shared Core authority-orchestration protected bytes are encrypted and persisted
exactly as supplied; Swift stores the separately supplied authority-set identifier as envelope binding
but does not parse or duplicate the evolving protected-state schema. Shared Core validates the
identifier, local identity, authority log and federation state when restoring. AgentHalo does not use
permanent Keychain `SecKey` references, because that path
requires an App-ID provisioning profile and otherwise fails with `errSecMissingEntitlement (-34018)`.
There is no software-key fallback: a missing Secure Enclave or an invalid blob fails closed and
requires explicit repair. Normal invitations never expose the publisher capability. Relay-owner
registration remains a deployment action and the product UI exposes neither the trust-anchor JSON
nor an `odp_` input. A prepared local owner may display the Shared QR contract before Relay
activation by deriving the credential-free HTTPS origin from the running local Muster identity; the
UI labels proof transport unavailable and must not claim that scanning completes authorization.

The live adapter now creates and cancels the Rust-owned local scan invitation. ABI 1.28 explicitly
reports `PAIRING_TRANSPORT` and Relay capability burn unavailable, so the QR screen does not poll the
legacy owner-directory V1 `/pairings` endpoint or present a false waiting-for-proof state. The retained
legacy grant/revocation path remains migration-only until Shared proof transport can deliver the
mobile proof into the opaque invitation handle. Once transport is available, approval must still
display the Shared SAS, require local owner authentication, sign through the ABI, apply the returned
grant state and reconcile target status. Revocation signs through ABI 1.12, revokes the Relay client
grant/read capability and removes the grant from local active state. Grant rows preserve
per-target `ready`, offline-waiting, service-unavailable and terminal-authentication states. The old
local roster/SPKI controls remain only under Developer diagnostics and are not presented as the
cross-device grant flow.

Approval is fail closed before the Relay POST. If the orchestrator is unavailable, an authority
source is malformed, a credential appears in the activation, or any active authority has not supplied
an active `agenthalo.remote-client` grant for the mobile hardware subject, Shared Core rejects the
activation and macOS neither submits nor persists the new grant. The mobile client must run Shared
validation before storing the separately redeemed, per-owner `odr_`, then reconcile the canonical
credential-free activation. The activation does not transport read capabilities. Multi-authority
management still requires a signed ceremony/transport that collects every active authority's grant;
the macOS adapter deliberately does not synthesize missing remote grants. Production owner creation,
secure one-time `odp_` handoff, a non-empty Relay ledger, Shared proof transport/Relay burn, Relay
publish/subscription, mobile redemption and real multi-device `8765`/session traffic remain separate
deployment and E2E gates.

Two security gates remain shared-layer work and must not be presented as complete by macOS. First,
the shared protocol currently defines and mobile clients accept only `agenthalo.remote-client`;
Terminal, Computer Use, Locked Use, device management and authorization management remain visibly
separate but disabled until shared identifiers and target enforcement exist. Second, relay revocation
burns future directory/client access, but Muster Agent does not yet expose a subject-grant enforcement
hook that closes already established target connections and sessions. The macOS UI must not claim
that teardown occurred until the shared runtime supplies and verifies it. Deterministic tests cover
canonical QR-only creation, forbidden credential/deep-link absence, pre-activation relay-origin
derivation and local cancellation with a software test signer; Secure Enclave, signed App, installed
runtime, proof transport and multi-device Relay E2E remain separate acceptance gates.

Remote peer reconnect ownership follows the cross-platform
[Remote Peer Lifecycle contract](agenthalo-cross-platform.md#remote-peer-lifecycle-cross-platform-contract).
ABI 1.13's shared Rust peer network owns each peer's first and subsequent reconnect,
authentication-error classification, capped `1s -> 30s + jitter` backoff, generation/sequence,
descriptor reconcile and cancellable shutdown. It exposes a stable typed connector to AgentHalo and
publishes ordered `discovering`, `connecting`, `transport_ready`, `backoff`, `offline`, terminal-auth
and `stopped` events. Pin mismatch, revocation and authorization failures are terminal and never
automatically retrusted.

`AgentHaloMusterPeerManager` is a thin MainActor event/reconcile consumer, not a second network
supervisor. The macOS transport callback launches the bundled helper in `--supervisor-child` mode for
exactly one authenticated WebSocket -> hardware identity mTLS/SPKI -> yamux session and exposes raw
child streams through an attempt-private `0600` UDS. The helper never retries; Rust is the sole retry
owner. Swift probes remote AgentHalo `8765` `/healthz` only after `transport_ready` and keeps probing
without a fixed deadline. A slow backend or failed health probe marks `serviceUnavailable`, but never
terminates the transport supervisor or disturbs another peer. Reconcile removes only identities that
the verified directory removed, recreates a connector only when connection identity changes, and is
idempotent for unchanged peers. App shutdown cancels UI/service tasks, suspends and disposes the shared
network, then closes platform sessions, streams, Processes, FileHandles and attempt sockets. The old
one-shot 60 x 100ms health window and Swift/CLI restart loop are not used by this reference consumer.
Normal App termination and `SIGTERM` both enter the same synchronous ownership shutdown before the
process exits; this is required because `NSApplication.willTerminateNotification` alone does not
reliably cover signal-driven termination.
Production owner-directory subscription and real multi-device relay/session E2E remain separate
acceptance gates.

Computer Use and Locked Use are separate AgentHalo workspaces/capabilities. Computer Use exposes only
the screen and input operations actually supported and authorized by the selected platform adapter.
Locked Use adds a stricter locked-device capability, lifecycle and audit boundary; the UI must never
infer it from ordinary Computer Use support. Their remote connections share Muster discovery,
authorization and encrypted transport rather than maintaining a parallel host registry.

## Visual language

- dark working canvas, slightly lighter sidebar and toolbar;
- warm coral accent for selection and primary actions;
- green online/running state, amber pending state, red destructive/error state;
- compact provider chips and session cards;
- monospaced blocks for terminal snapshots, commands, tool I/O and diffs;
- fixed bottom composer, independently scrolling content and sidebar;
- minimum desktop window size keeps sidebar, content and composer reachable;
- narrow layouts collapse the sidebar into a sheet rather than squeezing the workspace.

Agent conversations follow Codex's reading hierarchy: assistant Markdown is rendered directly on
the canvas, user messages use a restrained trailing bubble, thinking is a muted disclosure, and
tool calls are compact disclosure rows with one-line input summaries and expandable input/result.
Role labels and repeated outer message cards are intentionally omitted.

Conversation scrolling follows the same Codex product rule: opening a session starts at its latest
message and streaming content keeps following only while the reader remains at the bottom. Scrolling
up never gets overridden by a transcript refresh. While away from the latest position, a circular
down-arrow is shown at the bottom center of the message viewport; activating it jumps directly to
the bottom, hides the control, and resumes live following. Reaching the bottom manually has the same
effect.

The composer is one Codex-style rounded container rather than a detached editor, send button and
settings strip. Its bottom toolbar keeps attachments and approval on the left, then context usage,
combined model/reasoning selection and the circular send action on the right. Selecting a session
replaces those controls from that transcript's latest `turn_context`: model, reasoning effort,
approval policy, sandbox policy, workspace roots and per-root read/write access, plus network
permission. A previous session's selection must never leak across a session switch. Changing a
control is explicit user intent for the next AgentHalo-owned turn; otherwise the original session
settings remain selected.

For a remote session, the composer settings and provider options must be fetched from that session's
owning device rather than the Local backend. Preview, send, interrupt, approval, question, provider
options and runtime status share the full `(device_id, provider_id, native_session_id)` route. A new
session requires an explicit target-device selection; Local is a valid desktop target but must never
be used as a silent fallback. Offline devices and their session groups remain visible with mutations
disabled instead of disappearing from the sidebar.

A stored Codex rollout initially has a provider-native identity but no writable AgentHalo logical
session. The composer remains available when Codex advertises send support; its first send performs
an explicit `resume_native_session`, requires `thread/resume` to return the exact durable identity,
persists ownership for the current app-server generation, and only then starts the turn. For a
remote Muster device, the UI-facing identity stays device-namespaced while resume and send are routed
with the peer's raw identity. A resume failure preserves the draft and never attempts prompt delivery.

Each Agent turn separates its last assistant response from the preceding work log. While the
authoritative turn state is still running, `Worked for Xm Ys` is expanded by default even if a final
text fragment has already arrived; once the turn completes, the work log starts collapsed. User
expansion choices remain stable during the same live/completed phase. The expanded work log shows a
`Used <provider> integration` activity summary, while each individual tool invocation starts
collapsed and exposes its input/result only on demand. Elapsed time derives from provider timestamps,
and the summary includes edited/read/searched/command categories. Current Codex `custom_tool_call`
records and legacy `function_call` records are both
accepted. Tool-provided source paths are rendered as buttons; selecting one opens an in-app,
read-only UTF-8 or image preview resolved against the session working directory. Assistant-only
`<oai-mem-citation>` metadata is removed before rendering, while the same literal text remains
visible when the user quoted it.

## Terminal truthfulness

- iTerm2 may display rendered UTF-8 snapshots and accept Unicode text; it is not a raw VT stream.
- Ghostty remains control-only until its public API exposes screen contents.
- Unsupported snapshot, resize or raw-input controls are hidden instead of simulated.
- Connecting Ghostty is always an explicit user action; app launch and preflight never request TCC.

## iTerm2 native packaging

AgentHalo now links the in-process SwiftNIO WebSocket + SwiftProtobuf client. The build has no
`AGENTHALO_ITERM2_RUNTIME` override, portable-runtime target, CPython/PyPI resource copy, or nested
Python signing branch. Package inventory must reject Python interpreters, stdlib, `site-packages`,
`.py/.pyc`, and the removed helper name. Read-only socket discovery does not connect or request TCC;
the local Connect action obtains the required one-time cookie/key through AppleScript and therefore
remains subject to Automation consent. Distribution of pinned `api.proto` and generated code still
requires the independent legal gate in
[`agenthalo-terminal-native-iterm2.md`](agenthalo-terminal-native-iterm2.md).

## Local AgentHalo backend endpoint

The app resolves its local UDS endpoint in this order: the
`AGENTHALO_BACKEND_SOCKET` development override, the user-selected path saved by the app, then
`~/Library/Application Support/AgentHalo/run/backend.sock`. Settings exposes the effective path and
supports Apply & Reconnect, Reconnect, and Use Default. Paths must be absolute after `~` expansion;
changing the endpoint clears provider/session state before reconnecting, so UI state cannot leak
between two backends.

The standard app embeds the Rust `agenthalo-backend` binary under `Contents/Helpers`. Startup first
tries the configured endpoint, then starts the bundled helper only when that endpoint is offline.
Readiness requires a successful Unix-socket connection, so a stale socket file cannot create a
false online state. The app terminates only the helper process it owns. The helper and enclosing app
are both included in strict code-signature validation.

The local-admin socket is never a Muster product forward. During migration AgentHalo privately backs
up `agent.yaml` and removes the reserved legacy `8765 -> backend.sock` entry before starting either
the backend or the bundled agent. Remote AgentHalo mutations remain unavailable until the independent
GrantV2/MST2 listener resolves a remote principal and calls the restricted remote router; the app
must not restore the legacy forward merely to make a remote demo pass. The installer waits for the
old App and its exact bundled backend child to exit before replacing the bundle, preventing an old
owner generation or dead socket from surviving under a moved backup path.

A V2 Muster agent may run as an authenticated vnet participant with no legacy `forwards`. Owner
bootstrap publishes only its hardware-backed device identity and an empty service list until the
independent listener is ready. It must not advertise a placeholder AgentHalo service merely to
initialize the owner key or satisfy config validation.

The Codex rollout adapter advertises stored-session discovery and preview. An AgentHalo-owned
headless app-server generation additionally enables Create, Send, Interrupt, command/file-change
approval, and structured user-input requests only for threads it created. Desktop/CLI sessions stay
read-only. Pending interactions render above the conversation with only the decisions advertised by
Codex; unknown server requests and extra-permission/MCP requests fail closed. Claude transcript
discovery and preview are available read-only; Desktop App presence is not treated as an owned
control transport. The backend is not incorrectly rendered as offline.

The selected Agent session is followed independently of mutation ownership. Its first preview stores
the provider-native `sig`; subsequent 250 ms reads send `if_sig`, and an unchanged rollout/transcript
returns only `not_modified=true`. This preserves low-latency external-session updates without repeatedly
decoding or transferring the full transcript. Codex `commentary` and `final_answer` phases remain
distinct across Rust, FFI and Swift so in-progress text is not misclassified as the completed answer.

The app-server explicitly selects the human `user` approval reviewer at both thread and turn start,
so a host-wide automatic reviewer cannot bypass the AgentHalo approval card. A signed Release E2E
verified `waiting approval`, capability-filtered buttons, `cancel`, return to `idle`, and that the
denied command produced no filesystem side effect.

Every real Codex or Claude session created for development/E2E must be archived after verification.
The test is not fully complete until the session no longer appears in the active list and the provider
reports it as archived. Cleanup archives the session; it must not delete the transcript.

## Bundled Muster layout

AgentHalo's macOS build contains every unprivileged Muster executable under the signed App bundle:

```text
AgentHalo.app/Contents/Helpers/
├── AgentHaloMusterMigrator.app
├── muster
├── muster-agent
├── musterctl
├── muster-sep-helper
└── muster-net-bridge
```

Mutable data is never written into the App bundle and the installed App has no runtime dependency on
`/opt/muster`. The platform layer uses the normal per-user macOS domains:

- configuration: `~/Library/Application Support/AgentHalo/Muster/Config`;
- persistent identity, relay token, certificate cache and sealed ledger:
  `~/Library/Application Support/AgentHalo/Muster/State`;
- backend and Muster logs: `~/Library/Logs/AgentHalo/Muster`;
- local UDS: `~/Library/Application Support/AgentHalo/run/backend.sock`.

On a versioned first migration, an existing `/opt/muster/etc`, `var`, and `log/agent.log` are copied
into that layout. The source tree is not deleted, `/opt` state references are rewritten to the new
State directory, and legacy absolute helper paths are removed so `muster-agent` resolves both helpers
beside itself after every App move or update. A private migration marker prevents deleted new-state
files from being resurrected on later launches.

The v2 migration treats the still-running legacy LaunchAgent configuration and state as the active
source of truth. If an earlier AgentHalo build already created different destination files, it first
backs them up under `Muster/MigrationBackups/opt-muster-v2`, then synchronizes the active `/opt/muster`
data without deleting the legacy rollback tree.

Some internal ad-hoc App updates can inherit a macOS access-control label tied to the previous code
hash. If the updated App itself cannot read that older `Muster` directory, it leaves the directory
untouched and migrates the active legacy tree into `~/Library/Application Support/AgentHalo-Muster`.
Both locations remain in the normal per-user Application Support domain; the fallback also avoids an
access-controlled legacy `AgentHalo` parent without asking for Full Disk Access or weakening file
permissions merely to replace an internal build.

On every install, `deploy/install-agenthalo-user-runtime.sh` runs the migrator bundled in the App
being installed before it replaces the existing `dev.linsheng.muster.agent` user LaunchAgent. An
already selected canonical or versioned layout is still re-verified: "prepared" means that the
layout choice is known, not that current security migrations may be skipped. This prevents an older
layout from retaining the unauthenticated legacy AgentHalo forward on port 8765. The stable label
continues to start at GUI login, but its executable and configuration are now:

```text
/Applications/AgentHalo.app/Contents/Helpers/muster-agent
~/Library/Application Support/AgentHalo/Muster/Config/agent.yaml
```

The installer first validates the migrated identity and configuration in a one-shot launchd job. It
then replaces the stable user LaunchAgent plist with one whose `ProgramArguments` point to the signed
helper and migrated configuration. The plist uses
`AssociatedBundleIdentifiers=dev.linsheng.agenthalo`, so Background Task Management and System
Settings attribute the service to AgentHalo. The App deliberately does not also seal a same-label
plist under `Contents/Library/LaunchAgents`: macOS retains the pre-existing legacy BTM record by
label, and an in-place conversion to `SMAppService.agent` can leave that record disabled and reject
the replacement. Because `launchctl bootout` removal is asynchronous on current macOS, the installer
also waits until the old job is absent before bootstrapping the replacement; otherwise launchd can
report `Operation already in progress` through launchctl's generic I/O error. The installer verifies
plist ownership and process path, and rolls the old plist back if the signed bundled agent does not
remain running.

`agenthalo-backend` is a second, independent per-user LaunchAgent with the stable label
`dev.linsheng.agenthalo.backend`. It runs the signed helper from
`/Applications/AgentHalo.app/Contents/Helpers/agenthalo-backend`, owns only the local-admin UDS at
`~/Library/Application Support/AgentHalo/run/backend.sock`, and uses `RunAtLoad` plus
`KeepAlive=true` with a five-second launchd throttle. The SwiftUI process never spawns, terminates, or inherits the
backend. Closing every AgentHalo window or quitting the App therefore does not stop provider
sessions, local operation ledgers, or local API availability. The App only probes the UDS and
projects the LaunchAgent's real availability.

`deploy/install-agenthalo-backend-runtime.sh` installs this job only after the signed App and Muster
layout are available. It writes an associated, mode-0600 plist with exact absolute helper/config
paths, removes any verified legacy App-owned backend, bootstraps the user-domain job, and requires
both the expected launchd program path and a real `/healthz` response before committing. App bundle
replacement temporarily unloads an existing backend job to prevent launchd from respawning an
executable while its containing bundle is being moved. It then starts the same stable job against
the replacement bundle; failure restores the previous App and LaunchAgent. This service continuity
is installation/runtime evidence only and does not make `backend.sock` a remote product endpoint:
MST1, port 8765, raw TCP, and forwarding to the local-admin UDS remain rejected.

If an earlier ad-hoc App identity forced a machine onto the protected
`~/Library/Application Support/AgentHalo-Muster` fallback, the signed migrator can converge it back
to the canonical `~/Library/Application Support/AgentHalo/Muster` layout. The agent is stopped while
the migrator copies Config and State, rewrites layout-root references, preserves file permissions,
and writes the canonical migration marker last. The versioned source is retained until the canonical
service and a remote 8765 AgentHalo read have both passed.

Deployment does not depend on opening the SwiftUI window: the installer runs the executable inside
`AgentHaloMusterMigrator.app` as a one-shot user launchd job before the persistent agent is switched.
This gives migration the same service context as `muster-agent`, records a deterministic JSON report,
and leaves the old launchd job untouched if migration or validation fails. Both one-shot jobs run
with `ProcessType=Interactive`, like the agent, because the installer blocks on them. The installer
waits up to 60 seconds for each (`AGENTHALO_LAUNCHD_JOB_TIMEOUT_SECONDS`). Right after an install,
the first launch of each newly signed helper can wait behind syspolicyd on a loaded machine. A
non-zero exit or a terminating signal fails the install immediately. A job still running at the
deadline is stopped and reported as unfinished, not as a failed migration or validation.
`src/deploy/tests/install-agenthalo-user-runtime-test.sh` exercises these paths against a fake
`launchctl`. `--layout versioned` is
reserved for recovering an older ad-hoc App directory whose macOS access-control label cannot be
reused by the replacement App.

`muster-net-bridge` keeps its Developer ID Team ID and Hardened Runtime signature inside the outer
App signature because the root-owned `dev.linsheng.muster.nethelper` authenticates that code identity.
The privileged helper itself remains in `/Library/PrivilegedHelperTools` with root-owned configuration
and logs; putting a privileged mutable service inside the user-writable application data directory
would violate the macOS security boundary.

## First macOS delivery gate

1. Native app builds and launches from this repository.
2. Offline state remains usable and explains the missing backend or adapter.
3. Local UDS provider/session/preview/send operations go through `AgentHaloCoreClient`.
4. Terminal mode lists the truthful iTerm2/Ghostty capabilities.
5. The historically signed internal App listed seven real sessions after the user enabled the iTerm2 Python API, and completed
   `list -> select -> snapshot -> input -> automatically refreshed snapshot`. However, diagnose reporting
   `authorization_attempted=false` does not prove the actual connection avoided Apple Events; after native cutover,
   Allow/Deny/Revoke must be revalidated under a clean TCC user. Create, close, and resize still require separate live
   acceptance。
6. Ghostty live TCC acceptance is deferred by current product decision.
