# ADR 0134: Activate Claude's Next and Back through keyboard focus

- Date: 2026-11-05
- Status: Accepted; installed in 1.1.22 on three Macs, live card acceptance pending
- Related: [ADR 0017](0017-claude-question-submit-activation.md) (this record
  supersedes its "Back and Next retain their existing activation paths"),
  [ADR 0114](0114-restore-unlanded-1-0-21-field-fixes.md) (its `why=` warning),
  [QUESTION_CARDS.md](../QUESTION_CARDS.md)

## Context

Over 2026-11-02 to 2026-11-05, Mac-A's daemon log shows two answers to
two-question Claude cards. Both failed in the same way, on 2026-11-03 and on
2026-11-04, on two different sessions and builds. Each card had two
single-select questions, with two and three options, and the answer chose the
first option of each question.

1. The first page matched within 0.3 s (`AX question controls ready`).
2. One more tree was read, and `AX question actions dispatched ...
   completed=1/2` followed.
3. The second question never matched. The wait polled for 3 s until
   `locate question panel` timed out.
4. The ledger recorded an unknown outcome, because a question control had
   been pressed.

Every tree read in each attempt had the same node count (921 and 1129). The
page did not change after the first question.

The log rules out the other code paths:

- **No `AX question dispatch ... immediate=true` line.** The first question's
  options were therefore not all plain `AXButton`s. Claude's cards use radio
  options with a text child, so the page was not answered by pressing an option.
- **Only one tree read between the match and the dispatch line.** An option
  press re-reads the tree, so no option was pressed. The wanted option already
  read as checked.
- **A question write happened.** For a non-final paged page, the only other
  press is Next, sent by AXPress.

Earlier answers in the same log are consistent with this. Between 2026-10-02
and 2026-10-04, multi-question cards succeeded. Each first page either
answered on an option-button press, which advances by itself, or went through
Next by AXPress after a multi-select page. On 2026-10-22, ADR 0017 found that
AXPress on Claude's Submit only focuses it, and moved Submit to a focused Space
press. That ADR left Back and Next on AXPress.

This analysis rests on log lines and code paths. No live card's AX tree was
captured, and the explanation was not reproduced on a device.

ADR 0114's content-free `why=` warning never appeared in the log, including on
these failures. `locate_controls` checks its step before and after each tree
read, so an expired step returned an error before the loop reached the
warning.

## Decision

- **One activation path for Submit, Next and Back.** Rename the Claude profile
  flag `submit_via_focused_space` to `commands_via_focused_space`. When it is
  set, every card command uses ADR 0017's activation:
  1. Find the exact command inside the matched form.
  2. Re-read the tree and re-match the same form and element.
  3. Recheck the live guards.
  4. Focus the control and prove that focus.
  5. Send one Space.

  AXPress is never tried first, and nothing is retried after a key that might
  have been delivered. Codex keeps AXPress, since its flag is unset.
- **Fail as soon as Next is missing.** A non-final page of a paged card with no
  enabled Next now fails at once with "The question's Next button is not
  available", rather than after a 3 s locate timeout. If a control was already
  pressed, the outcome stays unknown.
- **Log the reason on every timeout exit.** Every expiry exit of the controls
  wait (`controls_step`) first logs the existing content-free summary
  (`visible=` and `why=`) for the latest tree.
- **Testable press rule.** Move the rule for whether an option press answers
  by itself into `Tree::answers_on_press`, so that it can be tested without
  AX.

## Alternatives

- **Press the checked radio option to trigger an advance.** In radio semantics
  this is a no-op. It would add one more write whose effect cannot be verified.
- **Retry AXPress on Next.** This can duplicate an action, and ADR 0017 showed
  that Claude's card buttons only take focus from it.
- **A pointer click.** This is not acceptable under Locked Use cover, for the
  same reasons as in ADR 0017.
- **A longer locate wait.** The tree did not change for 3 s, so waiting longer
  would only delay the same failure.

## Consequences

Next and Back now need provable focus, like Submit. Focus that cannot be
verified fails closed before any key is sent.

The inference may be wrong, for example if Claude's Next is a different
component. In that case the next field failure is still reported as an unknown
outcome. It will now carry a `why=` summary, and the new Next error if Next
disappears.

Selected-value echo is still not required before Next, which is unchanged
from earlier pages.

## Verification

- **Source and tests:**
  - New platform test
    `claude_radio_page_selects_then_activates_its_own_next_by_keyboard`:
    radio choices do not answer on press, while buttons do. The page's own
    Next and Back resolve to keyboard activation, and the second prompt is not
    yet matched.
  - Renamed agent test `only_claude_card_commands_use_keyboard_activation`.
  - Existing question-form tests pass.
  - Clippy passes with `-D warnings`.
- **Installed, 2026-11-05.** Shipped in Apple 1.1.22 (112) on Mac-A, Mac-B and
  Mac-C ([ADR 0138](0138-paired-release-1-1-22.md)).
- **Not done:**
  - Live acceptance. This needs an unanswered two-question Claude card on an
    installed build, answered from another device. Before answering, run the
    read-only `muster debug ax-question SESSION_ID` probe to record the page's
    roles. Then confirm both pages are answered and the provider receives the
    answer.
