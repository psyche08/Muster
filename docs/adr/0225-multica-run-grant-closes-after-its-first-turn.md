# ADR 0225: A Multica run's grant closes after its first turn

- Date: 2026-11-17
- Status: Accepted; source and tests verified, device acceptance pending
- Related: [ADR 0097](0097-task-scoped-scheduler-grant-for-locked-use.md), [ADR 0184](0184-multica-task-authorized-codex-creation.md), [ADR 0187](0187-trusted-multica-dispatch.md), [ADR 0221](0221-multica-workspace-folder-sessions.md)

## Context

A Multica run's first turn on a locked Mac runs under a scheduler grant for
the task `multica:<run>` (ADR 0184). The grant can be issued in two ways:

- **Trusted dispatch.** For an owner's own issue on the enrolled host, the
  daemon issues the grant itself and revokes it once the operation returns
  (ADR 0187).
- **Owner approval.** Otherwise the daemon proposes the turn and waits for
  the local owner to grant it. The documented flow had the owner revoke that
  grant when finished.

The registry admits only one live grant on a Mac (ADR 0097).

During the ADR 0223 device tests, the owner granted one locked Claude first
turn with a 15-minute lifetime. The turn was created and the run completed,
but the grant stayed live. The next locked Codex run was a trusted dispatch.
Issuing its grant was refused with "another task holds the live scheduler
grant", and the Multica run failed rather than waited. It would have gone on
failing until the Claude grant expired or was revoked by hand.

Nothing uses such a grant after its one operation:

- The namespace admits one producer creation under one operation ID.
- Later turns of the session are ordinary sends.
- The bridge reads the creation receipt with `status`, and `reconcile` binds a
  late receipt. Neither checks liveness.
- The bridge records the turn as attempted before asking, so it never asks
  for the same first turn again.

## Decision

Whoever issued it, a Multica run's grant is closed once its first turn's
operation returns. `multica_create::start` hands both paths to one function,
`run_once`. It runs the operation, or refuses it if the run was cancelled. It
then revokes the run's grant and logs `Multica grant closed for <task>`. The
outcome does not matter: applied, not started, unknown, or refused before
reservation.

Unchanged:

- A repeat of a recorded operation answers its record. Revocation is checked
  only for an operation that was never reserved, so a repeat never runs again.
- A closed grant's status, receipt and counts stay readable.
- A task id is never granted again (ADR 0097). A run that failed needs a new
  authorized dispatch, as an expired proposal already did (ADR 0184).
- General task grants (ADR 0097) still live until they expire or are revoked,
  and still exclude every other grant.

## Alternatives

- **Let a trusted dispatch wait for the live grant.** The run would still wait
  out a grant that nothing can use, up to its whole lifetime (as long as 24
  hours), and Multica's own deadlines would still fail it.
- **Stop counting a grant as live once its quota is spent.** A first-turn
  grant's quota is not spent after one creation: the scope admits sends that
  the bridge never makes. This would also change ADR 0097's accounting for
  every task grant.
- **Require the owner to use a short lifetime.** The flow already asked for "a
  short TTL". Even a short grant blocks every run that arrives before it ends,
  and the failure comes back as a refused Multica run rather than a wait.

## Consequences

- A finished or failed Multica first turn no longer holds the Mac's one live
  grant. The next run, from either provider, can be granted at once.
- The owner no longer needs to revoke a first-turn grant. Revoking it again
  changes nothing.
- An owner grant issued for a proposal after its waiter has given up is still
  never consumed. It stays live until it expires or is revoked, as before.
- The close is part of the request. A run whose request is dropped before it
  returns keeps the grant until it expires, also as before.

## Verification

Source and tests:

- `musterd` scheduler tests (`cargo test -p musterd --lib scheduler::`),
  including `a_runs_grant_closes_after_its_one_operation_whoever_issued_it`.
  The owner grants a run for 15 minutes, and the test then checks four things:
  - A run cancelled before submission closes its grant, and the closed grant
    still answers status.
  - The next run's grant is admitted, not refused for a live one.
  - An operation refused for a changed first turn closes its grant too.
  - A repeat on a closed grant is refused and never runs.
- The 20 scheduler tests pass with `RUST_MIN_STACK=8388608`. Under the default
  2 MiB test thread stack, the daemon-based scheduler tests overflow their
  stack in a debug build. That happens on `origin/main` without this change as
  well, and is left to its own fix.
- `cargo fmt --check` and strict Clippy (`--no-deps --tests`) on `musterd`;
  `scripts/check-adr.sh origin/main`; `scripts/check-public-content.py`.

Device acceptance is pending. It needs an installed build with this change
and a locked Mac. The owner grants one locked Claude first turn. Once that run
completes, the daemon log shows `Multica grant closed`, and status reports the
grant not live. A trusted Codex dispatch made next is then granted and
submitted, with no "another task holds the live scheduler grant" refusal.
