# macOS / HarmonyOS historical implementation and verification records

This document retains the complete English record from before the Chinese checklist was compiled, for tracing historical implementations, tests, and failure corrections.
For current status, the complete gaps, and unified acceptance requirements, refer to the [completion checklist](agenthalo-remote-completion-checklist.md).
The sections below record states at different times. Unfinished work in an old version must not be treated as the current conclusion, and software tests must not be treated as physical-device acceptance.

User instruction: finish the remaining implementation before asking the user
to repeat pairing or perform device acceptance. Internal regression checks are
not product acceptance. Do not deploy a partial client as a test-ready build.

## Remaining work by client (2026-09-10)

| Area | Implemented in source | Still incomplete |
| --- | --- | --- |
| Mac automatic publication | Shared listener-bound publication, renewal, durable reconciliation, stop withdrawal and legacy registration import; ABI 1.62 schedules transient import retries with exact canonical rereads and quit cancellation | Production restart/publication/withdrawal acceptance is pending. Rejected/corrupt state remains blocked, never silently replaced. |
| Mac compose/recovery UI | Local and authenticated remote create/send/explicit resume/interrupt/approval/answers use Shared. Five encrypted upload slots connect bounded file selection, recovery UI, exact selected-file proofs and attachment-only sending; no remote local-path insertion or implicit resume. One per-peer busy gate covers upload cleanup, proof capture, mutations and reconnect. ABI 1.75 adds Shared provider discovery; ABI 1.76 owns session aggregation, routes, failure isolation and cache scopes. ABI 1.77 wires directory and bounded file preview through exact Shared session routes; ABI 1.78 adds session-scoped Git history and patches. | Signed installed-App crash/recovery and real provider file/Git reading remain open. Legacy/retired cache-file lifecycle cleanup is still pending; old unscoped caches are not imported or deleted. |
| Apple authenticated remote bridge | Typed Swift adapters wrap Shared target selection, certificates, authenticated reads, mutations, upload proofs and reconnect. Native consumer pairing/ACK and five encrypted recovery slots connect to the production peer manager and upload UI. Certificate/mutation/upload plans survive identical-context lease changes. Backend health is checked independently after certificate completion. ABI 1.76 session/provider handles select exact source reads; remote namespaces are never decoded into arbitrary routes and cache rows cannot establish live routes. ABI 1.80 wires bounded multi-Owner storage selection, retained pairing models and active peer-manager handoff. | Physical multi-Owner switching and orphan-index repair remain open. Changed Grant/authority/Owner contexts remain blocked, with evidence retained; explicit migration/renewal and retained-record bounds still need completion. Fixtures do not establish live mTLS, remote write or file delivery readiness. |
| Mac interaction display | Shared owns per-source bounded reads, merging, freshness and retained failures. ABI 1.69 preserves matching explicitly scoped display rows across client replacement, always stale; every old reader lease is revoked. Failed devices do not block fresh cards from other devices. | Unknown or changed source scopes are intentionally not reused. Real peer reconnect/Owner-change acceptance and the separate authorized remote-client migration remain open; display retention does not establish connection authority. |
| Mac backend | Multi-device admission, paginated native catalogs, Codex write/control and authenticated attachment storage/delivery; seven-day collection of never-referenced new uploads with durable non-reuse tombstones | Referenced/legacy attachment lifecycle cleanup, non-Codex write parity, and remote Computer Use / Locked Use execution remain incomplete. |
| Mac Grant management | ABI 1.83 signed local-history catalog, explicit local revoke/resume, Shared-selected durable request and close/reopen fencing; independent protected-state observation now withdraws stale listeners and closes old sockets even while publication is blocked | Installed hardware/mTLS revocation acceptance, per-peer updates without disconnecting unaffected phones, multi-target propagation and consumer revocation state/UI remain incomplete. Local revocation is not global device removal. |
| Harmony native/UI | HUKS certificate effects, authenticated device/session/runtime/transcript reads, Codex create/send/resume/interrupt/approval/questions and retained mutation recovery UI; bounded file upload/recovery; ABI 1.67 adds Shared-scheduled transient directory retries and truthful recovery messages. ABI 1.71 uses native upload successors and independent send-time file snapshots; late volatile cleanup blocks slot reuse/reconnect. ABI 1.77 wires exact directory/file-reference reads through NAPI and the per-device transport. The conversation header now opens directory browsing, explicit path preview and bounded native text/image rendering, with cancellation and resource cleanup. | Referenced/legacy attachment cleanup and physical file/provider acceptance; expired Grant or changed Owner recovery and retained-record bounds; unresolved mutation cases without exact receipts; complete device revocation lifecycle. |
| Both clients | Source and software fixtures cover the above implemented paths; explicit final-owner lock release now passes descriptor-alias reproduction and 15 parallel backend rounds | Matching signed installation and real Relay/Secure Enclave/HUKS acceptance have not been completed. Ordinary startup authorization, default shield state, active-shield caffeinate, normal quit, backend survival, concurrent phones and >200 sessions still need live regression. |

## Current Harmony catalog-adapter checkpoint (2026-09-10)

Harmony now exposes ABI 1.80 catalog operations through its production C++
registry, with bounded input copying and exact integer/presence validation.
The thin ETS wrapper and serialized OS-effect driver retain Shared-owned
selection, exact pending-save recovery and storage-lease invalidation. Failed
writes permit read-only reconciliation; an explicit retry reuses the same
candidate. Captured old leases are checked before and after awaited OS effects.
Stop cannot activate a late write, and an uncertain/foreign catalog never
silently replaces its predecessor.

The actual ETS/C++/Rust host contract tests software storage effects, including
lost replies, conflicting writers, close/stop, stale lease results, capacity,
orphans and wrong network authority. The unsigned SDK app/native build passes;
the new ETS adapter is not yet imported by a page, so that build does not prove
its page integration. Concrete HUKS index/namespaces, retained per-slot pairing
models and the selector/recovery UI are still open. No signing, installation,
Relay changes or physical acceptance is included. ABI remains 1.83.

## Previous protected-listener withdrawal checkpoint (2026-09-10)

The Mac backend no longer relies on its publication supervisor to detect a
changed protected target/runtime. A separate observer reads the authenticated
OS broker; the accept loop checks its bounded freshness without OS I/O. Reads
poll every 250 ms; two seconds without a sufficiently recent observation
permanently withdraws the old lifecycle, even if a blocked read later succeeds.
An active certificate operation tolerates only its own exact CAS/readback,
never arbitrary external changes. Its response completion latches retirement
before releasing the lane, preventing back-to-back requests from hiding state
changes. The loop then closes old sockets and cancels provider work independently
of publication HTTP or supervisor progress. Already delivered provider effects
are not rolled back and unresolved evidence is not cleared.

This remains whole-listener replacement, not a per-phone hot update: keeping
unaffected phones connected during another phone's revocation is still open.
Likewise, signed multi-target distribution, both clients' consumer revocation
state/UI and installed hardware/mTLS acceptance are not completed by this change.
ABI stays 1.83; native UI/FFI source and Relay are unchanged. No signing,
installation, real-key operation or physical acceptance was performed.

Verification: Rust workspace/all-targets 717 tests, workspace Clippy with warnings
denied, and the optimized macOS backend build passed. The five added regression
tests exercise active-transaction state changes, exact failed readback, blocked
and late observation, terminal rejection after recovery, and actual loopback
socket shutdown without supervisor progress. Native UI/FFI sources were not
changed and their earlier results above/below are not new physical acceptance.
The actual loopback eviction regression also passed ten consecutive runs.

## Previous signed-history management checkpoint (2026-09-10)

ABI 1.83 adds a bounded Shared-owned catalog from current signed local Grant
history, explicitly separate from phone completion and live readiness. Shared
selects the retained/completed revocation request or generates a new request ID
only for an explicit selected Grant action. Mac now wires that catalog to its
unlocked device-management view and invokes the exclusive authenticated OS lane
for local revoke/resume. It rereads after ambiguous writes, never auto-replays,
fences closed-window results and serializes reopening behind dispatched work.
The old standalone model authentication is removed. No startup management read
or action was added. Full distributed device removal remains incomplete.

Verification: Rust workspace/all-targets 712 tests and Clippy with warnings denied
passed; CoreClient 66 XCTest plus 12 Swift Testing cases and Mac 357 tests passed.
Apple three-target XCFramework, three C ABI smokes and Harmony native/UI suites
plus unsigned PackageHap passed. Initial sandboxed CoreClient socket tests failed
and Mac tests stalled; both suites passed when rerun with local test socket access.
No signing, installation, Relay mutation or physical acceptance is included. The
remaining-function inventory remains the full table, not just this management lane.
The full host contract also passed with a fresh production Mac coordinator
fixture through actual Harmony ETS/C++/Rust ABI 1.83, including checkpoint
restoration and substitution rejection. Existing deployment-target linker
warnings remain; this is software-key evidence, not physical SE/HUKS acceptance.

## Previous durable-recovery checkpoint (2026-09-10)

ABI 1.82 persists one exact pending management intent in the hardware-signed
target snapshot before Owner revocation signing. It survives first-write reply
loss and preserves the request through unrelated target updates. The final
revocation write atomically removes pending state and retains signed history;
lost final replies recover without repeating completed signatures or writes.
The verified read-only pending query is connected to Apple and the Mac
coordinator; actual recovery UI and distributed device removal remain open.

Verification: Rust workspace/all-targets 711 tests and Clippy with warnings denied
passed; CoreClient 66 XCTest plus 12 Swift Testing cases and Mac 353 tests passed.
Apple XCFramework, three C ABI smokes, Harmony native/UI suites and unsigned
PackageHap passed. Intermediate test-fixture types and the Swift worker's explicit
return were corrected before these passing runs. No signing/notarization,
installation, Relay update, real hardware revocation or physical acceptance was
performed. Other functional gaps listed above and below are unchanged.
The full host contract also passed with a fresh actual Mac coordinator fixture
through production Harmony ETS/C++/Rust ABI 1.82. Existing host-link deployment
target warnings remain; this is software-key evidence, not physical SE/HUKS or
Relay/device acceptance.

## Previous OS-wiring checkpoint (2026-09-10)

- Mac automatic publication is driven by `macos_owner_publication_v2.rs` and the
  production backend lifecycle, with App startup canonical registration import.
  It is not a newly completed or physically verified feature in this checkpoint.
- Harmony's production transport, HUKS/NAPI adapters and conversation UI already
  wire runtime/session reads, Codex actions, uploads, workspace files and Git.
  The SDK unsigned build and adapter regressions were rerun successfully here;
  this does not establish phone layout, HUKS, real Relay or provider acceptance.
- New source work here prepares Mac's explicit Grant-management OS lane, keeping
  authorization and revocation decisions in Shared. Durable UI intent recovery,
  live listener updates, distribution and both clients' consumer revocation
  lifecycle remain incomplete. No signed build, installation, Relay mutation or
  pairing-data cleanup was performed.
- The Shared enrollment catalog and native selection UI are wired on Mac only.
  Harmony's NAPI/ArkTS still lacks that catalog/Owner-switching integration;
  multiple target devices under one Owner must not be confused with multi-Owner
  enrollment. Mac orphan-index repair remains incomplete as well.

Current checks: Mac 353 tests passed, broker software selftests passed, three
backend publication regressions passed, and Harmony native/UI suites plus
unsigned PackageHap passed. The initial Mac blob fixture used a Foundation-
normalized symlink ancestor and was corrected to a descriptor-safe temporary
root; the full suite then passed. Broker selftests required running outside the
sandbox for their LocalAuthentication-context check, without opening real keys.

The full remaining-function inventory is the table above and the final inventory
below. Do not reduce it to the current Grant-management lane.

## Implementation status

- [x] Expose Shared consumer catalog/lease APIs through Harmony NAPI and a thin
  ETS OS-effect driver. Native tests run against the actual Shared library,
  not a catalog reducer mock. This is a completed adapter layer only; HUKS
  namespace wiring and multi-Owner page integration remain unfinished.

- [x] Separate protected-listener observation from publication and certificate
  activity. Source now has a monotonic freshness deadline, terminal rejection,
  exact certificate CAS tolerance and response-completion retirement. A real
  loopback socket fixture verifies idle eviction, provider cancellation and
  listener closure without supervisor calls, using a test acceptor. Full
  installed hardware/mTLS acceptance and non-disruptive per-phone updates remain
  open; this is not distributed device removal.

- [x] ABI 1.83 Shared signed-history catalog and selected-Grant revoke/resume
  now back Mac's explicit device-management UI. Local issuance, expiry,
  supersession, pending and local revocation are displayed without inferring
  phone completion. Fresh signed catalog reads follow successful or ambiguous
  writes; no UI-generated operation IDs, automatic mutation retry, duplicate
  pre-authentication or startup management operation is introduced. Actual
  installed listener enforcement, per-phone updates, distribution and consumer
  lifecycle remain open.

- [x] ABI 1.82 adds a Shared-owned pending revocation intent inside the existing
  target-signed atomic snapshot. Production FFI persists this exact request
  before Owner signing, rejects replacement intent, and removes pending state
  in the same write that installs the signed revocation. Lost first/final
  readbacks recover without replacing the request or repeating completed writes.
  The read-only pending query verifies runtime/target/SPKI/current readbacks;
  it never authenticates, signs or writes. Apple/Mac worker/coordinator adapters
  expose it without startup bootstrap. Null is not completion evidence. The
  per-phone live listener updates, cross-target distribution and consumer lifecycle
  remain open; this is not complete device removal. ABI 1.83 now supplies the
  separate signed-history UI above; legacy phone-completion cache rows remain
  intentionally empty and are not its authority source.

- [ ] Complete device revocation through real clients. Shared now has an exact
  local-target Grant revocation transaction with hardware-signature effects,
  atomic protected-state CAS/readback, stable-intent recovery after lost replies
  and signed propagation evidence. Tests prove two-phone isolation and deny
  certificate/checkpoint exchange even for a previously installed certificate.
  ABI 1.81 and the typed Apple worker/OS-callback bridge are now implemented;
  tests exercise the actual FFI using real signed software-key pairing history.
  Concrete Mac management authentication, existing-key adapters and broker
  target-state CAS wiring now exist behind an exclusive coordinator entry;
  ABI 1.82 supplies durable intent recovery; ABI 1.83 connects a signed catalog
  and local revoke/resume UI. Installed hardware
  acceptance, per-phone listener update, multi-target
  distribution and consumer state/UI remain open. See
  `docs/architecture/agenthalo-owner-grant-revocation.md`.

- [x] Wire single-active-Owner enrollment selection. ABI 1.80 owns a bounded
  consumer storage catalog, exact-CAS selection readback and revocable opaque
  storage leases. Apple wraps it with a serialized encrypted-index driver and
  lease-checked five-slot stores, preserving the unchanged legacy layout. This
  does not establish Owner/Grant authority and never deletes pairing evidence.
  Production multi-slot UI retains pairing models, awaits old-operation cleanup,
  replaces only same-catalog/same-slot leases and withdraws old peer routes.
  Late directory/picker callbacks cannot reactivate the previous Owner. Unknown
  new-layout directories block startup/allocation without deleting evidence;
  unresolved saves cannot issue fresh leases. Physical multi-Owner acceptance,
  orphan repair and changed-Grant/context recovery remain open. See
  `docs/architecture/agenthalo-consumer-enrollment-catalog.md`.

- [x] Add Shared session-relative directory/file read protocol and typed Rust
  client. The real backend resolves cwd from the actual provider session, never
  caller metadata; the authenticated transport and backend both require the
  exact session-read operation. Descriptor-relative no-follow reads reject
  parent escapes, symlink roots/children, hardlinks and special files. Directory
  capacity is an error rather than a partial success; file preview bytes have
  bounded base64, digest, size and truncation checks. The real Unix-socket typed
  client fixture reads only its temporary workspace. See
  `docs/architecture/agenthalo-workspace-files.md` for the full boundary.
  ABI 1.77 now adds native local/authenticated reads and serving-backend absolute
  reference resolution. Mac project-files and conversation-file UI use those
  readers, never consumer-local URLs; Shared file routes reject unknown local
  IDs as well as stale/foreign remote IDs. Native rendering bounds and UI epochs
  prevent oversized/animated/truncated-binary rendering and late display updates.
  Harmony transport/NAPI reads, directory/path browser and native renderer UI
  are wired. Installed file acceptance remains unperformed on both clients.

- [x] ABI 1.78 replaces the Mac Git top-bar placeholder with Shared-owned,
  session-scoped history and patch reads, exact native local/authenticated
  adapters and history/patch UI. Reads are anchored to the serving session cwd;
  pages bind to HEAD and patches compare merges against their first parent.
  Unsafe selectors, stale routes, missing objects and excess output fail closed.
  Replaced sessions/Owners/clients reject late results. See
  `docs/architecture/agenthalo-workspace-git.md`. Installed Git acceptance remains
  open. Harmony now exposes the same history/patch reads through its per-device
  authenticated transport and conversation-header browser, with opaque cursor
  navigation, retry/HEAD refresh and lifecycle-fenced display updates.

- [x] ABI 1.76 moves Mac session aggregation, source failures, session identity
  validation/namespacing, created/resumed response projection, live port routing
  and persistent-cache scope decisions into Shared. Native provider handles
  select the reads, not JSON capability claims. Device reads execute independently;
  a failed local provider cannot discard successful remote sessions. Malformed
  responses keep only stale display and explicit errors; exact empty successes
  clear only their source. Unknown/foreign remote IDs never fall back to local.
  Created/resumed responses retain their routes against an older pending read.
  Every replacement revokes old handles; identical explicit scopes retain stale
  display, while changed/unknown/removed scopes lose it. Mac presentation epochs
  fence late results from a replaced workspace. The UI displays per-source
  failures and disables send/interrupt on stale session rows.
- [x] Shared exports version-2 cache payloads and opaque storage keys bound to
  device, Owner and Provider. Restored rows never install live routes, even if
  payload metadata claims reachability. The Mac adapter only encrypts/decrypts
  bounded bytes and reads/writes safe filenames with private permissions. Legacy
  unscoped cache files are not imported or deleted; no live state is cleared.
  The catalog supports more than 200 sessions with explicit limits (20,000 rows
  and 4 MiB per source/provider, 16 MiB aggregate rows, bounded live-route data,
  16 source ports and 128 providers per port). Capacity errors are not successful
  truncation. Cache retention/garbage-collection lifecycle remains a separate
  open item, not an authorization fallback.

- [x] ABI 1.75 moves the multi-device provider catalog, merge, per-source
  freshness/capabilities and session-read source selection into Shared. Exact
  bounded read tokens reject stale results. Failed sources retain display only;
  valid empty reads clear only their own rows. Rebinding revokes old handles,
  retaining stale display only for explicitly identical endpoint/owner scopes.
  Mac discovery includes remote-only providers, skips unsupported local session
  reads, and renders source errors with explicit refresh. Create/send/interrupt
  controls use each selected device's capabilities rather than a merged union.
  Dismissed create dialogs cannot overwrite the selected composer's options.
  The provider display handle never grants connection or mutation authority.
  ABI 1.76 above extends this boundary to session ID namespacing, routing,
  encrypted-cache scope and available-provider session-read failure isolation.
- [x] Apple XCFramework generation embeds each static archive's SHA-256 in its
  generated imported header. This makes Rust-only implementation changes
  invalidate SwiftPM's native module and relink consumers without changing the
  public ABI version or tracked source header. The unchanged offline-cache test
  reproduced a stale linked archive and passed after relinking; rebuilding with
  the generated header independently triggered recompilation and linking.

- [x] Wire the production Mac attachment UI to five retained encrypted upload
  controllers per authenticated peer. Multidevice routing unwraps the existing
  selected session for the upload and send ports; file paths never enter a
  remote prompt. User selections carry the exact Shared operation ID, and every
  send captures native proofs through ABI 1.74 before invoking the actual
  retained mutation controller. Shared still validates context, completed file,
  replacement and every recovery action. Slot/progress projections only render
  UI and protect against stale clicks; they cannot authorize sending.
  Restore/reselect/query/save/exact retry/cancel-proven-undelivered/retire UI
  preserves protected state. Picker callbacks are nonblocking and scoped URLs
  stay available through the bounded read. Cross-session/context stale pickers
  and late results cannot alter a newer draft. Clearing a draft never deletes
  uploads. Recovery rows remain visible without a selected session.
- [x] Serialize each Mac peer's file work, proof capture, mutation and rebind
  until volatile cleanup has finished. Suspension cancels subsequent upload
  effects after file reads/CAS, while retaining signed requests and unsaved
  outcomes. The same upload controllers rebind to the new native client and
  restore without replay. Busy-release connection retries now include uploads,
  not only mutations. Other peers remain independent.
- [x] Make Shared's save-only action retain an already sealed unsaved outcome
  idempotently. It reuses the exact candidate and operation without signing or
  dispatching again; exact durable readback remains required. This fixes the
  native PersistOutcome boundary exposed by the Mac recovery button for both
  attachment chunks and ordinary remote mutations. ABI remains 1.74.

- [x] ABI 1.74 binds an attachment selection to the exact Shared upload
  operation, not just its reusable platform slot. The native worker checks the
  selected operation while holding the upload lock, then validates completion,
  exact durable state, current target and signer before returning an independent
  opaque proof. Empty/stale selections fail without creating a send. Harmony
  UI retains the displayed operation ID and production NAPI operation 23 carries
  it to Shared. Partial proof capture is disposed if a later selection fails.
  Apple exposes the same checked snapshot through its bridge/upload controller;
  the subsequent Mac handoff above connects it to the production composer.

- [x] ABI 1.73 and production Mac authenticated peer handoff. The workspace
  consumes the native consumer directory and no longer starts the bundled
  legacy CLI/federation peer manager as an access fallback. Display service IDs
  select real opaque targets; only an existing consumer signing key can prepare
  certificates. Each device retains its certificate/mutation controllers through
  directory failures. Fresh directory observations drive independent connections,
  native health checks and exact mutation rebind; one device failure does not
  block another. Old results are rejected by lifecycle epochs. Busy mutation
  release may resume a waiting connection; other failures wait for another Shared
  directory observation or explicit user refresh. Connection errors are visible
  even when no workspace has connected yet.
- [x] Shared certificate lease rebinding preserves pending signatures, exact
  requests, known responses and storage predecessors. It requires unchanged
  canonical Grant, target/SPKI and authority head and a live replacement lease.
  Failed validation leaves the original plan unchanged. It cannot renew an
  expired Grant or migrate a changed authority context. Per-client cancellation
  and per-peer OS TCP shutdown do not invalidate the whole directory or other
  peers. Parent handles stay pinned across rebinding and blocking work.
- [x] Wire Mac desktop preview/inbox reads and create/send/resume/interrupt/
  approval/answer intents to the authenticated Shared client and encrypted
  mutation journal. Shared supplies durable native result envelopes; the App
  does not reinterpret a mobile result marker as a desktop session. Typed UI
  input carries approval mode and sandbox mode separately and no external
  operation ID. Sending never implicitly resumes a remote session. Recovery
  buttons retain the exact displayed operation and work independently of whether
  creation has produced a selectable session. Late create/send completion does
  not clear a newer draft or override a changed selection. The subsequent upload
  handoff above adds remote attachments without local path serialization.

- [x] ABI 1.72 adds Shared consumer protected-slot inventory and typed empty
  mailbox scheduling. Inventory selects empty/checkpoint/composite restoration
  and rejects orphan slots; it does not authenticate bytes. Actual checkpoint
  current-key proof and composite verification remain mandatory. No record is
  silently discarded, and failed composite verification cannot install a partial
  checkpoint that downgrades the next recovery attempt. Only verified open or
  redeemed empty mailboxes schedule another poll; terminal observations grant
  neither ACK authority nor permission to re-pair.
- [x] Add the actual Mac consumer enrollment actor, independent encrypted
  checkpoint/journal/first-pair/Grant/activation stores, and Join owner QR-image
  sheet in device management. Shared owns candidate validation, SAS confirmation,
  proof/confirmation requests, completion actions, exact readback and ACK.
  Mac supplies hardware signing, explicit confirmation authentication, bounded
  HTTPS, image decoding and lifecycle effects. A failed first checkpoint write
  retains the same signed artifact; retries do not authenticate again. Empty
  startup loads no key and requests no authentication. Existing startup recovery
  uses only the existing key and never creates a replacement. Confirmed records
  survive sheet closure. App shutdown cancels tasks/timers and rejects late
  results. QR images are bounded, single-image/single-code and untrusted until
  Shared verifies them. The picker uses a nonblocking AppKit callback.
  The ACK-gated directory starts only after native durable completion; its
  readiness stays separate from pairing success. This owns the enrollment and
  directory in the production workspace lifecycle. ABI 1.73 connects its
  directory-ready port to the authenticated peer manager and compose path.
  Multi-owner enrollment management and physical
  acceptance remain open. Pairing transport errors retain state for
  manual Resume; automatic transient-failure retry is not implemented here.

- [x] Add the Apple directory-recovery bridge and Mac ACK-gated directory
  lifecycle coordinator. The standard constructor accepts only the opaque
  completion plan; Shared rejects an unACKed completion before any HTTP work.
  Swift executes the exact native request, passes every bounded HTTP status/body
  to Shared, and reports only classified OS transport failures against the exact
  pending request. ABI 1.67 supplies every refresh/retry delay; a nil instruction
  stops automatic work. Failed reads preserve display only, mark it unverified,
  and cannot mint targets or connection authority from cached JSON.
  The ephemeral HTTPS adapter disables cookies/credential storage/cache and
  redirects, bounds headers/outbound bytes, reads at most 4 MiB plus one overflow
  byte, and cancels its underlying task on every exit. Empty HTTP errors remain
  real status/body observations rather than fabricated successes.
  Stop cancels in-flight HTTP and timer work and invalidates native derived
  connections; late results cannot update the display. Exact pending cleanup is
  awaited before releasing the busy gate. A manual refresh supersedes an old
  timer, which waits for that operation and uses its new Shared delay without
  duplicate or early retry. The timer does not keep an otherwise released owner
  alive. ABI 1.72 supplies the consumer pairing UI and durable
  enrollment store; ABI 1.73 supplies the production peer-manager switch-over.

- [x] ABI 1.71 retains a whole-file upload slot across successive files. Shared
  requires the exact durable terminal file, actual saved snapshot and unchanged
  authorization/key context before replacing it; partial progress, unsaved known
  outcomes, invalid content or failed preflight preserve the original plan.
  Shared also captures a completed file into an independently owned upload
  snapshot for sending. Source-slot replacement/retirement cannot change that
  captured file. A pending new file cannot export the older completed disk
  snapshot. No file URI, display metadata or platform phase grants send authority.
  Harmony uses native operations 21/23 and disposes temporary send proofs only
  while still owned. Upload-finally cleanup is awaited before releasing the busy
  gate; background cleanup also blocks replacement/reconnect until it returns.
- [x] Add the Mac native upload effect coordinator and bounded OS file reader.
  The reader accepts only local regular files of 1 byte through 20 MiB, rejects
  final-component symlinks and special files before reading, bounds each chunk,
  checks cancellation and size/time changes, and closes the descriptor on all
  exits. Shared owns hashes, chunking, operation IDs, completion and recovery.
  The coordinator restores without dispatch, performs exact encrypted CAS and
  readback, retains known unsaved outcomes, requests original bytes for partial
  recovery and awaits volatile cleanup before another operation. Its standard
  factory uses the real native bridge; ABI 1.72 supplies enrollment/lifecycle,
  ABI 1.73 supplies peer-manager/compose wiring; the handoff above adds attachment UI.

- [x] ABI 1.70 owns remote mutation successor transitions in Shared. The same
  retained native plan must be durably terminal and match the actual saved
  snapshot and exact authorization/key context. Optional attachment sends require
  completed opaque upload proofs. Invalid input, signing failure, stale storage,
  unfinished/retired uploads or failed authenticated preflight preserve the old
  plan; no fresh request replaces an unsaved outcome. Harmony invokes native op
  20 rather than inspecting phases to dispose/recreate the mutation. Both Apple
  and Harmony pin every supplied handle throughout the worker call. The native
  boundary bounds intent, upload count and async lifetime before dispatch.
- [x] Add Mac encrypted remote state and certificate/mutation effect controllers.
  Store addresses are length-delimited Owner/device/key labels with distinct
  certificate, mutation and five upload slots, not authorization proofs. Exact
  ciphertext CAS, flush/readback and whole-byte lost-reply recovery reuse the
  established OS primitive, with separate Secure Enclave key domains. The
  certificate driver requires a real ACK-gated directory and existing consumer
  signer, persists before exchange/client construction, retains failed exchanges,
  and renews only on Shared's typed phase. Stop invalidates directory work without
  clearing state. The mutation driver restores signed history, uses native next,
  retains failed persistence/signing outcomes, and never generates operation IDs
  or automatically resumes/replays. Cancellation is checked after awaited phase
  reads and before effects, not only at loop entry. These controllers have real
  native standard factories. ABI 1.72 supplies enrollment/lifecycle, but the
  ABI 1.73 supplies production peer-manager/compose handoff; the above upload UI now uses these slots.

- [x] Add the Apple native bridge needed for the authorized remote-client
  migration, without constructing trust from federation/display metadata.
  `AgentHaloPairingOwnerDirectorySession.selectRemoteTarget` is the only public
  target constructor. Opaque children retain their parent directory, every
  worker pins its handles, and explicit directory invalidation remains available
  to cancel derived work. Certificate exchange uses a fixed existing hardware
  signer; candidate state and actual readback stay opaque. Typed reads, mutation
  phases/actions and attachment-upload proofs use the existing Shared ABI.
  Reconnect updates the native binding and Swift parent ownership together,
  preserving the previous anchor on failure. No phase/HTTP/display result grants
  replay permission. Apps must still retain unresolved plans and drive durable
  storage before switching their production path.
  `AgentHaloSystemRemoteTCP` supplies only DNS and plain OS TCP (TLS remains in
  Rust), with 64 live streams, non-reused IDs, 16 in-flight resolver jobs, bounded
  buffers and Shared-supplied deadlines. Close wakes pending work without
  waiting for system DNS/read completion; late callbacks own no borrowed native
  pointers. Concrete loopback tests cover DNS, read/write/EOF, read timeout,
  cancellation, capacity limits and ID non-reuse. Subsequent entries supply Mac
  enrollment, persistence, compose and attachment UI coordinators; real Relay gates remain.

- [x] Retain Mac pending-interaction display after failed reads. Network and
  decoding failures no longer fabricate an empty inbox; only a successfully
  decoded empty response clears cards. A dedicated inline warning remains
  visible even when no earlier card exists, with an explicit refresh button.
  Previously loaded cards are read-only until refresh succeeds. This is UI
  freshness, not cached authorization: actual responses still use the existing
  Shared local journal/backend validation. Concurrent reads accept only the
  latest result; backend/client replacement and shutdown invalidate old display
  work. Approval/question buttons capture the full displayed card, reject a
  changed selection/request and suppress overlapping clicks. Question drafts
  are keyed by the entire card, not a cross-device-colliding request ID.
  A late action completion after shutdown cannot start another inbox read.
  Missing/null/wrong-shaped envelope fields are errors, not successful empties.
  ABI 1.68 replaces Swift inbox merging with the opaque Shared reducer below;
  no inbox projection creates a remote action route. This is not physical
  partial-outage or changed-Owner acceptance.
- [x] ABI 1.68 Shared per-device inbox display aggregation and Mac native/UI
  connection. Shared accepts up to sixteen fixed sources, each with at most
  512 rows and a 512 KiB response. It emits exact per-source read effects, owns
  monotonic read tokens, ignores late/foreign/consumed results and preserves only
  the failing source's previous rows as stale. Valid empty replies clear only
  their own source. Invalid envelopes, duplicate/unscoped identities, ambiguous
  legacy display namespaces, malformed question shapes and exceeded bounds
  never become empty success. Claimed source/freshness fields in response JSON
  are ignored and regenerated by Shared. Swift executes the bounded effects
  through existing source ports and renders the result; it no longer merges
  inbox JSON or constructs routes from interactions. Successfully refreshed
  device cards remain usable while unavailable-device cards are read-only.
  Freshness-only changes preserve entered answers, but buttons still compare
  the exact current displayed card. Neither this display handle nor its JSON
  grants authorization or allows mutation replay. ABI 1.69 below adds safe
  cross-client display retention; the actual authorized remote write migration
  and real-device acceptance remain open.
- [x] ABI 1.69 scoped inbox retention across native source-port replacement.
  Shared creates a new reader lease for each replacement and revokes every
  entry point on the old lease, including starting new reads, applying late
  responses, exporting display or rebinding again. Comparison and retention
  remain Rust-owned: only the same device with the same explicit Owner label,
  or local device with the same explicit endpoint, preserves rows; they are
  immediately stale and require another successful read. Unknown scopes,
  changed Owner/endpoint and removed devices do not retain display rows. These
  labels are presentation scopes, never identity/Grant proof. Invalid replacement
  configuration leaves the original generation and pending reads intact.
  Empty configuration closes all readers. Swift owns one serialized handle
  container across workspace clients, including the production local-only
  client. Failed binding cannot fall back to the container's older display.
  Root UI consumes the new lease's retained read-only projection immediately
  and closes its owned readers on shutdown. Existing generation checks still
  reject data already exported before replacement. No durable journals,
  credentials, pairing state or attachment data are deleted by this display
  lifecycle. This closes the source wiring gap, not physical reconnect or
  authorized remote operation acceptance.
- [x] Private-UDS durable mutation foundation for Mac local compose/control.
  Backend startup installs a distinct exclusively owned ledger keyed by the
  exact socket path under the existing private AgentHalo state directory.
  Only a closed list of typed create/send/resume/interrupt/approval/question
  requests can enter `/local_mutations/dispatch`, `/status` or `/cancel`;
  these routes are not exposed by the authenticated remote Grant router.
  Dispatch requires matching stable operation headers and stores the exact raw
  request fingerprint before Provider execution. Provider operation IDs are
  namespaced by the UDS endpoint, separate from remote peers. Eight bounded
  workers retain late results after the eight-second wait expires; a saved
  completion is returned without a second Provider call. Queries can recover
  an existing pending/unknown create/send only from the exact opaque Provider
  receipt. Missing history/receipts and changed request bytes do not authorize
  replay or cancellation. Cancellation fences only an existing pre-dispatch
  retryable failure; an already completed record remains completed, not cancelled.
  The Shared typed client uses one bounded encoder for dispatch/query/cancel,
  validates response identity, never generates a replacement ID and disables
  transport automatic replay. External attachment tokens and resume-fork are
  denied. A real UDS fixture discards the first reply, queries the completion
  and resubmits the exact request with one total Provider delivery.
- [x] ABI 1.65 opaque Shared local journal/coordinator and worker-only Swift
  facade. The shared durability engine now has distinct typed local and remote
  contexts: local intent binds an exact private UDS path and hardware key,
  without constructing a remote Grant. Local signatures use a separate domain;
  the existing remote JSON field order, signing domain and journal shape remain
  unchanged. Shared generates the operation ID, requires exact signed request
  readback before one dispatch, then requires exact signed outcome readback
  before exporting completion. Pending restoration is uncertain, never dispatch
  permission. Known but unsaved results survive signature failure in the retained
  handle and cannot be queried/replayed/cancelled in place of saving. Explicit
  local retry first queries the exact backend ledger; only a `retryable`
  pre-dispatch failure permits another same-ID checkpoint. Completed queries
  persist the original result without sending; missing/pending/unknown history
  cannot authorize a write. Signed cancellation requires the existing backend
  non-delivery fence, not deletion of local history. The C handle retains its
  fixed hardware callback and local transport. Swift performs all calls off
  the main actor, pins the handle during work and exposes only opaque storage
  bytes, typed phases/actions and display-only/result projections. It never
  generates IDs or falls back to legacy sending.
- [x] Connect Mac's local compose/control and recovery UI to exact encrypted OS
  persistence/readback. ABI 1.66 adds a successor transition on the retained
  Shared handle: only a terminal plan and its exact current OS snapshot can
  accept new input. Mac stores opaque signed bytes in an independent encrypted
  endpoint-hashed slot, with exact CAS and actual authenticated readback; it
  does not reuse or clear pairing/owner-registration data. A lost write reply
  is recovered only by reading back the exact candidate. Empty startup opens
  no signing key; restored history requires the existing fixed signing key.
  Local create/send/resume/interrupt/approval/answers use this driver in the
  production model, with no legacy fallback on failure. Resume is separately
  confirmed for the exact selected session and never sends the draft. Pending
  startup only restores; known unsaved outcomes stay in the retained handle.
  The UI exposes typed save-only, query, exact reconciliation and non-delivery
  cancellation actions. Duplicate clicks cannot create another plan; late
  sends preserve edited drafts and late creation preserves changed selection.
  Explicit recovery preserves the current draft and selection. ABI 1.73 extends
  retained remote writes and recovery to the authenticated production peer port;
  this is still not installed/hardware-backed recovery acceptance.
- [x] Independent multi-device Grant and TLS histories in Shared; exact atomic
  persistence, legacy migration, expired-subject isolation and per-TLS-peer
  Grant selection (`fd7db8ef`). Not installed at this checkpoint.
- [x] Bounded native-session pagination for Codex and Claude; Shared client
  follows continuations. The response's `total` counts the filtered catalog,
  not files on disk or the number of rows in the current page.
- [x] Codex app-server diagnostic follows `nextCursor`, with bounded requests
  and cycle detection, instead of treating one 200-row page as a total.
- [x] Shared owner certificate plan accepts both verified legacy and multi-device
  stores. It selects the requesting phone's exact current Grant and TLS history;
  installing one certificate preserves other devices, and lost-response retry
  reads the exact durable evidence after restart.
- [x] Shared consumer certificate coordinator: persist the exact hardware-signed
  request before transmission, reconcile a lost response, verify both
  authority-signed certificates, persist/read back complete histories and only
  then export opaque credentials. Never recreate a request from UI metadata.
  Initial issuance, exact pending-request restart, dual-certificate quorum/
  history validation, whole-state signature/readback, continuous same-key
  renewal and directory invalidation are implemented in Shared. A request-bound,
  30-second quorum-signed checkpoint now reconciles expired non-installation
  and multi-generation server catch-up; rollback/forks/revocation stay denied,
  and the whole recovered state requires hardware signature and exact readback.
  This is a Shared protocol gate, not production or real-device acceptance.
- [x] Owner certificate exchange is connected in source to the multi-device
  target store and Mac production startup. A bounded MCE2 lane reuses the
  existing raw Relay carrier and loopback listener, without changing Relay or
  exposing another port. Only the Shared driver handles certificate requests;
  product dispatch still requires inner mutual TLS/MST2. The Mac transaction
  lock covers fresh protected-state reads, signing, CAS/readback and response
  flush. Before non-install confirmation, an exact hardware-signed outer-state
  CAS fence invalidates any late write from a timed-out OS call; it adds no TLS
  certificate or issuance history. Changed target state/expiry refreshes the
  listener and admission set.
  Tests cover lost readback, exact retry without duplicate issuance, expired
  non-installation with a late-CAS fence, multi-device preservation, frame isolation,
  short I/O and cancellation. This code has not been installed or exercised
  against the real hardware broker/Relay/phone.
- [x] Publish/withdraw the AgentHalo service from the current Shared listener
  lease, preserving the existing owner registration and publication history.
  Shared now accepts a non-serializable witness bound to the actual loopback
  socket and exact durable owner runtime. The backend exposes reconciliation
  only through its live listener/carrier ownership, and invalidates witnesses
  on stop or worker/carrier unwinding. Registration retains the original
  credential, identity and signed history; only AgentHalo availability changes.
  New advertisements last at most 60 seconds and never outlive the listener
  lease. Restore defaults to withdrawal, and superseded signed PUTs remain
  recoverable without signing different content at the same revision.
  The Mac supervisor now drives Shared persist/sign/HTTP effects against a
  canonical broker CAS slot shared with the App. App startup imports exact
  existing legacy bytes only when the canonical slot is absent, without
  preparing authority or requesting authentication; errors retain the original
  data and fail closed. No legacy file is deleted. Shared validates the original
  identity, credential and signed history before resuming. Each effect checks
  the exact current owner runtime. HTTPS uses a bounded OS adapter with secrets
  on stdin only; ambiguous PUTs return to Shared readback reconciliation.
  Stop attempts withdrawal; crash/forced exit relies on the signed TTL. This is
  source implementation, not proof of production publication or withdrawal.
  ABI 1.62 adds a separate opaque Shared import scheduler. Only OS-unavailable
  and timeout results retry, with 1/2/4/8/16/30-second capped backoff and one
  in-flight token; rejection, invalid state and cancellation stop. Swift executes
  exact OS reads/CAS and sleeps only for the Shared delay. Every retry starts
  with a fresh canonical broker read, never cached legacy overwrite. An absent
  registration finishes without creating authority. Quit cancels the retained
  import task; startup does not request local authentication or delete old data.
- [x] Opaque paired-directory-to-authenticated-read-client ABI with bounded
  DNS/TCP/HUKS callbacks and cancellation on backgrounding or trust changes.
  ABI 1.55 provides consumer certificate create/phase/field/apply/exchange, an opaque
  client factory, health/providers/options and paginated session reads. The
  factory derives WSS endpoint, inner TLS/MST2, hardware signer and cancellation
  from verified state. The new exchange call performs bounded WSS/MCE2 I/O from
  the opaque target and retains the same persisted request after failure.
  NAPI now runs blocking calls off the JS thread, with eight bounded workers,
  pinned input handles and late-result rejection after disposal. HarmonyOS
  drives typed certificate phases through HUKS CAS/readback, then performs
  authenticated workspace and options reads. Backgrounding invalidates the
  Shared directory and cancels OS I/O. Construction is not handshake evidence.
- [x] Replace HarmonyOS workspace and provider-options placeholder rejections.
  Shared performs health checks, provider fan-out, pagination and display DTO
  conversion. UI progress updates do not recursively trigger workspace reads.
- [x] Replace HarmonyOS runtime and conversation-preview placeholder rejections.
  A new worker-only ABI validates selected device/provider/native session and
  accepts no endpoint or authorization fields. Shared projects native runtime,
  usage and bounded transcript/tool content; wrong response session identities
  are rejected. Scope for inbox reads comes from the opaque verified Grant.
  Inbox updates are independent from the transcript signature; a failed inbox
  read preserves existing cards with an explicit error, not a fabricated empty
  inbox. Production UI polling uses these typed reads. This does not complete
  reconnect/renewal or establish live-phone acceptance.
- [x] Connect HarmonyOS create, text send and explicit native-session control
  acquisition to Shared's signed mutation journal (ABI 1.58). Creation takes
  an explicit device, title and absolute remote working directory; its settings
  are separate from the selected conversation. Shared validates provider
  capabilities before creating a journal and requires exact live control before
  sending. Resume requires a separate UI confirmation and never sends a prompt
  or gains automatic replay after restart. Only a durably completed creation
  exports a device-bound session DTO; creation recovery can consume this result
  without creating another operation. Stored native rows are not projected as
  controlled logical sessions. Late responses cannot overwrite another device's
  view or erase new composer text. These are source/fixture results, not physical
  create/send/resume acceptance.
- [ ] Complete attachment upload and provider delivery. Shared now defines a
  bounded `/attachments/chunk` protocol and typed client upload helper; the
  authenticated backend namespaces upload tokens by the exact peer/Owner/Grant
  context and requires session-write scope. Codex accepts uploads only for a
  currently controlled session. Private storage checks exact chunk bytes,
  offset, metadata and whole-file SHA-256 before exposing a file to `turn/start`.
  Display names never become filesystem paths. Limits are 20 MiB per file,
  64 KiB per chunk, five attachments per prompt, and 128 uploads / 512 MiB
  reserved storage. Lost replies stop without automatic replay; explicit
  same-ID retries retain the exact content and remain subject to ledger fences.
  Upload quota does not delete earlier evidence. New never-referenced uploads
  now have a seven-day retention lease; later upload writes collect expired
  partial/complete content under the exclusive store lock. Before unlinking,
  Shared durably writes a permanent identity tombstone and reconfirms both
  file and directory fsync, including after a lost reply. Metadata and operation
  receipts remain; old IDs cannot recreate content or be sent after restart.
  Active quotas remain 128 uploads / 512 MiB; at most 4096 identity records
  are retained, with explicit failure rather than forgetting history when full.
  Paths are validated/private/no-follow; corrupt lifecycle evidence blocks
  cleanup, clock rollback never expires a lease, and missing legacy leases are
  not synthesized from mtime. Referenced content is pinned durably before the
  actual Codex RPC receives its path, and remains pinned even if that RPC fails.
  Referenced and legacy content are not automatically removed: conversation-aware
  retirement/migration is still incomplete.
  A retired attachment is rejected before Provider dispatch with a typed 410
  result, saved by the existing exact-operation backend ledger. A bare HTTP
  error still leaves the phone uncertain. Only an authenticated exact durable
  query for an attachment upload/send can produce Shared's typed
  `attachment_expired` outcome. It requires hardware signing and exact readback
  before new work, preserves unsaved results, exposes a reselection message,
  and never reports the old operation as successful. Foreign IDs, missing
  non-delivery fences, unknown records and non-attachment operations cannot
  use this outcome. Matching updated clients are required for this recovery;
  older clients remain fail-closed, not automatically replayable.
  ABI 1.63 adds an opaque Shared file coordinator: Shared computes the full-file
  hash and upload ID, persists one signed chunk request/outcome at a time, and
  advances only after exact readback. Restart preserves uncertain chunks and
  requires the original size/hash-matching file before continuing a completed
  partial upload. Known unsaved outcomes cannot be replayed or replaced. Only
  completed upload handles in the exact target/Owner/Grant/key/provider/session
  context can construct a send; generic caller-supplied tokens/URIs are rejected.
  Native workers pin all upload proof handles and reconnect only to an identical
  context. Five independent HUKS slots per device preserve existing certificate
  and mutation slot names and use immutable queued buffers with exact CAS/readback.
  ABI 1.64 connects the picker-authorized read-only file descriptor to the upload
  worker. Reads are bounded, handle short reads, check file changes, and close
  the descriptor on failure/cancellation. Shared alone computes hashes/chunks,
  validates same-file reselection and constructs the final send from completed
  opaque handles. URI strings and serialized upload tokens never reach a send.
  The phone drives exact per-slot HUKS CAS/readback and retains known unsaved
  outcomes across suspension/reconnect, while releasing volatile file content.
  The composer shows per-file progress and explicit add-to-message, continue,
  same-request reconciliation, outcome query, result persistence and original
  file reselection. Unknown outcomes cannot be replaced by another file.
  Shared local retirement requires a durable known chunk outcome, signs and
  persists the decision, and invalidates that plan's send proof; it neither
  claims non-delivery nor deletes server data. Partial known uploads can now
  release their local slot explicitly. Signing failures retain retirement and
  cannot revive the old proof. Generic external attachment tokens remain denied.
  The OS preference adapter now frames sealed records into 4096-character parts
  under a bounded header: a single string cannot exceed the SDK's 8192-character
  limit. It reads/authenticates the actual inactive candidate before switching
  the active pointer. Existing inline records remain readable; HUKS and Shared
  still validate the whole state. Both ordinary and paired atomic slots use this
  storage framing; it introduces no authorization or anti-rollback authority.
  Referenced/legacy server cleanup and actual physical provider consumption
  remain incomplete.
  The legacy multipart `/upload` helper remains
  unsupported. A verified path delivered to the Codex RPC is not proof that a
  real provider successfully read every image/PDF/file type.
- [x] Connect HarmonyOS interrupt, approval and structured-question actions to
  the client-bound Shared journal. The adapter drives only Rust phases through
  independent HUKS mutation slots and serialized exact CAS/readback. Queued
  buffers are copied; a lost response to an already committed CAS reads back the
  same candidate rather than overwriting it. Certificate slot names are unchanged.
  Rust ABI 1.57 supplies redacted status/recovery projections. The real page
  displays the retained operation ID, not-yet-sent, unsaved-outcome and unknown
  states. Explicit recovery can save a known result without a second network
  call; a restored pending interaction never gains replay permission. Once an
  unknown outcome is durably stored, it is not offered for repeated sealing.
  New-action buttons respect the Shared display state, while the opaque plan
  independently enforces every transition. Async results cannot remove an
  identically numbered card on a different selected device/session. Normal
  workspace refresh retains existing clients and known unsaved outcomes; Shared
  still checks current target validity for every read. Removed/backgrounded
  clients are cancelled and their journal retained. These are source/fixture
  results, not live phone or complete ambiguous-interaction recovery acceptance.
- [x] Shared hardware-signed mutation journal and opaque ABI/NAPI foundation
  (ABI 1.56). Request and outcome each require exact persistence/readback;
  restoring pending history is uncertain, never permission to dispatch. One
  unresolved operation blocks replacement with a new ID. Create/send support
  explicit same-ID reconciliation through the backend ledger; approval,
  question and interrupt cannot gain replay permission after restart. A known
  outcome whose signing failed can retry sealing without another network call.
  Selected device, owner, Grant, hardware key and response identities are bound
  and checked in Rust. The client handle retains its own signer/transport;
  platform input contains no route or credentials. The preceding interaction
  entry and ABI 1.58 compose entry connect this foundation to HUKS/UI.
- [x] Independent approval and sandbox fields in Shared create/send requests.
  Codex start/send RPCs preserve the selected read-only/workspace sandbox while
  mapping the approval reviewer. Conflicting legacy aliases or full-access
  combinations fail before provider delivery rather than widening permissions.
  Existing ABI callers retain legacy alias behavior. Mobile validation and
  backend execution now share one resolver; authenticated runtime projects the
  actual approval reviewer, leaving unknown combinations unset. Sending without
  an explicit approval selection preserves the thread's existing policy/reviewer.
  Granular per-provider capabilities require readable state, current Grant scope
  and the backend's advertised operation. Ordinary provider outages can be
  isolated, but trust/protocol failures cannot become partial-success UI.
- [x] Propagate verified target identity into remote session reads; keep
  equal native session IDs on different devices separate.
  The new Shared ABI session read replaces server-supplied device IDs with the
  authenticated selected target. Native clients and option reads are scoped by
  selected device; removed/backgrounded selections release their handles.
  Typed mutations also use the selected device's opaque client. Multi-owner
  switching, migration and revocation still require production lifecycle work
  and physical acceptance.
- [ ] Reconnect, certificate renewal, one-device revocation and ambiguous
  mutation recovery through the production connector.
  ABI 1.61 now preserves in-memory mutation evidence when the phone backgrounds,
  a target leaves the directory, or a workspace read fails. Shared-scheduled
  directory refresh reconstructs the connection from the exact certificate
  history; Shared requests renewal inside the existing allowed overlap window.
  A worker-only opaque rebind requires identical target, Owner, canonical Grant
  and hardware key, with fresh target/scope checks. It preserves the operation
  ID, phase, signed candidate and known unsaved outcome; it never dispatches or
  turns restored pending state into permission to replay. NAPI pins both handles,
  and every adapter call captures its connection epoch so a late old read cannot
  land after foreground/reconnect. Changed Owner/Grant contexts remain blocked
  with retained evidence, not silently migrated. Expired-Grant directory recovery,
  cross-context migration, retained-record lifecycle bounds, concurrent revocation
  and real certificate/OS lifecycle acceptance are still open.
- [x] ABI 1.67 transient owner-directory recovery while the exact paired Grant
  remains valid. Shared consumes only the exact pending request's typed OS
  failure; unavailable/timeout and HTTP 408/429/500/502/503/504 schedule bounded
  1/2/4/8/16/30-second backoff. Successful signed verification resets the count.
  Empty HTTP error bodies also reach the same Shared policy. Malformed/signature/
  rollback failures, authentication/protocol errors and Grant expiry stop retry.
  Failed reads invalidate old connection leases but preserve the signed revision
  and rotation history. An expired snapshot may recover with a fresh signed
  revision while the Grant is still valid; it never revives an old target.
  Harmony only maps documented OS error codes, reports the exact opaque request,
  and executes Shared's delay. Foreground/generation checks reject late failures;
  background cancellation clears scheduling. UI distinguishes scheduled retry
  from stopped retry, retains pairing and pending operations, and does not direct
  ordinary network failures toward resetting pairing. Fixtures verify no new
  authentication, durable completion rewrite, directory recreation or data clear.
  This does not renew an expired Grant, resolve a changed Owner, replay mutations,
  or prove installed-device offline recovery.
- [ ] Complete non-replayable mutation outcome reconciliation (especially
  resume/approval/question/interrupt after a lost reply or backend restart).
  ABI 1.59 adds exact authenticated outcome queries for all journal intents:
  dispatch and query share one typed request encoder, and every non-replayable
  action now sends the journal's operation ID (previously these four client
  methods omitted it and the remote backend rejected them). Query endpoints
  never dispatch providers and require the original operation's scope, opaque
  peer/owner/Grant/target namespace and exact request fingerprint. They do not
  depend on an already-consumed interaction remaining in the live inbox.
  The original backend worker durably saves its response even after its waiter
  times out. Late completion survives ledger reopen and cannot overwrite an
  already completed or retryable record. HarmonyOS exposes a separate
  no-replay query button; obtaining success still requires HUKS signing and
  exact local readback. Unknown/missing/error records remain unresolved, not
  fabricated successes. ABI 1.60 adds an explicit signed cancellation request
  for an existing backend `retryable` record (authoritative pre-delivery
  failure). Its exact intent is persisted before network delivery; the server
  atomically fences the original operation ID, then the phone signs/persists
  the confirmed cancellation before allowing new work. Cancellation failure
  cannot prove the original was undelivered. A cancelled ID cannot begin again,
  receive a late worker result, or be revived on restart; repeated cancellation
  reads the same outcome. An OS advisory file lock, retained by worker clones,
  prevents two current backend generations opening independent stale ledgers.
  Missing, pending and unknown records cannot be cancelled this way. Remaining
  work includes attempts with no durable provider receipt, absent/old un-ID'd
  attempts, provider-level errors not represented by a retryable outer record,
  and Grant/provider-generation changes. Exact Codex create/send receipts can
  now repair an existing pending/unknown outer record after restart without
  acquiring thread ownership or calling Codex. The provider receives the same
  authenticated namespaced operation ID used by the outer ledger; both paths
  use one encoder, with typed validation before JSON normalization. Recovery
  requires the exact ID, operation kind and request bytes in an opaque completed
  ledger receipt. It never falls back to legacy raw client IDs, recovers an
  absent outer record, or consults a provider for cancellation. This item is
  deliberately not complete.
- [ ] Complete the mobile Computer Use / Locked Use control surface and its
  Shared-authorized execution/OS-permission path. Scope definitions alone are
  not a working remote-control feature. Keep unsupported actions unavailable.
- [ ] Close provider-specific feature gaps. Current mobile compose/control
  capability projection enables Codex only; an installed/readable provider
  must not be advertised as writable without its actual backend workflow.
- [ ] Finish the Mac remote-peer UI migration. ABI 1.73 now routes production
  authenticated remote reads/writes through native clients and retained mutation
  drivers, with explicit resume and recovery. The composer now uses the bounded
  upload/proof controllers; remote files are never inserted as local paths.
  ABI 1.75 adds Shared remote-only provider discovery; ABI 1.76 adds session
  aggregation/cache scoping and per-source failure isolation. ABI 1.77 connects
  remote file preview/project files, with no local-URL fallback. Retired-cache
  lifecycle still needs completion; ABI 1.78 now supplies Mac Git history and
  patch UI through Shared. Failed inbox reads retain
  read-only cards; Shared isolates per-source freshness/outages and retains
  explicitly matching display scopes across whole-client replacement. Real
  reconnect acceptance and actual remote operation authorization remain open.

## Pagination contract

`GET /native_sessions?provider_id=codex` and `/live_sessions` return the existing
`sessions` array plus `total`, `catalog_revision` and nullable `next_cursor`.
Pass the returned cursor as an opaque `cursor` query value to continue. The
default page size is 200; `limit` accepts 1 through 500. An altered catalog or
foreign provider/source cursor returns a conflict rather than mixing snapshots.
The client rejects loops, revision/total changes and incomplete terminal pages.
Legacy responses without pagination metadata remain readable. Reaching the
provider discovery bound is an explicit error, not a successful partial total.

## Unified acceptance (not yet performed)

Previous ABI checkpoint (2026-09-10, before the OS-adapter checkpoint above): ABI 1.81 borrows the real
Shared owner runtime and performs explicit management authentication before
taking its state mutex. OS callbacks expose only existing-key SPKI/signatures,
current opaque runtime/target reads and exact target CAS/readback. Apple wraps
the call on a worker, copies callback inputs and suppresses late results after
cancellation. Returned evidence proves only local-target persistence.
The real FFI pairing fixture now has an actual target test key and exercises
the full transaction, including lost readback and retry with one durable write.
No production management broker lane, Mac revoke UI, persistent request recovery,
multi-target propagation or consumer revocation UI has been enabled by this ABI.

Verification: Rust workspace/all-targets 706 passed; workspace Clippy warnings
denied passed; CoreClient 65 XCTest plus 12 Swift Testing tests and Mac 345 tests
passed. Apple XCFramework, three C ABI smokes and full unsigned Harmony
PackageHap/suites passed. Intermediate Rust test-helper signature/type and lint
errors, and Swift static-method/C-pointer adaptation errors, were corrected
before the passing runs. No signing/notarization, installation, Relay change,
live authorization prompt, device revocation or physical E2E was performed.
The final host NAPI gate also passed with a freshly produced Mac coordinator
fixture through production ETS/C++/Rust ABI 1.81. Existing host-link deployment
target warnings remain. This is a software-key cross-adapter gate, not OS
hardware acceptance. The Apple ABI call uses a non-cooperative worker queue so
its blocking callback bridge does not occupy Swift's cooperative executor.

Latest Shared revocation checkpoint (2026-09-09): the explicit exact-Grant
transaction derives all trust fields from verified history, signs and atomically
persists the local target snapshot, and releases only signed propagation
evidence. Lost readback is recovered from the original intent without duplicate
signing/writes. Two-phone isolation, conflicting operation IDs, expired-history
maintenance and rejection of previously installed certificate/checkpoint reads
are covered. The Mac revoke button and consumer propagation are still not wired.

Verification: Rust workspace/all-targets 705 passed; workspace Clippy with
warnings denied passed. The first full test run hit sandbox-denied local socket
creation; the authorized rerun passed. An intermediate new fixture passed a
fixture object instead of its signer key and was corrected before the full run.
This source-only change adds no C ABI and no platform fallback. No native package
rebuild, signing/notarization, installation, Relay change, live revocation or
physical E2E was performed. Full device revocation remains an open gate.

Latest Mac Owner-selection checkpoint (2026-09-09): ABI 1.80 connects the
production workspace to bounded catalog slots, retained pairing models and
single-active-Owner peer handoff. Same-slot rebinding requires a currently valid
lease from the same Shared catalog and does not revive old handles. Switching
awaits cancelled pairing cleanup and fences old directory/picker callbacks.
Unknown new-layout directories block startup/allocation without erasing them.
Ambiguous catalog saves retain the previous model suspended; reload cannot issue
a fresh lease until Shared confirms or abandons the exact candidate.

Verification: Rust workspace/all-targets 700 passed and Clippy warnings-denied
passed; CoreClient 65 XCTest plus nine Swift Testing tests passed; Mac 345 tests
passed, including real temporary AES-GCM store rebinding, lost readback, late
callbacks and immediate selection-back during peer cleanup. Apple XCFramework,
C ABI selection/rebinding smoke, host NAPI ABI 1.80 contracts and full unsigned
Harmony PackageHap/suites passed. An intermediate Rust run failed after an ABI
edit accidentally changed a test public-key byte; the fixture was restored and
the full rerun passed. An intermediate Swift fixture missed `try`; it was fixed
before the passing full runs. No production trust checks were relaxed.
No signing/notarization, installation, Relay change or physical pairing/E2E
occurred. Changed authorization-context recovery, orphan repair and all other
remaining feature/physical gates above remain open.

Earlier consumer-catalog checkpoint (2026-09-09): ABI 1.79 adds the Shared
multi-enrollment local catalog and typed Apple storage leases. Add/select stay
pending until actual exact CAS readback; old leases retire after selection,
close or owner-driver release. A failed/ambiguous write retains the same
candidate. Abandon requires the actual unchanged predecessor and cannot discard
a persisted candidate. The native scoped store preserves the original five
legacy files and isolates five protected slots per new catalog component.
Actual temporary filesystem tests use AES-GCM software test keys, not real
Secure Enclave keys. Production workspace/selector integration remains open.

Verification: Rust workspace/all-targets 699 passed; Clippy with warnings denied
passed; CoreClient 64 XCTest plus nine Swift Testing tests passed with compiler
warnings denied; Mac 340 Swift Testing tests passed. Apple XCFramework, three C
ABI smokes (including wrong-handle/disposal/selection-readback checks), host
C++ NAPI ABI 1.79 contracts and full unsigned Harmony PackageHap/suites passed.
One intermediate Mac build stopped because the new test file was edited during
compilation; the finalized source rerun passed. Existing SDK deprecation
warnings remain. No signing/notarization, installation, production catalog
migration, pairing-data cleanup, Relay change or physical E2E occurred. This
does not close the full multi-Owner or unified-acceptance gates.

Latest Harmony Git UI checkpoint (2026-09-09): the conversation header opens
history, previous/next page and commit patches through native session operations
6/7. The payload contains only the typed session selection and opaque
`gitSelection`; no path or repository root is inferred by ArkTS. Shared still
owns identity, authorization, HEAD/revision validation and response bounds.
Refresh starts a new HEAD page; failures remain errors, not empty histories.
File/Git views are mutually exclusive. Closure, session/background/auth changes
and device removal/unavailability prevent stale display updates. Ordinary
unchanged usable peer updates retain the view. New production ArkTS controller/
page fixtures cover navigation, exact device selection, retries and late results;
remote-client fixtures cover rebind rejection and same-ID sessions on different
devices. Production C++ NAPI rejects an owner-anchor substitution on both Git
operations. Full unsigned Harmony Rust/native/ArkTS PackageHap and all its
existing suites passed; SDK deprecation warnings remain. No Shared/Mac source
change, signing, installation, Relay modification, live state cleanup or physical
acceptance occurred in this checkpoint. All other implementation and deployment
gaps below remain open.

Previous Mac Git checkpoint (2026-09-09): ABI 1.78 adds Shared read-only Git history
and patches, Apple native bridges and the Mac history browser. Actual isolated
repositories exercise scoped history/pagination, changed HEAD, unborn/corrupt
state, non-ancestor rejection, linked detached worktrees, first-parent merges,
disabled helpers/network fetch, bounded patches and process timeouts. Mac UI
tests cover exact device routes, cursor forwarding, failed reads and late results
after session/Owner replacement or shutdown. CoreClient uses the real local C
ABI against a temporary Unix socket. Harmony's native ABI/build is compatible,
but its Git history UI has not been wired. No signing, installation, live
pairing-state access, Relay change or installed-device acceptance occurred.

Verification: Rust workspace/all-targets 696 passed, Clippy with warnings denied
passed; Mac 335 Swift Testing tests passed; CoreClient 61 XCTest plus nine Swift
Testing tests passed with compiler warnings denied. Apple XCFramework,
unsigned Harmony Rust/native/ArkTS PackageHap, production host NAPI ABI 1.78
contracts and three C ABI smokes passed. The initial sandboxed Rust run could
not bind sockets (23 permission-denied failures); the authorized local
loopback/UDS rerun passed. These are software checks, not physical acceptance.

Latest Harmony file-UI checkpoint (2026-09-09): the production conversation
header now opens the file browser, with directory/back/refresh/close, explicit
path input and bounded UTF-8/static-raster previews. Shared ABI remains 1.77;
all paths still go through its existing authenticated file reader. The OS
ImageKit adapter receives bounded bytes, not local phone URLs. Superseded reads
and late decoded images cannot replace newer state; closure/session changes,
authorization/lifecycle events and unavailable/removed devices release display
resources. An unchanged usable peer update keeps the view open.

The full unsigned Harmony build and its existing protocol, page, pairing,
remote-client, mutation and upload suites passed. New tests execute the actual
ArkTS renderer/controller/page methods, checking invalid/oversized/truncated
input, MIME and animation rejection, thumbnail options, source/PixelMap cleanup,
failed reads versus empty directories, back/retry, late completion and exact
device paths. The real SDK compiler caught constructor-property/structural-type
restrictions; explicit fields and interface inheritance fixed those errors.
The final actual ArkTS CompileArkTS/PackageHap passed. Existing SDK deprecation
warnings remain. These OS effects are fixtures, not a physical ImageKit display
or HUKS/Relay test. No new Rust/Apple changes, signing, installation, Relay change
or live pairing-data access occurred; the full objective remains open.

Previous native workspace-file checkpoint (2026-09-09): ABI 1.77 adds the native
file APIs, serving-backend absolute reference resolution, exact Shared file
routes, Mac directory/conversation-file UI and Harmony NAPI/transport adapters.
Unknown local IDs cannot use the legacy local fallback; stale/foreign routes
fail before dispatch. Actual UDS/ABI tests validate encoded references and exact
returned content; Mac tests cover per-device routing, cancellation, Owner/session
replacement, retries and bounded text/image rendering. Harmony adapter tests
cover exact selection/path and rejected late old-connection results. Its file
browser UI remains open.

Final verification: 675 Rust workspace library tests and warnings-denied
backend/client/domain/FFI all-target Clippy passed. Three-target Apple XCFramework,
three C ABI smokes, production host NAPI/lifetime checks and the unsigned Harmony
Rust/native/ArkTS PackageHap passed. Mac passed 330 tests. The new image fixture
uses distinct GIF frames because identical generated frames were coalesced;
the strict multi-frame rejection assertion remains. A native TCP regression
also exposed EOF being forgotten when delivered with final bytes. A repeated-EOF
assertion reproduced EIO before the fix; the OS adapter now retains EOF without
bypassing bounds or cancellation. Final CoreClient passed 60 XCTest plus nine
Swift Testing cases with warnings as errors; 20 repeated real loopback EOF runs
passed. The final Mac rerun after the TCP fix also passed all 330 tests. These
are adapter/fixture results, not evidence of an installed authenticated channel.

No signing, notarization, installation, Relay change or live pairing-data access
occurred. This is not a user acceptance release; the remaining feature gaps in
the tables and unchecked items above still apply.

Previous workspace-file protocol checkpoint (2026-09-09): Shared backend/client
file reads are implemented; native/UI wiring and deployment remain open.
All 672 Shared workspace library tests passed, including actual Unix-socket
file reads and capacity/escape tests. Backend/client/domain all-target Clippy
passed with warnings denied. The final three-target Apple XCFramework passed
322 Mac tests and 58 CoreClient XCTest plus nine Swift Testing cases; CoreClient
warnings were treated as errors. Three C ABI smokes, production host NAPI and
lifetime checks, and unsigned Harmony Rust/native/ArkTS PackageHap passed.
ABI remains 1.76: these checks establish compatibility, not a new file-read
native adapter or UI. No signing,
notarization, installation, Relay change or live pairing-data access occurred.

Previous session-catalog checkpoint (2026-09-09): ABI 1.76 moves the actual Mac
multidevice workspace off its Swift JSON merging/routing and unscoped cache
envelopes. New Shared/native/Mac tests cover 501 sessions with local failure,
stale rows with no action route, exact response projection, source-handle
revocation, Owner-scoped encrypted cache restore, capacity errors and late
old-Owner reads during workspace replacement. All 665 Shared workspace library
tests and all-target client/FFI Clippy with warnings denied passed. The final
three-target XCFramework passed 322 Mac tests and 58 CoreClient XCTest plus nine
Swift Testing cases (CoreClient warnings as errors). Three C ABI smokes,
production host NAPI/lifetime checks, and unsigned Harmony Rust/native/ArkTS
PackageHap passed. The first Mac build reused an obsolete dependency source list;
cleaning only its generated SwiftPM build cache discovered the new bridge file.
The final run linked the final archive. An old presentation fixture was updated
to supply an explicit Owner for retention and require a fresh read after removal,
rather than expecting removed-source rows to reappear. No signing, notarization, installation, Relay
modification or physical acceptance occurred. Remote file preview and the
remaining implementation and deployment gates above remain incomplete.

Previous provider-catalog checkpoint (2026-09-09): ABI 1.75 passed 660 Shared
workspace library tests, client/FFI all-target Clippy with warnings denied,
320 Mac tests, and 56 CoreClient XCTest plus nine Swift Testing cases.
Three-target Apple XCFramework, three C ABI smokes, production host NAPI/lifetime
checks and unsigned Harmony Rust/native/ArkTS PackageHap passed. Native and Mac
fixtures cover remote-only discovery, per-device capabilities, failed-source
retention, invalid/oversized responses, exact owner/endpoint scope replacement
and obsolete reader rejection. Generated archive identity triggered consumer
recompilation and relinking without changing Swift sources. This turn changes
Harmony's ABI checks, not its native/UI feature set. No signing, notarization,
installation, Relay modification or physical acceptance occurred. Session
aggregation/cache migration, remote file preview and the remaining gaps above
are still incomplete; this is not a unified acceptance release.

Previous Mac attachment-UI checkpoint (2026-09-09): all 318 Mac tests passed
against the rebuilt Shared library. New actual workspace/model/controller tests
cover raw-session routing, selected operation handoff, attachment-only sends,
no path serialization, upload/rebind exclusion, suspension after file read/CAS,
exact saved-request recovery, changed-Owner picker rejection and preservation
of newer drafts. Handoff fixtures use explicitly wrong-kind native handles;
they cannot authorize real network mutations and are not physical file evidence.
Shared passed 656 workspace library tests (651 primary plus five auxiliary),
including exact sealed-outcome save retry with unavailable signing, no replay
and no success before actual readback. Client/FFI Clippy passed with warnings
denied. CoreClient passed 53 XCTest and nine Swift Testing cases with warnings
as errors; three-target Apple XCFramework, three C ABI smokes, production host
NAPI/lifetime checks and unsigned Harmony Rust/native/ArkTS PackageHap passed.
No signing/notarization, installation, Relay changes, real provider/hardware
acceptance or protected-state deletion occurred. Remote file preview,
provider/session aggregation and the other implementation gaps above remain
open. This is not a unified acceptance release.

Previous selected-attachment checkpoint (2026-09-09): ABI 1.74 passed all 655
workspace library tests (650 primary Shared tests plus five auxiliary tests),
client/FFI all-target Clippy with warnings denied, all 313 Mac tests, and 53
CoreClient XCTest plus nine Swift Testing cases with warnings as errors.
The initial sandboxed CoreClient run could not create its local test sockets;
the full authorized isolated rerun passed. Apple three-target XCFramework,
three C ABI smokes, production host NAPI/lifetime checks and the unsigned
Rust/native/ArkTS HAP passed. Existing SDK deprecation warnings remain.
New coverage checks slot reuse even for identical file bytes, restoration of
the replacement, stale/missing selection rejection before send, UI operation-ID
retention, partial proof disposal, wrong-kind handles and bounded pinned calls.
Apple upload-controller tests verify the selected ID reaches the native proof
boundary, not a fabricated proof. Mac remote attachment UI is still incomplete.
No signing/notarization, installation, Relay change, live pairing or protected
state cleanup occurred; the overall completion goal remains open.

Previous Mac authenticated-peer checkpoint (2026-09-09): ABI 1.73 passed all 654
workspace library tests (649 primary Shared tests plus five auxiliary tests),
warnings-denied Muster/FFI all-target Clippy, 53 CoreClient XCTest and nine Swift
Testing cases with warnings as errors, and all 313 Mac tests. Native certificate
fixtures cover invalidated-lease replacement at pending request, exact exchange
and unsaved response boundaries, plus unchanged state on wrong context/stale
target. Apple tests cover native wrong-handle rejection, independent OS peer
shutdown, typed read selections, no implicit resume/legacy ID, approval/sandbox
separation, multi-device create/send routing, outage isolation, retained peer
controllers and late shutdown/read results. Peer-manager/OS adapters use explicit
software fixtures; they are not a real installed-App mTLS or Provider acceptance.
Three-target XCFramework, three C ABI smokes, host production NAPI/lifetime checks
and Harmony unsigned PackageHap build passed. The initial sandboxed backend run
was denied test socket creation; the full isolated suite passed with permission.
No signing/notarization, installation, real pairing, Relay deployment or state
cleanup was performed. Changed authorization/history contexts, attachment UI,
provider/session aggregation and all other remaining work above stay open.

Previous Mac consumer-enrollment checkpoint (2026-09-09): ABI 1.72 passed 647
Shared library tests (155 backend, 82 client, 21 domain, 90 FFI, five local
transport and 294 Muster), plus warnings-denied Clippy. CoreClient passed 52
XCTest and nine Swift Testing cases with warnings as errors; Mac passed all 306
tests. New cases cover all 32 native slot inventories, malformed/orphan recovery,
bounded slots, encrypted exact CAS/readback, startup without authentication/key
creation, model cancellation/timer lifetime and actual QR decoding/ambiguity.
An actual native Mac consumer and existing native owner coordinator complete
proof/SAS/confirmation/Grant/composite/ACK, recover a failed first checkpoint
write without another authentication, and cold-restore durable ACK without
another ACK POST. Repeated restoration of a damaged composite journal retains
all slots and sends no request, rather than downgrading to checkpoint recovery.
This integration uses software test keys and a simulated
Relay; it does not prove real Relay signature enforcement or hardware keys.
The simulated Relay's nested confirmation-path parser was corrected after the
native receipt-substitution guard rejected the original fixture.
Apple's three-target XCFramework, three standalone C ABI smokes, production
host-NAPI contract/lifetime checks and Harmony HAP build passed. Harmony build
emitted its configured signed-debug and unsigned artifacts; no independent
signing verification or device installation was performed. Mac was not signed
or notarized. Relay, installed Apps, real pairing records and Keychain were not
changed. The remaining table above is still open; this is not a test-ready
release or end-to-end acceptance.

Previous Apple directory-driver checkpoint (2026-09-09): Mac passed 295 tests,
including nine new lifecycle/HTTP cases. They cover OS failure classification,
retained read-only display, empty HTTP errors, bounded reception, cancellation
propagation, late response rejection, exact request cleanup, timer ownership and
manual/automatic refresh overlap. HTTP tests use URLProtocol/byte-sequence
fixtures and a direct no-redirect delegate check, not real Relay authentication.
CoreClient passed 50 XCTest plus nine Swift Testing cases with warnings as
errors, including actual native invalid-handle recovery calls and major/minor ABI
rejection. The existing ABI 1.71 native library was used; Rust and Harmony source
were unchanged and their prior results were not rerun for this Apple-only change.
No signing, notarization, installation, Relay deployment, durable-state cleanup
or physical acceptance was performed. Consumer enrollment, production connection
ownership/UI wiring and all other remaining work above stay open.

Previous upload-driver checkpoint (2026-09-09): ABI 1.71 passed 645 Shared
library tests (155 backend, 82 client, 21 domain, 89 FFI, five local transport
and 293 Muster), plus client/FFI all-target warnings-denied Clippy. Four new
Rust tests cover terminal whole-file replacement, exact storage and context,
partial/unsaved state rejection, signing failure and independent completed-file
snapshots across source reuse/retirement. Mac passed 286 tests; six new cases
cover two-chunk effects, retained recovery, awaiting cleanup before reuse and
actual local file size/type/cancellation boundaries. CoreClient passed 49 XCTest
plus nine Swift Testing cases with warnings as errors, including wrong-kind
native successor/snapshot rejection. The initial Apple test run used the old
XCFramework header while its replacement was still building; the complete
rerun against the finished framework passed.
Apple three-target XCFramework/link smoke, three C ABI smokes, production host
NAPI ABI/lifetime checks and the complete Harmony build passed. The production
ArkTS fixtures exercise op 21/22, proof disposal and deliberately paused cleanup
that blocks both foreground slot reuse and background reconnect. These fixtures
do not establish physical remote file delivery. Harmony's existing build
configuration emitted unsigned and signed-debug HAPs, without independent
signature verification or installation. No Mac signing/notarization, Relay
deployment or pairing/durable-state cleanup occurred. Mac consumer enrollment,
production lifecycle/UI migration and the remaining items above stay open.

Previous remote-successor/controller checkpoint (2026-09-09): ABI 1.70 passed 641
Shared library tests (155 backend, 78 client, 21 domain, 89 FFI, five local
transport and 293 Muster), plus client/FFI all-target warnings-denied Clippy.
Three new Rust tests cover exact terminal replacement, unsaved outcome/signing
failure, foreign context/key, external tokens and incomplete/retired upload
proofs. CoreClient passed 49 XCTest plus nine Swift Testing cases with warnings
as errors, including actual native wrong-handle successor calls. Mac passed 280
tests. Eight new Mac cases exercise exact effects, lost OS replies, unsaved
outcomes, restored pending requests, explicit continuation, cancellation during
an awaited phase, certificate exchange/renewal ordering and independent encrypted
storage slots. Certificate fixtures deliberately stop at native client creation;
they cannot mint a verified client or establish a live mTLS session.
Apple three-target XCFramework/link smoke, three C ABI smokes, host production
NAPI ABI/lifetime tests and the complete Harmony build passed. Harmony's actual
ArkTS fixtures now require op 20 for repeated text/control/attachment intents;
native malformed-count/intent and disposed-proof cases fail closed. The existing
Harmony build configuration emitted unsigned and signed-debug HAP artifacts;
no independent signing verification or device installation was performed. No Mac
signing/notarization, Relay deployment, or pairing/durable-state cleanup occurred.
Mac ACK-gated enrollment/directory ownership, production lifecycle/UI integration,
the upload effect driver, remaining feature gaps and physical acceptance remain
open. The full goal is active and no unified acceptance is requested.

Previous Apple remote-bridge checkpoint (2026-09-09): CoreClient passed 49 XCTest
plus nine Swift Testing cases with warnings-as-errors, including eight new
native-boundary/ownership tests and three real loopback TCP tests. Mac passed
272 tests. The bridge exercises the current ABI 1.69; no Rust ABI change was
needed. Wrong-kind opaque handles cannot become targets, certificates, clients,
mutations or upload proofs, and cannot trigger signer/raw-TCP callbacks. Tests
also cover bounded C output copies, zeroed counts on error, callback retention,
concurrent close, parent rebinding, typed selection encoding, system DNS, actual
byte reads/writes/EOF, deadlines, stream capacity and non-reused IDs. The existing
local signer-lifetime test now uses a weak capture instead of a compiler-warned
unmutated weak variable; its native retention assertions are unchanged.
These are native-boundary and local OS tests, not a successful authenticated
Relay/certificate exchange. Mac production enrollment/Grant/directory ownership,
encrypted certificate/mutation/upload persistence, recovery coordination and UI
switch-over are still required. No Harmony source, Relay, pairing data, signing,
notarization or installed application was changed; the full goal remains active.

Previous source-port-replacement checkpoint (2026-09-09): 638 Shared library tests
passed (backend 155, client 75, domain 21, FFI 89, Muster transport 293 and local
transport 5), plus client/FFI warnings-denied all-target Clippy. Shared fixtures
verify matching-scope stale retention, changed endpoint/Owner, removal and
readdition, unknown scopes, close, exact pending-token invalidation and atomic
failure of malformed replacement. CoreClient passed 38 XCTest plus nine Swift
Testing tests; the actual FFI facade proves every old lease entry point is
rejected after replacement and failed binds preserve the previous valid lease.
Mac passed all 272 tests, including production model peer replacement, offline/
online recovery, Owner change/removal, and a late suspended read from the old
source port. A superseded client cannot even start new OS reads. Failed binding
does not expose the store's previous display under the new client. Apple three-
target XCFramework/Apple C smoke, three C ABI smokes, host NAPI ABI 1.69/lifetime
checks and Harmony unsigned Rust/native/ArkTS HAP passed. No signing,
notarization, installation, Relay deployment or durable-data cleanup occurred.
Other implementation gaps and all physical acceptance gates remain open;
the full goal is still active and no unified acceptance is requested.

Previous Shared inbox-aggregation checkpoint (2026-09-09): 636 Shared library tests
passed (backend 155, client 73, domain 21, FFI 89, Muster transport 293 and local
transport 5). Client/FFI all-target warnings-denied Clippy passed. Four Shared
fixtures cover independent source failure/recovery, superseded/foreign/consumed
read tokens, malicious source/freshness projections, malformed question shapes,
duplicate/unscoped identities, namespace ambiguity and source/item/byte bounds.
The production Swift facade passed two new native tests; CoreClient passed 37
XCTest plus nine Swift Testing cases. Mac passed all 270 tests, including the
actual Shared-backed aggregate and UI model retaining an unavailable remote's
read-only card while the local card remains usable, then recovering the remote
without routing its answer to local. Apple three-target XCFramework/Apple C
smoke, three C ABI smokes (including display-handle/remote-target separation),
host NAPI ABI 1.68/lifetime checks and Harmony unsigned Rust/native/ArkTS HAP
passed. No signing, notarization, installation, Relay deployment or durable-data
cleanup was performed. Whole-client-replacement retention, authorized Mac remote
mutation migration, remaining feature gaps and physical acceptance stay open.
The overall goal remains active; no unified acceptance is requested.

Previous Mac inbox-display checkpoint (2026-09-09): all 269 Mac Swift tests passed.
Six new regressions exercise retained approval/question cards across network and
malformed-envelope failures, successful recovery and authoritative empty reads,
out-of-order successes/errors, client replacement, cancellation and shutdown,
changed session/request selection, duplicate clicks while the Shared local
driver is pending, late completion after shutdown, and unavailable/malformed/
unscoped remote source propagation. SwiftUI now renders a dedicated refresh
warning and binds drafts/buttons to the exact displayed card. Shared and the
ABI remain unchanged at 1.67; their previous 632-test result was not rerun in
this UI-only checkpoint. No signing, notarization, installation, Relay change,
durable-state deletion or real-device UI acceptance was performed. Mac remote
mutation migration and independent per-device inbox recovery remain incomplete;
the overall goal is still active and unified acceptance is not requested.

Previous transient-directory-recovery checkpoint (2026-09-09): 632 Shared library
tests passed (backend 155, client 69, domain 21, FFI 89, Muster transport 293,
local transport 5). Warnings-denied all-target Clippy passed for transport,
backend, client and FFI. Shared fixtures exercise every backoff step, reset
after success, signed-history retention, stale/foreign/consumed failures, old
target invalidation, transient versus terminal HTTP/OS errors, directory expiry
while the Grant remains valid, and stopping at exact Grant expiry. The real FFI
fixture includes an empty 503 response and defensive cleanup preserving retry.
The production Harmony adapter fixtures exercise OS DNS/connect/timeout/read
failures versus certificate/unknown errors, UI messages, exact-request reporting,
one retained directory session, and recovery without authentication or state
deletion. Apple three-target XCFramework/Apple C smoke, three C ABI smokes,
production host NAPI ABI 1.67/lifetime checks and Harmony Rust/native/ArkTS
unsigned HAP build passed. Mac passed 263 tests; CoreClient passed 35 XCTest
plus nine Swift Testing cases against the rebuilt core. No signing, notarization,
installation, Relay deployment or user-data cleanup was performed. Expired or
changed Grants/Owners, all other unchecked implementation requirements and
physical acceptance remain open. This is a progress checkpoint, not completion.

Previous attachment-retention checkpoint (2026-09-09): 629 Shared library tests
passed (backend 155, client 69, domain 21, FFI 89, Muster transport 290 and
local transport 5). Backend/client/FFI all-target warnings-denied Clippy passed.
New fixtures cover seven-day unused-file collection, quota release, retained
identity bounds, permanent non-reuse, restart after the retirement marker but
before unlink, backward clocks, corrupt retention records, symlink protection,
and referenced/legacy preservation. The actual Codex RPC fixture verifies a
retention pin already exists when Provider receives the file path. Authenticated
backend query fixtures preserve the typed expiry receipt's peer and exact-byte
binding without Provider calls. Shared tests keep bare 410 errors uncertain,
require a complete exact receipt, retain outcome-signing failure, and confirm
the typed rejection only after exact readback; UI result export gives an
actionable reselection error, never a success DTO.
Apple's three-target XCFramework/Apple C smoke, three C ABI smokes, production
host NAPI ABI 1.66/lifetime checks and Harmony's Rust/native/ArkTS unsigned HAP
build passed. Mac passed 263 tests; CoreClient passed 35 XCTest plus nine Swift
Testing cases against the rebuilt core. These are software/native regression
results, not installed HUKS/Secure Enclave or physical Provider acceptance.
No signing, notarization, installation, Relay deployment or user attachment/
pairing-data cleanup was performed. Referenced/legacy file lifecycle cleanup,
Mac remote-peer mutation migration and all other unchecked requirements remain
open; the goal is not complete and no unified user acceptance is requested.

Previous Mac local-UI checkpoint (2026-09-09): 619 Shared library tests passed
(backend 147, client 67, domain 21, FFI 89, Muster transport 290 and local
transport 5). Client/FFI all-target warnings-denied Clippy passed. Mac passed
263 Swift tests; CoreClient passed 35 XCTest plus nine Swift Testing cases.
New checks cover concurrent clicks, exact OS CAS, lost write replies, retained
unsaved outcomes, restore-without-dispatch, empty startup without signing-key
creation, draft edits during send, stale resume confirmation, all six local
model action routes and absence of legacy fallback. A real UDS/native ABI test
verifies successor rejection before durable completion and against stale OS
snapshots; it also verifies save-only recovery preserves an existing candidate.
The encrypted storage fixture uses software AES-GCM, and native callback tests
use software P-256: neither proves installed Secure Enclave behavior.
The three-target Apple XCFramework/Apple C smoke, three C ABI smokes and
production host NAPI ABI 1.66/lifetime checks passed. Harmony's actual adapter
fixtures and Rust/native/ArkTS unsigned PackageHap build passed. No signing,
notarization, installation, Relay change, user-data cleanup or physical
acceptance was performed. Remote-peer Mac writes and the remaining items above
are still open; this checkpoint is not a test-ready release.

Previous local-journal/native checkpoint (2026-09-09): 613 Shared library tests
passed (backend 147, client 66, domain 21, FFI 89, transport 290). New tests cover
exact request/outcome readback, pending restart, unknown/missing no-replay,
backend-proven same-ID retry, signed cancellation, retained signature-failure
results, local/remote context isolation and legacy remote serialization/domain
compatibility. The actual randomly generated operation ID is included in the
initial wire-size validation, before a persistable candidate is exposed; a
boundary regression prevents saving a request that cannot fit the backend lane.
Local ABI errors leave null handles/phase sentinels and cannot
export a result. Client/backend/FFI all-target warnings-denied Clippy passed.
Apple XCFramework and the Apple C link smoke passed. CoreClient passed 34
XCTest plus nine Swift Testing cases, including a real UDS/native ABI fixture
that restores a pending request, queries the first result without a second
dispatch, recovers a failed outcome signature by sealing only, and verifies
the opaque native handle retains/releases its signing callback owner. Its hardware
key and Provider are software fixtures, not SEP or production durability proof.
Mac passed 253 tests; three C ABI smokes and production host NAPI ABI 1.65/
handle-lifetime checks passed. The current unsigned Harmony Rust/NAPI/ArkTS HAP
and its existing adapter fixtures passed. No signing, notarization, installation,
Relay write or physical-device acceptance occurred. Actual Mac encrypted
storage and compose/recovery UI wiring remain unfinished; the full goal is open.

Latest local-mutation foundation checkpoint (2026-09-09): 603 Shared library
tests passed (backend 147, client 58, domain 21, FFI 87, transport 290), plus
five local-UDS transport tests.
Client/backend/FFI all-target warnings-denied Clippy and the production backend
binary build passed. New regressions cover real UDS reply loss, cached exact
replay, backend restart, pending history without Provider receipts, late
completion, endpoint/request isolation, cancellation fences, input size limits,
normalization and external attachment denial. An initial new restart test
incorrectly expected a persisted `pending` record to be renamed `unknown`;
the assertion was corrected to the existing ledger contract, without relaxing
the production no-replay rule. This turn has not changed the native ABI/UI or
rebuilt Apple/Harmony artifacts. No signing, notarization, installation,
production backend startup, Relay write or real-device acceptance was performed.
The App compose coordinator/UI and the remaining gaps above are still open.

Latest phone-upload wiring checkpoint (2026-09-09): 592 Shared library tests
passed (backend 139, client 55, domain 21, FFI 87, transport 290), with
client/backend/FFI warnings-denied Clippy. Two new retirement tests cover
pending/unknown/unsaved denial, exact durable retirement, replacement without
deleting remote evidence, and signing failure without resurrecting send proof.
Actual ArkTS adapter fixtures exercise file reading, multi-chunk effects,
opaque attachment send, saved-state restart, rebind of retained outcomes,
wrong-file rejection, local retirement, and cancellation during picker reads.
SDK-parsed production page methods reject cross-device/session selection and
late progress, preserve full slots, and require explicit attachment selection.
An SDK-limit fixture rejects the old oversized single-string storage and
round-trips a 512 KiB sealed payload with the new framing. Missing/torn/corrupt
pieces fail without changing the active record; old inline records and paired
atomic records remain readable. This is software encryption/storage evidence,
not physical HUKS durability or power-loss acceptance. Apple XCFramework, Mac
253 tests, CoreClient 30 XCTest plus nine Swift Testing cases, three C ABI smokes,
production host NAPI ABI 1.64/lifetime checks, and the current unsigned Harmony
Rust/NAPI/ArkTS HAP passed. No signing, notarization, installation, Relay writes,
pairing-data cleanup or physical upload/provider acceptance was performed.

Latest durable-upload checkpoint (2026-09-09): 590 Shared library tests passed
(backend 139, client 53, domain 21, FFI 87, transport 290); client/backend/FFI
Clippy passed with warnings denied. Coordinator tests cover per-chunk exact
readback, restart/reselection, retained signing-failure outcomes, cross-context
and duplicate proof rejection, malformed receipts and bounded journals. The
production host NAPI ABI 1.63 tests cover invalid arguments and disposal while
upload proof handles are pinned, not a successful physical upload. HUKS store
adapter fixtures cover isolated upload slots, immutable queued buffers and
lost-readback recovery. Apple XCFramework, three C ABI smokes, Mac 253 tests,
CoreClient 30 XCTest plus nine Swift Testing cases and the unsigned Harmony
Rust/NAPI/ArkTS HAP passed before the final display-only Rust correction. That
correction distinguishes a saved chunk from a completed file and was rechecked
with the Shared suite and Clippy; native artifacts have not been rebuilt after
that last correction. The phone upload driver/UI are still disconnected;
these are software checks, not HUKS/Secure Enclave/Relay or provider acceptance.
No signing, notarization, installation, Relay changes or pairing-data cleanup
were performed.

Latest startup/lock checkpoint (2026-09-09): 583 Shared library tests passed
(backend 139, client 47, domain 21, FFI 86, transport 290), and warnings-denied
Clippy passed for domain/backend/FFI. A deterministic descriptor-alias test
failed before the lock fix and passes after it. Ledger locks are now explicitly
released only when the last Arc-owned worker state is destroyed, including
parse failure after successful acquisition; attachment stores follow the same
ownership rule. Failed competing opens never unlock the active writer, and
closing an old descriptor alias cannot release a new generation's lock. Fifteen
complete parallel backend runs passed (139 tests each). This demonstrates the
fixed alias-retention mechanism and absence of failures in those runs, not
forensic proof that every earlier intermittent failure had only that cause.
Apple XCFramework and the new import-plan C link smoke passed; three existing
C ABI smokes and production host NAPI ABI 1.62/lifetime checks passed. Mac
passed 253 tests, including real Shared-plan adapter tests for transient errors,
canonical state appearing during backoff, lost committed-write replies without
duplicate writes, terminal rejection/cancellation and absent registration.
CoreClient passed 30 XCTest plus nine Swift Testing cases. The Mac build script
now replans the SwiftPM manifest so newly added local CoreClient sources cannot
be omitted by a stale cached dependency source list; an actual uncached Mac
test build verified the new source was included. The current unsigned Harmony
Rust/NAPI/ArkTS HAP passed. No signing,
notarization, installation, actual pairing or Relay changes were performed.

Latest attachment-backend checkpoint (2026-09-09): 578 Shared library tests
passed (backend 137, client 47, domain 19, FFI 85, transport 290), and
warnings-denied Clippy passed for these crates. Real TCP/TLS/MST2 fixtures
exercise two-chunk upload followed by send through the authenticated router
and private store. A Codex control I/O fixture checks that `turn/start` receives
a path containing the exact uploaded bytes and a repeated send does not issue
another RPC. Tests also cover partial writes, restart, digest mismatch,
cross-phone/session/provider isolation, symlink denial and quota retention.
Parallel backend runs intermittently hit existing ledger reopen lock failures
in both the pending/unknown restart and exact-receipt recovery tests. An exact
restart rerun, intervening complete parallel runs and the final serial backend
suite (137 tests) passed; a later parallel run was 136 passed / one failed.
The cause is not proven, and no production lock was weakened. The new
text-only-send test also verifies that unrelated uploads cannot contend with
ordinary sends through the attachment store lock. The current unsigned HAP
and its actual ArkTS adapter fixtures passed. Apple XCFramework and C link smoke
were rebuilt successfully; Mac passed 248 tests, and CoreClient passed 30 XCTest
plus nine Swift Testing cases against this build. These results do not complete the phone
attachment flow or establish physical-device acceptance. No signing,
notarization, installation, Relay changes or pairing-data cleanup were performed.

Latest connection-retention checkpoint (2026-09-09): 571 Shared library tests
passed (backend 132, client 45, domain 19, FFI 85, transport 290).
Mac passed 248 Swift tests; CoreClient passed 30 XCTest plus nine Swift Testing
tests against the rebuilt Apple ABI 1.61, including the Apple C link smoke.
Client/FFI/transport Clippy passed with warnings denied. Three C ABI smokes and
production host NAPI ABI 1.61 passed, including disposal of the second pinned
rebind handle. Actual ArkTS adapter fixtures preserve known outcomes after
sign/readback failures across background/reconnect, save without a second
dispatch, reject old-epoch read results and retain clients across directory
removal/reappearance. The full unsigned Rust/NAPI/ArkTS HAP passed after fixing
explicit numeric typing and typed error rethrow caught by the ArkTS compiler.
These are software/adapter checks, not physical HUKS/Secure Enclave/Relay proof.
No signing, notarization, installation or Relay change was performed.

Previous provider-receipt checkpoint (2026-09-09): 569 Shared library tests passed
(backend 132, client 43, domain 19, FFI 85, transport 290). Mac passed 248 Swift
tests. Backend build and warnings-denied Clippy passed. New tests reopen inner
and outer ledgers, recover both
create/send receipts and verify subsequent queries use the repaired outer
cache. Missing outer history, changed raw request, wrong scope, foreign opaque
receipts and another peer's identical client ID cannot obtain a completion.
Legacy non-namespaced provider records remain uncertain. The real loopback
TLS/MST2 dispatch test checks the provider receives the namespaced ID in both
its typed request and exact body. ABI remains 1.60; no native ABI or HarmonyOS
source changed in this checkpoint. No signing, notarization, installation,
Relay deployment or physical acceptance was performed.

Previous non-delivery-cancellation checkpoint (2026-09-09): 562 Shared library
tests passed (backend 125, client 43, domain 19, FFI 85, transport 290).
Warnings-denied Clippy passed for backend/client/FFI/transport. The real
loopback TLS/MST2 test cancels a dispatch before provider delivery, fences the
same request through the authenticated endpoint, and proves a late original
request does not increment provider delivery. Ledger tests cover cancellation
versus admission races, repeated cancellation, refusal of missing/pending/
unknown or changed requests, restart and exclusive ownership until all worker
clones exit. Mobile tests cover signed intent before cancellation I/O, exact
result readback before unblocking, unchanged legacy canonical journal bytes,
and cancellation transport failure remaining uncertain rather than wrongly
rejecting the original operation. The actual ArkTS adapter fixture drives
explicit cancellation and returns a cancelled display result with zero original
dispatches. Full unsigned Rust/native/ArkTS HAP, production host NAPI 1.60,
three C ABI/link smokes, Mac 248 Swift tests and CoreClient 30 XCTest plus nine
Swift Testing tests passed. Signing/notarization, installation, Relay deployment
and physical acceptance remain unperformed; the overall goal remains open.

Previous outcome-query checkpoint (2026-09-09): 558 Shared library tests passed
(backend 123, client 41, domain 19, FFI 85, transport 290). Client/backend/FFI/
transport warnings-denied Clippy passed. A real loopback TLS/MST2/client/backend
test now queries the original durable response, rejects changed request bytes
and scope substitution, retains missing history and verifies no second provider
delivery. Another test isolates results between two authenticated operation
namespaces. The real backend deadline executor returns an ambiguous timeout,
then its original worker persists a queryable success with one provider call;
the ledger test also reopens that late result. Busy ledger queries fail promptly
instead of blocking on a disk writer; expired queries are rejected.
Shared mobile tests verify all four non-replayable intents carry the exact
journal ID, query result signing/readback before completion, no repeated network
call after signing failure, and unchanged unknown journals for missing/foreign
results. Actual ArkTS adapter fixtures exercise action 3 -> ABI action 4 with
zero dispatch calls. Three C/link smokes, host production NAPI ABI 1.59 lifetime
checks, and the final unsigned Rust/native/ArkTS HAP build passed. Mac passed
248 Swift tests; CoreClient passed 30 XCTest plus nine Swift Testing tests
against the rebuilt ABI 1.59. Hardware signing/provider actions in these tests
remain fixtures: no signing, notarization, installation, Relay deployment,
physical pairing or mutation acceptance occurred. Remaining unknown-outcome
cases above and all other unchecked items keep the full goal open.

Previous compose-wiring checkpoint (2026-09-09): 553 Shared library tests passed
(backend 121, client 38, domain 19, FFI 85, transport 290); warnings-denied
Clippy passed for domain/client/backend/FFI. Three C ABI/link smokes and the
production host NAPI lifetime checks passed against ABI 1.58. Actual ArkTS
adapter/page-method fixtures cover create/send/resume, separate creation
settings, explicit control confirmation, durable creation result recovery,
provider capability gates, stale cross-device completions and preservation of
new drafts. Shared tests cover exact live control, unsupported attachment and
conflicting-setting rejection before I/O, target-bound creation results only
after exact durable readback, stored-vs-owned session projection, reviewer
preservation and fatal trust/protocol errors during provider fan-out.
The full Rust/native/ArkTS unsigned HAP build passed; final wording changes
were rebuilt with the actual ArkTS compiler and PackageHap. Pairing UI no longer
claims the connector is unimplemented; it points to actual per-device results
without equating pairing with remote access. Mac passed 248 Swift tests and
CoreClient passed 30 XCTest plus nine Swift Testing tests against rebuilt
ABI 1.58. Existing SDK deprecation warnings and native deployment-target linker
warnings remain. No signing, notarization, installation, Relay deployment,
physical mutation, pairing reset or acceptance was performed. The goal remains
open; this is a coherent source checkpoint, not a test-ready release.

Previous interaction-wiring checkpoint (2026-09-09): 528 Shared library tests
(backend 121, client 34, FFI 83, transport 290) and warnings-denied Clippy passed.
Three C ABI/link smokes and production host NAPI checks passed against ABI 1.57.
The actual ArkTS effect adapter tests cover exact persistence before dispatch,
result-signing/readback recovery without network replay, pending restart,
background-generation cancellation, route isolation and complete handle release.
Actual store-method tests cover separate certificate/mutation/device slots,
unchanged certificate paths, immutable queued buffers, one CAS winner and lost
readback recovery. Actual page-method tests cover duplicate taps, uncertain-card
retention, typed answers, explicit recovery and late cross-device responses.
OS effects are fixtures; these checks do not prove HUKS or real provider behavior.
The real ArkTS compiler caught missing explicit numeric phase types; those were
fixed, and the final unsigned Rust/native/ArkTS PackageHap build passed.
Mac passed 248 Swift tests; CoreClient passed 30 XCTest plus nine Swift Testing
tests linked to the rebuilt ABI 1.57. No signing, notarization, installation,
Relay deployment, real-device mutation or pairing-data cleanup was performed.

Previous mutation-foundation checkpoint (2026-09-09): 528 library tests passed
(backend 121, client 34, FFI 83, transport 290); backend/client/FFI Clippy passed
with warnings denied. Tests cover exact request/outcome persistence, pending
restart, result-signing failure, no replay for interactions, foreign/tampered
journals, mismatched response identities and rejection of unsupported attachment
input. Independent approval/sandbox selections reach both Codex start and send
RPC fixtures. One full run exposed an existing listener test race: it stopped
after accept-loop session creation, before the worker's first readiness check.
Both idle-worker eviction tests now wait for that actual check; the full rerun
passes without weakening the shutdown assertions.

Three C ABI/link smokes passed against the current ABI 1.56. Production host
NAPI passed bounded worker, wrong-handle, disposal and oversized-input checks,
including the new mutation calls. HarmonyOS Rust/native/ArkTS unsigned
PackageHap and its production-adapter fixtures passed. Mac Swift passed 248
tests; CoreClient passed 30 XCTest plus nine Swift Testing tests, linked to the
rebuilt macOS ABI 1.56. Software-key journals, host negative-handle tests and
unsigned packages do not establish hardware-backed mutation persistence or
physical operation. No signing, notarization, installation, Relay deployment or
pairing-data removal was performed in this checkpoint.

Previous read-wiring checkpoint (2026-09-09): 518 library tests passed (backend 119,
client 27, FFI 82, transport 290). Clippy with warnings denied passed for backend,
client and FFI. Three C ABI/link smokes passed against ABI 1.55, including invalid
directory handles and all six read operations failing closed on null handles.
Production C++ NAPI loaded ABI 1.55 and passed bounded async/disposal checks.
Actual ArkTS adapter fixtures cover persistence before exchange/client creation,
failed exchange retention, background generation rejection, two-device routing,
coalescing, resource release and no workspace-refresh feedback loop. HarmonyOS
Rust/native/ArkTS unsigned PackageHap builds successfully. Mac broker software
self-tests passed. Mac Swift regressions passed 248 tests; CoreClient passed
30 XCTest tests plus nine Swift Testing tests, linked to rebuilt macOS ABI 1.55.
The production Shared client now reads mobile runtime/preview through the real
local TCP/TLS/MST2/backend test route. Pure fixtures cover exact inbox selection,
unchanged-transcript inbox updates, failed-inbox preservation, tool content,
unknown usage and bounded signatures. Actual production ArkTS page methods and
adapters are tested for inbox recovery and per-device runtime/preview routing.
Recovery fixtures use software signing keys and do not establish Secure
Enclave/HUKS, live production ingress or installed-device acceptance. The full
Mac-issuer / Harmony host pairing contract was not rerun in this checkpoint.

## Remaining feature gaps after native/read/action wiring

Scope: Mac host plus HarmonyOS remote client, not Windows or an inventory of
every unrelated feature in this repository. Source paths below are evidence,
not statements about the version currently installed on either device.

| Area | Still incomplete | Source / acceptance boundary |
| --- | --- | --- |
| Harmony session reading acceptance | Validate native runtime/settings/usage, conversation/tool content, polling and independently changing inbox against the phone | `runtimeStatus` and `preview` are wired; automated checks cover their data/identity paths. Physical operation and continuous connection recovery remain unverified. |
| Harmony session actions | Physical acceptance of create, text/attachment send, explicit resume, interrupt, approval and structured answers | These paths are wired through the Shared journal and HUKS adapter. Creation has independent title/absolute-cwd/settings input; resume requires explicit confirmation. ABI 1.64 adds bounded file reading, upload/recovery and composer selection. Source wiring is not physical provider acceptance. |
| Shared attachment protocol | Referenced/legacy content lifecycle cleanup and physical provider file consumption | Authenticated chunk upload, bounded storage, completed-upload proof, Codex delivery, seven-day unused-content collection and durable expiry recovery are implemented. Referenced files stay pinned; legacy files are not aged from mtime; permanent tombstones prevent replay and have an explicit 4096-record bound. Legacy multipart `/upload` and arbitrary external tokens remain deliberately unsupported. |
| Harmony capabilities and remote control | Non-Codex write support and Computer Use / Locked Use execution/UI | Shared now projects device/provider/action capabilities from authenticated reads and Grant scopes; the list is no longer empty. Codex create/send/resume are enabled only when supported. Other providers and Computer Use / Locked Use still need their actual workflows, not optimistic UI flags. |
| Harmony connection recovery | Expired-Grant/terminal directory recovery, cross-context recovery UX and real device acceptance | ABI 1.67 retries transient network/HTTP failures while the paired Grant is valid, retaining signed history and invalidating old leases. Shared-scheduled refresh reconstructs connections, follows certificate renewal and rebinds retained mutations without replay. Background/removal and read failure preserve known unsaved outcomes; old-epoch results are rejected. Changed Owner/Grant, retained-history bounds and physical lifecycle behavior remain open. |
| Multi-device completeness | One-device revocation propagation through actual clients, concurrent mobile clients and physical multi-Owner switching acceptance | Device-scoped reads and mutations, independent histories and provider operation-ID namespaces exist. ABI 1.80 implements single-active-Owner selection with retained pairing models, same-slot lease rebinding and late-callback fencing. Changed authorization-context migration and orphan-index repair remain open and must preserve unresolved signed evidence. |
| Harmony multi-Owner enrollment | Concrete encrypted catalog/namespace storage and Owner-switching UI | Harmony NAPI and the thin ETS catalog/lease driver now exist and pass actual Shared host contracts. The production HUKS storage, per-slot retained pairing models and selector/recovery UI remain unwired. Mac alone has page integration; multiple targets within one Owner do not complete multi-Owner enrollment. |
| Partial availability acceptance | Validate one-provider outage while another remains usable on the phone | Shared now isolates ordinary provider outages and returns per-device provider choices plus explicit errors. Trust, authorization and response-identity failures remain fatal. Source behavior is implemented; physical outage acceptance remains open. |
| Mutation recovery | Definitive reconciliation when no exact durable receipt exists, and recovery across Grant/owner changes | Signed HUKS journal, save-only recovery, exact no-replay outcome queries, proven-undelivered cancellation and Codex create/send receipt repair now exist. Legacy raw IDs, missing provider evidence and non-replayable unresolved interactions remain blocked. Never add blind automatic retries. |
| Mac publisher operational evidence | Authenticated App/backend access to the new canonical slot; live publication/renewal/withdrawal, restored-history CAS contention | Automatic driver and Shared-scheduled transient import retries are implemented. Every retry rereads canonical state; rejected/corrupt state remains blocked and quit cancels the task. Physical broker/publication acceptance remains open. |
| Mac compose recovery | Retired-cache cleanup and installed local/remote crash recovery | The production remote compose path uses retained authenticated Shared peer/mutation/upload controllers. ABI 1.77 adds exact Shared file routes, directory browsing and bounded previews; ABI 1.78 replaces the Git placeholder with scoped history/patch UI. Source/adapter checks do not establish installed file/Git reading or physical provider file delivery. |
| Harmony Git acceptance | Physical history/patch rendering and authenticated remote use | ABI 1.78 transport entry points, opaque selector navigation and conversation-header UI are wired. ArkTS/NAPI fixtures and SDK compilation pass; phone layout and real HUKS/Relay Git reads remain unverified. |
| Harmony workspace files | Physical file-browser/text/image acceptance | ABI 1.77 reads connect to directory navigation and explicit file-path preview, with ImageKit bounds and late-result/resource cleanup. Conversation content is plain text; references can be pasted into the preview field, not executed as local phone URLs. |
| Mac lifecycle acceptance | Ordinary startup without authentication, default cover off, shortcut + keep-awake, normal Quit without grey/stuck menu, backend survival | Existing source mechanisms remain; this turn does not establish physical OS behavior. No new power-key binding was added. |
| Matching deployment and E2E | Sign/install both current builds, real two-sided pairing ACK, HUKS/Secure Enclave exchange, actual Relay/MST2, >200 sessions, all interactions and lifecycle cases | Not performed. No Relay changes, signing, notarization, installation or pairing-data cleanup in this turn. |

The authenticated read connector is therefore implemented, but the full remote
Agent workflow is **not complete** and is not ready for unified user acceptance.

Only after all implementation items are complete:

1. Run complete automated/ABI/native regressions and verify no placeholder
   product operations remain. These checks alone do not mark any device gate.
2. Sign and deploy the matching Mac and HarmonyOS builds without clearing
   pairing data. Relay changes, if needed, remain scoped to the Muster service.
3. Verify backend survival, real listener/publication, both sides' durable ACK,
   actual Secure Enclave/HUKS use and authenticated certificate exchange.
4. Verify workspace discovery, more than 200 sessions, preview, create,
   send/reply, approval and question responses on the physical phone.
5. Verify two authorized devices, isolation when one is revoked, background/
   foreground and disconnect/reconnect, with no duplicate mutation replay.
6. Verify ordinary startup does not request authorization and normal Quit
   works. Record code, regressions, signing, installation and each real-device
   result separately. Do not label directory visibility as remote readiness.
