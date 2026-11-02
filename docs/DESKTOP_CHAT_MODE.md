# Desktop Chat Mode

Introduced 2026-09-29; moved out of [AGENT_INTERFACE.md](AGENT_INTERFACE.md), which links here.

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

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

## History display cache and incremental catch-up (2026-09-30)

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

## Composer persistence and UI update boundaries (2026-10-01)

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

## Default workspace and display work (2026-09-30)

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

## The Chat tab, the picked Mac and the app lock (2026-09-30)

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

## Model and reasoning effort

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

## Diagnostics

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
