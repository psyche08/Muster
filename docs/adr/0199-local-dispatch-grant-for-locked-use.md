# ADR 0199: A standing local dispatch grant for Locked Use

- Date: 2026-11-13
- Status: Proposed; source and tests only. No release, installation or
  locked-Mac acceptance.
- Related: [ADR 0087](0087-idle-compaction-on-a-locked-or-busy-mac.md),
  [ADR 0097](0097-task-scoped-scheduler-grant-for-locked-use.md),
  [ADR 0150](0150-remote-desktop-session-creation.md),
  [ADR 0184](0184-multica-task-authorized-codex-creation.md),
  [ADR 0187](0187-trusted-multica-dispatch.md);
  [LOCKED_USE.md](../LOCKED_USE.md)

## Context

A service on the same Mac drives Claude and Codex desktop sessions through the
local socket (`muster rpc`). The owner's PR review service is the case at hand:

- it creates a session with `agent_native_create`, in one of a few known
  project folders, with a model and effort it chooses;
- it sends later work into that same session with `agent_send`;
- it sends `/compact` to a session that has sat idle.

The Mac is usually locked while this happens. Both calls then fail:

- a creation answers "Native creation requires an unlocked desktop with
  Accessibility";
- a send fails its console check.

The service retries with back-off and gives up after four hours. On
2026-11-12 and 2026-11-13 six of its runs ended that way.

None of the existing Locked Use sources fits:

- **A phone-signed operation** (ADR 0007, 0120, 0150) needs a person to
  approve each unlock, and the person is away.
- **The idle-compaction lease** (ADR 0087) is one command, chosen by the
  daemon itself, never by a caller.
- **The task scheduler grant** (ADR 0097) belongs to one task:
  - at most 24 hours, never extended;
  - two sessions, 18 packages;
  - one directory, one provider;
  - a fixed model and effort, and creations take no model or effort at all;
  - one live task on the whole Mac, so a standing grant would also block
    every Multica run (ADR 0184/0187).

  The service runs for weeks, opens a session per pull request, and picks a
  model per rule.
- **Trusted Multica dispatch** (ADR 0187) verifies one external service's
  issue snapshot. It is specific to that service.

## Decision

### The grant (shared Rust, `muster_core::dispatch_grant`)

A **dispatch grant** lets one named local caller create and drive its own
sessions on a locked Mac. The local owner grants it. Its scope:

- a name (1-64 of `[A-Za-z0-9._:-]`), which the caller states on every
  request;
- the providers it may use: a non-empty subset of `claude` and `codex`;
- 1-16 roots: existing directories, stored by their canonical path, never
  `/`.

Its limits are constants in shared Rust:

- **Lifetime.** At most 30 days.
- **Re-granting.** Granting the same name again replaces the scope and the
  lifetime. It keeps the sessions the grant has bound and every operation on
  record, so an op_id never runs twice.
- **Locked leases.** At most 30 locked leases in any rolling hour. An attempt
  counts even when it turns out not to have started: the console may have
  been unlocked for it.
- **Records.** At most 2048 operations on record. A settled record older than
  seven days is dropped; an unknown or reserved one is kept.
- **Grants.** At most 4 named grants on a Mac.

Its rules:

- **Two operations only.**
  - `create`: an `AgentNativeCreateRequest`. Its directory must be a root or
    lie below one. It may name a model and effort.
  - `send`: an `AgentSendRequest`, text only, no attachments. Its session
    must be one this grant's own creation bound.

  `/compact` is an ordinary send. There is no interrupt, answer, option
  change, input, shell or other provider.
- **Sessions bind only by receipt.** A creation's applied outcome binds its
  receipt session to the grant. The receipt must be a new session, not in
  the creation's baseline. Nothing else binds a session: there is no
  adoption, and recency is not evidence. A session the user opened in the
  same folder, or one the caller created without the grant, cannot be driven
  through it.
- **op_id is the identity.** Each operation is recorded under its op_id with
  the SHA-256 of the exact inner request.
  - The same op_id with the same request answers its record and never runs
    again.
  - The same op_id with a different request is a conflict.
- **Reserve before acting.** Admission checks, in order:
  1. a repeat answers its record;
  2. the grant is live (not revoked, not expired);
  3. the provider is granted;
  4. a creation's directory is under a root, a send's session is bound to
     this grant with the same provider;
  5. on a locked console, the hourly lease limit;
  6. the record limit.

  It then stores the operation as reserved.
- **Settle from evidence**, as ADR 0097 does:
  - applied keeps the record;
  - proven not started becomes rejected;
  - unknown stays unknown and is never replayed.
- **Expiry and revocation** stop new reservations at once. A revoked grant can
  be granted again by the owner.
- **Restart.** An operation still reserved when musterd starts becomes
  unknown.
- **Lease digest.** SHA-256 over `MUSTER-DISPATCH-OPERATION-V1\0`, then the
  grant name, op_id, kind, provider, directory and session separated by NUL,
  then the request digest.

### The lease

A locked operation runs under exactly one **scheduled** Locked Use lease, the
lease kind of ADR 0097:

- **Opened only from a reservation.** `Gate::issue_scheduled` takes a core
  `Reservation`. Only a grant's `reserve` in `muster-core` constructs one:
  the task scheduler grant's or the dispatch grant's.
- **No key arms it.** Only `arm_scheduled` admits it. It binds no provider
  queue and accepts no remote request.
- **Same local switches as ADR 0087 and 0097.** It needs `allow_locked_control`,
  `offer_agent` and the provider's app in `ax_allowlist`, at issue and on every
  gate tick.
- **Checked on every OS call.** The operation's console check also requires:
  - this dispatch grant is still live;
  - the same console session and input generation;
  - an unlocked screen;
  - the 15 s window.
- **Everything else is the ordinary transaction:**
  - the cover over every display and its watchdog;
  - one-time plugin consumption;
  - local input or a console change stops it;
  - relock before the cover and console are released.
- **The lease never waits for the model.**
  - A creation looks for its receipt for at most 3 s inside the lease.
  - A send ends when its submit is confirmed.
  - After relock, an unknown creation or send is looked up read-only for at
    most 45 s.

The lease records which grant holds it. A task grant's lease checks that task's
liveness; a dispatch grant's lease checks that dispatch grant's.

### The daemon (`musterd/src/dispatch.rs`)

- **One RPC, local only.** `agent_dispatch` has the actions `grant`, `revoke`,
  `status`, `create` and `send`. Only this Mac's local socket can call it.
- **The answer is the ordinary one.** `create` and `send` answer the
  `op_result` that `agent_native_create` and `agent_send` answer:
  - `applied` with the session id for a creation;
  - `rejected` when nothing was sent;
  - `unknown` when it may have been, never replayed.

  A caller changes only how it wraps the request.
- **Locked or not.** The caller sends every create and send through the grant,
  so every session it creates is bound:
  - On a **locked** console: the scheduled lease above. A console owned by
    another Locked Use transaction is busy and is never preempted.
  - On an **unlocked** console: the ADR 0097 quiet path. It runs after 1 s
    without local input (waiting at most 30 s), only while no Locked Use owns
    the console, with the grant checked on every OS call.
- **One operation at a time.** Dispatch operations wait for each other. The
  console itself is still shared with every other writer through the Locked Use
  write lock.
- **Persistence.** The grants are stored under `dispatch.grants`. Every change
  is stored before it takes effect. If the stored grants cannot be read, every
  dispatch request is refused and nothing is written.
- **The ledger is shared.** The inner request is ledgered exactly as the plain
  RPC ledgers it (`agent.native_create`, `agent.send`). Settling reads that
  ledger. An op_id that is already on the ledger but not on this grant's record
  is refused before anything is reserved: inside a lease it could only answer
  its old row, and the Mac would have unlocked for nothing. This is the case of
  a plain request that a locked Mac refused, retried through the grant under
  the same op_id.

### Unchanged

- The plain `agent_native_create` still needs an unlocked desktop. A plain
  `agent_send` on a locked Mac fails as before.
- Phone-signed Locked Use, the ADR 0087 `/compact` lease, the task scheduler
  grant and the Multica paths behave as before.
- No password, Keychain, system lock setting or `security unlock-keychain` is
  involved.

## Alternatives

- **Widen the task scheduler grant.** It would need several live grants,
  per-creation model and effort, many sessions and directories, and terms in
  weeks. Each of those loosens a limit ADR 0097 chose for one task. Rejected:
  a separate grant keeps those limits where they are.
- **Let the plain RPCs unlock the Mac when a grant covers them.** No caller
  change, but any local caller of `agent_native_create` would unlock the Mac
  without asking for it. Rejected: the caller names the grant on every
  request.
- **Identify the caller by code signature.** The socket's peer is the `muster`
  command line, not the service, and a parent process can be anything that
  starts that command. Rejected for now: like ADR 0097 the grant bounds what a
  same-user caller can do and is not a boundary against the same user.
- **Admit sends to any session under a root.** The roots are project folders
  the owner also works in. Rejected: the grant would let the caller drive the
  owner's own conversations while the Mac is locked.
- **Adopt existing sessions.** Sessions the caller created before it used the
  grant would keep working while locked. Rejected for now: adoption by a
  caller cannot tell its own session from the owner's. New work creates new
  bound sessions.
- **Run only locked operations through the grant.** Sessions created on an
  unlocked Mac would then be unbound and unusable when it locks. Rejected.

## Consequences

- **Security tradeoff, chosen by the owner.** With a live dispatch grant and
  the local switches on, this Mac unlocks itself briefly with nobody approving
  that unlock. Each unlock is one creation in a granted folder or one send to a
  session the grant created. The cover, watchdog, local-input stop, relock, the
  hourly limit and the 30-day end bound it.
- **Same-user limit.** Any process running as the same macOS user can call the
  local socket, including `grant`. The grant bounds what such a caller can do
  under it. It is not a boundary against the same user, as for ADR 0097.
- **Off by default.** No grant exists until the owner grants one. The switches
  ADR 0087 lists stay off by default.
- **Callers must wrap their requests.** A caller that keeps using the plain
  RPCs keeps today's behaviour. Its sessions created before it switched are not
  bound and still need an unlocked Mac. A request it retries through the grant
  needs a new op_id.
- **Unknown outcomes are final.** A caller must not send an unknown operation
  again under a new op_id.
- A person who returns during an unlock sees the cover. Touching the keyboard
  or mouse ends the lease and relocks the Mac.
- **Generated code.** The Swift bindings are regenerated with the CI
  generator. The Harmony bindings are not: nothing on Harmony calls the local
  dispatch RPC.

## Verification

2026-11-13, on a development Mac:

- **New tests:**
  - `muster-core/tests/dispatch_grant.rs`: scope and roots, bound sessions,
    receipts, op_id records and conflicts, expiry, revocation and re-granting,
    the hourly limit, record pruning, restart and the digest;
  - `muster-core/tests/locked_use.rs`: a dispatch reservation opens the same
    scheduled lease, and no key arms it;
  - `musterd` dispatch RPC: canonical roots, refusals that reserve nothing, an
    op_id already on the ledger, an undrivable desktop, recorded answers,
    restart, unreadable grants;
  - `musterd` lease: unlock under cover, run and relock; revocation stops it
    before the desktop; a lease refuses another grant's job;
  - `muster-cli`: the JSON of every dispatch action.
- **Passed:**
  - `cargo test` for `muster-core`, `musterd`, `muster-proto`, `muster-store`
    and `muster-cli`;
  - `cargo clippy --workspace --all-targets -- -D warnings`;
  - `swift test` for MusterCore against the rebuilt `libmuster_ffi`;
  - the Swift bindings, regenerated with `protoc-gen-swift` 1.38.1 built from
    the resolved checkout; only the new messages and name maps changed;
  - `check-public-content.py`.
- **Not caused by this change, seen on an unchanged `main` too:**
  - `musterd --test access`'s
    `catalog_watch_coalesces_changes_and_invalidates_ownership_immediately` is
    timing-dependent here: it failed 3 of 3 runs on `main` and passes or fails
    on this branch;
  - `check-layers.py` reports `muster-acp -> muster-agent` and
    `muster-ffi -> muster-paths`;
  - `cargo fmt --check` differs only in `muster-agent/src/questions.rs`.
- **Not done:**
  - no release, installation or notarization;
  - no run on a locked Mac. Acceptance needs the owner to grant the service on
    an installed build, lock the Mac, and observe one creation and one send
    unlock, act and relock.
