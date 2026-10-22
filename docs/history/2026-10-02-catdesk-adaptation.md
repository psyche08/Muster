# CatDesk adaptation: field investigation and first source slice (2026-10-02)

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](../PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

Scope: architecture §3.6.2 / §8.2 "Agent Halo / Catdesk Desktop". Evidence was
collected read-only on Mac-B (macOS 27.0.1) over SSH while its console was
locked; nothing was written to CatDesk, its store or its settings.

## What CatDesk is

| Item | Observation |
|---|---|
| App | `/Applications/CatDesk.app`, Electron 42, version 2026.917.2015 |
| Identity | bundle `com.catpaw.cowork`, Developer ID "provider signing identity" team `BHWTW6L8X6`, hardened runtime |
| Processes | GUI main process (ppid 1) plus helper processes that run the same `MacOS/CatDesk` binary as Node (`ELECTRON_RUN_AS_NODE`) — an executable-suffix match alone returns three PIDs |
| Store | `~/Library/Application Support/catpaw-desk/catpaw-memory.db`, SQLite in WAL mode: `sessions` (JSON `extra` with generated columns), `ui_sdk_messages` (one row per message, `(conversation_id, seq)`, stable `message_id`, `finished` flag in the payload), `conversations` (legacy runtime keeps a JSON message array), `deleted_sessions` |
| Encrypted state | `catpaw-desk-unified.json` is encrypted; project paths live there, not in the SQLite store |
| Command socket | `~/.catdesk/catdesk.sock` (mode 0600). The app's own `catdesk` CLI speaks HTTP/1.1 over it: `GET /cli/health` returns the GUI pid; `POST /cli/session` with `{"action":"session","sessionAction":"list"|"current"}` returns sessions with live status (`running` while loading, streaming or awaiting approval), project path and the current session. Replies are chunked; 60 ms for health + current + list |
| Window routes | main window `file://…/app.asar/dist/index.html` (no session id in the URL; the renderer reports its current session to the main process), popouts `#/popout?sessionId=<id>`, other routes show no session; every page's title is `CatDesk` |
| Composer | Lexical contenteditable, `aria-label` "聊天输入框" / "Chat input" |
| Send / stop | one slot: "发送"/"Send" once the composer holds text ("请输入内容后再发送"/"Enter a message before sending" while disabled), "停止"/"Stop" while a turn runs, "发送中..." while sending. There is no queued send |
| Sidebar rows | plain `div` with a click handler and `data-session-id` (not exposed to AX); the title is a static text, optionally followed by the "等待批准"/"Awaiting Approval" badge |
| Locked console | AX exposes only the menu bar; no window or web area (same as the 2026-09-26 Claude record) |

## Decisions in this slice

- **Viewer.** `muster-agent::catdesk` opens the store read-only (never
  `immutable=1`), lists non-deleted sessions, and shows a row once it settled:
  finished, overtaken by a later user turn, or unchanged for ten minutes.
  A changed row at the last shown `seq` voids the history (reset). Message ids
  are the app's `message_id`. Change detection stamps the database and its WAL.
  Legacy conversations are parsed from their JSON array.
- **Runtime.** musterd asks the socket for the GUI pid, verifies it against
  the CatDesk signing requirement and its start time, then records the live
  status and project directory of each reported session. The socket is never
  an input channel; only Accessibility writes.
- **Resolver.** A popout resolves to the id in its URL; the main window to
  the app's current session. Two web areas resolving to one session refuse.
- **Selection.** Only when the session's title is unique in the store: the
  static text equal to the title, its nearest pressable ancestor, and that
  row's own labels must be exactly the title (plus at most the known badge);
  exactly one candidate in the main window. Traces log roles and counts only.
- **Draft.** Chromium writes a contenteditable's `AXValue` straight into the
  DOM, which Lexical reverts. CatDesk drafts by focusing the composer,
  selecting its whole text (`AXSelectedTextRange`) and setting
  `AXSelectedText`, an editing command the editor applies itself. The existing
  exact-echo, input-generation, single-press and receipt rules are unchanged.
  A send is confirmed only by a new user row with the exact text.
- **Authorization.** `agents:catdesk` joins the offered providers and the
  join defaults; Locked Use accepts the `catdesk` provider queue; the target
  still requires `com.catpaw.cowork` in its local AX allowlist
  (`muster policy --ax-allow com.catpaw.cowork`), which is not a default.

## Verification state

- Code: Rust unit and integration tests for the affected crates pass; Clippy
  clean. A consistent `.backup` copy of the real Mac-B store parsed all 34
  sessions (ui-sdk and legacy) in 0.24 s; the copy was deleted afterwards.
- Build: a Developer ID signed Release bundle and Velopack portable package
  (`1.0.31-catdesk.1`, daemon stamp `c0af6056`) passed `verify-release.sh` and
  strict signature checks. It was not notarized or published. That stamp is
  the pre-rebase tree; rebuild from the merged source before installing.
- Installation, runtime and device acceptance: **not performed**. Installing
  on Mac-B and allowlisting CatDesk there await the user's approval. Still
  required after installation: a covered read-only `muster --target Mac-B
  inspect <catdesk session>` to confirm sidebar selection and composer
  settability, then a phone-initiated send, receipt, display and relock.
