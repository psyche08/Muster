# Owner Grant revocation transaction

`drive_durable_owner_grant_revocation_v2` is the production Shared transaction for an explicitly
requested revocation of one exact Grant in a local target's protected history.
The Mac management button now invokes this local transaction. Propagation to
every target and HarmonyOS remains incomplete; this is not complete device removal.

## Signed catalog and Mac management UI (ABI 1.83)

`agenthalo_owner_grant_catalog` reads the exact runtime-bound, signature-verified
local target history. Shared projects issued-locally, not-yet-effective, expired,
superseded, locally revoked and pending records and their available actions.
Duplicate Grant IDs, missing pending Grants, stale runtime identity or excess
output fail explicitly; the catalog is never silently truncated. Current durable
bytes are checked again before delivery. Queries never authenticate, sign or write.

`agenthalo_owner_grant_revoke_selected` accepts only a selected Grant ID. After
explicit OS management authentication, Shared selects its retained/completed
request or generates a new random operation ID before the durable transaction.
The UI does not construct request IDs, derive authorization or replay writes.
Another Grant is unavailable while one request remains pending.

The Mac management view opens the catalog only after explicit device-management
unlock, not on ordinary startup. Revoke/resume uses the exclusive coordinator
and existing-key OS effects. The former model-level standalone authentication
was removed; Shared requests transaction authentication. After both successful
and ambiguous writes, the UI rereads the catalog instead of inventing success.
Closing the view fences late results and preserves the busy lane until dispatched
OS work returns; reopening can queue a read, never a repeated write. No history
or unresolved evidence is deleted. Labels distinguish local signed history from
phone completion, live session closure and distributed revocation.

## Durable management intent (ABI 1.82)

After explicit local management authentication, Shared validates the exact
Grant/request and first commits one bounded pending intent inside the existing
hardware-signed target snapshot. Only after its exact CAS/readback does Shared
create the Owner revocation signature. The final signed target write applies
the revocation and removes its pending intent atomically; the signed Grant
history retains the original operation and reason. No extra platform-owned JSON
state machine or separate cross-store completion flag is introduced.

At most one request is pending for a target. Another Grant, operation ID or
reason cannot replace it. Ordinary target/TLS/interaction updates retain it.
Its presence does not revoke a Grant, deny admission or authorize a retry by
itself. Signatures, explicit authentication and current runtime/target reads are
still mandatory. Absent-intent serialization remains byte-compatible with old
signed snapshots; snapshots containing a pending intent require the new reader.

If the first write's reply is lost, restart restores the signed intent before
any Owner revocation signature was issued. If the final reply is lost, restart
finds the completed signed revocation and returns the same evidence without
signing or writing again. The lower-level `drive_owner_grant_revocation_v2`
primitive still supports already-managed callers, but the production FFI uses
the durable wrapper and rejects conflicts with any retained intent.

`agenthalo_owner_grant_pending` verifies the real opaque runtime, existing target
SPKI, target signature and exact current readbacks before returning the request
or null. It invokes no authentication, signing or write callback. Its result is
display/user input only: null is not proof of revocation or global completion.
Apple wraps this query on the same bounded worker/cancellation bridge and Mac
serializes it with pairing/management without bootstrapping a missing runtime.
The recovery query is now complemented by the catalog/UI above; multi-target
and consumer distribution remain unwired.

The existing Mac live service still emits an empty `grants` list deliberately:
Relay owner-completion/cache records must not masquerade as a verified phone
terminal receipt. The separate management catalog above now supplies the UI's
local signed history without promoting those cache records into phone completion.

## Protected listener withdrawal

### Shared peer-continuity restriction (Mac observer wired; acceptance incomplete)

`LocalTargetPeerContinuityV2` is an opaque Rust restriction created from a
signature-verified target and its exact Owner runtime. It is not TLS admission,
runtime readiness, durable anti-rollback evidence or permission to publish a
listener. It records the original target digest and per-peer Grant/TLS content
digests. Each observation accepts only another verified target; native/UI code
must not parse snapshots or classify peers on its behalf.

An original peer remains eligible for this extra continuity check only while its
identity, Grant and TLS histories are unchanged. New peers never become eligible.
Changed or absent peers remain withdrawn in this process even if a later signed
branch restores their original bytes. A pending management intent alone does
not revoke a peer. Owner/runtime, shared interaction or server TLS changes,
decreasing generation/time and equal-generation forks terminally fail the guard.
Skipped generations are permitted only for this content-continuity comparison;
they are not claimed to prove an observed predecessor chain.

Actual signed pairing/revocation and certificate fixtures exercise two-phone
isolation, pending/final revocation, repeated observations, competing branches,
non-resurrection, clock reversal, changed client TLS and common server TLS/runtime
changes. These are software-key tests, not proof of socket preservation.

The Mac bootstrap now creates the guard from the same verified target used for
the consumed backend bootstrap bundle, retaining its exact Owner runtime and
hardware subject. `observe_owner_protected` verifies each signed target in Shared
before updating restrictions; decode/signature failure terminally rejects every
peer. The single observer writer exports an opaque `LocalTargetPeerRestrictionV2`
whose per-original-peer atomic eligibility can only shrink. TLS checks do not
wait on a mutex held while verifying a large snapshot. Dropping the writer
terminally rejects all read-only views. No view can authorize peers by itself.

The production TLS admission source retains the same certificate/observation
handler installed on the listener. Only after the existing runtime and certificate
admission succeeds does it check observation freshness and unchanged-peer eligibility.
The restriction therefore reaches the handshake verifier, completed stream,
resolver and independent socket/Provider cancellation polling.

Regardless of certificate-lane activity, the observer reads runtime/target, invokes
Shared verification, then rereads both exactly before updating its current target
baseline. Failure is latched before releasing an idle transaction lock. A valid
restriction-preserving external update may narrow admission while a certificate
request waits; an old draft then fails its exact CAS before any OS write. Shared
response delivery still requires its original exact target and cannot merge or
silently replay across that change. Read-only/idempotent requests no longer cause
whole-listener retirement solely because another peer changed. An actual new
certificate write still retires the listener on response completion: changed/new
client admission is not installed into an old runtime snapshot. That
admission-rebuild case remains incomplete; so does
joint signed-management-to-mTLS evidence and installed broker/hardware acceptance.

### Connection-scoped Provider cancellation

The production listener now derives a distinct Provider cancellation scope when
registering each accepted socket. Its worker passes that scope to the existing
deadline/operation-ledger path; it no longer passes a clone of the listener-wide
cancel flag. Socket-lease shutdown or drop cancels only that scope. Listener stop
still propagates to all descendants and closes every registered socket.

Verified peer binding, independent cancellation polling and the Mac protected
observer restrictions are now wired as described above and below. No new route,
authorization decision or blind replay is added. A dispatched mutation remains
ambiguous after cancellation; its detached worker stays charged until it returns.
A real two-socket/software Provider fixture checks one-connection shutdown,
continued sibling I/O/completion, worker capacity retention, lease drop and global
stop. It is not a real mTLS/hardware revocation test.

### Live TLS peer checks (production stream path)

The rustls server retains its Shared certificate-admission source in every
authenticated stream. It checks the actual TLS certificate digest and SPKI
again after handshake completion, before constructing an authorization resolver,
and before/after reads, writes and flushes. Timeout configuration also checks the
peer. Failure permanently evicts only that TLS session and closes its socket;
a later permissive admission result cannot revive that stream. These checks do
not replace MST2/Grant validation or undo bytes already sent by an in-flight OS write.

A real rustls two-phone fixture verifies that one rejected peer cannot read
buffered application data while the other connection keeps working, and tests
withdrawal between handshake certificate verification and stream delivery.
Its extra restriction is explicitly test-controlled after real Shared admission;
it is not a signed-revocation/observer integration or hardware acceptance result.
The Mac protected-state observer now supplies current continuity restrictions
for both idle and busy-lane verified updates. Common-state failures and actual
certificate-write retirement retain whole-listener withdrawal;
joint signed-revocation/real-TLS acceptance is pending.

### Authenticated socket registry and Provider waits

The completed rustls handshake now issues an opaque, cloneable peer-check handle.
It carries the actual certificate/SPKI, the existing Shared admission source and
the exact session's terminal eviction lease. There is no principal/string-based
constructor. Stream I/O, the resolver and the registry share that same handle;
MST2 resolution checks it before and after Shared authorization, even without
another stream read. It only restricts existing authorization.

Each production worker binds this handle to its already registered socket after
TLS, before constructing the resolver. Binding checks current admission and the
connection's cancellation scope under the registry lock; a stopped or already
bound registration is rejected and closed. Registry polling uses the same lock,
so a registration cannot be silently omitted by a concurrent poll. Withdrawal
after the binding check is caught at the next poll and by stream/resolver checks.

The actual accept loop polls all authenticated sockets independently of Provider
work and stream I/O. Failed checks shut down only that socket and cancel its
Provider scope. No OS protected-state reads or network effects run in admission
checks. The configured 25 ms idle accept polling interval is not a hard real-time
latency promise. Global unreadable/stale-state withdrawal remains unchanged.

A real two-phone rustls/MST2/accept-loop test blocks both Provider calls, withdraws
one actual peer's admission and verifies that its socket closes before Provider
release, the listener and sibling stay active, both underlying calls remain
charged until return, and the sibling receives HTTP 200 with the actual software
Provider response. Another real-TLS test covers withdrawal after handshake/before
registration, retained-resolver denial, shared terminal eviction, duplicate
binding and listener stop before binding. The extra admission restriction is
test-controlled after real Shared validation, not a signed OS management update.
These tests now exercise the production observer hook in Shared certificate
admission, but do not supply actual signed OS updates. Hardware acceptance,
joint signed-update-to-TLS evidence and propagation remain incomplete.

The Mac backend now observes the exact startup runtime and verified target updates on a
dedicated worker, independent of the publication supervisor's HTTP effects.
Its accept loop checks a non-I/O freshness guard. The observer polls every
250 ms and expires two seconds after the beginning of the last successful
observation; a blocked read or late success cannot extend that deadline or
revive a rejected lifecycle. These are scheduling bounds in source, not an
installed real-time latency guarantee.

An active certificate transaction never bypasses Shared verification. Its own
exact CAS/readback and independently verified restriction-preserving target
observations may update the expected baseline. Observation and CAS are serialized;
an in-flight draft with the old expected target is rejected before persistence.
Absent/unreadable/invalid state, inconsistent readbacks and changed runtime still
withdraw serving. Known retirement rejects even byte-matching late drafts. The
Mac effect checks observation freshness both before waiting for the CAS lock and
inside the persistence closure, before dispatching to OS storage. Already
dispatched writes remain uncertain and cannot be rolled back by cancellation.
After a state-changing response finishes (including failure), retirement is
latched before releasing the transaction lane. Back-to-back certificate
requests cannot hide idle gaps and keep stale admission alive.

Guard failure immediately stops the old Shared lifecycle, closes its active
sockets and cancels provider dispatch, without waiting for directory publication
or supervisor progress. This does not undo provider work already delivered;
unresolved mutation evidence is retained. Restart must still rebuild from
verified protected state. Verified per-peer changes now narrow admission without
global withdrawal even during read-only certificate requests, while common-state
changes, invalid observations and actual certificate-write retirement still
withdraw the entire listener. The installed simultaneous
phone flows and certificate conflicts have not been accepted.

Software tests cover the exact-state/transaction boundary, lost readback,
observation timeout and terminal rejection. An actual loopback socket fixture
proves idle connection eviction, refusal of new connections, lifecycle stop
and provider cancellation without calling the supervisor. That fixture uses
a test acceptor; it is not installed broker/SEP or real mTLS revocation proof.

The busy-lane ordering regression holds a real transaction on another thread,
accepts an explicitly verified-effect target update, and proves the old CAS
cannot invoke its persistence callback. The read-only branch remains live after
transaction drop; the branch with an actual certificate write still retires for
admission rebuilding. Another regression rejects delayed persistence after a
known observation failure even when the old expected bytes still match. These
are software storage/verification-effect tests, not the joint signed-management
to mTLS scenario. The ordering test passed 20 consecutive runs; backend all-target
tests (190), strict Clippy and the macOS backend release build also passed.

## Trust and persistence boundary

The request contains only the exact Grant ID, a stable revocation operation ID
and the reason. Shared resolves the subject, revision, Owner and authority epoch
from verified history, not platform JSON. The target snapshot must match the
current durable owner runtime. Unknown Grants, reused IDs across phones, changed
intent and changed runtime/target snapshots fail closed.

The OS effects reread current runtime/target bytes, sign Shared's original owner
message, sign the target snapshot, and execute exact compare-and-swap/readback.
The caller must serialize this lane with certificate and device-management work
and must obtain the required explicit local management authentication before
invoking it. No new unauthenticated HTTP management route is introduced.

Only verified owner signatures enter the Grant ledger. Only a verified target
signature and exact actual durable readback produce a result. The signing and
persistence sequence checks monotonic observed wall time and state replacement.
No other phone's Grant/TLS ledger or interaction history is removed or rewritten
by the revocation itself. An expired historical Grant may be revoked without
creating an active admission set or extending its lifetime.

## Ambiguous writes and propagation evidence

A failed write/readback does not prove that storage is unchanged. The caller
must retain the request, reload verified state and retry it. When the exact
revocation ID, Grant and reason already exist, Shared returns the original
signed evidence without signing or writing again. A different intent is not an
idempotent success. History is retained, including revoked/expired peers.

The result can release its signed revocation bytes only after rereading the
exact target/runtime again. Those bytes are propagation evidence, not proof of
global delivery or a phone UI update. Every receiver still has to verify the
Owner signature, persist its own ledger and retire its own live readers.

## Native management boundary (ABI 1.81)

`agenthalo_owner_grant_revoke` borrows the actual opaque owner runtime. It rejects
wrong handles and malformed/incomplete callback tables before calling any OS
effect. The explicit management authentication callback runs before the runtime
mutex is taken; cancellation or anything other than the exact approved response
stops the transaction. Authentication is not itself evidence of Grant authority.

Shared derives the local hardware subject from the existing P-256 SPKI, verifies
the target snapshot's signature/context and checks exact current runtime/target
readbacks throughout the transaction. Runtime contention fails instead of
waiting while another operation owns the lock. Callback buffers have explicit
bounds and are invoked once, not probed for sizes. A failed or oversized result
does not leak partial evidence.

Apple's `AgentHaloOwnerGrantRevocation` executes this synchronous ABI
on a non-cooperative GCD worker and wraps typed async OS effects, so blocking on
the ABI cannot starve the Swift executor that must run those callbacks. Inputs
are copied before callbacks await. The Swift callback-context lock is not held
while waiting; Shared retains its runtime lock for non-authentication transaction
effects, whose callbacks must not reenter Shared or wait for UI. Cancellation suppresses late results
without assuming a dispatched write stopped. The returned type explicitly means
local signed propagation evidence, not global revocation or consumer completion.
The Mac coordinator now has exclusive explicit-management and pending-read entries,
with concrete OS effects. It refuses an unprepared runtime without bootstrapping.
The management button is connected through the catalog/recovery model above;
mutation must not run during ordinary startup or automatic reconnect.

`AgentHaloOwnerGrantManagementOS` registers the broker before entering Shared,
opens only the existing configured target and Owner Secure Enclave keys
noninteractively, and passes Shared's original messages to the distinct signers.
The bounded read-only helper returns an OS locator, not authority. Blob reads
reject symlinks, hardlinks, special files, wrong ownership/modes and oversized
or changed files; they do not create keys or repair permissions. Local management
authentication runs only on Shared's explicit authentication callback.

The signed-App broker lane can read and exact-CAS update an existing target
snapshot, but cannot create one or access TLS state. Code identity and nonce
checks remain required. Prepared transfers skip service-registration/UI hops
inside the Shared runtime mutex. These native adapters are source-implemented,
not evidence of installed hardware or live listener revocation.

## Verification and open integration

Software-key tests exercise actual signed pairing/target histories, incorrect
owner/target signers, changed state, failed writes, lost readback, retry without
duplicate issuance, stale evidence delivery, backwards time, expired-only
history, two-phone isolation, conflicting cross-phone operation IDs and denial
of certificate/checkpoint exchange after a previously issued client certificate
is revoked. Existing real-rustls tests remain separate from physical hardware
acceptance.

The real FFI pairing fixture now binds its target to an actual test public key,
runs the management callbacks and verifies returned signed revocation evidence.
It loses the pending-intent write's readback and resumes the original request,
with two target writes/signatures and one Owner signature overall. Catalog and
pending queries leave authentication/sign/write counters unchanged. Separate tests check authentication without
holding the runtime mutex, rejected authentication, malformed SPKI, callback
bounds, Apple callback dispatch and cancellation after a dispatched write.

Software native tests cover the actual broker handler with temporary sealed
slots, nil/stale predecessors, idempotent readback and TLS denial. Mac tests cover
the existing-key locator, bounded child output, unsafe-file rejection, distinct
signers/stores, exact CAS forwarding and no implicit authentication/bootstrap.

Mac model fixtures additionally exercise explicit-open reads, ambiguous-write
pending display, manual-only resume, failed post-write catalog reads, and
close/reopen during uncancellable OS work. These are software lifecycle checks,
not evidence of installed windows or hardware effects.

Still required: installed authenticated OS broker/hardware and mTLS revocation
acceptance, per-peer listener update without interrupting unaffected phones,
signed multi-target distribution
and consumer revocation state/UI for Mac and HarmonyOS. Do not clear unresolved
mutation/upload evidence, silently repurpose device-revoke into Grant renewal,
or claim a local snapshot write proves global revocation.
