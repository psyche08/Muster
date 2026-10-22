# Device-network mutual trust

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](../../PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

Status: 2026-09-14. Mac-A is the designated Master, a computer-only role. After distribution, devices connect directly and routine verification does not require the Master online. Both Macs and HarmonyOS have versions containing ABI 1.90 installed; the phone's legacy directed authorization to Mac-A passed real session-read validation. Mac-A's Master record has not yet been created, and phone-to-mac-c access is not working; automatic multi-device authorization cannot be claimed complete.

## 2026-09-21 user confirmation: the creator only authorizes device enrollment

After enrollment, members access one another as peers. Any device going offline (including the creator) does not affect access among other online devices through Muster. When online, the creator is an ordinary service target too. Offline status is not revocation and does not change other members' permissions. The creator is not a required online node for ongoing authorization, directory, forwarding, or credential maintenance. Pending enrollment approvals may wait; existing-member access may not.

The former “creator centrally manages all permissions” proposal is no longer confirmed product design. Post-enrollment permission-change/revocation governance will be defined separately. The network layer still uniformly enforces service scopes; removing centralized dependencies does not justify weaker signature, expiry, or revocation checks.

**Implementation gap:** `NETWORK_DISTRIBUTION_LIFETIME` is currently 90 days, `distribution_step` requires Master renewal, and access still checks distribution expiry. The implementation therefore establishes independence from an online Master only within valid-record windows, not indefinite independent operation. Member credential maintenance, cold-start discovery, and listener lifecycle must remove creator-online dependencies; the specific protocol needs separate design and validation. This round corrects design only, without changing these runtime mechanisms.

Master renewal, global permission governance, and field results in older sections describe historical/current implementation and do not override this section's latest product principles. All platform pages follow the [Devices-page design](agenthalo-device-page.md).


## 2026-09-14 service-authorization integration

The source adds `NetworkRemoteAuthorizationV2`, produced only from persisted membership chains and Master distributions. It binds the real hardware SPKIs of local device, requester, and target, the full record generation, and expiry. It cannot be constructed from JSON and neither promotes members to Owner authority nor fabricates legacy `GrantV2` objects.

MST2 adds an explicit Master-policy entry point, retaining the existing bounded frame format, session deadlines, nonce deduplication, and rate budgets. Clients generate exact distribution selectors from opaque authorization; servers obtain requester identity from verified TLS sessions and check that the target is local. Only AgentHalo read/write/approval operations are accepted. The legacy Owner Grant entry remains separate, without fallback to the other entry after authorization failure.

New software tests cover a phone enrolling through a second computer, accessing both computers after Master distribution, recovery and requests while Master is offline, plus wrong TLS identities, target substitution, cross-network objects, stale selectors, restricted capabilities, revocation, expiry, and rejected requests consuming rate budgets.

Client TLS integration adds `create_network_certificate_proof_v2` and `verify_network_peer_certificate_v2`: the local hardware identity issues a network declaration for an existing persisted certificate, explicitly binding the full distribution selector, device, DER, SPKI, and a validity period of at most seven days. Pin the target member's public key before signature verification. WebPKI still validates actual certificate expiry, usage, and signature; declarations cannot extend expired certificates.

`RemoteClientCredentialsV2::new_distributed` integrates these opaque results with existing `RemoteMst2TransportV2`, reusing actual TLS 1.3, hardware CertificateVerify, MST2, HTTP, concurrent scheduling, and write-uncertainty rules. Network state is revalidated around queuing, signing, and I/O. Software socket tests verify real TLS/MST2/HTTP exchanges between devices with different legacy Owners under one Master policy, and that closing the network rejects new requests before opening an OS connection.

The server adds opaque certificate-admission snapshots `NetworkTlsClientAdmissionV2` to the actual TLS handshake and pre/post-connection checks of `RustlsRemoteServerV2`. The entire declaration batch is verified before activation, constraining targets, duplicate members, exact certificate hashes, distribution state, and wall/monotonic deadlines. A busy runtime rejects rather than blocking sockets. A declaration's entire validity interval must fit inside the actual DER interval to prevent cached declarations outliving certificates. Network mode never falls back to legacy Grants. Real backend TLS socket tests cover a phone with a different Owner and no local Grant completing the hardware-signature interface, TLS, and MST2 under Master distribution, and old connections terminating after network closure. Hardware signatures in tests still use software fixtures.

Backend request integration adds `VerifiedRemoteRequestV2`, explicitly distinguishing Owner and Master-network origins. Network requests carry opaque `VerifiedNetworkRemoteAuthorizationV2`, without fabricated Owner/Grant principals. The request resolver must share the exact network object used for TLS admission; the target comes from local signed readiness. Network TLS cannot fall back to the Owner resolver. Production `ProductTlsGenerationV2` selects a matching-origin resolver from encapsulated TLS input; a new macOS hardware-broker preparation entry can construct this generation.

Network requests reuse existing Provider dispatch, absolute timeouts, concurrency limits, and durable operation receipts. Revalidate network state before execution. The network operation namespace binds network ID, subject, and target and remains stable across distribution revisions and TLS renewal, preventing re-execution. Software socket cases now traverse the production TLS generation, MST2 resolution, HTTP, Provider executor, and BackendRouter, read Provider-returned sessions, and verify revocation after parsing denies dispatch and terminates old connections.

Shared authorization, client TLS, backend TLS/request parsing, and session Provider paths are connected. Actual application bootstrap still selects the legacy Owner path. Network certificate declarations lack a production exchange/persistence driver, and ABI, service directory, and platform calls are not wired. Server persistence ownership for network approval interactions remains disconnected; it is not inferred from Owner interaction records, so approvals remain rejected. All items below are required to close the multi-device goal:

1. **Network authorization throughout real connections**: integrate the same opaque authorization and live invalidation checks into TLS peer verification, network-target construction, Provider dispatch, and interaction ownership. Preserve hardware CertificateVerify, seven-day certificates, and renewal 60 minutes early; do not fabricate legacy Grants or skip certificate checks.
2. **Network service directory**: verify member-signed service/routing declarations and bind them to persisted Master distributions. Phones retain independent targets/connections to both computers; UI, hostnames, and old Owner lists cannot imply authorization.
3. **Real enrollment and sync**: Mac-A creates Master; both computers and phone persist signed membership chains through explicit network-enrollment transactions. Reuse hardware identities and authenticated channels. Legacy directed pairing alone cannot masquerade as network-enrollment consent. Computers do not use QR codes; phone enrollment and automatic distribution integrate HUKS, OS authentication, and protected storage.
4. **Production effects and ABI**: expose these capabilities through opaque handles; Mac/HarmonyOS execute only OS authentication, signing, storage, transport, and UI. Synchronize complete member/distribution records across endpoints, preserving correct state through restart, offline catch-up, renewal, revocation, and pending readback.
5. **Installation and device acceptance**: after signing, replace applications only, without backing up old apps or clearing pairing. The phone shows Mac-A and mac-c simultaneously and reads real sessions independently. After one distribution, verify direct phone-to-mac-c access without Master participating in the control channel. Separately verify automatic renewal, revocation propagation, restricted-capability rejection, and restart recovery.

Earlier TLS integration validation: `cargo test -p muster-transport -p agenthalo-backend --lib --locked --offline` passed: 352 transport tests, 456 backend tests, with 14 pre-existing backend tests ignored. New cases also cover concurrent network-state reads, denial during writes, signed declarations exceeding real certificate validity, wrong targets, and duplicate subjects. An old-admission-snapshot test exposed premature snapshot selection by a new check; it was fixed and the full suite rerun successfully. No signing, notarization, installation, or phone hardware acceptance this round.

Product request integration validation this round: the same full library suite passed, with 352 transport tests, 457 backend tests, and 14 pre-existing ignored tests. Real TLS session-read/revocation cases now use the production TLS generation; new tests cover operation isolation by network/subject/target and receipt reuse after restart. No signing, notarization, installation, or phone hardware acceptance.

## Product rules

- Phones, Macs, and Windows PCs are members of the device network. The UI explicitly labels “Phone” or “Computer.”
- Local Mac-A acts as Master; only computers may create Master networks. Master signs authorization distributions with its product hardware identity.
- Devices enter the same membership network after enrollment authorization. Existing members may invite new members; Master automatically signs distributions for membership changes, without each recipient manually approving the same distribution.
- Devices retaining valid Master distributions verify each other's permissions locally. Routine connections go directly to targets, without Master forwarding or continuous availability. New members and expiry renewal still need Master.
- For example, once Mac-A and mac-c trust each other, a phone enrolling through either should access the AgentHalo services each computer offers.
- Computers use discovery and code confirmation with one another; computers and phones use scanning and code confirmation.
- Offline members remain “Pending sync” and resume automatically on return. Network removals must propagate too; revoking one local Grant does not establish network-wide completion.
- Membership and service capability are separate: phones do not gain desktop services they do not offer; ordinary AgentHalo access must not implicitly become Terminal, Computer Use, or system-administration permission.

These rules supersede separate manual phone approval on each computer. Merging existing authorization records in the UI does not turn them into network membership credentials.

## Current implementation gaps

`AgentHaloLiveMobileAuthorizationService.performApproval` accepts only `currentLocalAuthorityDevice`; issuance, persistence, and ACK in `AuthorityPairingRuntimeV2` still revolve around that directed authorization.

`MobileConsumerOwnersBinding` pauses the old model when switching pairing spaces, and `RustMusterAgentHaloTransport` consumes only the current model's directory. Making both multi-active supports concurrent independently paired connections, but does not implement the requested transitive mutual trust.

`federation.rs` already has signed membership logs, quorum, and rollback/fork checks; `authority_orchestrator_reconcile_federation_input` has verification/persistence boundaries, but no complete product enrollment and automatic authorization-distribution flow. Legacy federation seeds, service-discovery results, and ordinary PC-to-PC remote-client Grants cannot substitute for that missing flow.

## Protocol integration plan

The complete plan follows; implemented scope is documented later:

1. Introduce device-network enrollment transactions in Shared Rust, binding user-confirmed enrollment to network identity, previous membership log, both hardware identities, granted service scope, and expiry. Enrollment authorization explicitly means network-wide access; do not silently expand authority using old consent for local-device access only.
2. Synchronize signed membership records through authenticated management channels. Each recipient verifies signatures and membership authorization chains, rejects rollback/conflict, and completes exact persistent readback. Relays carry content but do not decide trust.
3. Shared drives target authorization and consumer directory/certificate updates from accepted membership records. Platforms execute hardware signatures, authentication, network I/O, and storage callbacks only; do not assemble Grants, target sets, or trusted routes from display JSON.
4. Manage multiple remote connections uniformly. Selecting a computer changes browsing focus only and cannot pause another trusted computer. Every actual access uses that target's Shared opaque authorization handle.
5. Member removal, key rotation, and authorization invalidation cover offline catch-up, new handshakes, and existing sessions. The UI separately shows local persistence, remote sync, and actual connection state.

## Acceptance

- Computers A and B enroll; phone C scans/confirms only with A once, then accesses both A and B. The symmetric path where C pairs only with B also passes.
- C joins while B is offline; B automatically syncs on return without rescanning or manual authorization.
- Later devices enrolling through an enrolled phone or computer likewise synchronize network-wide.
- Shared rejects forged membership, modified targets/permissions, cross-network replay, old-log rollback, forks, and reconnection after revocation.
- Migrating existing directed pairings between two PCs preserves identities and unfinished transactions, explicitly records enrollment authorization, and does not delete pairing data to rebuild state.
- Separately accept Shared tests, platform bridge tests, signed installation, and real simultaneous access across two computers and a phone. Current documentation/UI changes do not satisfy those protocol/device gates.


## 2026-09-13 implementation progress

Implemented:

- `muster-transport/src/device_network_v2.rs`: a signed device network independent of authority quorum. Network identity binds the root hardware public key and OS randomness. Enrollment records bind previous head, sequence number, fixed AgentHalo remote-client policy, issuer/joiner hardware public keys, randomness, and a 15-minute confirmation window. Signature encoding uses a separate domain and fixed binary fields, not JSON ordering as a canonical form.
- Enrollment requires both existing-member and joining-device signatures on the same record. Shared emits the same verification code for both parties; signatures and codes bind network and predecessor record. Other members verify/catch up from complete signed records without signing new-member authorization again.
- New members can invite more devices, including later computers invited by an enrolled phone. Membership neither promotes them to owner authority nor constitutes a target Grant, TLS identity, or remote-client handle.
- Membership records take effect only after exact OS persistent readback. Failure retains the original candidate; retries cannot replace pending requests, and abandonment must prove disk still contains the original predecessor. Shared exposes separate persistence effects, candidate head, and predecessor head; platforms do not reconstruct business state from display JSON.
- Shared checks additions, removals, offline multi-step catch-up, wrong signatures, altered policy, cross-network replay, old logs, forks, and reordering. Membership-check handles invalidate during pending persistence, removal, and closure; revoked records are retained.
- ABI 1.88 exposes runtime/draft/membership opaque handles. A new lock-free retire hook immediately stops membership checks even while NAPI workers retain the runtime or verify, without waiting for the state lock.
- Apple wrappers provide only typed ABI, exact signature bytes, and storage effects. HarmonyOS verifies membership on its existing bounded asynchronous queue of eight workers; no signed-state parsing on the JS thread and no alternative TCP/hardware-signature implementation.

Software acceptance:

- Shared three-device tests use software P-256 keys: A invites B; phone C is invited only by B; A accepts C after sync; C's membership checks cover A/B and C can invite a fourth device.
- Rust C ABI and Apple wrappers repeat the same path, wrong-hardware checks, exact readback, restoration, and revocation. C ABI also proves membership handles cannot be passed to existing RemoteTarget constructors.
- HarmonyOS host NAPI uses production C++ workers and the current Shared static library to test the three-device path, eight concurrent membership checks, destruction after enqueueing, boundary inputs, and revocation. Software keys do not establish HUKS, UserAuth, or phone UI acceptance.

Current limitations and next work:

- Mac has a production constructor reusing the product SEP key, LocalAuthentication, and encrypted CAS storage, but pairing UI/management channels do not call it yet. Phone HUKS/UserAuth/storage-effect drivers are not connected; existing directed pairings between computers are not automatically migrated into enrollment transactions.
- Automatic management sync, target-computer Grant issuance, phone multi-target directories/connections, and service/certificate freshness checks are not connected. This membership-layer code therefore does not change installed-app service authorization.
- Restore requires the local hardware public key to match the saved latest head. That head is protected-storage input, not an independent hardware anti-rollback witness. Final persistence adapters must save Shared document and head in one exact transaction and complete existing freshness/recovery boundaries.
- Logs are limited to 512 entries and 512 KiB total input, accepting only append chains with exact predecessors. Concurrent enrollment forks are explicitly rejected; no automatic re-signing/coordinated merge exists, and independently created networks cannot merge by concatenating logs.
- No API exists for revoked-hardware-key recovery, log compaction, or network merging; clearing saved state must not bypass this.
- This version was not deployed with signing/notarization, and network-wide access was not verified on Mac-A, mac-c, or a real phone. Seven-day TLS validity and 60-minute-early renewal remain on their original independent path.


## ABI 1.89 enrollment driver and Mac OS adapters

- Shared `NetworkEnrollment` defines five transaction types: create network, invite, accept invitation, revoke, and sync. Platforms execute typed approve/sign/exchange/persist/complete/cancelled effects only; Shared chooses signature slots. Syncing an existing network does not require repeated user signatures, and an empty device cannot bypass enrollment confirmation through sync.
- Invitation replies must match this transaction's exact signed predecessor, operation, and inviter signature; another validly signed enrollment record cannot replace this confirmation. Every effect checks source runtime and current saved record; another concurrent commit invalidates old effects.
- Protected storage uses a single bounded Shared record containing log and head. Shared permits enrollment-completion export only after whole-record CAS and exact readback. Wrong readback retains the candidate; a received but unpersisted revocation blocks membership checks and sync export.
- Restart can restore from protected records and export the current persisted signed log for management resync/lost-reply recovery. This does not depend on a live pairing window or prove remote receipt/persistence. Uncommitted invitation transactions do not survive restart; catching up signed records afterward requires an authenticated sync channel.
- `AgentHaloDeviceNetworkCoordinator.standard()` reuses the product identity's SEP key, explicitly authenticates network-wide mutual trust, and uses a separate `device-network-v2.sealed` slot with existing encryption/cross-process CAS adapters. It does not replace signing keys, authorize enrollment using old Grants, rewrite owner slots, or clear pairing data.
- The Mac driver retains the original transaction across awaits, prevents reentry, and rereads protected records before signing/completion output. Hardware/storage retries reuse the approved transaction; lost persistence results allow completion only via exact candidate readback.
- HarmonyOS adds protected-record recovery and complete enrollment-effect calls within its existing eight asynchronous slots. Source-network closure rejects operations before locking, and waiting work cannot revive it. This is still native bridging, not an ArkTS product driver or real HUKS acceptance.

Validation: 335 Shared tests, 110 C ABI tests, five Mac enrollment-driver tests, and nine targeted Apple CoreClient regressions passed; HarmonyOS host scripts and the new enrollment suite passed. The full cross-adapter PairingContract suite received syntax checks only, not execution; no real hardware/network acceptance.

Software coverage includes Shared/C ABI enrollment ordering, valid but mismatched replies, wrong hardware, expiry, revocation/closure, no commit without an output pointer, exact persistence, and restart recovery. Mac driver software signing/storage/authentication fixtures cover three devices, further invitations by the phone, authentication denial, storage corruption, retry, and lost write replies. HarmonyOS host tests the same enrollment flow through actual C++ workers/C ABI/Shared.

Real network-wide service access is still unavailable: pairing UI, authenticated management sync, automatic target Grants/revocation for network members, phone multi-target directories/connections, and independent freshness/rollback witnesses remain disconnected. Membership logs reject known stale predecessors/forks only; rollback of the complete protected record together with its head cannot be detected from that record alone. Local Cancel cannot recall signed invitations already sent; revocation still requires signed member removal and propagation. These limitations remain explicit; no signing, notarization, or deployment this round.


## ABI 1.90: Master distribution and management channel

This implementation follows the user's new rules:

- The genesis product hardware key is Master. Shared creation is restricted to macOS, Windows, and non-OHOS Linux computers; iOS/Android/OHOS cannot create Master networks. Harmony Rust target `target_env="ohos"` was checked. This is a product-platform restriction, not manufacturer attestation of remote form factor; remote trust still comes from explicit enrollment confirmation and signed chains.
- New fixed signing policy: `agenthalo.remote-client.master-distribution.v2`. Previous undeployed transitive membership records cannot directly authorize under the new Master policy. Existing production directed Grants are fully retained and cannot be projected as new network membership.
- Master automatically issues one bounded authorization distribution for all currently valid members, binding network, Master key, computer role, membership-log head, increasing revision, fixed AgentHalo policy, and 90-day expiry; renew 60 minutes early. This is the network distribution's lifetime, separate from TLS certificates valid for at least seven days and their 60-minute rotation mechanism.
- Distributions activate with exact whole-protected-record CAS/readback. Older interfaces confirming only member documents cannot activate or abandon persistence transactions containing new Master authorization. Rollback, same-revision forks, wrong Master signatures, and cross-network records are rejected; locally accepted revocation immediately restricts prior distribution permissions.
- Management requests/replies are signed by both product hardware keys and bind exact request digest, sender/recipient, randomness, and a 120-second window. Shared validates log predecessors, currently valid senders, and distribution versions; it can pull newer state or push offline catch-up. Replies are sent only after local persistence; control messages do not invoke service routes.
- Shared selects endpoints for known valid members from discovery snapshots and prefers Master. Platforms execute those bounded endpoint indices and Shared's polling cadence. Bonjour labels locate peers but do not establish trust.
- Mac `AgentHaloDeviceNetworkModel` joins workspace lifecycle and device-management UI. Existing Network.framework transport adds separate `_agenthalo-net._tcp` service and management message bounds while retaining original computer-pairing windows/limits. Startup only restores state; “Make this device Master” is explicit and OS-authenticated. Source does not infer Master power from hostnames or automatically create another Master on mac-c.
- After membership persistence, the Mac driver executes Shared's Master-issuance effects; ordinary members only persist, verify, and forward. Hardware/storage failure preserves the transaction. Automatic issuance retries remain bound to original authorization expiry and exact source state, avoiding a deadlock from 120-second system-signature delays. Automatic management sync does not repeat user authentication.
- During simultaneous polling, outbound network waits do not hold inbound processing locks. Replies commit only when local OS operations are idle; the queue holds at most eight. Shared retains exact source bindings, rejecting stale replies if membership/distribution changes meanwhile.
- HarmonyOS ABI supplies bounded asynchronous calls for the same management/distribution flow and concurrent local authorization checks. Phones cannot create Master. Host software keys do not replace HUKS or phone UI acceptance.

Actual integration boundaries remain explicit:

- Device management can create Master, display membership/distribution state, restore, and start signed management sync. Existing “Add computer/Add phone” still initiate directed pairing; complete new-member confirmation UI/migration need wiring. Old Grants were not automatically expanded network-wide.
- Master distributions and local per-operation checks are implemented, but **do not yet replace target GrantV2/TLS service admission or consumer-directory construction**. New distribution checks are not existing RemoteTarget/service Grant handles; this code does not establish phone access to both PCs.
- Distributions must enter Shared service admission alongside actual target TLS identities, specific AgentHalo capabilities, revocation, and freshness. Network distribution does not grant Terminal, Computer Use, or system administration.
- Protected records still lack independent hardware rollback witnesses; simultaneous full-history rollback cannot be detected from them alone. Master transfer/recovery, log compaction, independent-network merging, and long-offline revocation freshness lack product flows.
- This round neither deployed nor operated real Master configuration, mac-c, or phone, and does not claim real dual-PC/phone connectivity.


Validation: 341 Shared Rust and 110 C ABI tests passed; seven Mac driver and three real local TCP transport tests passed. New targeted regressions cover simultaneous bidirectional sync, delayed hardware signing, renewal 60 minutes early, local rejection after Master revocation, and restart recovery. TCP tests first failed because the sandbox prohibited listeners, then passed with temporary local ports allowed. All HarmonyOS host scripts and new Master-distribution tests passed, covering eight concurrent local authorization checks, checks after Master closure, protected-record restart recovery, and non-Master issuance rejection. Hardware signatures use software fixtures; production OS adapters compile, but real SEP/HUKS, signed/notarized installation, and physical connectivity remain unaccepted.

## Shared service directory and client targets

Subsequent Shared work binds Master distributions to actual TLS identity, MST2 routing, and backend request handling. Member-signed service announcements additionally bind current distribution, device identity, real listener-publication credentials, Relay address, and target certificate. Announcements last at most five minutes and cannot outlive publication credentials or certificates.

Existing signed management requests/replies carry announcements automatically; Master can forward addresses/certificates signed by mac-c itself. Member updates require exact protected-storage readback before importing accompanying directories. Consumers obtain opaque targets from the same network runtime and revalidate membership, distribution, announcement, and certificate on connection. Device lists are display data only.

Directory caches are currently memory-only. Mobile client ABI is connected, but platform service-publication drivers/enrollment UI remain unwired, and actual startup does not select network-authorized listeners. Installed phones therefore cannot yet show mac-c using this code.

Software tests distribute mac-c → Master → phone directories using real temporary TCP listener-publication credentials and signed management messages. The phone obtains both computers' targets/TLS credentials and covers tampering, closure, expiry, and incorrect persistence readback. Full library suite: 458 backend tests passed, 14 ignored; 352 transport and 110 FFI passed. Signatures use software fixtures; no signing, notarization, installation, or physical three-device acceptance.

## ABI 1.91: network directory and mobile read client

- `agenthalo_device_network_apply` operation 15 outputs verified service display lists; `agenthalo_device_network_remote_target_select` selects opaque targets from the same Shared network runtime. Device IDs are selectors only; caller-supplied addresses, permissions, and certificates are not accepted.
- `agenthalo_device_network_remote_client_create` builds an independent client from a target handle and an existing consumer's persisted local certificate. Legacy Owner directories/Grants do not authorize this target. The local certificate must remain valid, have exact readback complete, and match member hardware identity; pending-persistence consumers cannot export it.
- FFI network state is unified as `Arc<RwLock<DeviceNetwork>>`; clients retain the same authorization runtime. One-way atomic retirement propagates network closure/upper-handle destruction to retained Shared targets without waiting for workers. A separate serialized cache cannot continue access.
- Existing session, Provider, preview, and file-read interfaces reuse network clients. Mobile write transactions still support only legacy Owner Grants, so network clients explicitly reject send/attachment transactions and presentation does not claim those writes available.
- HarmonyOS reuses its eight asynchronous slots: `deviceNetworkCall` operation 20 selects targets, operation 8/option 15 reads directories; `securityV2RemoteCall` operation 24 creates network clients, with the retained consumer certificate handle as seventh argument. Platforms do not parse authorization records to reconstruct connection state.

The actual phone still shows only mac-a's 431 sessions. This section's code is not installed; ArkTS enrollment drivers/multi-device directories, real Mac service publication, and network listener startup remain incomplete. mac-c cannot be reported available on the phone.

Validation: 352 Shared and 111 FFI tests passed, as did targeted backend three-device directory tests and all `build-host-napi.sh` suites using the current ABI 1.91 static library, including directory selection and wrong-handle rejection. Initial retesting found an old test treating new operation 15 as invalid and a new assertion incorrectly assuming a historical directory fixture had only five-minute validity; corrected assertions passed. Host linking still warns about macOS 26.5 objects versus a 26.0 target. Host results are not phone HUKS, signed installation, or physical connectivity acceptance.

## ABI 1.92: production management channel and GUI/backend shared protected state

The Mac backend's existing product-byte entry accepts separate bounded MNM2 management frames. Shared verifies member-signed requests, persists protected records exactly, automatically issues/persists Master distributions, then signs replies. Management frames grant no product-request rights. The backend lazily reads a GUI-created network and rejects missing enrollment state rather than creating one itself.

The TLS broker uses the GUI's existing `device-network-v2.sealed` file, lock, SEP key blob, and AHSB1 encryption format, without identity migration or protected-data duplication. A fixed slot permits GUI/backend access after signed-identity checks; backend writes must carry the original record. OS adapters supply only raw protected bytes, CAS, hardware signatures, and time; Shared owns protocol/persistence state.

Shared adds protected-record observation; ABI apply operation 16 lets GUI catch up to valid backend-persisted records. Acceptance stops the old UI operation; the next automatic sync continues. Known historical rollback, forks, and lost pending member updates close the old authorization runtime. When two local Master issuers race CAS, only an unpublished local distribution candidate may yield to another valid saved result; received distributions still strictly reject same-version forks.

Validation: 353 transport, 461 backend (14 ignored), and 111 FFI tests passed, plus eight Mac coordinator tests, all Host NAPI suites, broker software-encryption interoperability, and code-identity rejection tests. Mac Shared release static libraries built. New Mac tests initially omitted two `try` keywords; corrected tests passed. Software-signature fixtures were used; no signing, notarization, or installation, and real signed-backend broker admission remains unverified.

Rereading the actual phone layout still shows only mac-a's 431 sessions; SSH confirms mac-c backend is alive. Actual Mac service publication/network TLS admission, phone management exchange/enrollment driver, and device-directory UI remain unwired. These management tests do not complete the multi-device goal.

### Phone-to-Master management exchange

ABI 1.92 additionally exposes `agenthalo_device_network_management_exchange`. HarmonyOS `securityV2RemoteCall` operation 25 takes a signed-request handle and the paired-target handle as its seventh argument. Shared checks exact target/request matching, carries MNM2 over existing raw TCP/WebSocket connectors, and enters pending persistence only after verifying the full signed reply. Platforms continue exact persistence using management effects 0/5/6/7/8; no platform-supplied endpoints, trust flags, or authorization projections are accepted.

Network runtime locks do not span OS I/O. Before each partial read/write, recheck network, source record, target validity, and the absolute 30-second exchange deadline. This bounds one network call only and does not alter renewal 60 minutes early. Concurrent calls for the same request fail fast; completion/failure cancels its OS connection resources. Real local TCP tests use production Shared clients/backend management drivers to obtain mac-c's signed announcement and verify wrong targets, invalid routes, request replay, and revocation after connection stop before any management frame is sent.

Validation: 353 transport, 461 backend (14 ignored), and 111 FFI tests passed; all Host NAPI suites with current static libraries passed, including wrong-handle/argument rejection for the new operation and continued use of the original request afterward. This adds no claim of installed-client availability.

Live checks also confirmed `State/device-network-v2.sealed` was absent on both Mac-A and mac-c: legacy bidirectional pairing had not enrolled them in the new Master network. The phone still shows only mac-a. Next, complete production service publication/network TLS admission, phone enrollment/persistence/sync drivers, and device-directory UI, then sign/replace installations and accept actual enrollment across two Macs and phone.

## ABI 1.93: computer enrollment UI

Mac device networking adds separate “Join Master network” and “Invite computer to network” flows, without reusing directed pairing as enrollment authorization or using QR codes. The receiving computer explicitly opens short-lived `_agenthalo-join._tcp` discovery; Bonjour TXT carries only existing OS public keys. Neither display names nor public keys directly grant membership.

Before invitation signing, both parties compare device verification codes generated after Shared verifies public keys. The inviter freezes the selected public key and generates a signed invitation only after the user confirms a match and passes local OS authentication. Shared verifies the invitation on the recipient, displays its verification code, then the user compares, authenticates, signs, and persists exactly. After the inviter verifies/persists the reply, the original Master driver issues a distribution; ordinary members catch up over the signed management channel. Device codes are display digests, not membership credentials or replacements for signed invitations.

New system Bonjour declarations cover pairing, management, and enrollment services. Platforms only discover metadata, carry opaque proposals/replies, perform OS authentication/hardware signatures, and persist exactly. Shared retains enrollment phases, codes, membership, and distribution decisions. UI cannot infer completion from TCP success or device lists.

Source validation: 353 Shared and 112 FFI tests passed, plus nine Mac coordinator and three actual network-transport tests. New cases use actual NWConnection/NWListener, both Shared signatures, protected-record persistence, then receipt/restoration of Master distribution. All Host NAPI suites passed. Initial Mac compilation used a nonexistent local-ID field in a display structure; reverting to the existing field passed. Identity/authentication use software fixtures; real enrollment still awaits both computers' user confirmation and OS authentication. Signed installation and physical acceptance must be recorded separately.

### Actual deployment progress on both Macs

On 2026-09-14, the release package from commit `3930e5e778ead32710dbf7b1fcbb9347fe05e5b5` passed Developer ID signature checks and embeds ABI 1.93. The real signed backend passed broker code-identity checks; ordinary executables were denied. Six CoreClient tests passed. No new notarization was performed for this package.

The same package replaced installations on Mac-A and mac-c, without backing up old apps or clearing app data. Both GUI SHA-256 values were `c81698d6ec1142abf778f7b0c5baa575a8546ae322052d3e7882a033317de36d`, backend `ce08a75ae8956c1ff7f41cbc54e9933d0cb08f109b2ab0be0728d90a3839a854`. Health checks passed on both; mac-c broker was running and port 8765 listening.

The user confirmed and authenticated Master creation on Mac-A. After replacement, the actual GUI again showed “This device is the Master computer,” “Master: Mac-A,” “1 network member saved,” “Management channel enabled,” and saved Master-signed authorization. Mac-A still displayed mac-c's 22 sessions through legacy pairing; this does not prove Master-network access.

Mac-A opened “Invite computer to network” discovery. The new mac-c app was running but enrollment remained incomplete. After reconnecting screen sharing, the image updated and apps opened, but device-management mouse clicks still caused no UI transition; awaiting the user to directly open “Join Master network” and compare device codes. Installed packages on both ends or a single-member Mac-A Master record do not establish enrollment of both Macs. Phone Master enrollment, multi-target directory, and production network TLS/service publication remained incomplete.

### Two-Mac enrollment acceptance (2026-09-14 17:56)

After the user enrolled mac-c, both actual UIs showed two saved network members and Master-signed authorization, with Mac-A as Master. Both `device-network-v2.sealed` files were 3784 bytes, persisted at 17:53:09 and 17:53:14 respectively. mac-c health via `backend.sock` passed; 8765 is the product TLS/MST2 port and cannot be checked with plaintext HTTP. This accepts enrollment and management sync only, not phone membership or access to mac-c sessions.

## ABI 1.94: production service distribution and network TLS channel

Signed management messages add bounded member certificate-proof attachments. Shared validates attachment hardware signatures, current Master distribution, actual DER/SPKI, and validity before adding them to volatile caches. Attachments accompanying new member/distribution records cannot be used for TLS until exact protected-record readback. Bad attachments cannot prevent valid revocation persistence. Caches are not stored in membership records; duplicate attachments do not extend monotonic deadlines. Hardware signing occurs outside the live network lock, with source records rechecked afterward.

The actual Mac backend retains installed certificates, hardware signers, activation handles, and deadlines in one TLS generation. Network clients explicitly select Master channels using `network.<certificate-selector>.cert.agenthalo.invalid`; those channels bind both network certificate admission and MST2 resolver. Missing matching channels reject connections without Owner Grant fallback. Legacy Owner channels remain for unmigrated connections. Network channels do not inherit legacy Owner remote-member allowlists, but retain the same local hardware/listener-state checks.

The backend supervisor obtains public route hints from verified Owner registration plans and uses credentials from the actually bound listener for network service announcements. Announcements enter management replies only while the network TLS generation is installed and the listener remains available. Mac Bonjour management listeners forward opaque signed messages to the same backend driver via a new local Unix-socket ABI, letting GUI and product management channels exchange actual backend service endpoints/certificates rather than maintaining mutually invisible announcement caches.

Validation includes: a phone with a different Owner obtains real test-Provider sessions through a Master TLS channel derived from the original Owner listener; revocation denies streams and queued requests; production management drivers publish the mac-c endpoint/admission through a real Unix socket; phone certificates forwarded through Master remain unusable until exact target persistence; wrong signatures, retirement during signing, and TLS-selector fallback are rejected. Passed: 462 backend tests (14 ignored), 119 client, 112 FFI, and 356 Shared; seven Swift CoreClient tests and all current Host NAPI suites. The stale ABI 1.90 constant in C linking checks was updated to 1.94.

This is a code/software validation record; signed packages, replacement installations, and real announcement verification are recorded separately. HarmonyOS Master enrollment, HUKS protected records, certificate-publication driver, and multi-target directory UI remain unwired. These tests do not prove phone access to mac-c.

### Actual ABI 1.94 deployment and cache-direction issue

Commit `6a09189f819bca8327be366bfe7833b1c689d2f7` produced a clean signed release package replacing both Mac installations. Both reported ABI 1.94; read-only calls from signed GUIs to real brokers returned `read_completed`, while ordinary executables failed code-requirement checks. GUI SHA-256: `3a099474377b0051b10d4e4b76d65ae844317096a74433e9383800b7c3593963`; backend: `e26198823c2fbc2963ceabbc762629db4913d2724eeb50f8256a0ff2fcc7dda8`. No old-app backups or pairing-data changes.

Actual local read-only `/device_network/services` was empty on both machines. Bonjour discovered each peer, system network logs showed management TCP connections, and sampled backend supervisors ran normally. Data-flow review found each GUI absorbed only the peer backend's announcement, then sent it back on the next poll; local announcements never entered the local GUI's send cache. This deployment therefore cannot be marked as passing service distribution.

ABI 1.95 adds Shared local-backend-reply ingestion. Before forwarding a reply, GUI supplies the original request and local backend's signed reply to Shared. Attachments are cached only after checking both signatures, request digest, nonce, target, deadline, and consistency with current persisted member/distribution records. This path does not change membership records. A regression uses actual backend replies and independent GUI caches to reproduce the two-process direction, then verifies the next poll to Master carries mac-c's announcement. Altered attachments are rejected.

ABI 1.95 passed 356 Shared, 113 FFI, nine Mac coordinator, seven CoreClient tests, and all Host NAPI suites. Initial Swift compilation found a new C declaration used `void *` instead of the existing opaque-handle type; it was corrected and passed again. New backend dual-cache/actual Unix-socket regressions passed. Signed deployment and real directory results follow.

### ABI 1.95 signed deployment and bidirectional service discovery (2026-09-14 18:48)

The release package from clean source commit `7d02d68471ca9b9ebb514a4180a2b6389886da64` was signed with Developer ID (Team `89LGY6BD53`) and replaced Mac-A/mac-c installations. ZIP SHA-256: `226bde9b1e1bde1f22a6796408f23099d261d3ddc7532b484426e7e3ee7e84ea`. Actual GUI SHA-256 on both: `0e84b91f00025b2ffef6c15eadf010ac60fe59326c8b85b2d1d779f361391ece`; backend: `4a19d5a9f85e89d1a3781b01b11d6041a54c9e17be9a609cb16321da152abeea`. Both runtime reports showed ABI 1.95. Deep strict signature checks and real signed-GUI broker read-only calls passed; ordinary programs failed code-requirement checks. No new notarization; applications only were replaced, without old-app backups or app-data cleanup.

After management polling, actual Unix-socket `/device_network/services` returned `mac-c` (`example/mac-c`) on Mac-A and `mac-a` (`example/mac-a`) on mac-c. Both services were `agenthalo`, capability version 2, forward port 8765. This proves both real backends verified and received peer-signed announcements, resolving the ABI 1.94 empty-directory issue on devices. Both `/healthz` checks passed; one local five-second timeout occurred after installation, then passed without restart. Sandbox socket failures are not evidence of actual service failure.

Acceptance limits remain: no phone Master membership, phone multi-target UI, or real mac-c session reads over Master channels were verified. HarmonyOS enrollment, protected storage, and runtime-driver wiring remain incomplete; this hdc query also found no usable target. Both Mac logs recorded listener rebuilding after `remote listener runtime readiness expired`; bidirectional directories do not establish that stability problem is fixed. Full E2E and sustained-connection acceptance remain pending.

### ABI 1.96: HarmonyOS certificate publication and enrollment OS executor

Shared adds network-certificate publication from exactly persisted consumer certificate plans, exposed through C ABI and asynchronous NAPI operation 26. Callers provide only opaque network/consumer handles; Shared signs through retained consumer HUKS callbacks and accepts no platform-assembled certificates, declarations, or membership identities. Signing holds no network lock; afterward, recheck certificate expiry and original network record. Repeated publication neither repeats signing nor extends certificate validity. Legacy Owner-directory invalidation is not a source of new network authorization.

`HarmonyDeviceNetworkStore` persists one Shared network record in a separate device-level HUKS container, independent of the current Owner storage slot. It supplies bounded exact CAS, recovery after lost readback, and cross-instance write serialization. Missing keys when reading existing ciphertext cause rejection, not replacement-key creation. `HarmonyDeviceNetwork` only adapts existing phone hardware identity, OS authentication, entropy, byte transport, and this protected store. `DeviceNetworkEffects` follows Shared's next step for enrollment, signing, exact persistence, management sync, and network-client selection, without a phone Master-creation entry point.

The executor retains dispatched management plans and pending enrollment transactions. Management effect 0 now distinguishes Exchange awaiting a reply from Persist after receipt, allowing the same persistence operation to recover after a lost NAPI completion. Display reads no longer import candidates and discard unfinished transactions. Closing runtime first retires network permissions; late authentication/signing results cannot continue persistence or return client handles.

Validation: 357 Shared tests, 114 FFI tests, and all Host NAPI suites passed. New actual-NAPI executor regressions cover signed invitations/SAS, one authentication, successful persistence with lost result, restart recovery, signed distribution and lost management-worker results, closure during authentication, and bad-storage rejection. HUKS software OS tests cover chunked AEAD, same-slot CAS, Owner isolation, and missing keys. Initial C++ compilation found a version check mistakenly inserted into a struct; it was fixed and compiled on both platforms. Initial sandbox FFI Unix-socket tests hit permission restrictions; the full suite passed after authorization. HarmonyOS native libraries and unsigned HAP built; hdc again recognized target `5NC0225C10020352`.

These remain runtime-driver/software validations. The executor is not yet connected to phone enrollment UI, invitation delivery, or session-directory entry points. This section includes no signing, phone replacement installation, real HUKS enrollment, or mac-c session-access acceptance. Both installed Macs remain ABI 1.95. Next connect pages/current remote flows to the executor and verify real distribution and TLS/MST2 reads. Management location while Master is offline, cold-start certificate recovery, and observed listener-readiness expiry also still need acceptance.


### ABI 1.97: phone enrollment requests, bilateral confirmation, and session-directory wiring

A separate `MNJ2` byte channel lets phones send hardware-signed enrollment requests over existing paired-computer routes. Requests are bounded, time-limited candidates and inherit no legacy Owner authority. Shared generates signed invitations only after Master selects the device, compares its code, and completes OS authentication. The phone compares the invitation code, consents to network-wide mutual trust, authenticates, persists exactly, then returns Shared's full signed document. Master verifies/persists through the retained enrollment plan. Transport submitted/committed hints grant neither membership nor TLS rights.

Phone pages connect device-level HUKS records, retained-transaction retries, Master-signed distributions, consumer certificate publication, and verified service directories. Selected targets are accessed through Shared network-target/network-client handles; no fallback constructs legacy Owner Grants from UI fields. Foreground polling waits for Master persistence, then syncs signed management messages; restart reads existing protected records and restores sync.

Mac adds “Invite paired phone to network.” Local Unix-socket management only transports candidates, invitations, and signed replies. Supervisor retains pending requests across listener rebuilds, preserving the original 15-minute deadline, hardware identity, signatures, and current protected-record checks. Busy-state checks were fixed for interleaved UI polling/confirmation while waiting for phone replies.

Validation: 359 Shared, 114 FFI, 119 client, and the prior full backend suite of 462 tests (14 ignored) passed. New targeted tests passed for actual MNJ2 TCP-channel isolation and request retention after listener replacement. Mac app 414 tests, CoreClient seven, all Host NAPI suites, and new actual Shared/NAPI phone-executor tests passed. The latter cover requests not writing membership, bilateral SAS, authentication, phone-first persistence, Master-second persistence, and transport status not creating clients. Production HarmonyOS ArkTS/native libraries and unsigned HAP built. These are software checks; signed installation/device acceptance are separate.

This device check found `/device_network/services` empty again on both Macs, with `remote listener runtime readiness expired` still logged; the earlier single successful bidirectional discovery does not establish sustained stability. Losing pending requests during lease rebuild is fixed, but seamless listener renewal remains incomplete. Phone Master enrollment and actual mac-c session reads still await device acceptance; code/build success does not complete the goal.


### ABI 1.97 signing and replacement installation on all three devices (2026-09-14 20:50)

Clean commit `f8eebb18f5e3333c97efc33dc48bf4191af16d6a` produced a Developer ID release Mac package reporting ABI 1.97. ZIP SHA-256: `741c0e5cbda468936cbbed404a9c3893094a6ef7f549f01dddd775fd938867d0`. Mac-A/mac-c replacement installations succeeded; both GUI SHA-256 values were `26b3a30f26bc2d707c40d4e445d308d58d9e637ce5743477f6288028973aca5d`, backend `30dd3f9b1cee0ae76fd0dae9ff3e7217eeceba1587465211cc5d3f51eb054150`. Strict signatures, actual ABI reports, health checks, and signed GUI→broker `read_completed` read-only calls passed on both; no new notarization this round.

The first local installation could not quit normally because a device-management sheet remained open; the installer stopped replacement and restored the backend. After closing the sheet and quitting normally through the app menu, retrying the same installation succeeded, without forced termination. Both ends replaced applications only, without old-app backups or application/pairing-data cleanup.

HarmonyOS target `5NC0225C10020352` successfully replaced its signed HAP using hdc `install -r`, preserving app identifier `6918740782471784453`. The developer leaf certificate in the signing chain is valid from 2026-09-14 06:03:30 UTC to 2026-09-28 06:03:30 UTC; it signs HAP installation, not runtime network certificates. App/native-library ABI 1.97 has build validation, but real UI enrollment/session acceptance is still needed.

After updating, actual `/device_network/services` again returned peers bidirectionally: local saw mac-c; mac-c saw mac-a. Master's phone-request list was empty and awaiting user actions. The phone had not completed Master enrollment, bilateral SAS, or hardware authentication, so phone access to mac-c cannot be claimed. Five-minute listener-readiness lease rebuilds and sustained discovery across cycles remain open acceptance items.
