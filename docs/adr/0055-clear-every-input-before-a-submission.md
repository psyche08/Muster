# ADR 0055: Clear every input before a submission, and hold a quiet cover

- Date: 2026-10-27
- Status: Accepted
- Related: `docs/AGENT_INTERFACE.md` (native send and question forms),
  [0005](0005-isolate-desktop-rpc-requests.md) (Locked Use display guard)

## Context

Device logs from the three Macs on the network (Mac Mini 1.0.94, M1Max
1.0.94 (85), M4Pro 1.0.98 (89)) were read for every send and answer that
failed. Four causes account for almost all of them, and each one is a
mechanism defect rather than a policy the operator wanted.

1. **A write over a draft is merged, not replaced.** `agent.send` writes the
   prompt with `AXValue` and then requires an exact echo. When the composer
   already holds text, Electron's rich editors keep their own document and
   re-read the DOM, so the field ends up holding both. The echo can never
   match, the request is refused as "the app did not confirm the draft", and
   the mixed draft stays behind for the next attempt to grow:

   | Device | Time (UTC) | Written | Composer read back |
   | --- | --- | --- | --- |
   | M1Max | 2026-10-22 14:52 | 50 chars | 70 chars (3 attempts) |
   | M1Max | 2026-10-21 | 1,721 chars | 2,423 chars |
   | M1Max | 2026-10-21 | 198 chars | 238 chars |

   42 refusals on the Mini, 129 on M1Max and 20 on M4Pro reached the user as
   "not sent" this way.

2. **A draft blocked answering outright.** `agent.respond` refused with "The
   native question already contains a different draft … it was not
   overwritten" (14 times, most recently 2026-10-23), and a draft in the
   conversation's own composer broke the question card as well: it adds a
   second, enabled Send to the surface ("expected one send button, found 2",
   5 times) and holds the application's focus, so the card's Submit cannot
   take focus and no activation key is sent ("The question submit focus
   changed", with `application=Different`, 6 times).

3. **`CoverLost` never once meant an uncovered display.** All 23 stops on
   M1Max and all 14 on M4Pro carry the same evidence: a *healthy* watchdog
   report, 250-462 ms old, with the owner's own poll delayed in the same tick.
   The watchdog checks the cover's health every loop and reports a lost cover
   explicitly; it never did. The owner was aborting whole operations because
   one report arrived later than `MAX_OBSERVATION_AGE` (250 ms), and
   `GuardSample::alive` reported the watchdog lost for the same gap.

4. **A destroyed submit control refused the send.** Electron rebuilds the
   composer and its controls around a write. A candidate it destroyed answers
   `kAXErrorInvalidUIElement`, so its enabled state reads as unknown, and an
   unknown duplicate is deliberately ambiguous — the live, enabled control was
   discarded along with the dead one.

## Decision

**Every submission starts from a confirmed-empty editor.** Shared Rust owns
the policy in `muster_agent::native_send`: `ClearWait` decides each attempt and
`Clearing` reports what the value says. It tries the editor's own editing
command first (focus, select the whole value, replace the selection with
nothing) and a plain empty value write second, alternating every 400 ms for at
most `CLEAR_DEADLINE` (2 s), because no profile can state which mechanism a
given app build honors and only the confirmation decides. An unreadable value
is never an empty one. The platform performs those OS calls in
`clear_editor` and re-resolves the exact session between them.

This applies to the composer before every draft write (`empty_composer`), to a
question's custom answer field before the answer is written
(`empty_question_editor`), and to the conversation's composer before a question
card is answered. Explicit sends already replaced a differing native draft with
Muster's text, so for sends this makes an existing decision work; for answers it
is a change: a draft in a question field is now cleared instead of preserved,
which the operator asked for so that each submission starts from a known state.
A field already holding exactly the text to write is left alone, and a
conversation with no visible composer has nothing to clear: the answer drives
the card's own controls, while a send still needs the composer and says so. If
an editor cannot be emptied, the request is definitely not sent and says so.

**An unreported cover is held, not lost.** `muster_core::locked_use`
checks `WatchdogLost` before `CoverLost`, so the tolerance can never apply
without a live witness, and tolerates a cover that carries no fresh report for
`MAX_COVER_QUIET` (1 s) once this lease has seen one. While the cover is
unreported, `Gate::cover_quiet` withholds every admission: `poll` reports
`WaitForUnlock` instead of `ReadyForAction` or `MonitorAction`, so no operation
starts and no queued operation is dispatched, and the actor leaves the queue
untouched rather than popping an item to a refusal. Work already in flight
continues, because the watchdog owns the cover: it reports a real loss
explicitly and locks the screen itself if this process disappears. A cover that
never reported at all is still lost at once. `GuardSample::alive` now reports
the watchdog process's own liveness, independent of `covered`.

**A destroyed submit candidate is dropped before counting.** `find_controls`
excludes candidates that answer `ax::INVALID_ELEMENT` and records how many in
its control diagnostics. An element that is gone can press nothing, so it must
not make the live control ambiguous. Unknown-but-live duplicates stay
ambiguous.

`question_diagnostic` also falls back to the surface diagnostic when no card is
waiting, so the composer and send controls a session would drive can be read
without one.

## Alternatives

- **Keep refusing when a field holds a draft.** This is the current behavior
  and it is what the logs show reaching the user as a dead end: the refusal
  names a draft the operator cannot see from the phone, and for sends the
  overwrite decision had already been taken.
- **Write with the selection mechanism everywhere.** The profiles choose their
  draft mechanism from recorded app behavior; replacing that is a wider change
  than the clear needs, and `catdesk` already uses it. Clearing is the step
  that must work from any starting state.
- **Raise `MAX_OBSERVATION_AGE`.** It governs every observation, including the
  screen and console state that authority depends on. The cover's freshness is
  the only one with an independent witness that reports its own failures.
- **Treat a stale cover report as proof of cover.** Rejected: admissions are
  withheld for the whole quiet window, so nothing is pressed on evidence that
  is merely old.
- **Raise the 3 s `locate question panel` budget.** The 15 timeouts show
  `matched=false` on every pass with a complete tree: the card is not in the
  conversation, so a longer budget would only make the same failure slower.

## Consequences

- A send or answer into a conversation whose composer or answer field holds
  text now succeeds instead of failing, and leaves no mixed draft behind.
- A draft the operator left in a question field is discarded. The composer's
  draft was already replaced by an explicit send; this extends the same rule to
  answers, and `AGENT_INTERFACE.md` records it.
- Each submission can spend up to 2 s emptying an editor before the existing
  4 s echo wait, well inside the 15 s Locked Use lease.
- A cover reporting gap of up to 1 s no longer aborts an operation, at the cost
  of up to 1 s in which the cover's freshness is unknown and nothing new is
  admitted. A genuinely lost cover is still reported by the watchdog, which
  locks the screen independently.
- Physical acceptance on the installed builds is still required; see
  Verification.

## Verification

- **Source and tests:** `cargo test --workspace` and `cargo clippy --workspace
  --all-targets -- -D warnings` pass. New Shared Rust tests cover both clearing
  mechanisms, the clear deadline and an unreadable value
  (`muster-agent`), and the cover tolerance: a quiet cover holds and admits
  nothing, resumes on a fresh report, stops after `MAX_COVER_QUIET`, is lost at
  once when it never reported, and never masks a lost watchdog
  (`muster-core/tests/locked_use.rs`).
- **Device evidence read:** daemon logs of all three Macs through
  2026-10-27 10:57 UTC, as tabulated in Context.
- **Builds, signing, notarization, publication, installation:** not performed
  in this change.
- **Runtime and provider acceptance:** not performed. Clearing a composer and a
  question field against the live Claude and ChatGPT desktop apps, and a send
  during a watchdog reporting gap, still need acceptance on an installed build.
  No claim is made that either path has run on a device.
