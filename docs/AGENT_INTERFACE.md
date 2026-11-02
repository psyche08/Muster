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

## Idle compaction (2026-11-02)

[ADR 0083](adr/0083-idle-compaction.md). On a Mac where the local owner ran
`muster debug idle-compact on`, the daemon types `/compact` into a Claude Code
conversation once its newest message is 30 to 60 minutes old. Within that
window the provider's prompt cache is still warm. Only conversations that are
`idle`, `running`, writable, unarchived and without descendant activity
qualify. `off` disables it, and `status` reports it. The setting is local to
the Mac and absent means off.

- **When it runs.** One conversation per 60 s tick, never while the screen is
  locked, without Accessibility, within 2 minutes of local keyboard or mouse
  input, or while Locked Use holds the console. It takes the session's queue
  and the same native navigation and write as a send.
- **Drafts.** A different draft in the prompt is kept, and the attempt is
  refused as a conflict.
- **Repeats.** Each idle period gets one attempt, and new activity opens the
  next one. A conversation whose newest transcript record is a compaction is
  skipped, which also holds after a daemon restart.
- **Receipts.** A Claude command record (`<command-name>/compact</command-name>`
  with empty `<command-args>`) now confirms a typed `/compact`. Claude writes
  it after compacting, so the attempt normally ends as an unknown outcome. The
  unconfirmed-write latch clears when the record appears.
- **Codex.** Codex desktop runs Compact only from its slash suggestion menu,
  and a sent `/compact` would be an ordinary prompt. Codex conversations are
  therefore refused and nothing is sent.

Rust tests, targeted Clippy and builds passed. Device acceptance with a live
Claude Desktop conversation has not been run.

## Desktop Chat mode

The Claude and ChatGPT apps' ordinary conversations, read and driven through
Accessibility, are specified in [DESKTOP_CHAT_MODE.md](DESKTOP_CHAT_MODE.md).

## Field records

Dated verification, diagnosis and incident records are in
[AGENT-INTERFACE-RECORDS-2026-09-28-TO-10-27.md](validation/AGENT-INTERFACE-RECORDS-2026-09-28-TO-10-27.md):

- [Cache accounting verification (2026-10-01)](validation/AGENT-INTERFACE-RECORDS-2026-09-28-TO-10-27.md#cache-accounting-verification-2026-10-01)
- [Verification (2026-09-28)](validation/AGENT-INTERFACE-RECORDS-2026-09-28-TO-10-27.md#verification-2026-09-28)
- [Mac-C send prerequisite repair (2026-09-28)](validation/AGENT-INTERFACE-RECORDS-2026-09-28-TO-10-27.md#mac-c-send-prerequisite-repair-2026-09-28)
- [Native send draft recovery (2026-09-29)](validation/AGENT-INTERFACE-RECORDS-2026-09-28-TO-10-27.md#native-send-draft-recovery-2026-09-29)
- [Desktop catalog and send investigation (2026-09-30 evening)](validation/AGENT-INTERFACE-RECORDS-2026-09-28-TO-10-27.md#desktop-catalog-and-send-investigation-2026-09-30-evening)
- [Claude paginated question diagnosis (2026-10-02)](validation/AGENT-INTERFACE-RECORDS-2026-09-28-TO-10-27.md#claude-paginated-question-diagnosis-2026-10-02)
- [Unconfirmed send recovery and pre-unlock refusals (2026-10-02)](validation/AGENT-INTERFACE-RECORDS-2026-09-28-TO-10-27.md#unconfirmed-send-recovery-and-pre-unlock-refusals-2026-10-02)
- [AX recordings on Mac-C (2026-10-03)](validation/AGENT-INTERFACE-RECORDS-2026-09-28-TO-10-27.md#ax-recordings-on-mac-c-2026-10-03)
- [Locked Use acceptance on Mac-B, 1.0.34 (2026-10-03)](validation/AGENT-INTERFACE-RECORDS-2026-09-28-TO-10-27.md#locked-use-acceptance-on-mac-b-1034-2026-10-03)
- [Dropped Claude send and latch release (2026-10-04)](validation/AGENT-INTERFACE-RECORDS-2026-09-28-TO-10-27.md#dropped-claude-send-and-latch-release-2026-10-04)
- [Claude Submit activation and Codex approval labels (2026-10-22)](validation/AGENT-INTERFACE-RECORDS-2026-09-28-TO-10-27.md#claude-submit-activation-and-codex-approval-labels-2026-10-22)
- [Inputs cleared before every submission, and a quiet cover (2026-10-27)](validation/AGENT-INTERFACE-RECORDS-2026-09-28-TO-10-27.md#inputs-cleared-before-every-submission-and-a-quiet-cover-2026-10-27)
- [Session scans skip transcripts; control scans retry a vanished node (2026-10-27)](validation/AGENT-INTERFACE-RECORDS-2026-09-28-TO-10-27.md#session-scans-skip-transcripts-control-scans-retry-a-vanished-node-2026-10-27)
- [Several windows on one conversation (2026-10-27)](validation/AGENT-INTERFACE-RECORDS-2026-09-28-TO-10-27.md#several-windows-on-one-conversation-2026-10-27)
