# Consumer enrollment catalog (ABI 1.80)

This is a local storage-selection primitive for multi-Owner enrollment.
It is **not** Owner, Grant, directory, certificate or connection authority.
Every selected enrollment must still restore and authenticate its own Shared
checkpoint/completion, exact protected stores and durable ACK before networking.

## Shared contract

`ConsumerEnrollments` owns the bounded inventory and active local slot. Absent
index starts with `legacy`; that slot maps to the original five files in
`ConsumerEnrollment`. It never copies, resets or deletes those files. New
slots use exactly 16 OS-random bytes encoded by Shared as one lowercase hex
component, under `ConsumerEnrollment/owners/<component>`. The catalog contains
only local slot IDs, a selected ID and a revision, never a Relay URL, Owner ID,
Grant, key or trusted readiness flag. It cannot deduplicate or authenticate
Owners from metadata.

Limits: 16 slots (including legacy), 8 KiB encoded catalog, version 1, unique
IDs and mandatory legacy first entry. Invalid/unknown selected IDs, malformed
paths, duplicates, unsupported schema, overflow and exhausted revisions fail.
Capacity never prunes pairing evidence. No remove/reset API is provided.

Add/select prepares an exact candidate without changing active selection.
Further edits are blocked while its write is unresolved. Native storage must
CAS against the exact predecessor bytes, then return actual readback. Only
exact candidate readback commits selection. A lost write reply retains the
same candidate; reloading its exact bytes can finish without reallocating.
Abandon requires actual predecessor readback (including exact absence). It
cannot abandon a write that reached storage. Foreign readback never silently
replaces the expected predecessor or restores a new catalog in place.

Opaque storage leases are issued only for the active selection when no save is
unresolved. Each confirmed
selection retires old leases, even when later selecting the same slot again.
Close or catalog disposal retires all derived leases. A process-local epoch is
never restored from serialized metadata. Parent-state retention prevents
use-after-free, but does not keep a disposed catalog's leases usable. These
handles are rejected by authenticated remote-target APIs.

This catalog does not provide anti-rollback security; canonical protected
pairing records and their verification remain independent. A restored catalog
is an inventory hint, not proof that any indexed directory or enrollment is
valid. The OS adapter inventories immediate new-layout directories without
following symlinks; Shared rejects unknown, duplicate or unsafe components.
Missing/corrupt indexes with orphan directories block restoration/allocation,
preserving all evidence. Indexed empty slots are valid. No orphan import,
repair or deletion is inferred from filesystem names.

## Explicit catalog recovery (Shared / ABI 1.84)

`ConsumerEnrollmentRecovery` is a raw repair transaction separate from a usable catalog; ordinary restoration failure
does not automatically create one. It currently serves startup recovery before catalog activation, or recovery after all
original operations have durably converged and the old catalog has closed. Repair must not discard unresolved in-memory
operations still depending on that catalog. Live handover of a catalog with pending models is not implemented.
Adapters fully scan directories without following symlinks and obtain the exact decrypted bytes of the original encrypted index.

The transaction can import valid orphan directories when the index is absent, add omitted directories to a valid index,
and repair duplicate slots, missing `legacy`, or inconsistent selection in a decodable version 1 index. It preserves valid
empty slots, a valid original selection, and all actual directories; it deletes no pairing files, keys, or unknown records.
Capacity overflow, invalid old slots, unknown versions/fields, and undecryptable or undecodable indexes still block;
there is no fallback reconstruction.

Pre-write checks require the current index to equal the original bytes and the scan set to equal the original scan;
the OS must still perform exact CAS. After writing, reread the index and fully rescan. Confirm only if both candidate bytes
and directory set match. Added, missing, duplicate, or invalid directory names retain the candidate and reject confirmation.
Scan order does not affect set comparison. A lost reply confirms the same candidate directly, without rewriting.
Abandonment requires the actual original index to remain unchanged; neither abandonment nor closure returns a usable catalog.
If index and directory scans cannot be atomically locked, subsequent storage access still requires per-operation checks;
these observations are not hardware or network authorization.

Repair candidates persist `selection_required: true`. After exact repair confirmation, the returned catalog may only display
and prepare an explicit selection, not allocate new slots or issue storage handles. This restriction survives restart
and lifts only after explicit selection and confirmation of its actual write. Display `selected` is then only a placeholder;
UI must follow Shared's `selection_required` with “Select a recovery slot,” not show a connected Owner.
Ordinary catalogs omit this field, retaining the original encoding shape. Older versions unable to read repaired indexes
with the new field must block rather than reset. Recovery supplies neither rollback resistance nor Owner authorization.

ABI exposes `agenthalo_consumer_recovery_create`, `apply`, and `confirm`. All inputs are at most 8 KiB; only one successful
confirmation can produce a catalog handle. Storage and remote-authorization APIs reject recovery handles.
Shared unit tests and actual C ABI cover read/write mismatch, directory races, lost replies, closure, null output pointers,
and cross-handle rejection. Native wiring follows; this is still not real HUKS/Secure Enclave acceptance.

### Mac cold-start recovery wiring

`AgentHaloConsumerRecovery` wraps only ABI creation, preview, original/candidate bytes, pre-write checks,
confirmation, abandonment, and closure. The current Swift catalog wrapper requires ABI 1.84 and does not infer defaults
for a missing `selection_required` field in older libraries. Shared handles still check actual storage eligibility.

`AgentHaloConsumerCatalogDriver` permits raw repair preparation only before a usable catalog exists;
it never replaces a catalog that has issued handles. Preparation/persistence reads the index, scans fully, then reads it again,
requiring both OS reads to match. Commit first attempts Shared confirmation using actual readback. If incomplete,
Shared must check exact original index and scan set before the existing encrypted-storage CAS runs. Whether the write
returns or throws, obtain fresh actual storage observations; lost replies do not replay completed writes.
Every asynchronous boundary rejects activation after stop/cancel. Already-issued writes may finish, and restart retains
the explicit-selection-required marker.

`AgentHaloConsumerOwnersModel` and UI offer three explicit actions: preview, save/confirm, and exact abandon.
Before repair confirmation, do not replace the catalog display with an active Owner. After repair, show a selection placeholder
without obtaining storage handles or creating models. Reloading a pending explicit-selection save preserves recovery errors/buttons.
No index reconstruction is offered while an original model exists; old callbacks and late errors after stop do not publish to UI.

Model/driver regressions use real Shared, explicit software OS effects, and actual encrypted files, preserving every original
pairing file. They simulate hardware and are not Secure Enclave acceptance. Actual installed prompts, pixel layout,
and multi-Owner recovery remain unproven; undecodable indexes and live handover remain unsupported.
HarmonyOS install-marker recovery follows; Mac installation paths still need independent verification.

### HarmonyOS recovery native bridge

The C++ registry exposes `consumerRecoveryCreate`, `consumerRecoveryApply`, and `consumerRecoveryConfirm`.
Each buffer is checked against 8 KiB before copying; presence tags, integers, operation numbers, and argument counts
are checked strictly. Borrow original Shared handles under the registry lock; register newly confirmed catalogs independently.
Catalog, storage, and remote APIs cannot accept recovery handles.

Host native contracts cover handle misuse, oversized/extra arguments, original-candidate retention, directory changes,
duplicate confirmation, closure/abandonment, and explicit selection after restart, retaining existing ETS/storage/Owner regressions.
Subsequent `2c49e286` connected ArkTS recovery wrappers, HUKS persistence drivers, and page/coordinator flows:
confirm the original write first, then check original index/directories and CAS. Ability destruction waits for recovery;
recreation retains the original request. Repair and explicit selection remain separate; reloading unresolved selection
keeps the save entry visible. A full unsigned SDK build passed, still without physical-device acceptance.

## Preservation-only install-marker recovery (Shared / ABI 1.85 foundation)

`ConsumerInstallMarkerRecovery` is a local transaction independent of catalog, storage, Owner, and network authorization.
Install markers only track startup bookkeeping: they prove neither identity, authority, nor durable ACK, and authorize no deletion
of pairing, legacy layouts, keys, catalogs, or pending records. Shared does not infer that an empty catalog or missing marker
allows cleanup. The transaction never accesses this material, so markers can be repaired independently even if indexes cannot decode.
Subsequent catalog/protected-state validation remains mandatory and cannot be skipped because the marker was repaired.

`ConsumerInstallMarkerReady::restore` can read the old OS marker's exact JSON encoding `true`, but carries forward no
“already cleaned, may delete again” permission. The recovery candidate is the exact JSON string `"preserved-state-v1"`,
meaning state was preserved rather than fabricating successful legacy deletion. Missing, unknown, or corrupt bounded markers
require an explicitly user-initiated original recovery transaction; ordinary restoration failure does not create one automatically.
All inputs are at most 8 KiB; errors contain no original marker content.

Recovery retains original bytes and a fixed candidate. Pre-write checks require the actual original marker to match,
and the OS must serialize CAS for that field. Confirm using actual post-write readback; a lost reply only confirms the written candidate,
without rewriting or cleaning up. Wrong readback retains the original transaction. Abandonment must prove the original marker unchanged.
Confirmation, abandonment, and closure are one-shot; only successful confirmation returns a local marker-snapshot handle.
Catalog-selection, storage, and remote-authorization APIs reject that handle; it does not prove OS state remains unchanged.
It cannot restore Owners, issue storage handles, or start networking.

ABI 1.85 exposes `agenthalo_consumer_install_marker_restore`,
`agenthalo_consumer_install_marker_recovery_create`, `recovery_apply`,
`recovery_confirm`, and `ready_check`. Operations 1/2 read original/candidate bytes; 3 checks before writing;
4 abandons after confirming the original marker; 5 closes. Other operations are rejected.
Null outputs and failed readback cannot consume the original transaction.

### HarmonyOS native startup wiring

The NAPI registry now connects all five entry points; ArkTS holds actual native handles only. Startup no longer imports/calls
legacy-storage or identity-key cleanup. Regardless of new-slot presence, legacy material cannot be cleared.
An initially missing/corrupt marker displays a Chinese-language recovery page; explicit actions prepare and confirm
the original preservation-only request. Healthy markers continue after Shared checks without introducing hardware authentication/fingerprints.

`HarmonyInstallMarkerOS` modifies only the existing preference's install-marker field. Reads/writes for the same preference directory
share a process-local serial queue; instances are not retained across operations. Before reads, SDK `removePreferencesFromCache`
releases in-memory instances (without deleting files), then disk contents are reacquired. Release must wait for the previous instance's
operation to finish, or inconsistency may result. Check the exact original value before writing and copy candidates before enqueueing.
Both success and lost replies trigger disk rereads for Shared, never trusting unflushed caches. This queue is not a cross-process lock;
actual multiprocess/storage semantics need independent verification.

Process-level `harmonyInstallLifecycle` retains the original recovery handle across ability/window destruction.
Successful confirmation only tells the current `EntryAbility` to recheck the actual marker; it cannot directly open business pages
or replace pairing/catalog/network validation. Duplicate startup merges; already-running state is not rebuilt. Windows load by generation,
old abilities unsubscribe, and late success/failure cannot change new windows. Business runtime is still dynamically imported only after
this check passes. The marker's pure-native module only allocates nonrunning state/registers entries, without starting peer workers
or hardware operations.

Production ETS/C++/Rust and software preference-adapter contracts cover original requests, separate disk/cache state, failed flush,
lost replies, competing CAS, stop, and unchanged legacy layouts; actual ability code participates in recreation checks.
SDK page parsing and real-method tests cover buttons, visibility, and late errors. These are not physical window/storage or two-endpoint
acceptance. Disk readback for other HUKS-protected preference records follows.

### Disk readback for protected Preferences records

`OpaqueHuksStore` now releases Preferences caches and rereads at each OS phase. Candidate ciphertext write/flush,
candidate disk reread/decryption, active-pointer switch/flush, pointer checks, and final active-content reread/decryption
use separate cache instances. Clear caches only after old instances exit. The original serial queue and Shared storage lease
still constrain complete operations for the same store. Only memory caches are removed, never files or keys.

Normal and dual-record activation states require exact final actual-content matching. Receipt digests derive from the final
reread plaintext, not caches preceding the switch. Rollback restores the original pointer under the existing OS protocol and must
then verify disk state. Failed reads or unrestored pointers cannot report rollback success. Catalog writes retain their no-speculative-rollback
policy; Shared's original transaction decides confirmation/retry/abandonment for failed candidates.

Host software preference adapters explicitly separate cache/disk, check old-instance invalidation, and forbid cache removal during flush.
Together with production ETS/C++/Rust and software AES-GCM, tests cover 16 pairing/remote/upload record types, dual-record states,
rollback, changed ciphertext after switching, and 512 KiB chunked storage. A falsely successful flush cannot turn pending state into
durable success. Real HUKS, SDK/power-loss/process-exit semantics, and cross-process races still require independent verification.
No rollback resistance is added or claimed.

## Apple adapters

`AgentHaloConsumerCatalog` now requires ABI 1.84 (the base catalog API began in
1.79 and rebinding in 1.80); the platform never
decodes catalog JSON to decide persistence, selection or lease validity.
`AgentHaloConsumerCatalogDriver` executes OS encrypted CAS/readback, serializes
awaited writes and rejects results after cancellation/stop. Empty startup reads
no enrollment key and creates no index. Its standard index has a separate
Secure Enclave encryption tag and sits beside the existing enrollment files.
An OS write may finish after stop; the stopped driver cannot activate it, while
a new startup can recover its exact durable bytes.

`AgentHaloConsumerEnrollmentStore.scoped` uses the typed storage lease's current
component for OS path adaptation. It checks the lease before and after awaited
loads/writes. A retired lease cannot open a newly selected slot; an already
started write can affect only its captured original slot and its result is
rejected after retirement. All five encrypted protected stores stay separate.
Returning to a retained store requires Shared to check that its replacement
lease belongs to the same catalog and slot and is currently selected. This
does not revive the old lease; in-flight operations still check their captured
original lease on completion.

## Harmony native adapter

Harmony now exposes the existing catalog create/apply/selected/component/rebind
ABI through its production C++ handle registry. Inputs are copied only after
the 8 KiB bound is checked; exact integer IDs, operations, presence tags and
argument counts reject malformed calls. The bridge holds the registry lock
while borrowing Shared handles and uses normal owned-result/disposal paths.
The catalog's own ABI 1.80 minimum is checked; no pairing or remote-readiness
capability is inferred from the existence of this local inventory.

`HarmonyConsumerCatalog` wraps these calls without interpreting persisted JSON.
Only its display method decodes the Shared projection. `HarmonyConsumerStorageLease`
checks the captured old handle before and after awaited OS access; replacing or
closing a lease cannot bless a late result from another slot. Same-slot rebind
checks go back to Shared and do not revive the original retired handle.

`HarmonyConsumerCatalogDriver` serializes OS effects, checks observed namespaces
before restoration/allocation/selection, and confirms only exact readback.
A failed write can be reconciled by a read, never by an automatic second write.
Unknown readback leaves the same candidate unresolved. Retry-save and
predecessor-proven abandon are explicit operations. Stop closes the catalog,
retires leases and suppresses activation after an already dispatched write.
Empty startup neither allocates a slot nor writes an index.

The host regression executes this production ETS source through the actual
C++/Rust library, covering lost replies, stale competing writers, stop during a
write, late lease results, parent disposal, capacity/orphan rejection and use of
catalog/slot handles as invalid network authority. OS effects in this test are
software fixtures, not HUKS or phone preferences.

`HarmonyConsumerCatalogHuksOS` provides the concrete encrypted index adapter.
Its distinct `agenthalo.consumer-catalog.v1` preference record and HUKS alias
never reuse an enrollment key. It compares opaque predecessor/candidate bytes,
serializes index writers in this process, and returns actual decrypted readback.
Catalog pointer/flush errors preserve the candidate rather than speculatively
rolling back; the Shared driver decides exact retry or predecessor-proven
abandon. Reads never create missing keys. It does not provide a cross-process
lock, an independently authenticated catalog, or rollback resistance.

The OS inventory reads immediate entries in `context.preferencesDir`, rejects
symlinks/non-files and incomplete scans (over 65,536 entries), and extracts
unique components from `agenthalo.consumer.<component>.*` names. Multiple
records within a slot legitimately yield one component. Shared alone checks
membership and component validity. Unknown/malformed namespaces, missing
indexes with existing slot files, and I/O failures block rather than disappear.
This inventories preference files, not otherwise unreferenced HUKS aliases.

`HarmonySecurityV2Stores(lease)` binds all nine pairing/credential/activation
containers, plus each target's certificates, mutation journal and five upload
slots, to an immutable Shared storage lease. New preference names and AAD use
the component prefix; key aliases hash the full prefixed legacy alias to a
bounded 64-character OS alias. The empty component retains exact legacy names,
keys and AAD. Each internal record remains bound to its original lease. The
retained group facade can replace records only after Shared's same-catalog/slot
rebind check and an explicit quiescence check covering queued remote CAS and
record OS effects. Old queued calls capture the original record before enqueue;
they never silently follow current selection. Whole OS operations check the captured
lease at execution and completion. Per-record queues and per-component remote
CAS queues also serialize recreated store objects against old dispatched work.
An old write can finish only in its original namespace, but cannot return a
valid completion under a retired lease; queued old work fails before OS access.
Install-marker and boolean-auth cleanup are explicitly unavailable for bound
catalog stores; they do not imply a Shared slot-removal authorization.

`HostNapiConsumerStorage.test.cjs` runs these production adapters and native
Shared catalog handles with software preferences/AES-GCM. It covers every
pairing/remote store, unchanged legacy paths, cross-slot ciphertext rejection,
missing-key read failure, no-key empty reads, late writes, same-slot recreation,
encrypted index CAS/retry/abandon, lost pointer replies and unsafe/orphan scans.
The SDK now compiles the catalog and Owner lifecycle through the actual App
import graph (including generated ArkTS bytecode), not only host transpilation.
This exposed and fixed previously untested ArkTS inference/throw restrictions.

## Harmony application integration

`AgentHaloRuntimeLifecycle` initializes `MobileConsumerOwnersBinding` after
the install barrier. Page initialization starts the HUKS-backed catalog driver;
there is no unscoped singleton fallback. `Index` provides Owner selection,
add, exact retry-save, predecessor-proven abandon, and current-slot recovery.
These are local enrollment slots, not authenticated Owner identity labels.
Selecting a slot alone never claims pairing or connection success.

The coordinator immediately withdraws routes and invalidates UI generations,
pauses the old model and awaits its pairing tasks before writing the selection.
An active user-authentication or maintenance operation must finish first.
Each slot retains its `MobileAuthorizationBinding`, FirstPair ceremony, Shared
transport and pending completion evidence. A same-slot replacement reuses the
model only after Shared approves the replacement storage lease. Save uncertainty
keeps the model paused until an explicit retry/abandon/recovery; Ability foreground
cannot resume a switching or inactive Owner. Terminal model shutdown cancels pairing first and
waits for active tasks before disposing their handles.

`HarmonyRemoteClient` captures one specific pairing model at construction; its
storage and directory calls never resolve through a mutable global selection.
The transport retains per-model client maps, including unsaved mutation/upload
handles, while withdrawing connection resources during switches. A recreated
client remains subject to the original model's generation and Shared's exact
rebind checks. Late old-owner workspace loads cannot publish into the new map.
UI directory/preview/creation/scanner generations reject stale results; visible
rows and selected attachments are withdrawn. Unsent text is retained in memory
by local slot/session for explicit reselection; no upload or mutation is replayed.

Install-marker preparation now never deletes identity, legacy or new-slot
material, even when no catalog exists; explicit preservation recovery is
described above. A truly absent
preferences directory is accepted only on the OS's exact ENOENT code; access
errors and unsafe filesystem entries remain failures. Global authenticated
cleanup is unavailable through the multi-Owner facade because one hardware
identity is shared by retained enrollments. No implicit deletion, Owner migration,
or unscoped fallback is authorized by selection.

`HostNapiConsumerOwners` exercises the production coordinator/storage/native
catalog with an explicit pairing-model lifecycle fixture. Separately,
`ProductionPairingBinding` checks the actual pairing model's retained live
FirstPair/transport handles, quiescence, same-slot adapter replacement, and
shutdown ordering using native/OS contract fixtures. Actual transport tests
cover equal device/session IDs across Owners, retained original clients and
late reads. The SDK parser executes the real page action/reset/read methods
and verifies the four recovery controls. None of these are rendered pixels or
physical HUKS/Relay acceptance; the full Mac issuer → Harmony native pairing
contract remains a software-key boundary.

### In-process retention of Mac Owner switches and service directories

When switching Owners, Mac must retain the original `AgentHaloRemoteDirectoryController` and its opaque Shared service-directory
session in addition to storage directories and pairing models. `suspendDirectory()` no longer clears it;
returning to the original Owner reuses/revalidates the same object. Otherwise rebuilding the directory from an ACK completion plan
would forget directory versions/key-rotation history already accepted in this process. This describes in-process retention only,
not new independent rollback resistance or durable history across process restarts.

Suspension is OS lifecycle handling: cancel timers/HTTP, ask Shared to withdraw, and wait for original-request cleanup.
Do not delete records, renew Grants, or replay pairing/pending writes. Suspended directories cannot export;
new loops cannot start before original cleanup. Withdraw again before request completion to cover late native apply generating
a new lease after initial cancellation. Failure remains terminal, without bypass through a new object.
An old loop releases only its own generation and cannot clear a new loop; multiple model suspensions await the same cleanup.

“Revalidate directory” performs the same suspension/restart of the original session, without reauthentication or pairing.
Directory HTTP/protocol repairs can then be revalidated, but authorization expiry, revocation, and context migration still need their
respective Shared recovery flows. The button does not permit restoration of permissions. Mac controller/model, Shared directory,
and existing HarmonyOS production directory recovery have separate software contracts; installed environments still need joint validation.

### HarmonyOS UIAbility recreation and in-process retention

UIAbility destruction no longer means Owner-model termination. Runtime uses one current instance to guard global network cancellation;
late background/foreground/destruction callbacks from old instances cannot affect new ones. Storage receives application-level context
from `getApplicationContext()`. New abilities do not replace existing encrypted indexes, storage handles, or pairing models,
and matching paths do not imply network authorization.

Destruction synchronously withdraws routes/cancels OS connections, forcibly invalidates the original authentication generation,
then waits for counted pairing tasks, Owner selection, and facade-initiated maintenance. Retained remote clients remain owned by their
original pairing models; unsaved results/upload handles are not replaced by ability recreation. New abilities wait for this drain;
failed draining cannot be bypassed, and repeated attachment rejects stale attachment by ability generation. Original models and Shared's
connection, directory, and exact rebinding checks still decide network restoration.

Index writes already dispatched during destruction may finish, but old selections cannot activate automatically. Unknown/incomplete
selections use existing explicit recovery entry points. Terminal `shutdown` of the original model remains non-reinitializable;
UIAbility uses reattachable suspension without reviving terminal handles. Actual process exit relies only on existing durable recovery
protocols, never on restoration of unsaved memory. Production model/coordinator, native catalog, runtime, and ability tests cover their
respective boundaries; real HUKS and OS window recreation still await physical-device validation.

Remaining: physical switching and foreground/restart acceptance, real HUKS
storage/inventory behavior, explicit orphan/index/install-marker repair,
Grant/authority-context migration and complete revocation UX. The compiled UI
is not evidence that these installed-device gates have passed.

## Verification and remaining integration

Rust tests cover legacy preservation, pending selection, lost replies, failed
writes, exact abandon/confirm, duplicate/capacity/path rejection, restart and
old-lease retirement. C ABI tests cover wrong handles, presence tags, discarded
parent handles and misuse as network authority. CoreClient tests exercise those
native APIs, rather than a mock reducer. Mac fixtures cover concurrent writes,
lost persisted replies, stop during a write and actual temporary filesystem
CAS with AES-GCM software keys and all five independent enrollment slots.
Software keys are not a Secure Enclave/Touch ID acceptance result.

The production workspace now uses `AgentHaloConsumerOwnersModel` and a native
slot selector with add, switch, retry-save and exact-unwritten-abandon controls.
Switching immediately withdraws old workspace routes, fences old directory and
QR-picker callbacks, and awaits pairing cleanup before replacing storage leases.
Per-slot pairing models retain pending artifacts; per-peer controllers retain
mutation/upload evidence. Only the active slot's verified Shared directory can
reconnect. Save uncertainty keeps the old model suspended, including on reload.
Last verified Owner IDs are display labels only, never routes or authority.

Native fixtures cover switching back, pending work, lost readback, quit during
cleanup and late callback rejection. This implements single-active-Owner
switching, not simultaneous aggregation across Owners. Physical switching,
Grant/authority-context recovery, orphan repair and installed-device acceptance
remain separate open gates.
