# ADR 0308: A watchdog that cannot raise its cover leaves no quarantine

- Date: 2026-11-25
- Status: Accepted. Source and unit tests only; installation and runtime acceptance pending.
- Related: [Locked Use](../LOCKED_USE.md) section 6, [The panel helper keeps its own identity](0128-the-panel-helper-keeps-its-own-identity.md), [Task-scoped scheduler grant](0097-task-scoped-scheduler-grant-for-locked-use.md), [Local dispatch grant](0199-local-dispatch-grant-for-locked-use.md)

## Context

The watchdog helper writes `locked-use.pending` before it raises the display
cover. The record holds the console's uid, its audit session and the
transaction's nonce. The helper removes it only when the owner's `f` asks it to
release the cover. While the record names the current audit session,
`MacLockedUse::update_ready` refuses every Locked Use transaction with "an
earlier Locked Use transaction needs local recovery (log out and log in)". A
transaction that may have let an authorization start cannot be proven finished.
Only a new login can end that state.

On 2026-11-25 a Mac in daily use was quarantined by a transaction that had
asked for nothing (times UTC):

- **06:15:26.** A scheduled dispatch send leased the locked console. The cover
  rose ("displays black=0; under the black panel=1") and the plugin consumed
  the nonce. The watchdog then stopped on local input before any provider
  submission. The cover was released and the record removed.
- **06:16:28.** The retry armed its lease, and the owner's OS sampling was
  delayed 175 and 275 ms. Within a second the owner found the watchdog process
  gone. The helper had written its record, but it had sent no cover counts
  (`D`) and no status. The owner stopped with `WatchdogLost`. Its release wrote
  `f` into a closed pipe and logged "display guard release failed". The record
  stayed, naming the current audit session.
- **06:18:31 to 06:31:23.** Nine locked dispatches settled "not started"
  without a lease. The refusal reached the requester only; the log said
  "settled not started". The user then deleted the record by hand, which the
  Locked Use document rules out as recovery. It happened to be safe here: the
  log shows no lock-screen request and no plugin consumption for that
  transaction.

The helper's stderr was inherited from musterd. Under launchd it reaches no
file, so the helper's exit error was lost. The evidence narrows the failure
to the steps after the record is created and before `D`, which is written as
soon as the cover is raised. Those steps are:

- writing and syncing the record;
- `Shield::raise`, which fails with "no display to cover" or "cannot cover
  every display";
- a crash inside it.

## Decision

1. **Clear the record on a startup failure before the cover.** The steps after
   the record exists and before the cover is raised run in `before_cover`.
   These are writing and syncing the record and `Shield::raise`. If they fail,
   the helper removes the record, reports `F` and exits with its error. If the
   record cannot be removed, it stays, the error says so, and the quarantine
   stands.
2. **The invariant that makes this safe.** The owner starts the lock-screen
   request only for a fresh ready report (`R`, the cover nonce) from a live
   watchdog. The gate's plugin consumption also requires that cover. The
   helper writes `R` only after the cover is raised. A startup that failed
   before then cannot have let an authorization start.
3. **A confirmed release.** `Guard::release` first drains the watchdog's
   reports. If the newest is `F` (cover down, record removed), the release is
   done; the owner does not write `f` into a closed pipe. Otherwise the
   release is asked for and confirmed as before. A failed release still
   leaves the record and the quarantine.
4. **The causes reach the log.**
   - The helper's stderr is piped into musterd's log as "Locked Use watchdog
     stderr: …". The helper keeps no logger, so this carries its exit error or
     a panic.
   - The owner logs the watchdog's exit status once.
   - A settled dispatch logs why it was not started, or why its outcome is
     unknown, as well as reporting it to the requester.

A crash, kill or panic before the cover still leaves the record, and the login
stays quarantined. The record remains the helper's own: only the helper
removes it, after a confirmed release or a startup that never raised the cover.

## Alternatives

- **The owner removes the record when the watchdog dies before any `R`.** This
  would also cover crashes. But the owner would then remove the record that is
  meant to outlive it, and Locked Use section 6 says deleting the record
  cannot substitute for recovery. Not chosen without evidence that crashes
  happen.
- **Write the record after the cover is raised.** This changes the
  write-ahead order that the rest of the design and its review assume, for no
  gain over removing the record on the same failures.
- **Report the startup error as a new status frame.** This changes the pipe
  protocol. Stderr also carries panics, which a frame would not.

## Consequences

- A transaction whose cover cannot be raised ends "not started" without a
  quarantine. The next dispatch is admitted as usual, and the helper's error
  is in the log.
- A watchdog that crashes or is killed before its cover still needs a logout
  and login. Its exit status and any panic text are now logged, so the next
  occurrence names its cause.
- New log lines carry error strings that already went to the requester, the
  helper's own errors and an exit status. They carry no user content.
- The manual deletion is not endorsed. A record whose transaction may have
  asked the lock screen for authorization still needs a logout and login.

## Verification

- **Source and unit tests:**
  - `macos::locked_use` tests: `before_cover` removes the record on a failed
    startup, keeps it after a raised cover, and keeps it (with the reason) when
    it cannot be removed. Only `F` counts as released. All 8 tests in the
    module pass.
  - `musterd` dispatch test: a settled dispatch logs why it was not started.
    It passes.
  - The other `musterd` `locked_use` tests (unit tests, not the `sep` build):
    41 pass and 12 fail. The same 12 fail on unchanged main on the same Mac
    with "The foreground window is unavailable", so they depend on the desktop
    they run on, not on this change.
  - Clippy reports nothing in the changed files.
- **Not verified:**
  - building and signing the app;
  - installation;
  - a startup failure on a device, which needs a locked Mac whose displays
    cannot all be covered;
  - whether the 2026-11-25 exit was such a failure or a crash. The new stderr
    and exit-status lines will show it the next time.
