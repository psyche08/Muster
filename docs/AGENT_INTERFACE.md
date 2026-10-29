# Agent interface and turn usage

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

The home screen shows device tabs above the agent catalog. “All devices” is the
initial selection. Shared Rust fetches each desktop/server catalog concurrently,
limits each request to three seconds, attaches its device identity, and rejects
late results from an earlier selection. A failed device does not block the other
catalogs. Only desktop/server devices appear as service targets. Phones and
other mobile devices appear only in the complete network-settings roster; their
details omit Agent/Terminal actions. Mobile settings also omit service-offer
toggles. Shared Rust supplies this capability projection to the native UI. A row is
identified by `(device_id, session_id)`; actions route to that exact device.
Local members route through the local socket even if NetworkStatus arrives
before DeviceInfo. Failed batches retry while this screen is foreground, with
one-, two-, four-, then eight-second delays; leaving the screen or changing the
selection invalidates the pending retry. Errors include the source device name.
Device details remain in each tab's context menu. On a Mac, Settings,
invitations and pending join decisions remain in the home toolbar menu. Phones
show a Settings gear instead, with a dot while a device waits to join; Settings
lists invitations and pending join decisions with the network devices. Choose a
device before creating a new session.

Tool calls and results use closed DisclosureGroups. MarkdownUI 2.4.1 renders GFM
headings, lists, links, code blocks and tables. Image providers show explicit
links/placeholders and do not start native network requests outside Rust's DoH
path. Local time formatting and disclosure state are presentation concerns only.

## Conversation layout and previews

Shared Rust projects messages into stable timeline blocks: user messages,
collapsed work details, the final Markdown response, and turn summaries. Native
views own only disclosure state. Tool entries remain collapsed inside work
history and now retain up to 16 KiB of arguments/output instead of a 160-character
call excerpt. User messages use a small neutral bubble; assistant replies use
an open document layout.

Each completed turn may carry recorded file changes. Codex `item_completed`
FileChange, successful patch_apply_end/apply_patch results and turn_diff snapshots
are supported. Claude Edit/MultiEdit/Write results are accepted only after a
non-error tool result. Structured result hunks take precedence over proposed edit
text. Failed calls, duplicates, and changes from other turns are excluded.
This is recorded edit history, not a snapshot of the whole dirty working tree:
changes made by arbitrary shell commands without provider file-change events
cannot be reconstructed. Repeated edits retain successive hunks; their +/-
counts represent recorded edit operations, while a supplied turn_diff replaces
those hunks with the provider's aggregate snapshot.

The change card is collapsible; a file opens its recorded red/green diff without
requiring the file to still exist. “Current file” loads current contents from the
same session/device through the existing confined file API. Missing diffs,
binary files, truncation and read failures are explicit. Deleted/out-of-root
paths do not offer a current-file action. Markdown relative/file links resolve
inside the session directory in Rust; escapes and unsupported URL schemes are
rejected, and daemon reads still enforce symlink confinement. File loading
rejects late responses from another path or from before a diff preview opened.

Limits: 32 recorded files, 32 KiB per diff, 256 KiB total diff text per turn;
truncation is exposed to the renderer. Historical provider records are the
source of truth; no file contents or Git state are fabricated to fill gaps.

## Turn accounting

Rust emits a structured `turn_summary` message at provider-confirmed completion
(Codex task_complete/turn_aborted; Claude end_turn/stop_sequence/turn_duration).
Its timestamp is the provider end timestamp; the shell formats it in the device's
current time zone. Missing times, usage or pricing are displayed as unavailable.

- Codex: prefer `token_usage_record` request usage, scoped to turn ID and
  deduplicated by response ID. Its legacy `token_count` mirrors are not added
  again. For old logs, repeated cumulative events are ignored; prefer
  last_token_usage, otherwise take the delta of cumulative totals only with a
  valid, monotonic baseline. Missing baselines or counter resets without request
  usage are unavailable, never session totals attributed to one turn. Reasoning
  output is already part of output and is not added again. Input already includes cached reads/writes.
- Claude: sum request usage across the turn, deduplicated by message/request ID;
  later output usage replaces partial usage. Input includes ordinary input plus
  cache reads and cache creation. One-hour cache writes use their separate rate.
- Cache hit percentage = sum(cache read tokens) / sum(all input tokens) × 100,
  within one completed turn, weighted by tokens rather than averaging request
  percentages. All model calls/tool continuations count. Claude steering with an explicit
  matching turn index, compaction summaries and tool-result text do not clear
  an active turn. A new user turn preserves an unfinished prior turn's recorded
  counts in a separate partial footer before the new prompt; its completion
  time and full hit rate stay unavailable.
  Zero denominators and incomplete usage have an explicit unavailable state.
- Each footer includes request count and a cache expiry estimate. Expand cache
  details for read/write tokens, the first request's hit percentage, and the
  gap from the previous observed request (when the model matches). A high turn average can hide a cold first request; neither percentage
  nor an observed gap proves the lifetime of the same cached prefix.
- Cache timing is computed in Shared Rust from the last request's usage
  timestamp, not the turn end. Duplicate/streaming records do not extend it.
  These are historical estimates at the turn, not live cache status; later
  requests may refresh matching prefixes. Unknown timestamps/policies or
  incomplete usage do not produce fabricated deadlines.
- Claude: explicit `ephemeral_5m_input_tokens` / `ephemeral_1h_input_tokens`
  writes give a 5-minute / 1-hour estimate for newly written prefixes (a range
  for mixed TTLs). Read prefixes can differ. Without write TTL evidence, show
  the possible 5-minute–1-hour window, not an inferred exact TTL.
- Codex: for GPT-5.6 / GPT-6, the documented API minimum is 30 minutes after
  reuse/write: label this as an estimated earliest expiry, with no upper bound.
  Recorded legacy retention policies use their documented typical windows;
  unknown model/policy combinations remain unknown. Desktop subscription policy
  is not exposed and API policy is explicitly an estimate, not a guarantee.

- Recorded per-request costUSD takes precedence when present. Otherwise prices
  produce an API-equivalent USD estimate, not subscription charges. Long-context
  tiers are applied to each request, not the whole accumulated turn. Known fast
  multipliers are applied; unknown models/tiers do not silently become free.

Cache policy sources (checked 2026-10-01):
[OpenAI prompt caching](https://developers.openai.com/api/docs/guides/prompt-caching#cache-lifetime),
[Claude prompt caching](https://platform.claude.com/docs/en/build-with-claude/prompt-caching).

Price snapshot: 2026-09-28, from
[ccusage](https://github.com/ccusage/ccusage/tree/ce7d60cf9877f9fac8d397ba208311b153b300e2),
`rust/crates/ccusage-core/src/models-dev-pricing.json` (OpenAI/Claude subset) and
`fast-multiplier-overrides.json`. The data is bundled for offline rendering; its
age is visible in the cost tooltip. Update the dated snapshot as prices change.
[MIT attribution](third-party/ccusage-LICENSE.txt). Parsing and calculation are
implemented in Shared Rust; no ccusage process runs on the phone.

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

## Conversation styles (2026-09-28)

The Apple clients offer **Codex** and **Claude** in the conversation toolbar's
paintbrush menu and Settings → Appearance → Conversation style. The selection
defaults to Codex and is remembered locally across launches for all conversations.
It is a shell-only rendering preference; it does not select an agent/model, send
an intent, reload history, or change authorization. Composer identity remains
device/session based so switching styles preserves its draft and attachments.

Codex keeps neutral gray surfaces, system body text, rounded user bubbles and
controls inside the multiline composer. Claude uses a warm paper light palette
and near-black dark palette, a larger Georgia-based reply font with system glyph
fallback, more paragraph spacing, smaller bubble corners, and a bordered Reply
field with controls below. Its design references the installed Claude app's
ordinary chat UI observed on September 28; system fonts approximate its typography,
and no Claude font or logo asset is copied. Both styles follow system light/dark
appearance and retain Markdown, collapsed activity/compact context, file changes,
turn accounting, attachments and preview.

Verification: seven Swift regression tests passed; the native rendering fixture
produces both palettes for both styles at a 402-point conversation width. macOS
and iOS Release builds and strict signature verification passed. Installed on
Mac-A, Mac-C, mac-b and iPhone; the three Mac app executable hashes match and
iPhone launch succeeded. No new notarization was performed. Native Mac-A UI
verified Codex → Claude → Codex switching with an unsent draft preserved, then
Claude selection surviving app quit/relaunch. The test draft was cleared without
sending. iPhone visual interaction remains a separate acceptance step.

## Composer attachments (2026-09-28)

The Apple composer uses the conversation palette and body typography, a rounded
multiline input, an attachment menu and a bottom send/stop row. Files can be
selected or dropped; images have thumbnails and a larger preview before sending.
macOS exposes image/file paste and an explicit Paste image action. iOS uses the
system file/photo pickers and a native text view paste callback. These OS input
adapters hand bytes to Shared Rust; they do not decide authorization or delivery.

Shared Rust accepts at most eight attachments and 6 MiB total per send, validates
names and metadata, and checks the target's `agent.attachments` capability before
transmitting. The authenticated RPC carries attachment bytes. Locked Use proofs
and the operation ledger bind names, media types and contents to the operation.
Retries of the same operation do not create another upload or controller send.
The shell clears a submitted draft only after a matching Rust success receipt;
an error leaves its text and attachments available.

On the target Mac, the daemon writes a random batch beneath its state directory's
`attachments/`, with private directories and files (0700/0600). Controllers receive
the user's text plus quoted local file references. This is file-reference input,
not native attachment cards in the provider desktop app or structured model image
input. Files remain available for asynchronous agent reads; automatic retention
cleanup and previews of uploaded images in historical messages are not implemented.

Validation: Rust protocol, agent, app-core and daemon tests passed, including a
512 KiB RPC upload with exact byte comparison, duplicate/tampered operation checks,
private file permissions, traversal/size rejection and Locked Use digest binding.
Clippy and seven Swift regression tests passed. macOS and iOS Release builds and strict signatures passed; the
signed bundle was installed on Mac-A, Mac-C, mac-b and iPhone, with no new
notarization. All three desktop daemons advertise `agent.attachments` and report
the relay registered; iPhone installation and launch succeeded.

Native macOS interaction verified file selection, thumbnail display, enlarged
preview and removal using a generated two-color test PNG. Clipboard paste was
attempted but not confirmed during this run, so it is not counted as passed.
iPhone photo/paste interaction and a real provider reading an uploaded file remain
end-to-end acceptance items; the controller in the RPC test is a capture fixture.

## Visible style control and Chrome setup

Codex / Claude options are hidden by default in conversations and Settings.
The Style button reveals a single segmented control; pressing it again hides it.
Opening a different conversation collapses the control. The selection is still
a persistent local rendering preference and preserves the draft and provider state.

Settings → Chrome extension and the device's Chrome browser action open the
Rust-owned browser view. The Mac bundle includes the extension and native host;
setup registers the exact host path for the fixed extension origin, then guides
the user through Chrome's own Load unpacked permission flow. Only a successful
bridge ping renders Connected. Remote users see target-Mac installation guidance.

The browser page can review JSON requests and authenticate remote execution.
`muster --target Mac-B browser --approve-in-app '<JSON>'` fills that review page;
it never executes from a deep link alone. Browser authorization uses independent
`browser.view` / `browser.control` scopes; locked targets additionally require
`locked.control`, local policy and a request-bound presence proof. CDP leaves the
screen locked and does not acquire a temporary desktop unlock/cover lease.
See [extension guide](../src/browser/agenthalo-extension/README.md).

Validation: 44 Rust, JavaScript and Swift checks passed, along with targeted
Clippy and macOS/iOS Release signing. Native Mac interaction verified default
collapse, reveal, Codex/Claude selection and hide, plus the setup guide. The
signed update is installed on Mac-A, Mac-B, Mac-C and iPhone; the final iPhone
launch was denied while locked. No new notarization was performed. A live remote
browser request without browser.view was rejected. Locked-browser authorization
and replay tests use a Chrome bridge fixture; extension loading and real
Chrome plus hardware-authentication end-to-end acceptance remain pending.

## Accessibility-only agent operations

Codex and Claude send, approval and interrupt operations use their desktop
Accessibility controller exclusively. Provider CLI / app-server and terminal
agent controllers have been removed. The CLI-backed New session UI, command,
App Intent and RPC are retired; their protobuf field numbers remain reserved.
Open a session in its desktop application first. Missing Accessibility control
returns an error, while transcript history stays readable. Independent terminal
and browser services retain their own capabilities and authorization.

Native selection and discovery follow the daemon's injected platform, not
the build target: a platform whose `accessibility()` is `None` (a Linux
server, or a test daemon on `DefaultPlatform`) never selects, discovers or
opens desktop apps. There, only a controller attached in-process can act, and
a send without one fails with "no desktop Accessibility control" before the
ledger. Test daemons on a Mac therefore no longer drive the host's apps.

Validation for this removal: 55 Rust tests passed (one existing FSEvents test
ignored), seven Swift tests passed, targeted Clippy passed, and the Harmony
JavaScript binding passed syntax checking. Missing-AX rejection covers both
providers; retired wire field decoding is tested. Android/Harmony app builds,
new signing/installations and physical AX end-to-end checks were not run for
this source cleanup. Existing installed versions still need this update.

### Independent Accessibility step timeouts (2026-10-10)

Native operations use per-step readiness budgets (`muster-agent::native_step`),
not one timer for the complete question or navigation chain. A new budget starts
only when moving to the next step; polling a stalled step cannot renew it.

- Each newly received AX element gets a 250 ms messaging timeout, including
  children and attribute-derived elements. The macOS SDK specifies that a
  timeout on the application element does not apply to its children. Tree walks
  retain their separate node/time bounds.
- Question lookup has 3 s; after opening a collapsed question, its controls get
  a fresh 3 s. Each subsequent question and final Submit wait starts separately.
  The whole-form 90 s timeout is removed.
- Opening model/effort menus and entering a model submenu each get 2 s; closing
  a menu gets 1 s. Mode changes, dialog dismissal and session selection retain
  separate waits. A timed-out dialog dismissal is not pressed again.
- Chat and Code share the draft state machine: 4 s for exact echo, then 2 s for
  send readiness after the one focus action returns. Provider receipts start a
  separate 10 s wait after the single send press. No timeout resends an answer
  or message automatically; uncertain delivery remains an unknown outcome.
- History loading has a 10 s stalled-page budget, reset only by new entries,
  with a 120-page cap instead of a 60 s budget for the whole list. Partial lists
  still carry a warning.

Step clocks do not renew Locked Use. Its signed authorization expiry (currently
at most 15 s), cancellation, local input, session identity and cleanup rules
remain independent limits. Unit tests cover long step chains, stalled polling,
slow focus, late controls and revoked authorization. Live desktop/iPhone timing
acceptance and release installation are still pending for this change.

## Process-backed session lifecycle

`AgentSession.process_state` reports running, ended or unknown independently
from turn activity (`state`). The desktop controller binds a session to a
signature-checked process PID plus its kernel launch time. A different launch
generation or confirmed exit ends that binding; an OS read failure is unknown.
Transcript contents, file modification times and the presence of an unrelated
provider process cannot establish runtime liveness. Unbound history is unknown.

Switching tabs, locking the screen or losing AX access clears stale activity and
approval observations without declaring the process dead. Process polling keeps
running while locked. A confirmed exit remains ended after controller removal,
and late exit events cannot detach a controller from a newer launch. Apple
session rows and the Muster diagnostic CLI show process lifecycle separately
from working / idle / approval activity. Shared app processes establish the
liveness of the bound host, not that every historical session is generating.

Claude's verified worker registration owns activity after an assistant reply:
`end_turn` ends that reply but does not end busy Stop hooks, queued continuation,
or pending approval. A busy/waiting registration therefore survives a newer
reply-end record until the worker reports idle, exits, or its runtime evidence
expires. New transcript activity still overrides an older idle registration.

Transcript `Worked` groups remain historical. When the worker is active after
a completed segment or before its first new message, Shared Rust adds a separate
`Working` marker. A parent with only active descendants shows `Subagents working`
(or `Subagents need attention`). This marker never reopens a completed reply or
grants input authority; it disappears when activity clears. Timeline caching
includes own and descendant activity, so no message append is needed to refresh
it. Native shells render the empty-child status row without disclosure controls.
Apple transcript scroll views use the native bottom anchor for asynchronously
loaded history and reset their scroll identity when the selected session changes.
Opening a conversation therefore starts at its latest content, while the composer
keeps its separate identity and explicit history navigation remains available.

Validation: 50 Rust tests passed (one existing FSEvents test ignored), including
kernel PID / launch-time observation and a test-owned child exit, plus simulated
PID reuse, missing evidence, stale transcript and late-exit regressions. Seven
Swift tests and targeted Clippy passed. The macOS Release bundle was signed and installed on Mac-C while diagnosing
the send failure below. Provider quit/restart end-to-end checks remain pending;
other devices have not received this source update.

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

## Desktop Chat mode (2026-09-29)

Besides Code sessions (Claude Code in the Claude app, Codex threads in the
ChatGPT app), Muster reaches the apps' ordinary conversations: the Claude
app's "Claude" mode and the ChatGPT app's ChatGPT mode. These conversations
have no transcript on disk, so the target Mac reads and drives them through
Accessibility only. Every Muster app (iPhone, iPad, Mac) has a **Chat** tab,
left of Agents and Terminals, with Claude / ChatGPT at its top; the list
shows the historical conversations from a Mac the core picked (see
"The Chat tab" below).

- **Listing**: host probes (`AgentChats`, `agent.view`) remain read-only.
  Entering or returning to the Chat tab always refreshes the full history
  (`full_history`, `agent.control`) on the selected Mac. Under that provider's
  queue it switches to Chat mode, opens Claude's View all page, and drains
  explicit history paging controls. Stable conversation IDs are preferred;
  repeated titles without IDs remain ambiguous. Incomplete AX snapshots,
  unsupported paging, or a timeout report a warning, never a complete list.
  Opening an older conversation can revisit the full history page.
- **Selecting** a conversation (`AgentChatOpen`, `agent.control`) switches the
  app to Chat mode if needed, presses that conversation's sidebar entry and
  waits until the page shows it. The conversation must be on screen to be
  read, so opening changes what the Mac shows, like Code-session selection.
  Selection needs an unlocked console and a steady local input counter; a
  title-only entry must be unique in the sidebar.
- **New chat** opens an empty `<provider>-chat-new` session in Muster without
  touching the Mac. Its first send switches the app to Chat mode, presses
  New chat, drafts and sends; that send also works through Locked Use. The
  send answers with the conversation it became, and the chat view follows
  it. If the app has not named the conversation within ten seconds, the
  message is still sent and the view says it will appear in the list.
- **Sending and Stop** use the Code-mode write rules: the page must resolve
  to the requested conversation before drafting and again before the press,
  exactly one prompt field and one send button, the exact draft echoed, the
  local input counter unchanged, and no automatic retry after a press whose
  outcome is unknown. Chats have no approvals or question cards; Files and
  Git are hidden because a chat has no working directory. Chats take text
  only: Rust offers no attachments for them and the daemon rejects any.

### History display cache and incremental catch-up (2026-09-30)

Shared Rust persists display-only session lists and messages in `view-cache-v1`,
scoped by network, source host and session (Chat catalogs by network/provider).
Pending questions, permissions and send results are never restored from cache.
Entering a conversation renders its cache immediately, then establishes the watch
before synchronizing. The request carries the first/last IDs and a hash of the
cached prefix; the server returns the mutable last message plus new messages in
bounded pages. Missing cursors or rewritten prefixes invalidate the cached range.
The wire format remains compatible with older peers, which return their usual tail.

The Apple conversation UI has right-side first/last buttons. First loads every
older page through Rust before scrolling; Last returns to the latest content.
Agent catalogs request all available sessions rather than the default 200.
Archived agent sessions are excluded from both restored caches and live catalogs;
an ordinary ended session remains visible until it is explicitly archived.
Cache writes coalesce streaming updates, use private files, and bound memory to
64 MiB; cache or disk errors do not authorize operations or prevent live refresh.

### Composer persistence and UI update boundaries (2026-10-01)

Apple editors keep keystrokes in the native input buffer. Editing alone sends no
intent, performs no draft disk write, and publishes no new Rust ViewModel. The
shell attaches that buffer to the next user action with `ComposerAction`; Shared
Rust validates the conversation key and saves before navigation, session/device
switches, or an explicit application restart/update. Save failure keeps the
current page and its input visible. There is no periodic or per-keystroke save;
an abrupt exit before a boundary can lose edits made since the last switch.

Sending captures the submitted revision in memory without persisting an edit.
Only a matching Applied/Duplicate receipt clears it, including after navigation;
failed/unknown outcomes and newer revisions survive. A stale navigation snapshot
cannot restore the same revision after its successful receipt cleared it.

The Agent and Chat lists retain only their displayed sections and selection IDs.
Row closures capture the core and scalar presentation values rather than the
entire ViewModel/catalog. ChatView receives only ChatView data, so SwiftUI row
comparison does not repeatedly traverse unrelated session arrays.

### Default workspace and display work (2026-09-30)

Agents is the default root after network activation. The legacy Home intent maps
to Agents in Rust, so Mac startup and subsequent agent selection consistently
show a list on the left and content on the right. Network actions remain available
from the list toolbar.

Rendering coalesces reducer events within a bounded 16 ms frame. Unchanged
presentation metadata plus an unchanged chat-content revision produces no encoded
snapshot or shell callback. Chat timelines are reused until messages or running
state change; hidden conversations are omitted from other screens. RTT and uptime
alone do not invalidate agent/chat workspaces, while their latest values remain
in Rust and appear on details pages. Permission, lock, cover, membership and
connection changes still invalidate immediately. Unchanged Apple message blocks
also skip body/Markdown reevaluation.
Changed message caches are encoded once per frame, with an explicit flush before
switching conversations or terminating; repeated identical events do not rewrite
the cache.

OS network-interface enumeration is shared between candidate exchange and system
DNS fallback. A five-second TTL covers missed notifications, and `network_changed`
invalidates the cache immediately. Candidate filtering still applies the current
virtual prefix and reflected address on each call.

### The Chat tab, the picked Mac and the app lock (2026-09-30)

- **No device list.** Every Mac signed in to the same account lists the same
  conversations under the same ids, so the core asks every reachable
  desktop (`AgentChats`, after `DeviceInfo` shows `agent.chat`) and picks one
  (`muster-app-core/src/chat_hosts.rs`): the app must be running and not held
  by an agent operation; preferred are the app already in Chat mode, then no
  agent session of that app running (`agent_working`), then no chat
  operation in progress, then the nearest (this Mac first). The picked Mac is
  kept while it stays usable and is named in the list's footer; when it is
  not, another is picked and a toast says why. No usable Mac: the list says
  why for each one.
- **One lock per app per Mac.** Agent operations (send, answer, stop, and the
  session selection before them) and chat operations (open, send, model and
  effort) drive the same app, so each takes that app's lock for its whole
  duration and releases it when done, success or failure
  (`muster-agent/src/desktop_lock.rs`). The lock is a file in
  `~/Library/Application Support/Muster/desktop-locks/<app>.lock`, shared by
  every daemon of the user (a development daemon with `$MUSTER_DIR` uses the
  same directory), created atomically (write a private file, hard-link it
  into place) and removed only by its own holder (a random token). It
  expires after at most five minutes, so a daemon that dies holding it
  blocks nothing longer; an expired lock is moved aside, checked and only
  then removed, so a fresh lock placed meanwhile survives.
- **Who waits.** An agent operation waits up to 30 s for the app. A chat
  operation waits up to 20 s behind another chat operation but fails at
  once with `BUSY` when an agent operation holds the app: the client then
  picks another Mac and opens the conversation there (once). A chat send
  that meets `BUSY` moves the chat view to another Mac without resending;
  the draft stays in the composer and the user sends again. The catalog
  reports `busy` ("agent" / "chat") so a list never picks a held Mac.
- **Agents return to Code.** Before an agent session is selected, its app is
  brought back to its Code (Codex) mode: from Chat mode, from its settings
  (Claude's `/settings/…` pages are left through Go › Code; ChatGPT's
  settings through "Back to app") and from a dialog over the page (only its
  Close / Dismiss button is pressed; a dialog without one, such as a delete
  confirmation, is left alone and the selection fails). Chat operations
  leave settings and dialogs the same way on their way to Chat mode. An app
  that shows no mode these rules know is not touched.
- **On a Mac** the Chat and Agents tabs are split: the list on the left
  quarter, the conversation on the right, and a welcome page there until a
  conversation is chosen. The core names the tab a screen belongs to
  (`ViewModel.workspace`); the shell only lays it out.

Verified on Mac-C on 2026-09-30 with a development daemon (the installed
build predates the lock): with an agent lock planted on ChatGPT the chat
list reported `in use by agent`, and `chat-open codex` failed at once with
`BUSY` while ChatGPT stayed in Codex mode; with a chat lock planted, an
agent-side mode switch waited until the lock was removed (3.1 s), then ran
and released its own lock. With ChatGPT switched to Chat mode,
`debug ax-select-session` of the Codex thread it had shown returned the app
to Codex and selected that thread in 1.3 s. Not verified live: leaving
Claude's or ChatGPT's settings and closing a dialog (unit tests only; access
to ChatGPT for opening its settings was declined), and the Mac split view
and Chat tab on a screen (the macOS and iOS builds compile).

Session ids are `claude-chat-<uuid>` from claude.ai's `/chat/<uuid>` page
and `codex-chat-<id>` from a ChatGPT `/c/<id>` URL or the sidebar link marked
current; otherwise the id is derived from the title. The Claude app's
sidebar lists titles while its pages carry ids: opening a title-only row
answers with the page's own session, and the list shows that id from then
on. A ChatGPT title-only conversation renamed on screen (same opening
message) keeps its id while the daemon runs.

App rules observed on 2026-09-29 (Claude 2.9939, ChatGPT 26.924):

- Claude: Mode radio group ("Claude" / "Code", which can read "Code,
  working"); a "New" sidebar button; recents are title buttons each with a
  "More options for <title>" menu; the prompt is "Write your prompt to
  Claude". Turns are articles with a "You said:" heading.
- ChatGPT: "Switch mode, current mode: …" opens a menu whose entries read
  "ChatGPT Create, learn, and explore" / "Codex Build, debug, and ship";
  the sidebar has Pinned and Projects (project rows carry "Project actions
  for …" and are not chats; a project's chats are listed under it) and
  Recents; a conversation row carries "Chat actions" / "Pin chat". The new
  chat page offers Chat and Work: Muster selects Chat before a new chat's
  first message and refuses to send from a Work page. Turns are split at
  "You said:" / "ChatGPT said:"; the disclaimer and the "Latest response"
  landmark are not message text.

Messages are what the page renders, parsed in Shared Rust
(`muster-agent/src/desktop_chat.rs`): controls and status regions are
skipped, lists and headings become Markdown. Message ids are positional
(`ax<generation>-<index>`), so a streaming reply updates in place; a page
that no longer matches the known prefix starts a new generation and
watchers reload. A shorter snapshot (a partial walk) changes nothing. The
daemon polls the page every second only while an agent watch is open,
otherwise every three seconds for process liveness only. History is what
the app currently renders; it is kept in memory and not persisted.

### Model and reasoning effort

Coding sessions also expose **Model and effort** in the conversation menu on
Apple, Android and HarmonyOS. The core requires `agent.options.v1` from the
Mac before sending the existing options RPCs for a coding session; older
hosts show an update request. Read-only and cloud sessions have no entry.
The daemon resolves and selects the requested native session, retains its
controller, and holds the same provider queue used by other native operations.
Reads and writes require an unlocked, idle console. The native adapter scopes
all controls to that session's web area, rejects incomplete/ambiguous menus,
and reads back the selection before reporting success. Model names and
independent effort-menu choices come from the live enabled entries.

The combined Codex Model/Power panel is supported for models and the safe
middle-level Power operation described below. Full effort selection is
available only when the provider exposes a selectable effort menu; the Power
slider and hover-only Claude submenu retain their Accessibility limitations.
Unsupported controls are explained in the panel instead of reporting a change.
New coding-picker paths have fixture and RPC coverage; device acceptance is
pending an unlocked Mac (2026-10-10). Existing Chat-mode acceptance below does
not constitute acceptance of these new coding-session paths.

The chat view's model button reads the choices from the app on the Mac
(`AgentChatOptions`, `agent.control`, unlocked console): it shows the
conversation, opens the picker next to its prompt, reads it and closes it
without a new choice. Choosing a model (`AgentChatSetOptions`) presses that
entry and is confirmed by the picker's label (Claude) or a fresh read
(ChatGPT). Names come from the app's own list; a description is display only.

- Claude: models in the main list (Fable 5.1, Opus 5.5, Sonnet 5.5, Haiku
  4.5 on this build) can be chosen. The effort is read from "Effort <level>".
  Its levels and "More models" are submenus that open on hover only:
  AXPress, AXShowMenu and focus leave them closed, so they are not offered.
- ChatGPT: the models behind "Select model" (Latest, GPT-5.6 Sol, GPT-5.5)
  can be chosen. The effort is its "Power" slider, read from the panel's
  status ("Medium, 2 of 5."); it moves with the arrow keys and exposes no
  settable value. Its three nested parts all span the whole track (no
  separate thumb): AXPress on the track clicks its centre and lands on the
  middle level ("High, 3 of 5."). So while Power is below the middle level,
  Muster offers one effort choice, "Middle": it re-reads the level, presses
  the track only if still lower, and confirms the middle level. At or above
  the middle nothing is offered, and a "Middle" request changes nothing (a
  centre click would lower it); any other effort is refused before anything
  moves. The slider is
  Radix UI Slider with a WebGL canvas fill and a framer-motion thumb; its
  real thumb is `aria-hidden` with `tabIndex=-1`, so no AXSlider exists,
  and keyboard control lives on the "Power" menu item. The component can
  render per-option buttons (`ExplicitOption`) for options that require
  explicit selection, but in 26.924 the model picker marks every Power
  option `requiresExplicitSelection: false`, so none appear (checked in
  both the Chat and the Work / Codex picker, whose "Select model" lists
  models only, not model-and-effort bundles).

Any other effort change in either app would need synthetic keyboard or
pointer input, which the Accessibility-only rule excludes; such a request is
rejected before anything moves on the Mac, with the app's reason.

Middle-level check (2026-09-30, Mac-C): two conversations stated High (3 of
5) and Extra High (4 of 5), and the new-chat page High; none offered
"Middle". A "Middle" request was refused above the middle and a no-op at it,
another effort name was refused, and nothing was pressed. After the user
set the laptop conversation to Medium, `chat-options` read "Medium, 2 of 5"
and offered "Middle"; `chat-set --effort Middle` pressed the track and
confirmed "High, 3 of 5." in 1.6 s, the panel closed, and ChatGPT was
returned to Codex. The send path and the phone UI were not part of this
check.

### Diagnostics

On the Mac: `muster debug ax-chat Claude|ChatGPT` (read-only report of mode,
sidebar, conversation and composer), `muster debug ax-chat-options
Claude|ChatGPT` (opens and closes the model picker, listing entries and AX
actions), and `muster debug ax-chat-mode "<Claude|ChatGPT> <chat|code>"`
(switches the app's mode either way under the navigation guards). Saved
`muster debug ax-dump` output can be parsed with `MUSTER_AX_DUMP=<file>
MUSTER_AX_APP=claude|codex cargo test -p muster-agent dump_file -- --ignored
--nocapture`. `muster chats`, `chat-open`, `chat-options` and `chat-set`
exercise the RPCs.

Verification (2026-09-29): Rust unit and integration tests cover the parser
on fixtures modelled on the observed trees (including ChatGPT's mode menu,
project rows and Chat/Work page, and the Claude app's sidebar), session ids,
transcript growth and reset, title aliasing and rename following, the
service projection, the platform gate, and the app-core list → open → live
update → model choice → new chat → follow flow against a fake daemon.
Clippy passed with warnings denied on the whole workspace; the Swift package
builds and its nine tests pass.

A development musterd built from this change (own state directory, no relay,
Accessibility through the console session) drove the installed apps on
Mac-C: ChatGPT switched Codex → Chat (1.6 s) and back (0.8 s), listed 41
conversations without project rows, selected Chat on the new page, opened
and read two existing conversations, listed models and effort, chose
GPT-5.6 Sol and restored Latest; an effort request was refused. Claude
switched to Chat and back, listed recents, opened "Claude site access" (a
title row resolved to its `/chat/<uuid>` session), read both turns, chose
Sonnet 5.5 and restored Opus 5.5. No message was sent in either app, both
apps were returned to their Code / Codex mode, and every picker was closed.
Locked Use sends, the SwiftUI screens on a device, and a signed build or
installation remain unverified.

Sending (2026-09-30, Mac-C, at the user's request, one message per app
from a new chat: "Muster Chat 模式发送测试，请只回复：OK" (Muster Chat mode send test; reply only: OK)): ChatGPT drafted,
confirmed the echo, pressed its single Send button and followed the new
conversation (2.2 s); both apps answered "OK" and Muster read both turns.
Two rules came from this run. After a send the page is observed for 60 s
even without a watcher, so the reply and ChatGPT's automatic title are
recorded (the title changed right after the first message; without the
follow the renamed row no longer matched its session). The Claude app shows
its send button only while the prompt has focus: when the echoed draft has
no send button, Muster focuses the prompt through AX once and looks again;
the first Claude attempt, before this rule, stopped with "expected one send
button, found 0" and left the unsent draft, which the retry reused. Claude
was returned to Code and ChatGPT to Codex.

Locked Use attempt (2026-09-30): this build was installed on Mac-C with
`scripts/install-mac.sh` (Developer ID signature verified, daemon SHA-256
`6dc0259d…` after the second install, Locked Use plugin re-verified by the
root helper, Accessibility still granted) and `allow_locked_control` was
turned on there at the user's request; Mac-A received the same signed bundle.
A Claude chat send from Mac-A to the locked Mac-C reached Locked Use and was
refused before a challenge: "requester has no enrolled presence". Only
phones enrol a Presence Key (the embedded core registers the Secure Enclave
key that Face ID guards); a Mac's daemon has none, so a desktop cannot
request Locked Use. A ChatGPT new-chat send made after Mac-C was unlocked
stopped with "the send button is disabled": ChatGPT enables Send a moment
after the draft lands (it read enabled minutes later). The send now focuses
the prompt once and waits up to 2 s for one enabled send button; this change
was installed on Mac-C but not yet re-run. The second installation ran
while Mac-C was locked (a chained check did not stop it); it completed and
verified as above. A ChatGPT turn can still
carry a localized "thought for" line from the page.

## Desktop catalog and send investigation (2026-09-30 evening)

[CHAT_CATALOG.md](CHAT_CATALOG.md) records the typed local ChatGPT directory,
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
that editor is explicitly auxiliary. See [ADR 0062](adr/0062-scoped-composers-and-send-preparation-outcomes.md).

## CLI protocol access and read-only inspection

The installed CLI is `/Applications/Muster.app/Contents/Helpers/muster` on macOS.
`muster schema` prints the authoritative daemon RPC schema without requiring a
running daemon. Every request in that schema is available through
`muster [--target DEVICE] rpc --request FILE --timeout SECONDS`; use `-` for
stdin. JSON uses snake-case request names, for example:

```json
{"device_info": {}}
```

Omitted message fields use protobuf defaults; unknown fields and request names
are errors. Each received event is one `{"event": ...}` JSON line, followed by
one `{"response": ...}` result. Transport/remote errors and rejected or unknown
operation outcomes exit unsuccessfully. Streaming requests are cancelled on
Ctrl-C or timeout. Mutations need stable operation IDs from the schema; do not
retry a timed-out/unknown write automatically. The generic entry point preserves
all daemon scope, presence, Locked Use and local-only checks. It does not expose
an arbitrary shell or permit calling local-only methods through `--target`.

`muster --target Mac-B inspect SESSION` performs a signed, read-only Locked Use
inspection and prints the question forms and AX nodes as JSON. No answer input
or submission is part of this operation. See LOCKED_USE.md for authorization
and cleanup. `debug ax-question SESSION` remains a local, already-visible view
inspection without temporary unlocking.


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


## User-confirmed answer delivery (2026-10-02)

The user explicitly requested removing answer-click confirmation for any number
of questions. This supersedes the post-click selected-value and provider-receipt
waits described in earlier diagnosis entries. The adapter validates the complete
request, locates each exact question and executes its option/text/page actions.
It does not poll for selected-value echoes or wait for a provider answer record.
For immediate option buttons it does not press Next again; on the last Claude
page it makes one fresh lookup for a separate enabled Submit control.

Successful action dispatch returns an OpResult message with the number of
questions: “已执行 N 道题的答题操作，请在原会话确认结果。” Shared Rust displays
this as a non-error notice and exits the submitting state. Applied here means
the requested UI actions completed, not that the provider acknowledged the
answers. No InteractionResolved event is synthesized; actual provider events
remain independent. The same behavior covers one or many questions.

Live authorization, exact session/question matching, local-input guards,
operation idempotency and automatic relock remain required. Real action errors
or inability to locate a later question remain explicit; earlier clicks are
not automatically replayed. Readiness lookup refreshes the exact session surface
rather than assuming that a retained composer implies a current question tree.

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

See [ADR 0017](adr/0017-claude-question-submit-activation.md). On Mac-A with
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

See [ADR 0055](adr/0055-clear-every-input-before-a-submission.md). The daemon
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

See [ADR 0056](adr/0056-bounded-scans-skip-transcripts-and-retry-vanished-nodes.md).
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

See [ADR 0058](adr/0058-duplicate-views-use-the-main-window.md). The Codex app
opens a window per thread activation, and Mac-B's ChatGPT app held nine
windows, three on the same thread; every send to that thread was refused as
"more than one view shows this session". When several views show the target,
the one in the app's main window is used; an inactive app may report no main
window (as under Locked Use), and then the frontmost visible view is. Minimized
windows are never chosen, and undecidable cases still refuse. The whole
operation stays in the chosen view.
