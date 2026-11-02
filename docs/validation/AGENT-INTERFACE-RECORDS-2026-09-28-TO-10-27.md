# Agent Interface Field Records, 2026-09-28 to 2026-10-27

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](../PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

Dated verification, diagnosis and incident records moved out of
[AGENT_INTERFACE.md](../AGENT_INTERFACE.md), which keeps the interface rules.
Where a record changed behavior, its ADR (linked in the record) is the
decision; a record that says "above" means that document.

## Cache accounting verification (2026-10-01)

Read-only comparison of 127 completed local Codex turns found agreement between
legacy totals and response records. Two recent Claude transcripts contained one
and four active-turn resets respectively, demonstrating the dropped-usage path.
No transcript content or local session data is stored in the repository.

Agent and app-core tests: 72 passed, two existing environment-dependent tests
ignored. Agent library and CLI Clippy checks passed; all-target Clippy remains
blocked by an existing `inconsistent_digit_grouping` warning in
`muster-agent/tests/process_state.rs:182`. Rust FFI built. The Apple protobuf
round-trip and 350-point footer rendering test passed, with range and unavailable
previews visually checked. This is source/fixture verification, not a signed
release, installation, live cache-expiry measurement or device end-to-end test.

## Verification (2026-09-28)

- Rust: 46 tests passed across agent, app-core, FFI, protocol and daemon; one
  opt-in test ignored. Clippy passed with warnings denied. Regression coverage
  includes delayed DeviceInfo plus a transient catalog failure, tab changes,
  chat catch-up, per-request cache accounting and missing/fast-tier usage.
- Swift: six tests passed. An offscreen 402-point-wide native render checked
  horizontal device tabs, a closed tool result, Markdown code/list/table and
  the turn footer. This is component rendering, not an iPhone interaction test.
- Real mac-b Codex transcript: the two Locked Use test turns expose structured
  summaries, including 34,658 input, 8 output, 34,432 cached input (99.3%) and
  estimated USD 0.037092 for the second turn. No new prompt was sent.
- Release builds and strict signatures passed for iOS and macOS. The final
  app was installed on iPhone, Mac-A, Mac-C and mac-b; no new notarization was
  performed. All three desktop catalogs respond after installation, and the
  iPhone startup log confirms DoH relay resolution and peer connections.
  The redesigned phone screen still needs user interaction acceptance.

Mobile-target follow-up: Shared Rust now exposes only desktop/server service
targets while preserving the full network roster. Nine app-core tests and six
Swift tests passed, including a mixed desktop/server/phone/tablet roster.
The signed mobile-target update was installed on iPhone, Mac-A and Mac-C.
The user subsequently identified the mac-b black screen as a display-brightness
change; its cause remains unknown. No brightness, DLP, lock or cover setting was
changed during that investigation.

Conversation-style follow-up: 54 Rust tests passed (one optional FSEvents test
ignored), Clippy passed with warnings denied, and seven Swift tests passed.
Native 402-point component renders verified collapsed work details, final
Markdown, file-change cards and a non-wrapping colored diff. Release builds and
strict signatures passed; the signed app and matching daemon were installed on
iPhone, Mac-A, Mac-C and mac-b without new notarization. The installed daemon
returned historical Codex file-change records with nonempty diffs from this
development session. Phone startup and desktop catalog reads passed; physical
phone taps through the new preview flow are still an interaction acceptance step.

## Mac-C send prerequisite repair (2026-09-28)

The target's live DeviceInfo reported `accessibility: missing` and the requested
Codex session had no AX controller. The user enabled musterd in macOS
Accessibility. After the signed update and a Muster-only service restart,
DeviceInfo reported `granted` and Codex AX binding resumed. The currently visible
session was a different thread; the requested thread must be opened for its
exact AX surface to bind. No additional prompt was sent to the user's training
session during diagnosis.

Agent writes now check missing OS Accessibility permission before touching the
operation ledger or attachment files, and name the target Mac plus the system
setting to enable. A session without a controller gets on-demand exact-session
AX discovery instead of waiting for the background polling tick. This does not
select or send to a different visible thread. The regression for permission
failure and no attachment side effects passed; `muster info` now reports the
live Accessibility state for remote diagnostics.

## Native send draft recovery (2026-09-29)

On Mac-B, `Example app v1.0.0` had an attached Codex AX controller, Accessibility
permission and a registered relay. The 14:54:18 send was rejected before the
button press (`desktop changed; the draft was left in place`); the 14:54:23
retry was then blocked by an incorrectly latched unknown outcome. AX inspection
confirmed the original unsent draft remained in the requested thread. The old
combined guard did not distinguish local HID input from a changed AX surface,
so the original trigger is not proven.

The send path now refreshes the verified session's composer after AXValue writes,
requires an exact text echo and a unique current send button, and distinguishes
local-input changes from surface changes. Pre-press failures preserve the draft
without setting an uncertain-submission latch. An explicit subsequent send may
reuse an identical draft. Per the user's explicit rule, differing native drafts
are replaced in full with the Muster text using AXValue; the exact replacement
must be read back before sending. Unreadable drafts and intervening local input
still reject the send. Only an
attempted button press can set the unknown latch, and an uncertain press blocks
replay even when the text still matches. No retry is automatic. Draft text is
no longer included in send diagnostics.

Regression tests cover identical-draft retries, differing-draft replacement for
both providers, unreadable-draft rejection, and uncertain-press rejection with
matching, different and empty drafts.
Live send acceptance is recorded separately after deployment.

Desktop package 1.0.13 was signed and installed on Mac-B, Mac-A and Mac-C;
all three reported Accessibility granted and relay registered, with daemon
SHA-256 `f9e8a067796064eff381d0e95e210b229e7e7afdbb3060bfb9db53f82b71b306`.
The HTTPS update feed serves 1.0.13. Platform unit tests (6), daemon unit tests
(13), Clippy and the release build passed. The app and feed signatures were
verified; notarization was not performed. iPhone stays on Build 9 because this
change is in the Mac native controller. At 15:15:06 CST the user's retry was
applied, and the current Codex rollout recorded one corresponding user message
at 15:15:10.675; the preceding attempt at 15:14:57 was rejected before any draft
write because input generation changed. The specific event source/type was not
recorded, so this is not evidence of an AX-generated input event.

Read-only send preparation now tolerates a transient input-generation change:
Shared Rust waits for 150 ms of quiet, then re-resolves the same session and
re-reads the draft. This settling phase is bounded to one second and checks
live authorization across each wait. Preparation errors, including uncertain
prior submissions, are not retried. Once drafting starts, the input generation
is fixed and any change still aborts; no draft write or button press is replayed.
Regression tests cover fresh preparation after input settles, ongoing activity,
authorization revoked during the wait, and non-retryable preparation errors.

Desktop package 1.0.14 with this preparation policy was signed and installed on
all three Macs, which reported the same daemon SHA-256
`ec0f66141b669dec800e4a36bfb77d7f250fb0d09b7e88386eafa616dceb192a`, Accessibility
granted and relay registered. The signed update feed was published and checked
over HTTPS. Agent unit tests (27), platform unit tests (6), daemon unit tests
(13), Clippy and the release build passed. No notarization was performed.

On 2026-09-29 the user confirmed first-attempt success on Mac-A in the Claude
session titled `Codex会话数据审核要求分析` (Codex session data audit requirements analysis). The send ledger recorded one applied
operation at 15:24:13 CST and the native transcript recorded one corresponding
user message at 15:24:13.787 CST, with no duplicate delivery observed. No
preparation retry was logged for this send, so it validates ordinary delivery
on 1.0.14 but does not exercise the transient-input settling branch. A new
Mac-B live send on 1.0.14 remains unverified; the successful 15:15 retry above
was on 1.0.13 and must not be counted as acceptance of the new settling behavior.

## Desktop catalog and send investigation (2026-09-30 evening)

[CHAT_CATALOG.md](../CHAT_CATALOG.md) records the typed local ChatGPT directory,
ChatGPT-first default, desktop cache behavior, supported schema and incremental
boundaries. It supersedes the AX-only catalog description above; desktop
conversation control and message viewing still use Accessibility.

Read-only investigation of the user's mobile send failures (times below CST):

- Mac-C's operation ledger records four rejected sends at 19:45:06, :22, :27,
  :38: `expected one send button, found 0; draft preserved, not sent`.
  Native logs show draft write/reuse and no send confirmation. This proves a
  send-control matching failure after drafting, not a transport failure. It
  does not establish whether the live app label/state changed or the control
  appeared later; the historical AX snapshot was not recorded.
- Mac-C at 19:54:54 verified Presence but at 19:54:58 reported
  `the Apple lock-screen authorization field is unavailable`. Cleanup returned
  to Idle. This separate failure occurs before temporary unlock and before
  any native send.
- Mac-B's three attempts at 19:53:21, :30, :57 verified Presence, consumed the
  plugin grant, reached Active and admitted the bound operation. Each finished
  within about one second and relocked. No corresponding send operation was
  inserted into the ledger. The failure boundary is before ledgered sending:
  provider reservation / mode restoration / exact-session AX preparation.
  Existing logs omit the actual pre-ledger error, so the precise reason is not
  established. A post-unlock AX-readiness race is a hypothesis, not a finding.
- Both machines subsequently reported Accessibility granted, Relay registered
  and screen locked. Mac-B's earlier SEP signing errors at 19:20 recovered on
  daemon restart and dual-carrier authentication at 19:27; they do not explain
  the authenticated and executed 19:53 requests.

No send, unlock, restart, signing or installation was performed during this
investigation. The source now logs native-preparation errors before the ledger without
logging the request body. Mobile UI error text and a controlled reproduction are still
needed to settle Mac-B's exact failure and validate a repair.

### Claude Agent sidebar status labels (2026-09-30 20:28 CST)

A read-only AX capture during a phone-authorized Locked Use attempt on Mac-B
settled the selection failure. The target was visible in the sidebar:
its AXButton title was `Idle 示例应用导航任务续`, containing an `AXImage`
labelled `Idle` and an `AXGroup` with one `AXStaticText` value
`示例应用导航任务续`. The transcript and Claude desktop metadata agreed
on the title and CLI session ID; the desktop record was not archived.
Exact comparison of the compound button title with the catalog title returned
zero candidates. The main web area was the Code home page. Presence, plugin
consumption, temporary unlock and subsequent relock all completed normally.

The Rust selector now also accepts the exact independent text label of a
Claude sidebar button, only when the catalog title is unique and the bounded
read found exactly one nonempty passive text label. It does not strip status
prefixes, match substrings, inspect sibling rows or descend into nested
interactive controls. Incomplete label reads and ambiguous candidates fail
closed. Codex retains its existing matching behavior. Selection still presses
once and verifies the opened session ID before drafting or sending.

Regression coverage includes status changes, near-match titles, absent or
multiple labels, catalog ambiguity and the provider boundary. This repair
addresses the confirmed Mac-B selection failure; Mac-C's separate missing
send-control failure is not established as having the same cause.

### Bounded AX operation diagnostics

The subsequent Mac-B retries at 20:44:33 and 20:44:43 CST still returned zero
sidebar candidates. Native Agent selection and sending now emit INFO-level
`AX diagnostic trace=<id>` records to
`~/Library/Logs/Muster/musterd.log`; no environment setting is required.
Each trace identifies the provider, session and operation, records its last
stage and elapsed time on exit (including early errors or cancellation).

Selection reports the current web areas' route categories and resolved session
IDs, navigation-area count, row roles, bounded sidebar titles/descriptions,
ID/title match decisions, AXPress availability and independent-label read
results. Label reads include visited-node count, skipped interactive groups
and the reason an incomplete read stopped. The first 24 rows and relevant
later rows are logged, with a hard cap of 48. AX traversals report their node
count, elapsed time and whether they completed, hit a node/time limit or were
stopped by the visitor. These reports reuse the selection traversal.

Sending reports preparation, drafting, echo, send-control discovery, final
validation, press result and provider-receipt stages. Control samples expose
roles, known control labels, enabled/focused flags and composer/send match
decisions, capped at 16 samples with a 150 ms sampling budget. The final
control counts and traversal status are always reported. Arbitrary main-pane
button labels are redacted; logs do not dump AXValue, message/transcript
text, full URLs, signatures or authorization material. Sidebar metadata is
limited to 96 characters per field and Debug-escaped. Background observations
do not create these operation traces. Diagnostics add no clicks, retries or
authorization changes and do not establish successful message delivery.

### Native send readiness and Claude label traversal repair

The 20:57 Claude traces showed the exact target row but `labels=[]` and one
skipped AXPress group. Electron advertises AXPress on the group wrapping its
static title. Title traversal now follows AXGroup by role, regardless of its
actions, while still stopping at nested buttons, links, rows, menus and popup
controls. The bounded traversal lives in Shared Rust and has a regression
fixture matching this real button/image/group/text shape.

The Mac-B Codex attempt at 20:56:47 was rejected for missing draft echo; the
20:57:07 retry reused that draft and was Applied with a provider receipt.
Native Agent sends now resolve a fresh, uniquely identified session surface
on every readiness poll. Shared Rust permits up to two seconds for echo and,
once echoed, up to two more seconds for a unique enabled send button. The
composer may be focused once, without rewriting the draft or sending keys.
Missing/replaced composers can settle within the echo window; ambiguous
buttons remain rejected. Live authorization/input checks continue before
focus and the single final press; a press is never automatically retried.

Codex's `composer_scope = "main-conversation"` profile can resolve duplicate
prompt editors only when one is in the main landmark and all others have
explicit auxiliary-pane ancestry. Unknown ancestry or two main editors still
refuse. The selected editor's submit cannot come from a foreign editor's branch
or an ancestor shared with it. Single-editor layouts remain compatible unless
that editor is explicitly auxiliary. See [ADR 0062](../adr/0062-scoped-composers-and-send-preparation-outcomes.md).

## Claude paginated question diagnosis (2026-10-02)

The post-1.0.22 answer failure was on Mac-B. Its operation ledger recorded an
`agent.respond` rejection after the three-second form/selection wait; Locked
Use unlock and exact-session selection had succeeded. The native transcript
still had no answer result for the pending three-question call. The old error
combined initial form matching and selected-state readback, so the precise
historical wait stage cannot be recovered from that log.

A signed read-only CLI inspection subsequently completed under Locked Use and
confirmed relock before returning. The live form was on page 2/3. Only the
second question matched a complete form; its options were AXButton elements
with no AXValue and AXSelected=false. Recommended labels matched exactly.
This reveals two unsupported states in the old adapter: starting on a later
page, and a single-choice button advancing instead of retaining a checked
value. No answer was selected or submitted during the inspection.

Claude answer handling now returns to the first question using only a Back
control scoped to a uniquely matched live form. After an option press it
accepts either a confirmed selected value or the exact adjacent question
form; it never treats an arbitrary page change as acknowledgement or presses
Next again after observing advancement. A final button may itself submit, so
its outcome stays unknown until selected-state readback or the provider's
answer receipt. Timeouts do not trigger retries. Codex's existing card behavior
is unchanged.

Validation: 14 platform tests pass, including the sanitized 2/3 AX structure,
transcript prompts outside the form, the separate sidebar Back control,
nonadjacent pages and missing options. Affected-target Clippy passes. CLI
structured DeviceInfo calls succeeded for Mac-A, Mac-B and Mac-C; malformed
JSON fields are rejected. The real read-only inspection and relock passed on
Mac-B. The answer-path change has not submitted the user's pending answers;
physical phone answer delivery remains unverified.

The generic CLI covers daemon RPCs. Native UI-only workflows and operations
requiring interactive Presence retain those requirements; raw RPC access does
not itself supply an interactive authentication/signing flow.

## Unconfirmed send recovery and pre-unlock refusals (2026-10-02)

Mac-A (installed 1.0.27) refused every send to Codex session `example-codex-session` from
13:45 to 17:55 CST with "an earlier send has an unknown outcome". The 13:42:29
press (op `app-1790919746987954000-15`) received no provider receipt within
10 s. Its ledger body hash matches no user record in any local Codex transcript
of 2026-10-01/02, while the same hash check matches applied sends; the queues
held no item. No late receipt existed for it, so the latch was held correctly.
The defects were around it:

- Evidence was read only by the AX observer, which needs an unlocked screen and
  the target conversation visible. Shared Rust `unconfirmed::WriteGuard` now
  owns the latch. It reads only provider evidence (a new user record with the
  exact text, or a new Codex queue item), from the controller's poll loop even
  while locked or while another conversation is shown, and again before every
  new send, queued send and answer. Elapsed time, new requests, AX loss and the
  lock state never clear it. A rewritten transcript can no longer confirm it.
- A refused request was reported as `UnknownOutcome` and left a `pending`
  ledger row although nothing was typed or pressed. The refusal is now a
  definite "Not sent" naming the unconfirmed op and its time. The ledger also
  settles any tracked `UnknownOutcome` that reached no delivery-capable press
  as rejected, with or without an op_id. After a possible press it stays
  unknown and is never replayed. Approval presses that report an AX error,
  and question flows that fail after a press or text write, are now unknown
  instead of rejected.
- Claude Code session `example-claude-session` (started with `claude -p`, transcript
  `entrypoint=sdk-cli`) was sent to at 18:55 CST. Claude.app had no conversation
  for it, so Locked Use unlocked the Mac and the sidebar search found nothing.
  A Claude transcript written only by `cli`/`sdk-cli` that no Claude.app
  `claude-code-sessions` entry drives (`cliSessionId`) is now refused before
  Locked Use: it has no supported input channel. Locally, none of 7 such
  transcripts is registered, while all 104 desktop transcripts record
  `claude-desktop` and keep the desktop lookup.
- The 16:19 answer timed out locating controls for a Codex asynchronous
  question whose turn had completed at 16:18. Of 104 local asynchronous
  questions still open at `task_complete`, none was answered afterwards, and
  AX showed no extra controls. Such questions now retire at turn completion.

Not determined from the logs, so only diagnostics were added: the two
`CoverLost` stops (16:18:52, 17:55:03; the watchdog never reported an unhealthy
cover, and the owner saw delayed OS sampling at the time) now log the newest
watchdog status age, owner sampling time and pulse gap; the 19:03 Codex
selection that still showed the previous thread after 3 s now logs whether the
pressed sidebar row survived and reports itself selected; an echo that still
reads the composer placeholder is classified `placeholder`, not `different`.
Cover freshness, retries and selection timeouts are unchanged.

Physical end-to-end verification of these paths is still required on the
installed build.

### Claude prompts queued during a running turn (2026-10-02)

Mac-B refused every send to Claude session `example-session` from 18:57 to 19:41 CST
with the 1.0.27 message "an earlier send has an unknown outcome". The cause is
not covered by 1.0.29. The 18:57:01 press (op `app-1790938619634168000-18`)
landed while a Claude turn was running. Claude Code recorded the exact prompt as
`queue-operation`/`enqueue` 0.2 s later, absorbed it into the turn as a
`queued_command` attachment, and removed it from the queue (`absorbed_mid_turn`).
It never wrote a `user` record, so the receipt timed out and the latch stayed.
The ledger body hash matches both records. The user's retry with the same text
(op `-31`, 19:43:54 CST on 1.0.29, after the deployment restart had dropped the
in-memory latch) produced the only `user` record of that text.

Claude send receipts now also accept a newly appended prompt-queue `enqueue`
with the exact text: provider acceptance, like a Codex queue item. They also
accept a human `queued_command` prompt attachment. Task notifications, peer
messages, other command modes, sidechains, `dequeue` and `remove` records do
not confirm a send. Locally, prompt-mode `queued_command` attachments (about 300
per Mac) are common, so this applies to every send made while Claude is busy.

Codex queue admission now uses the same provider-scoped full-text URL matching
as transcript receipts. Its pre-press baseline retains both physical item IDs
and available client-message IDs, so local-to-server migration of a known item
does not establish a new submission. The client uses a separate display-only
matcher for queued previews; a clipped preview cannot settle an unknown receipt.
See [ADR 0063](../adr/0063-queue-receipts-use-provider-text-matching.md).

## AX recordings on Mac-C (2026-10-03)

The states that the 2026-10-03 fixtures lacked were recorded on Mac-C (Muster 1.0.31) with the console in use: a running Claude Code turn, a running Codex turn, a Claude Code question card, and both apps' Chat mode. No message was sent. The running turns were already running; the question card was this repository's own agent asking; Chat mode was entered with `chat-open` and left with `debug ax-chat-mode … code`. The redacted trees are in `src/muster/crates/muster-agent/tests/fixtures/ax`.

Findings:

- **Claude's Stop has an empty title.** Read live, its AXTitle is `""` (present, not absent), with "Stop" in AXDescription. Up to 1.0.32 the controller read the title and fell back only when it was missing, so a running Claude Code turn was neither reported nor interruptible through Stop. 1.0.33 names controls by title, else description, and counts an empty title as none. That build is on Mac-B; acceptance is still pending.
- **The question card was answered through Muster.** The card was answered from the Muster app. The 1.0.31 controller found its controls and dispatched both answers through AX (`question_forms` log, `agent.respond` applied).
- **ChatGPT's long Codex thread exceeded the dump limit.** It ran past `ax-dump`'s 1,500-node limit before the composer was reached. The dump now reads up to 20,000 nodes.
- **A draft hides Stop in both apps.** While a turn runs and the composer holds a draft, neither app shows Stop. Claude shows Send, which queues the prompt into the running turn; Codex shows Queue in the composer's branch. Both are in the profiles' send labels, so a send while busy finds its control. The running state normally comes from each app's own runtime status (Claude's worker registration and transcript, Codex's rollout, CatDesk's socket); the Stop check is only the fallback when that status is missing or older than 10 s. There, a draft without Stop now reads as unknown rather than idle (`native_send::observed_state`), since Claude's Send looks the same idle or busy. An interrupt that finds no Stop but a draft says so: "The composer on the Mac holds an unsent draft, which hides Stop; clear it there to interrupt". Device verification comes with the next build.
- **Two Chat-mode opens misbehaved (not yet investigated).**
  - `chat-open codex <id>` for the first listed conversation failed with a bare "remote error"; the next conversation opened.
  - `chat-open claude <id>` reported Chat mode but left the app on its new-chat page.

## Locked Use acceptance on Mac-B, 1.0.34 (2026-10-03)

The user sent from the phone to the locked Mac-B; its daemon log was followed over the LAN.

- **Codex passed.** Session 「仅回复 LOCKED_USE_OK」 received `Muster 验收：请用终端运行 sleep 60，结束后只回复 OK`. Every step ran in one operation:
  - challenge, operation signature, plugin-authorized unlock;
  - sidebar selection (1 candidate among 101 rows) and the selected surface verified;
  - draft written and its echo confirmed;
  - the one Send button found (`send_match=true`) and pressed once;
  - receipt confirmed by queue admission, ledger `applied`, relock under cover.

  Codex ran `sleep 60` and answered "OK". No interrupt was tried.
- **Claude failed before writing anything** (five attempts). The target session 「屏幕亮度降至0%」 (2026-09-29) was not among the 40 sidebar rows. The Recents list shows the latest 20 behind "Show 20 more in Recents".
  - Row matching itself worked: titles carrying a status ("Idle …", "Running …") were matched through their standalone text.
  - Each attempt refused with "cannot uniquely select the session in the desktop sidebar (0 matches …)" and relocked.
- **Fix:** the Claude profile names its "more" control ("Show <count> more in Recents"). When the session list has no row for the session, selection presses it under the same guard as a row, waits up to 2 s for more rows and looks again. It does this at most five times, and stops after a press that lists nothing more.
- **Retry on 1.0.35 (22:23 CST).** Expansion worked: the list grew from 40 to 60 to 69 rows, then "Show more" was gone, and the session was still not among them.
  - Cause: Claude.app had archived that conversation. `isArchived: true` is set in `claude-code-sessions/<org>/<user>/local_<id>.json`, and archived conversations never appear in Recents.
  - Muster read archive flags only from Claude Code's `sessions-index.json`, so it still listed the session on the phone.
  - Fix: Muster now reads that desktop registry, re-parsing a file only when it changes (bounded).
  - A Claude Code session counts as archived when every desktop conversation driving it is archived. It is then hidden, and a send is refused before any unlock: "… is archived, so Claude.app no longer lists it; unarchive it there first. Nothing was sent."

## Dropped Claude send and latch release (2026-10-04)

From 03:39 to 05:13 UTC, Mac-B (1.0.37) refused every send to Claude session `example-session` with "the earlier send … has no provider receipt yet". Mac-C had no failures that day.

What happened:

- **03:39:01.** A send pressed while the session was idle was confirmed at once (a `user` record) and started a turn.
- **03:39:14.** The next send was pressed 13 s into that turn, before its first reply:
  - Stop was visible, the draft echoed, and the composer showed Send.
  - The one Send button was pressed (`press_result=Ok`).
- **No provider ever had the prompt.** The transcript gained no `enqueue`, no `queued_command` attachment and no `user` record with it.
  - Claude.app's own registry entry for the conversation (`latestUserFrameAt`) still pointed at 03:39:01.
  - The turn ended at 03:40:28 (`end_turn`, then `stop_hook_summary`), and the transcript was not written again.
  - At 05:12 the composer was empty: Send was present but disabled, as on an idle, empty composer.
  - Claude.app took the text out of the composer and dropped it.
- **The latch could not end on its own.** Only a receipt cleared it, and only a daemon restart dropped it.
  - The user's six later sends were all refused.
  - Restarting Mac-B's `musterd` at 05:21 cleared it. The 03:39:14 text has to be sent again by hand.

The guard (`muster_agent::unconfirmed`) retains uncertain writes until a
provider receipt/queue admission or an explicit user release. The former
120-second turn-end heuristic was removed on 2026-10-13: Mac-C's October 10
send received its receipt after 1,069 seconds, despite having been reported as
dropped. A completed turn and elapsed time cannot prove non-delivery.

Provider evidence now confirms the original `agent.send` ledger row as well as
clearing the session latch. The callback is captured before the press and bound
to the original operation id, kind, request hash and session. It is idempotent
with synchronous completion, never replays a send, and cannot overwrite a
rejected or differently bound operation. If persistence fails, the guard retains
the consumed evidence and retries persistence on reconciliation. It cannot be
released as undelivered while that confirmation is pending. This applies to
live controllers; it does not reconstruct receipt watchers lost in a daemon
restart or retroactively resolve historical unknown operations.

A refusal behind a latch carries its token (`OpResult.unconfirmed`): the ledger
operation id, or its press time for an anonymous write. The phone's
「确认未送达，继续发送」 action sets `AgentSendRequest.release_unconfirmed`, part of
the signed Locked Use operation. The daemon rereads evidence before releasing
exactly that token. If the provider confirmed it, release is refused to prevent
duplicate delivery. A different token is refused with the current token.

### Explicit delivery resolution (ADR 0061)

Codex user-record receipt matching accepts only equal-label HTTP(S) Markdown
links as formatting of the original bare URL. It does not equate different
labels, destinations, literal code, assistant output or older transcript records.
The timeline uses this same comparison for complete user messages.

An updated Apple client also offers a read-only recheck and a separate confirmed
"seen on Mac, do not resend" action. `agent_confirm_delivered` takes the entire
original `AgentSendRequest`, requires the current provider control grant and
checks its ledger binding. It records `USER_CONFIRMED`, distinct from `APPLIED`
provider evidence, and clears only the corresponding write latch after durable
storage. It never submits input or unlocks the host. Missing/conflicting/rejected
operations remain unresolved. Repeated confirmation is safe; status queries
recover the decision after a lost response. The host advertises
`agent.send.confirm.v1`; older hosts require an update. Newer composer edits
survive settlement, and unknown outcomes are never labelled "not sent".

### Native send readiness (2026-10-13)

Mac-C exposed two Codex `Send` controls after drafting: one disabled and one
enabled. After composer/queued-row scoping, explicitly disabled duplicates are
excluded when an enabled candidate exists. Unknown enabled states remain
candidates; two actionable candidates still reject. The exact candidate and
its enabled state are revalidated immediately before submission.

Draft preparation now resolves a fresh session surface, focuses the composer
before writing, then resolves and validates it again because focus can rebuild
the editor. It still replaces the entire draft once, requires exact echo, and
never retries a submit. Question answers use the existing bounded quiet-input
preparation before the first action; input changes after actions begin still
stop the workflow. Navigation failures and active local input remain explicit
failures rather than authorizing a write into an unverified session.

The iOS app renders the release; the Android and HarmonyOS apps do not yet show it. Their sends are still refused with the Mac's message as before.


### Code-mode navigation readiness (2026-10-16)

The pre-send Code-mode check and initial session selection now use Shared
Rust's bounded read-only preparation. A complete application/menu-only AX tree
is not evidence that the renderer or requested session is absent. Missing
content/navigation areas are re-observed for up to a three-second retry budget.
If input changes during these reads, the snapshot is discarded and a 150 ms
quiet interval is required before a fresh read. Process identity, unlocked
console and live authorization remain mandatory. Ambiguous surfaces and
observation errors fail immediately; no alternative composer is selected.

No navigation press, composer write or submission belongs to this retry phase.
Once a navigation action starts, its input generation stays fixed, and a change
still stops the workflow. Existing write preparation retains its one-second
budget. Failure logs include the previous and current input-generation totals,
without key content, to distinguish a real counter change from unavailable AX.

This closes two preparation gaps seen during 1.0.58 acceptance: zero navigation
areas failed immediately, and input during the first read-only mode snapshot
aborted before the existing write-settling phase could run. A separate attended
eight-second input-counter sample was stable; it does not identify the event
source of the earlier failures or prove that AX generated them. Physical send
and card acceptance must still be reported separately from these regressions.


### Execution traces and Claude desktop sub-sessions

Conversation pages expose Conversation, Tracing, Subagents and Sessions views.
Shared Rust pairs tool calls/results only by unique provider call IDs; repeated
IDs and orphan outputs remain separate. Trace times are approximate seconds from
recorded timestamps, and missing or reversed times stay unknown. Closing a turn
never invents successful tool results. Earlier events use normal history paging.

Claude's `toolEndsTurn: true` user result is an explicit handback boundary, even
when no assistant `end_turn` follows. The same grammar drives viewer summaries
and runtime recency, so completed inline children clear descendant activity in
both session lists and open conversations; later real activity can resume them.

Claude desktop `spawnedFrom.sessionId` links are resolved from desktop IDs to
`cliSessionId` inside one organization/user registry. These independently running
sub-sessions retain their own verified worker and existing input authority;
inline subagents remain read-only. User forks are navigable but do not contribute
to ancestor activity. Missing/ambiguous mappings are not guessed. The bounded
wire summaries carry direct children only; activity includes deeper descendants
with cycle protection. Each related session opens the same conversation/trace
views and offers navigation back to its parent.

### Mobile conversation terminal

Mobile conversation pages also expose a Terminal tab. Opening it asks Shared
Rust to start or reattach a tmux session on the conversation's device, using the
working directory from that device's AgentSession record. No command or cwd is
accepted from the client. The host needs tmux installed. Cloud chat sessions
without a workspace cannot open this terminal.

Start reuses the same shell; End kills only this agent's tmux session; Restart
ends that shell and creates a fresh one in the recorded workspace. Leaving the
tab, navigating away, or backgrounding the phone detaches its display and leaves
the shell running. Foreground reconnect restores the attachment. A dedicated
socket under the daemon data directory isolates these sessions from personal
tmux servers. Terminal control and provider read permissions are both required
for lifecycle actions. These PTYs do not operate native desktop windows.

Lifecycle RPCs carry operation IDs and use the daemon ledger. A failed/unknown
operation is displayed rather than automatically replaying a restart. Stale
results cannot attach to another conversation or a tab that has been closed.

#### 1.0.70 (61) acceptance on Mac-A, 2026-10-20

Release source: `ce78b04796d22d606c08848d8e20f6eb4f3964da`. macOS and iOS
Release builds succeeded. Apple notarization
`a5709ef6-2ca7-44c2-9c13-1237c8591c39` was Accepted; stapling, signature and
Gatekeeper checks passed for the published and installed Mac app. Public ZIP
SHA-256: `ef5fc0eac4bc378f6f92e8996e331600973ff7158fcd798245b48d987d762756`;
IPA SHA-256: `ff2b23e67f57dbc033fe67673de26af1abaeba00599b993313f214ec5b100a7b`.
Public OTA manifest and downloaded IPA both identify build 61.

The installed Mac-A daemon retained its identity, accessibility grant, relay and
vnet registration. With the console still locked, an existing dedicated Claude
acceptance session opened tmux in `/Users/developer/Developer/Projects/Muster`.
Installed RPC checks passed for cwd, repeated Start returning the same terminal,
shell environment persistence, resize, Restart returning a fresh terminal and
clearing environment, duplicate Restart preserving the new environment, End,
and a subsequent fresh Start/End. The test terminal was ended afterward. The
isolated tmux integration test additionally verifies service reconstruction,
metacharacters in cwd, invalid restart preserving a shell, and isolation between
two agent sessions. No provider message or desktop input was sent.

Only Mac-A was updated for this release. Android Kotlin compilation passed;
HarmonyOS bindings were regenerated but no HarmonyOS package was built. The
iPhone was unavailable, so installation and Terminal-tab interaction on physical
mobile hardware remain unverified. The earlier independent-subagent live
completion acceptance remains outside this terminal release's evidence.

Validation limits: the full Rust workspace run reported 632 passing tests,
9 ignored tests and two failures. The missing-cloud-credentials test incorrectly
consulted the host's signed-in Keychain; its file-only fixture was corrected and
its rerun passed. The pre-existing network presence test
`an_absent_mac_is_not_dialed_until_the_relay_says_it_registered` failed its wire
packet count assertion during both parallel-suite runs but passed when run alone
from the original workspace test binary. Its intermittency is not claimed fixed.
The tmux, app-core and daemon suites and workspace Clippy passed; no remote CI
checks were created for this PR.

## Claude Submit activation and Codex approval labels (2026-10-22)

See [ADR 0017](../adr/0017-claude-question-submit-activation.md). On Mac-A with
Muster 1.0.79, a dedicated Mac-B answer operation returned applied after AXPress
focused Submit, but the form and provider transcript did not advance. A separately
authorized mouse retry produced the answer receipt. A new independent test also
succeeded with a single Space on the keyboard-focused Submit button.

Claude's compiled question profile therefore uses focus plus application-directed
Space for a separate Submit control. Rust rechecks the exact session/question,
control identity, enabled state, actual application focus, authorization and local
input generation. It performs one key pair, without preceding AXPress or automatic
fallback. This does not alter immediate-choice, Back, Next or Codex submit paths.
The existing nonblocking dispatch notice still does not claim provider receipt.

Codex's exact approval vocabulary now includes its installed application's
`Allow this conversation` and `Always allow`. Their different scopes remain
visible under their original names. Disabled controls remain unavailable;
Muster never synthesizes an approval or submits one automatically. Option-set
logs include counts of recognized enabled/disabled controls without card content.
A test message reached the specified idle Mac-C session, but `/usr/bin/true`
was automatically approved by the environment, so live approval-card acceptance
is still pending. These source changes are not yet installed-device validation.

## Inputs cleared before every submission, and a quiet cover (2026-10-27)

See [ADR 0055](../adr/0055-clear-every-input-before-a-submission.md). The daemon
logs of all three Macs were read for every failed send and answer. A draft left
in an editor was the largest cause: a write over it is merged by Electron's rich
editors instead of replacing it, so the composer held the old draft *and*
Muster's text (50 characters written, 70 read back; 1,721 written, 2,423 read
back), the exact echo never matched, and the request was refused as "the app did
not confirm the draft" — 191 times across the three devices. A draft also
refused an answer outright, added a second enabled Send to the surface
("expected one send button, found 2") and held the application's focus so a
question card's Submit could not take it.

Every submission now starts from a confirmed-empty editor. Shared Rust decides
each attempt and owns the confirmation: it tries the editor's own editing
command (select the whole value, replace the selection with nothing), then a
plain empty value write, alternating every 400 ms for at most 2 s, because no
profile can state which mechanism an app build honors. This clears the composer
before a draft write, a question's custom answer field before the answer, and
the conversation's composer before a card is answered. A field already holding
exactly the text to write is left alone, and a conversation with no visible
composer has nothing to clear; an unreadable value is never an empty one. An explicit send already replaced a differing native draft, so this makes
that decision work; for a question field it is a change — the draft is cleared
instead of preserved, so each submission starts from a known state. An editor
that cannot be emptied is reported as definitely not sent.

`CoverLost` never once meant an uncovered display. All 37 stops recorded on
Mac-A and Mac-B carry the same evidence: a healthy watchdog report 250-462 ms
old, with the owner's own poll delayed in the same tick, while the watchdog —
which checks the cover every loop and reports a loss explicitly — reported
none. A cover that carries no fresh report is now held for one second once the
lease has seen one, with every admission withheld meanwhile: no operation
starts, no queued operation is dispatched, and the queue is left untouched
rather than popped to a refusal. Work already in flight continues, because the
watchdog owns the cover and locks the screen itself if this process disappears.
A lost watchdog is still checked first, and a cover that never reported at all
is still lost at once. The watchdog's reported liveness is now its own, no
longer conflated with a fresh cover status.

A submit candidate the app destroyed around a write answers
`kAXErrorInvalidUIElement`; it is excluded before the controls are counted,
since an element that is gone can press nothing. Unknown-but-live duplicates
remain ambiguous. The question diagnostic falls back to the surface diagnostic
when no card is waiting, so the composer and send controls a session would
drive can be read without one.

Source changes with unit coverage only. Clearing either editor against the live
Claude and ChatGPT desktop apps, and a send during a watchdog reporting gap,
have not been accepted on an installed build.

## Session scans skip transcripts; control scans retry a vanished node (2026-10-27)

See [ADR 0056](../adr/0056-bounded-scans-skip-transcripts-and-retry-vanished-nodes.md).
The last live "incomplete scan" refusals in the device logs had two causes. The
session scan walked into every open conversation's transcript, so with five
Codex views open on Mac-B it reached the 4,096-node ceiling in 363 ms after it
had already found the target, and refused the send and several background
attaches. A web area is resolved from its own URL and title, so the session
scan no longer enters one that resolves to a conversation; it still enters the
app shell and any web area that resolves to nothing, still refuses ambiguity,
and the control scan still requires exactly one composer in the chosen view.

The control scan gave up on the first node read that failed. Electron rebuilds
the composer around a write, and a node destroyed mid-walk refused a send 482 ms
after a successful write on another Mac. A scan that stopped on a failed node read is
now walked again, up to three walks in total with 100 ms between them; the node
ceiling and the time budget are not repeated. Every refusal names its stop
("node limit", "time limit", "node read failed (code)"). Source changes with
unit coverage only; not yet accepted on an installed build.

## Several windows on one conversation (2026-10-27)

See [ADR 0058](../adr/0058-duplicate-views-use-the-main-window.md). The Codex app
opens a window per thread activation, and Mac-B's ChatGPT app held nine
windows, three on the same thread; every send to that thread was refused as
"more than one view shows this session". When several views show the target,
the one in the app's main window is used; an inactive app may report no main
window (as under Locked Use), and then the frontmost visible view is. Minimized
windows are never chosen, and undecidable cases still refuse. The whole
operation stays in the chosen view.
