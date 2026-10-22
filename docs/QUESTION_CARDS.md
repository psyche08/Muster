# Agent question cards

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

Muster reads Claude `AskUserQuestion` and Codex `request_user_input` /
`request_user_input_async` transcript records as structured questions. Transcript
access is read-only. All answer delivery uses the verified native application's
Accessibility controls; no CLI or private application state writes are used.

Shared Rust owns question IDs, option validation, single/multiple selection rules,
provider completion, capability negotiation and submission state. Apple views
hold only input drafts. Selecting an option does not submit it. The user submits
all remaining answers explicitly. Codex asynchronous questions are optional: they
start collapsed, do not block normal messages, do not force scrolling or a
notification, and retire from current prompts when their turn completes or a
new turn or ordinary user message begins (the desktop offers no answer controls
after completion). Retirement is not an answer acknowledgement. Rejection preserves the card and draft;
unknown outcomes block resubmission until completion is observed. An individual
Codex asynchronous answer does not resolve the other questions in that call.

Custom answers appear as an explicit **Other** row alongside the supplied
options. Selecting it reveals the text field; selecting it again removes that
draft from the submitted answer. Multiple-choice questions may combine Other
text with supplied options. The native adapter reconciles and verifies Other's
checked state too, so a prior failed attempt cannot leave an extra native
selection in a later submission.

The native adapter checks process/session identity, authorization, local input
generation, question identity and exact controls before changing the form. It
refuses incomplete AX trees, ambiguous controls and a different existing custom
answer. Native writes are queued FIFO per **host + provider**, including session selection,
Chat/Code mode changes, input and receipt confirmation. Claude and Codex on the
same host have independent queues; other hosts also proceed independently. The
console-wide Locked Use safety reservation remains separate. A queued operation
expires after 30 seconds without executing. Question preparation waits for fresh
AX controls/selection state rather than fixed sleeps, and the whole answer
operation is bounded to 90 seconds. A final button press is
acknowledged only when the provider records the corresponding result. Uncertain
presses and lost RPC replies must not be reported as successful submissions.

Servers advertise `agent.questions.v1`; clients require it before submitting
structured answers. Update the target Mac as well as the phone. Existing approval
cards retain their separate option-based protocol.

## Verification

- Parser tests cover Claude completion, Codex synchronous completion and partial
  asynchronous replies. Validation tests cover stale IDs, incomplete answers and
  ambiguous selections.
- RPC tests cover rejection/retry, duplicate submission while pending and unknown
  outcome blocking. Native observation tests preserve a question when evidence
  is temporarily unavailable.
- SwiftUI render tests cover Codex and Claude styles at phone width.
- The reported mac-b session `sunhuiling MIS 早晨5点授权卡片推送` (sunhuiling MIS 5 a.m. authorization card push) had already
  recorded answers at 12:24 on 2026-09-29. Its answered transcript was inspected;
  it must not be resubmitted as a test.
- On 2026-09-29, the signed Mac-A daemon inspected the unanswered Claude card
  in `Dunbo native 续` (Dunbo native continuation) using the production form matcher: all four multi-select
  `AXCheckBox` controls matched, were enabled and unselected; the separate
  custom-answer editor and Other control matched. Submit was visible but
  disabled with no answer selected. No controls were changed or submitted.
- The user's subsequent iPhone submission was rejected at 14:35:05 before
  Submit: `The native app did not confirm the selected answers`. A read-only
  probe then confirmed all four native checkboxes exposed `AXValue = 1` but
  `AXSelected = false`. The adapter now prefers AXValue for checked state;
  AXSelected is only a fallback when no control value is available. Mixed
  numeric values remain unconfirmed. Regression tests cover these conflicts.
- Local-only `muster debug ax-question SESSION_ID` reports matching, selection
  and submit availability without navigating or writing. It distinguishes a
  disabled Submit control from a missing one, and omits question/answer text.
- Live single-card submission passed on 2026-09-29: the user submitted all four
  options plus Other text from iPhone Build 9. Claude recorded the matching
  `tool_result` at 14:47:14.759 CST without an error, and the Muster ledger
  marked that interaction applied at 14:47:15. A subsequent read-only probe
  found no pending question, and the session resumed running. This verifies
  combined option/custom answers, not just the button press. Multi-page
  navigation still needs a corresponding live card.
- Delivery on 2026-09-29: iPhone Build 9 was installed and launched, with its
  version read back from the device. It is published at the fixed
  `https://linsheng.dev:8443/muster/ios/` URL. Signed desktop package 1.0.11 was
  installed on Mac-A, Mac-C and Mac-B; all reported Accessibility granted and
  relay registered with the same daemon SHA-256
  `ce0e80bc9056eb169202528354649794a2fecef9b47775a796caa58c4f46451f`.
  Package and feed signatures passed; this release was not notarized.
- Checks for this change: 23 agent, 4 platform, 13 daemon unit tests; 4 Apple
  rendering tests; Clippy; macOS/iOS release builds. The daemon tests required
  access to loopback sockets outside the sandbox. Phone-width Codex/Claude
  previews were inspected for the explicit Other row. The live submission
  above was verified separately from these build and automated checks.

### 2026-09-30 answer and send result fixes

- Provider completion removes only its own question error. A late rejected or
  unknown RPC response cannot restore that error or create a global error toast.
- Same-ID partial replies update the remaining questions and do not emit a
  whole-call resolution. Unchanged asynchronous questions can continue after a
  prior answer is recorded. Independent Codex cards submit and confirm each
  answer in turn; a shared Submit button remains a single final submission.
- Every selection and page transition re-reads and confirms the exact AX form.
  Identical option/Submit labels on other cards cannot select another card's
  controls. Multiple accessible aliases on one Submit count as one control.
- Send success requires a new exact provider user-message record in Code mode,
  or a newly appended matching user message in the same Chat conversation.
  Old matching text, assistant text and tool-result records do not acknowledge a
  send. Final presses wait up to 10 seconds for a receipt. Missing receipts are
  reported as timeout with unconfirmed delivery, never as a definite failure or
  success, and do not trigger automatic resubmission.
- Automated regression coverage includes both acknowledgement/error arrival
  orders, complete two-question RPC payloads, partial question updates, scoped
  independent/shared Submit controls, provider FIFO isolation across two hosts,
  timed-out queue cancellation, and new-record send confirmation.
- The real Claude/Codex multi-page flows still require a live unanswered card;
  these tests do not resubmit any of the user's already answered questions.
