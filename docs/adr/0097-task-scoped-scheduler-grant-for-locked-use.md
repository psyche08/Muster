# ADR 0097: Task-scoped scheduler grant for Locked Use

- Date: 2026-11-02
- Status: Accepted; source and tests verified. No release, installation or
  device acceptance: the grant has never run on an installed build.
- Numbered 0097: ADR 0095 and 0096 on `main` are the shared AX navigation
  and question editor decisions.
- Related: [ADR 0007](0007-desktop-manual-locked-use.md),
  [ADR 0087](0087-idle-compaction-on-a-locked-or-busy-mac.md),
  [ADR 0094](0094-local-plugin-contract-and-receipt-recovery.md);
  [LOCKED_USE.md](../LOCKED_USE.md#2026-11-02-a-task-scoped-scheduler-grant),
  [AGENT_INTERFACE.md](../AGENT_INTERFACE.md#scheduler-grant-2026-11-02)

## Context

The Mac's owner runs a long CPU research loop with Claude Desktop. A separate
supervisor process on the same Mac does the waiting and checking:

- it hands one work package to a Claude Desktop conversation;
- it waits for the turn to finish and checks the files it produced;
- it hands over the next package, which builds on those files.

The person is usually away and the Mac is locked. Today every temporary
unlock needs a person's Presence signature (ADR 0007). The only exception is
this Mac's own lease for one `/compact` (ADR 0087). Neither fits:

- A Presence prompt per package expires unanswered while the person is away.
- Widening the `/compact` lease would turn a single-command authority into
  general desktop control.
- A global unattended switch would let any local caller drive any
  conversation, provider or directory, with no end.

The person asked for an authority that belongs to one task. Program checks
must enforce it; telling the agent not to overstep is not enough. Every
existing Locked Use safeguard stays.

## Decision

### The grant (shared Rust, `muster_core::scheduler_grant`)

A **scheduler grant** belongs to exactly one task. Its scope is fixed when it
is granted:

- task id;
- SHA-256 of the task's contract;
- one provider (`claude` or `codex`);
- one canonical directory;
- the exact model and effort names the conversations must use.

Its limits are constants in shared Rust:

- at most 24 hours, never extended;
- at most 2 sessions, one per role: a producer and an independent reviewer,
  each created or adopted;
- at most 18 packages (creations and sends), of which at most 6 are reviewer
  packages;
- at most 72 control operations (reading or setting the model options,
  interrupting).

Its rules:

- **One live task.** Granting the identical scope again (a restarted caller)
  answers the existing grant unchanged. A different scope, a second task, or
  a task id that has ended is refused. Revocation is immediate and final.
- **Operations.** Only `create`, `send`, `read_options`, `set_options` and
  `interrupt`. There is no input, shell or other provider. `adopt` is not a
  desktop operation (below).
- **Sessions bind only by receipt.** A creation names no session. Its applied
  outcome binds the receipt session to its role. That session must be new:
  not present before the creation, and not the other role's session. Every
  later operation must name its role's bound session. `set_options` may only
  name the scope's model and effort.
- **Adoption binds an existing session.** A task may already have a
  conversation open. `adopt` binds a session that exists now to a role
  instead of creating one:
  - the grant is live, and the request's provider and directory equal the
    scope;
  - the provider's own session registry lists it with this provider, code
    mode and cwd equal to the directory, not archived, read-only or a child
    session;
  - the role has no session and no creation that is reserved, applied or
    unknown, and the session is not the other role's;
  - creations and adoptions together stay within the 2 sessions.

  The same adoption again while the grant is live changes nothing. A role
  that adopted cannot create, and a creation's receipt can never be an
  adopted session. Adoption touches no desktop, costs no package, and ends
  with the grant.
- **op_id is the identity.** Each operation is recorded under its op_id with
  a SHA-256 digest of the exact request. The same op_id with the same request
  answers its record and never runs again. The same op_id with a different
  request is a conflict.
- **Reserve before acting.** Admission checks, in order:
  1. a repeat answers its record;
  2. the grant is live (not expired, not revoked);
  3. provider and directory equal the scope;
  4. the session rule;
  5. options;
  6. quotas.

  It then charges the counts and stores the operation as reserved.
- **Settle from evidence.**
  - Applied keeps the charge.
  - Proven not started releases it.
  - Unknown keeps it and is never replayed.
- **Expiry and revocation** stop new reservations. Read-only reconciliation of
  earlier operations still works.
- **Restart.** An operation still reserved when musterd starts becomes
  unknown.
- **Lease digest.** Each reserved operation has one digest: SHA-256 over
  `MUSTER-SCHEDULED-OPERATION-V1\0`, then the scope, op_id, kind, role and
  session separated by NUL, then the request digest. The scheduled lease is
  bound to that digest and op_id.

### The scheduled lease (`muster_core::locked_use`)

`Scope::Scheduled{provider}` is a third kind of this Mac's own lease:

- **Opened only from a reservation.** `Gate::issue_scheduled` takes a core
  `Reservation`. Only `Grant::reserve` constructs one. Its turn is the op_id
  and its digest is the reservation digest.
- **No key arms it.** `arm` refuses it, and so does the `/compact` lease's
  `arm_unattended`. Only `arm_scheduled` admits it.
- **Closed to everyone else.** It binds no provider queue, accepts no remote
  request, and borrows no other requester's lease.
- **Same local switches as ADR 0087.** It needs `allow_locked_control`,
  `offer_agent` and the provider's app in `ax_allowlist`, checked at issue
  and at every gate tick.
- **Checked on every OS call.** The operation's console check also requires:
  - the grant is still live;
  - the same console session and input generation;
  - an unlocked screen;
  - the 15 s window.
- **Everything else is the ordinary transaction:**
  - the cover over every display and its watchdog;
  - one-time plugin consumption;
  - local input or a console change stops it;
  - draining, revocation;
  - relock before the cover and console are released.

### The daemon (`musterd/src/scheduler.rs`)

- **One RPC, local only.** `agent_scheduler` has the actions `grant`,
  `revoke`, `status`, `operate`, `reconcile` and `adopt`. Only this Mac's local socket
  can call it. No remote device or phone can.
- **Persistence.** The registry is stored in the daemon's key-value store
  under `scheduler.grants`. Every change is stored before it takes effect.
  If the stored grants cannot be read, every scheduler request is refused and
  nothing is written, so counts and unknown operations are never reset.
- **`operate`, one at a time:**
  1. Admission is decided on a copy first, before anything about the desktop.
     Scope, liveness, quota and op_id refusals do not depend on the screen,
     and a repeat answers its record.
  2. Desktop preconditions are checked: Accessibility, the switches, and
     Locked Use for a locked console. A failure reserves nothing.
  3. Creations record the provider's existing sessions in the directory as
     the baseline.
  4. The reservation is stored.
  5. The operation runs:
     - On a **locked** console: exactly one scheduled Locked Use lease. A
       console owned by another Locked Use transaction is busy. It is never
       preempted.
     - On an **unlocked** console: only after 1 s without local input (waiting
       at most 30 s), and only while no Locked Use owns the console.
  6. The outcome is settled from evidence:
     - An operation that never began was not sent.
     - A ledgered operation (`agent.native_create`, `agent.send`,
       `agent.interrupt`) is what its ledger row says: no row or a rejected
       row means not sent, a pending row means unknown.
     - Reading options changes nothing. Setting them after they began is
       unknown unless the options came back.
- **The lease never waits for the model.**
  - Inside the lease, a creation looks for its receipt for at most 3 s.
  - A send ends when its submit is confirmed (ADR 0094).
  - After relock, a creation or send with an unknown outcome looks for
    evidence read-only for at most 45 s.
  - The `reconcile` action repeats that read-only lookup later.
  - Turn completion and artifacts are the supervisor's job, through the
    existing read-only interfaces.
- **Creation receipt.** The unique session that satisfies all of these:
  - not in the baseline or already bound;
  - this provider, code mode, cwd equal to the directory;
  - its first user message is exactly the package text.

  More than one match is an integrity conflict and binds nothing. A matching
  receipt confirms the creation's ledger row.

### Unchanged

- Phone-signed Locked Use, the ADR 0087 `/compact` lease, provider queues and
  the legacy `agent_native_create` RPC behave as before.
  - `agent_native_create` still needs an unlocked desktop.
  - It now shares the receipt code with scheduler creations.
- No password, Keychain, system lock setting or `security unlock-keychain` is
  involved anywhere.

## Protocol field renumbering

The branch this decision was written on carried the grant as request and
status field 74 in `muster.proto`. [ADR 0116](0116-failure-screenshot-analysis.md)
landed on `main` with 74 for its vision request and
[ADR 0111](0111-native-organization-ax-and-file-confirmation.md) with 75 for
organization, so the grant takes the next free number in each oneof — 76 — and
the schema says what the old number was and why it moved. Nothing addresses
these fields by number in Rust, Swift or Harmony, and the Locked Use
`Operation` oneof is untouched, so no lease digest changes. No released or
pre-release build ever spoke the old number: the grant has never shipped.

The record keeps its own number. 0097 was free on `main` when this landed —
0096 and 0098 are the question editor and focused deletion decisions — so the
number this branch chose stands, and nothing cross-referencing it moved.

## Alternatives

- **A Presence signature per package.** It keeps the existing invariant, but
  the person is away when packages are due. Rejected by the person.
- **Reuse or widen the `/compact` lease.** That would turn a one-command
  authority into general control. Rejected.
- **A global unattended switch.** It would have no task, directory, provider,
  quota or end. Rejected.
- **Bind the producer by picking the latest session.** Recency is not
  evidence, and a second conversation in the directory would be taken
  silently. Rejected: adoption names the exact session and musterd checks it
  in the registry.
- **Only creations, no adoption.** The task's producer conversation already
  existed with its context. Creating another would discard that context and
  spend a session on a duplicate. The person chose adoption.
- **Enforce the limits only in the supervisor or the agent's prompt.** That is
  not a program boundary inside musterd. Rejected; the supervisor and prompt
  only add checks on top of the grant.
- **Hold the lease until the model answers.** That would keep the Mac unlocked
  for minutes. Rejected: the lease covers one UI submit, and receipt
  reconciliation happens after relock.

## Consequences

- **Security tradeoff, chosen by the person.** With a live grant and the local
  switches on, this Mac unlocks itself briefly with no person approving that
  unlock. Each unlock does one reserved operation in one task's directory and
  conversations. The cover, watchdog, local-input stop and relock bound it,
  and so do the quotas and the 24-hour end.
- **Off by default.** No grant exists until the local owner grants one. The
  switches ADR 0087 lists stay off by default.
- **Same-user limit.** Any process running as the same macOS user can call the
  local socket. The grant bounds what such a caller can do, but it is not a
  boundary against the same user. This matches the existing local RPC threat
  model.
- **Unknown outcomes cost quota.** A package whose outcome stays unknown
  counts and is never replayed. The supervisor must reconcile it read-only or
  stop. It must never send it again.
- A person who returns during an unlock sees the cover. Touching the keyboard
  or mouse ends the lease and relocks the Mac.
- **Generated code.** The Swift bindings were regenerated with the CI
  generator. The Harmony bindings were regenerated with the pinned
  `protobufjs-cli` 1.3.3. That output also picks up schema changes already on
  `main` that earlier commits had not regenerated for Harmony: the
  desktop-update `AUTO_ON`/`AUTO_OFF` actions and `automatic` field, and, on
  landing, the session sections, organization, catalog group and vision
  messages of ADRs 0102, 0111 and 0116.

## Verification

- **Source and tests:**
  - `cargo test -p muster-core -p musterd -p muster-proto -p muster-store`
    passed: 0 failures; 2 ignored in `musterd`'s unit tests and 1 in its
    `deepseek` test, as before.
  - `cargo test -p muster-cli rpc_json` passed.
  - New tests in `muster-core`, `tests/scheduler_grant.rs` (13):
    - scope validation;
    - one live grant, never re-granted;
    - out-of-scope directory, provider and session refusals;
    - binding only by receipt;
    - expiry and revocation versus reconciliation;
    - quotas, released only on proven non-start;
    - no replay after restart, conflicting bodies;
    - reconciling an unknown creation;
    - the digest binding;
    - persistence round trip;
    - adoption once per role, counted as a session, never the other role's
      or a creation's receipt;
    - adoption needs a live grant and a role without a creation;
    - adoptions persist, and older records read as none.
  - New tests in `muster-core`, `tests/locked_use.rs` (2):
    - only its reservation opens a scheduled lease and no key arms it;
    - the local switches, a locked console and local input gate it.
  - New tests in `musterd`, the lease (5):
    - unlock under cover, run the one operation, relock, then release;
    - a phone request cannot join;
    - revocation stops it before the desktop;
    - local input before the submit stops it as not sent;
    - another owner's console is never preempted;
    - Locked Use off starts nothing.
  - New tests in `musterd`, the RPC (12):
    - exact single grant that is never extended;
    - out-of-scope, unknown-task and malformed refusals reserve nothing;
    - an undrivable desktop reserves nothing;
    - expiry, quota and revocation, and no re-grant;
    - recorded op_ids answer their record, including after revocation;
    - restart turns reserved into unknown and keeps counts;
    - unreadable stored grants fail closed and are not overwritten;
    - evidence-based settling for every ledger state and options;
    - reconciliation binds an unknown creation only from its ledger receipt;
    - adoption outside the grant, without the registry, or for a role with
      a creation is refused and binds nothing;
    - a done adoption answers again until revocation;
    - only the scope's code session in its directory is adoptable.
- **Lint:** `cargo clippy --workspace --all-targets -- -D warnings` reported
  nothing, and `src/muster/scripts/check-layers.py` passed. The touched files
  are rustfmt-clean with both the Homebrew 1.99.1 and rustup-stable rustfmt.
  When this was written, `cargo fmt --all --check` still reported six
  untouched files that already differed on `origin/main`; they have since been
  formatted there.
- **Generated code:**
  - Swift: swift-protobuf 1.38.1 (55d7a1cc) with the vendored protoc.
  - Harmony: protobufjs-cli 1.3.3 with protobufjs 7.6.6.

## Verification on landing

Re-run on the three commits rebased onto `main` at 583b8b29, with the field
renumbering above and the bindings regenerated rather than merged:

- `cargo test --workspace` passed, 0 failures.
- `cargo clippy --workspace --all-targets -- -D warnings` reported nothing.
- `cargo fmt --all --check` is clean across the workspace.
- `src/muster/scripts/check-layers.py`: 18 crates follow the layer map.
- `scripts/check-adr.sh origin/main`, `python3 scripts/check-public-content.py`
  (0 findings over 877 files) and the 14 script tests passed.
- `swift test --parallel` in `src/native/apple/MusterApp` passed 109 of 109
  against a debug `libmuster_ffi.a`, so the regenerated Swift bindings compile
  and the app's own tests still pass.
- The grant's operation kinds are still the closed set of create, send,
  read_options, set_options and interrupt. The organization operation that
  landed meanwhile ([ADR 0111](0111-native-organization-ax-and-file-confirmation.md))
  is not among them, and a scheduled lease cannot carry it.

Nothing here is installation or device acceptance; see the status line.
