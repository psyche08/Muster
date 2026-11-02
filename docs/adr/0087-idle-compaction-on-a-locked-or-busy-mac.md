# ADR 0087: Idle compaction on a locked or busy Mac

- Date: 2026-11-02
- Status: Accepted; source and tests verified, device acceptance pending.
  Amends [ADR 0083](0083-idle-compaction.md).
- Related: [ADR 0006](0006-desktop-presence-and-console-routing.md),
  [ADR 0007](0007-desktop-manual-locked-use.md);
  [LOCKED_USE.md](../LOCKED_USE.md),
  [AGENT_INTERFACE.md](../AGENT_INTERFACE.md#idle-compaction-2026-11-02)

## Context

ADR 0083 compacts an idle Claude Code conversation only when nobody could
notice. It skips the tick in four cases:

- the screen is locked;
- Accessibility is not granted;
- this Mac's keyboard or mouse was used in the last 2 minutes;
- a Locked Use transaction holds the console.

In practice a conversation often goes idle while the person works elsewhere
on the same Mac, or after they lock it and leave. Both cases hit the 30 to 60
minute window without ever passing these checks, so the cache expires and the
compaction never happens. The person asked for every case except missing
Accessibility to compact anyway:

- a locked Mac uses Locked Use;
- while they use the Mac, a notification-style notice at the top right says
  so and closes by itself once compaction finishes.

Locked Use until now needs a person's Presence signature for every temporary
unlock. Asked how the daemon should get that approval at compaction time, the
person chose an unattended unlock, with no Face ID or Touch ID prompt.

## Decision

- **Only missing Accessibility stops the loop.** The feature switch from
  ADR 0083 still applies. Screen lock, local input and Locked Use no longer
  skip a due conversation.
- **A locked Mac uses its own unattended Locked Use lease.**
  - `muster-core` adds a lease kind, `Gate::issue_unattended` and
    `Gate::arm_unattended`. The requester is the target Mac itself, which must
    be an active roster member.
  - The lease has no operation key. No signature arms it, not even this Mac's
    own, and `arm` refuses it. Only `arm_unattended` admits it.
  - Authorization needs this Mac's local switches: `allow_locked_control`,
    `offer_agent`, and the provider's app in `ax_allowlist`. The daemon adds
    the idle-compaction switch. Every tick of the gate and every OS call of
    the operation checks them again. Turning any of them off ends the lease.
  - The lease runs exactly one bound operation: `/compact` in one session.
    The turn is `idle-compact-<unix ms>`. The digest is SHA-256 over
    `MUSTER-IDLE-COMPACT-V1\0`, the provider, a zero byte and the session id.
    No provider queue binds to it, and no remote request can enqueue into it.
  - Only the daemon's idle-compaction loop issues the lease. No RPC reaches
    `Owner::unattended_compact`.
  - Everything else is the ordinary Locked Use transaction:
    - a cover over every display and the independent watchdog;
    - one-time plugin consumption and the 15 s window;
    - any local input or console change ends the lease;
    - drain OS work, relock, then release the cover.
  - If Locked Use is off on this Mac, a locked Mac waits for an unlock.
    Waiting does not use up an attempt.
- **A busy Mac gets a notice and a pause.**
  - On an unlocked Mac, `musterd --muster-notice` shows a borderless panel at
    the top right of the main display. It is a separate helper process
    because AppKit needs a main thread. The panel never activates, takes
    focus or receives clicks.
  - The loop then waits up to 30 s for a 1 s pause in local input. It takes
    the console like an ordinary write and runs the same send path.
  - The native write still stops if local input changes during it.
  - After a press, confirmed or unknown, the notice stays until the
    transcript's newest record is a compaction, for at most 3 minutes. When
    nothing was pressed it closes at once. The daemon exiting closes it too,
    and the helper never outlives 10 minutes.
- **Locked Use busy means try again later.** If a Locked Use transaction holds
  the console, the loop looks again on the next tick without using an
  attempt.
- **One press per idle period, a few tries.** The tracker stays in shared Rust.
  - A press, confirmed or unknown, ends the idle period.
  - An attempt that ended before any press may run again on a later tick, up
    to 3 times in the same period.

## Alternatives

- **Ask for Presence on the phone or Mac at compaction time.** This keeps the
  existing invariant, but the person usually is not there when the window
  opens, so the prompt would expire unanswered. The person rejected it.
- **Wait for the next unlock.** No new authority is needed, but the cache
  usually expires before the person returns. That defeats the feature.
- **A macOS user notification instead of a panel.** It needs notification
  permission and a bundle identity for the daemon. It also stays in
  Notification Center after the work is done instead of closing itself.
- **Compact while the person types, without a pause.** Rejected: an AX write
  must never mix with local keystrokes, and the existing write would stop
  anyway.

## Consequences

- **Security tradeoff, chosen by the person.** With the switches on, this Mac
  unlocks itself briefly with no human approving that unlock. The lease can
  do only one thing: type `/compact` into one idle Claude Code conversation.
  The cover, the watchdog, the local-input stop and the relock still bound
  it. It reaches no other operation, requester or provider queue.
- The four local switches are all off by default:
  - idle compaction;
  - Locked Use (`allow_locked_control`);
  - agent control (`offer_agent`);
  - the provider's app in `ax_allowlist`.

  Turning off any one of them stops the unattended path.
- A person who returns during the window sees the black cover. Touching the
  keyboard or mouse ends the lease and relocks the Mac.
- A person using the Mac sees the notice and the selected conversation change
  in the desktop app. Their focus and frontmost app do not change.
- Claude records the command only after compacting. A locked attempt therefore
  normally ends as an unknown outcome after relock, as in ADR 0083.

## Verification

- **Source and tests:** `cargo test -p muster-core -p muster-agent -p
  muster-platform -p musterd --no-fail-fast` passed with no failures. New
  tests cover:
  - In `muster-core`:
    - no signer arms an unattended lease;
    - it binds no provider queue;
    - it admits only its own target, turn and digest, once;
    - each local switch, a locked console and an input generation gate its
      issue;
    - policy, input and watchdog changes revoke it while it runs.
  - In `musterd`:
    - the owner unlocks under cover, runs the compaction past the console
      checks, relocks, and only then releases the cover and console;
    - a phone request cannot join the lease;
    - turning idle compaction off stops the operation at its first OS call;
    - with Locked Use off, nothing starts.
  - In `muster-agent`: the attempt and press rules of the tracker.
- **Lint:** targeted Clippy (`-p muster-core -p muster-agent -p
  muster-platform -p musterd -p muster-cli --all-targets`) reported no
  warnings. The touched files are rustfmt-clean.
- **Builds:** `cargo build -p musterd -p muster-cli` passed, including the new
  Objective-C notice panel. No app build, signing, publication or
  installation was done.
- **Device acceptance: not run.** These are unverified on a Mac:
  - the unattended unlock, cover and relock;
  - the notice panel's look and placement;
  - compaction while the person is using the Mac;
  - the notice closing after Claude records the compaction.
