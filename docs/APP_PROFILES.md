# Desktop App Profiles

Status: the profile form decided in ARCHITECTURE.md §3.6.4 (pending decision
6, "yes"), in use since 2026-10-04 for Codex, Claude Code and CatDesk,
including the record grammar of the two JSONL sources (step 3).

A profile describes one desktop app whose sessions Muster views and
controls: who it is, how its windows and sessions are found, what its
controls look like, which of the engine's strategies apply, and what it
offers. Profiles are TOML files in `src/muster/crates/muster-agent/profiles/`,
compiled by that crate's `build.rs` into the statics of
`muster_agent::profile` (`CLAUDE`, `CLAUDE_CHAT`, `CLAUDE_APP`, …, and
`APPS`). The binary carries the compiled data and never reads a profile from
disk, so the signed binary stays the only source.

The engine is shared and a profile can only narrow it. What a profile can
never change: process signature and start-time binding, the user-input
generation check, empty-draft and exact-echo checks, `control_guard`, one
press per delivery attempt, no replay after an ambiguous write, receipts
confirming sends, the Locked Use authorization chain and relock, and the
target's local Accessibility allowlist.

## Files

| File | Content |
|---|---|
| `vocabulary.toml` | `[labels]`: named label lists shared by profiles, referred to as `"$name"` |
| `claude.toml`, `codex.toml`, `catdesk.toml` | One app each; the file name is its agent provider |
| `build.rs` | Lists the apps in scan order (`APPS`), validates every file, writes `$OUT_DIR/profiles.rs`: the `Controls`, `ChatControls`, `Records`, `Cloud` and `App` statics |

The build fails with the file and table named when a field is unknown, a
strategy is not one of the engine's, a `$label` has no list, or a table
needed by a strategy is missing.

## Schema

Labels are matched exactly, on the roles given, in any of the attributes
given. Wherever `labels` appears it is either a list of strings or
`"$name"` for a list in `vocabulary.toml`.

### `[app]`

| Field | Values | Meaning |
|---|---|---|
| `provider` | `[a-z0-9-]+` | The agent provider; must equal the file name and a `muster_core::apps` entry (`CLAUDE`, `CODEX`, `CATDESK`), which holds labels, bundle id and signing team |
| `doc` | text | The generated static's documentation |
| `executable` | path suffix | The main executable, e.g. `Claude.app/Contents/MacOS/Claude` (libproc lookup) |
| `gui` | `executable`, `command-socket` | Which processes show windows: every process running the executable, or only the one the app's owner-only command socket reports (CatDesk's helpers run the same binary) |
| `session_key` | `url`, `title`, `runtime` | How a web area names its session: the page URL (Claude's `/epitaxy/<desktop id>` → CLI session), the web area's title (a Codex thread, only while unique), or a popout URL with the main window showing the runtime's current session (CatDesk) |
| `source` | `jsonl`, `sqlite` | Where history is read: one append-only transcript per session, or the app's own SQLite store |
| `lock` | text | The cross-process desktop lock's name (`Claude`, `ChatGPT`, `CatDesk`); every daemon of the user keys the lock file by it, so it never changes between releases |

### `[capabilities]`

| Field | Meaning when `true` |
|---|---|
| `send` | Prompts can be sent to a session shown in the app |
| `respond` | Permission prompts can be answered |
| `interrupt` | A running turn can be interrupted |
| `files` | The session's working tree, files and git history can be browsed |
| `model_options` | Native agent and Chat modes offer model and effort choices |

Three more capabilities follow from the tables present and cannot be
declared separately: `questions` (a `[controls.questions]` table), `queue`
(`[controls.queue]`) and `chat` (`[chat]`). The daemon rejects an operation
the profile does not declare before any queue, lock, unlock or ledger entry
("CatDesk does not offer queued messages"), and `muster-app-core` projects
the declared set into `ChatView.can_interrupt`, `can_choose_model` and
`can_browse_files`.

### `[source]`: the record grammar of a JSONL source

Required when `app.source = "jsonl"`, forbidden otherwise. It tells the one
JSONL engine (`muster-agent/src/jsonl.rs`) which records are messages,
tool calls, tool results and turn boundaries, and where their fields are.
A record condition (a *match*) is an inline table whose keys are dotted
field paths and whose values are a string (the field is that string), a
list of strings (one of them) or `true` (the field is `true`); every key
must hold.

| Field | Values | Meaning |
|---|---|---|
| `home` | `{ env, dir }` | The state directory: the environment variable when set, else `dir` under `$HOME` |
| `timestamp` | path | The record field holding an RFC 3339 time |
| `cwd` | list of paths | Fields naming the working directory; the first present wins |
| `skip` | list of matches | Records to ignore entirely (Claude's sidechains and meta records) |
| `bootstrap_until` | match, optional | Everything before the first such record is the app's own bootstrap and is never shown (Codex's `turn_context`) |
| `summary_before` | match, optional | A record whose turn summary belongs before its own messages (a Claude prompt that closes an interrupted turn) |

`[source.message]`:

| Field | Values | Meaning |
|---|---|---|
| `when` | match | The record is a message |
| `envelope` | path | The object holding role and content (`payload`, `message`) |
| `role` | path in the envelope | The role; `role_fallback` (optional, a record path) names it when the envelope has none |
| `other_roles` | `skip`, `assistant` | What a role that is neither `user` nor `assistant` is |
| `content` | path in the envelope | A string, or a list of blocks |
| `text_blocks` | block types | Blocks whose `text` a person reads |
| `media_blocks` | block types | Blocks that are content without text (images); count for recency only |
| `block_ids` | `text`, `all` | How blocks are numbered in message ids (`<offset>:<index>`): among text blocks (tool calls and results being records of their own) or among all blocks. Ids must not change between releases |
| `phase` | path in the envelope, optional | The turn phase of the message (`final_answer`) |

`[source.tool_call]` and `[source.tool_result]`: either `record = { … }`
(a record of its own, fields read from the message envelope) or
`block = "type"` (a content block inside a message). A call has `name`,
`id` and `arguments` (the first present of several fields); a result has
`id` (the call it answers), `output`, optionally `error` (a field that is
`true` when the tool failed) and `interaction_tools` (tools whose results
are the agent asking the person something).

`[source.turn]`:

| Field | Meaning |
|---|---|
| `started` | A record that starts a turn (the app's own event), optional |
| `ended` | A record that ends a turn |
| `ended_also` | Additional explicit end-record matchers, such as Claude `toolEndsTurn` results |
| `user_text_starts` | A user message with text starts a turn (a source without events) |
| `mark_final` | The text of the record that ends a turn is the final answer |
| `activity_on_messages` | Any message record counts as activity (recency) |
| `final_phase` | A message in this phase ends the turn (recency) |

What the grammar cannot say is a named hook per source (`jsonl::Hooks`,
implemented in `codex.rs` and `claude.rs`): how a text block is cleaned
(reminders, memory citations, pasted content, command wrappers), which
records rename a session, the turn usage accounting, and a prompt the app
queued for a running turn. The engine keeps every rule the readers kept
before it: message ids from record offsets, the bootstrap hidden, tool
results named by their calls, titles from the first prompt, state from the
turn rule.

### `[cloud]`: the app's cloud sessions (optional)

The same account's sessions in the provider's cloud, listed, followed and
written through the provider's own API with the credentials its CLI keeps
on this machine (`muster-agent/src/cloud.rs`). A cloud session is a session
of its own kind, `mode = "cloud"` with `controller = "cloud"`: no process,
no window and no working tree here, so no desktop lock, Accessibility or
Locked Use takes part. Needs a `[source]` table: the credentials live under
its `home`.

| Field | Values | Meaning |
|---|---|---|
| `api` | `claude-code`, `codex-tasks` | The engine's strategy: Claude Code on the web (`GET /v1/code/sessions`; `GET …/{id}/events`, read by the one JSONL engine over the `[source]` grammar, continuing from the last sequence number; `POST …/{id}/events` for a prompt) or Codex cloud tasks (`GET /wham/tasks/list`; `GET /wham/tasks/{id}`: the prompt, the latest turn's replies and its diff as a `diff` tool pair, re-read whole) |
| `base_url` | https URL, no trailing slash | The API origin |
| `label` | text | The provider as the shells name a cloud session ("Claude Cloud") |
| `poll_secs` | 5 or more | How often the daemon lists the sessions and reads the ones someone follows while anyone watches; a quarter as often otherwise |

`[cloud.credentials]`:

| Field | Meaning |
|---|---|
| `file` | The CLI's credentials file under the source's home (`.credentials.json`, `auth.json`) |
| `token` | Dotted path of the bearer token in its JSON |
| `expires_at` | Path of the token's expiry, milliseconds since the epoch; optional, without it the token's own `exp` claim counts when it is a JWT |
| `account` | Path of the account id sent as `ChatGPT-Account-Id`; required by `codex-tasks` |
| `keychain` | macOS: the generic password holding the same JSON when the CLI keeps it in the Keychain instead of the file, read through the system's consent prompt, never by unlocking a keychain |

Tokens are read for each request and never refreshed or stored: an expired
or refused one lists nothing, and the daemon's log says to sign in with the
CLI again.

`[cloud.capabilities]`: `send`, whether a prompt can be posted into a
session. Approvals, interrupts, question cards, queues and files are never
offered by a cloud session: the daemon refuses them by name and
`muster-app-core` projects `can_interrupt` and `can_browse_files` as false.

### `[controls]`: the agent conversation

| Field | Values | Meaning |
|---|---|---|
| `draft` | `value`, `selection` | How the prompt is put into the composer: set its value (ProseMirror re-reads its DOM) or focus, select all and replace the selection (Lexical reverts edits it did not make). The engine then waits for the exact echo before any press |
| `placeholders` | list | What an empty composer reports as its value (Codex: "Do anything"); never a draft |
| `composer` | selector | The text area a prompt is drafted in |
| `send` | selector | The control that submits it; looked up again when sending, since it may appear only with text and give way to Stop |
| `stop` | selector | The control that stops the running turn; an enabled one means a turn is running, and an interrupt presses it |
| `approve` | selector | The options of a pending permission prompt, each offered by its name |
| `approval_scope` | `web-area` (default) or `main-conversation` | Require complete bound ancestry; the latter requires a main landmark and excludes complementary/navigation/tab panels |
| `sessions` | table | How sessions are listed (below) |
| `queue` | table, optional | The app's queue of prompts sent while a turn runs: `row` (a control only a queued row carries), `send_now`, `delete` |
| `questions` | table, optional | Question cards: `submit`, `next`, `back`, `other` labels; `paged` (one page per question), `refresh_renderer` (ask Chromium for a fresh read), `button_choices` (a single-select card of plain buttons answers on press), `immediate_suggestions` (optional suggestion cards answer on press) |

A selector is `roles` (Accessibility roles, e.g. `["AXButton"]`), `attrs`
(any of `title`, `description`, `placeholder`, `name`; `name` is the title,
else the description, which finds a Chromium button labelled only by
`aria-label`) and `labels`.

`[controls.sessions]`:

| `kind` | Fields | Meaning |
|---|---|---|
| `sidebar` | `landmarks` (navigation and complementary landmarks list sessions too), `standalone_title` (one exact title with recognized status/delegation metadata; incomplete or ambiguous labels refuse selection), `more` (`prefix`, `suffix`: the control that lists older sessions, named `<prefix><count><suffix>`) | Rows in the app shell's sidebar |
| `page-rows` | none | Pressable rows of the main window's page, found from the text reading the title |

### `[chat]`: the app's Chat mode (optional)

| Field | Values | Meaning |
|---|---|---|
| `provider` | text | The chat provider (`chats:<provider>`), as `muster_core::apps` names it |
| `doc` | text | The generated static's documentation |
| `area` | `"main"` or `{ url = "…" }` | Which web area shows a conversation: the one with a main landmark (else the only one), or the one whose URL starts with the prefix |
| `send`, `stop`, `new_chat`, `close_dialog` | selectors | The conversation's send and stop controls, what opens the empty new-conversation page, what dismisses a dialog (looked for only inside a dialog) |
| `leave_settings` | selector, optional | What leaves the settings page when it covers the conversation |
| `settings_navigation`, `settings_home` | selectors, optional pair | Settings navigation landmark and fallback Home in the same web area's app shell; only a complete tree and unique exit authorize navigation |
| `sidebar_new` | selector, optional | A plain new-chat control that counts only in the sidebar |
| `url` | table | `segment` (the path segment before a conversation's id), `uuid` (ids are UUIDs; otherwise eight or more of `[A-Za-z0-9_-]`), `new` (the empty page's path, optional), `only` (only the URL names a conversation; otherwise the sidebar link marked current or the page title can) |
| `modes` | table | `chat` and `other`: the names of Chat mode and of the agent mode beside it; `switch` below |
| `history` | table | How the history lists conversations, below |

`[chat.modes.switch]`:

| `kind` | Fields | Meaning |
|---|---|---|
| `radio` | `group`, `menu`, `other_path`, `settings` | A radio group with one radio per mode (a status may follow the name after a comma) and the same commands in a menu-bar menu; the page URL says the mode: a path containing `other_path` is the other mode, one under `settings` neither |
| `menu` | `prefix` | A pop-up button titled `<prefix><current mode>` opens a menu whose entries start with a mode's name |

`[chat.history]`:

| `kind` | Fields | Meaning |
|---|---|---|
| `title-buttons` | `options` (the prefix of each row's own menu, "More options for "), `pages` (paths of the full history page) | Title buttons in the sidebar, each beside its own options menu; navigation buttons have none |
| `lists` | `actions` (a row's own commands, never its title), `markers` (a conversation row carries one), `project` (a row with this action is a project) | Buttons in the lists of the navigation landmark |

## How Profiles Are Proven

- `build.rs` checks structure at compile time.
- `profile.rs` unit tests evaluate every selector against Accessibility
  trees recorded from the real apps (`tests/fixtures/ax`, redacted by
  `scripts/redact-ax-dump.py`; the fixture README lists each recording),
  check that the profiles cover the app registry, that each capability
  matches the tables present, and that lock names are the ones daemons
  already use.
- A change to how a control is found is re-accepted on devices per
  LOCKED_USE.md; a profile does not replace investigating a new app's data
  sources and controls.

### Codex follow-up composer labels (2026-10-09)

The installed ChatGPT/Codex 26.1002.52244 (13536) bundle names an existing
thread's composer `Work with Codex`, while its home composer remains
`Do anything`. The source is the `composer.placeholder.workWithCodex`
message in `webview/assets/app-primary-15d1279f1ff0.js` inside `app.asar`;
its description explicitly calls this the placeholder and accessible label.
The bundle's zh-CN, zh-HK and zh-TW catalogs supply the Chinese labels in
`codex.toml`. Both the composer selector and empty-value placeholders must
include these names. Matching remains exact and limited to `AXTextArea`
inside the controller's verified session; no arbitrary text-field fallback.

The regression tests substitute these bundle-derived names into the older
recorded idle, running and drafted layouts. They verify one composer, empty
placeholder handling, and the scoped Queue control, including exclusion of
the adjacent Markdown editor. These variants are not new live recordings
and do not establish installed-daemon or real-send acceptance.

### Codex composer suggestions (2026-10-17)

After a turn completes, the Codex desktop app (ChatGPT 26.1002, CLI
0.163.0-alpha.10.2) can show a predicted next user message as the gray
composer placeholder. It keeps that text in memory only. The rollout, the
SQLite stores, `.codex-global-state.json` and the app's support directories
do not contain it, in either UTF-8 or UTF-16. The Accessibility tree does not
expose it either: the composer reports title and description `Work with Codex`,
a value of `\n`, and no `AXPlaceholderValue`. The text is drawn by a
ProseMirror placeholder decoration. The app produces it through its own
app-server connection, using a prompt fetched from `/wham/predictions/config`.
App-server 0.163 rejects `thread/prediction/request` as not implemented, and
`experimentalPredictionMode` forks need the parent thread loaded. A second
app-server cannot load the parent while the app holds its writer.

`muster_agent::prediction` therefore asks for its own suggestion. It does so
only for a watched Codex session: when the session is first watched, and when
its transcript shows a completed turn (`task_complete`). One request covers
each completed turn:

1. Start the desktop app's bundled `codex app-server` with
   `-c mcp_servers={}`.
2. Send `initialize` with `experimentalApi`.
3. Call `thread/fork` with the rollout `path`, `ephemeral: true`,
   `permissions: ":read-only"` and `approvalPolicy: "never"`.
4. Run one `turn/start` that asks for `{"suggestion": …}`.
5. Close the server's input. The server exits; one that lingers is killed with
   its process group.

The fork is never persisted, and the source rollout is not written. The
suggestion is kept while that turn remains the newest. A new turn drops it.
`AgentSession.composer_suggestion` carries it, and `ChatView.composer_placeholder`
shows it when present. Otherwise the placeholder is the provider app's own
text: `Do anything` and `Work with Codex` for Codex, `Ask ChatGPT` for its Chat
mode, and Claude.app's new-chat text for Claude. Each suggestion is one
low-effort model call, and its wording may differ from the app's.

### Subagent viewing

The native Mac, iOS and HarmonyOS chat screens expose a Subagents section
and a Parent session link. The daemon returns up to 200 recent direct children
with each selected session; children stay out of the normal session catalog.
Selecting a child uses the existing transcript history, paging and watch stream.
Selecting a nested child loads that child's own direct descendants, so cyclic
provider metadata cannot recursively expand a response.

Codex relationships come from explicit parent metadata (including
`source.subagent.thread_spawn.parent_thread_id`). A fork origin is used only
when the provider also declares the rollout a subagent; ordinary forks remain
independent sessions. Claude children are discovered in
`projects/<project>/<parent UUID>/subagents/agent-<agent ID>.jsonl`, with IDs
scoped to the parent. Only these separate child files read `isSidechain`
messages; parent transcripts retain their sidechain filtering.

Shared Rust projects the relationship and read-only flag, scopes navigation to
the same device, hides composer/approval/interrupt controls, and rejects
mutations before native input or Locked Use. Recorded subagents do not imply a
live process; unavailable activity remains unknown. Both the serving Mac and
the viewing client need this protocol update. This does not start or control
subagents independently.

The session-list badge also includes verified descendant activity through
`AgentSession.descendant_activity`: an active child or deeper descendant keeps
the row working even when the parent is idle; approval takes precedence. The
aggregate is independent of the 200-child detail limit and rejects cycles and
cross-provider links. Raw session state and input authority remain unchanged.
Codex children require their own rollout bound to a live worker. Claude inline
children use their registered parent worker's launch identity and their own
sidechain transcript, including completion records; parent idle status does not
end a background child. Missing or expired runtime evidence cannot keep the row
working. Serving and viewing apps both need the updated protocol/core.

## What Is Still Code

Per app, outside the profiles, until a later step of architecture §3.6.4:

- The two cloud strategies `[cloud].api` names (`cloud.rs`): how each
  API's lists, events and task details are read and a prompt is posted, and
  the credentials file's reading with its macOS Keychain fallback.

- Session discovery and head metadata: where a source lists its sessions
  and what their first records say (Codex's `session_index.jsonl`, Claude's
  `sessions-index.json`, title records and the desktop registry;
  `codex.rs`, `claude.rs`), the CatDesk store reader (`catdesk.rs`), and
  the hooks named above.
- Question cards: each app's own protocol for asking and answering
  (`questions.rs`).
- The two resolver implementations the `session_key` strategies name
  (`muster-platform::macos::native_app::{claude_resolver, catdesk_resolver}`)
  and CatDesk's routes (`catdesk_ui.rs`).
- Chat mode's model and effort pickers (`native_chat.rs`).

## Native Claude completion and older proposal cards

The composer offers **Read suggestion** for Claude Code desktop sessions.
`AgentComplete` is a separate operation bound to the session and operation id,
using the existing control scope, provider queue and Locked Use authorization.
It focuses an empty native composer, sends one application-targeted Tab, reads
the completed value, and confirms restoration of the empty draft before
returning the suggestion. It never sends Return or presses Send. An existing
or unreadable native draft, changed session, local input or lost authorization
stops the probe. The client exposes the result for explicit use and discards it
after the session's turn changes; it does not generate a substitute suggestion.

Claude may omit an older pending task proposal from the current accessibility
tree. Before giving up on such a card, the adapter can use the verified
session's unique **Load earlier messages** control, bounded to three pages and
the original operation's authorization deadline. Each refreshed form must still
match the exact pending proposal and its controls. Question inspection can
perform this same history navigation, but never selects or submits an answer.
