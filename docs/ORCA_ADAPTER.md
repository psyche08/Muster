# Built-in CLI sessions (Orca Rust port)

Muster includes an independent CLI adapter in Shared Rust. It ports the subset of
[Orca](https://github.com/stablyai/orca) needed for native sessions: process
ownership, a revisioned journal, fenced operations, provider receipts, and prompt
compare-and-set. Muster does not install, launch, download, or distribute Node,
Electron, an Orca app, JavaScript runtime, or the Claude JavaScript SDK.

The reference reviewed was Orca commit
`8a1c63b381fc6f3fb9cf756b0aea2f7c7358f0ee`, especially
`agent-session-journal-types.ts`, `structured-agent-session-mutation.ts`,
`codex-app-server-session.ts`, and the structured Claude connection. The upstream
MIT notice is retained in [third-party/orca-MIT.txt](third-party/orca-MIT.txt).
This is a focused Rust port, not full compatibility with Orca's workspace,
orchestration, browser, SSH, mobile or Desktop app features.

## Use

In Muster's Agents view, choose **New CLI session**, a device, Codex or Claude,
and an existing absolute directory on that device. The session works in that
directory; creation does not create a Git branch/worktree or run setup scripts.
Open it and send a message using Muster's normal composer. The CLI is started
lazily on the first message. Native Codex/Claude CLIs must already be installed
and signed in on that device. On macOS, the native CLI bundled with the installed ChatGPT desktop app is
preferred over standalone Codex installations, which can lag behind the models
configured by the desktop app. If that native executable is unavailable, lookup
falls back to absolute PATH entries and the usual native installation directories.
Script/Node launchers and non-executable files are not selected. Muster owns session management, not the providers' account setup.

macOS and iOS share the creation UI. The same typed Intent/ViewModel/RPC supports
other platform shells; their native creation forms are not included in this change.

## Ownership and transport

- `muster-agent::orca::runtime` runs inside `musterd`; there is no local TCP/UDS
  Orca server, bootstrap token, or additional device transport.
- Codex uses its native `app-server --listen stdio://` protocol with explicit
  `workspace-write` sandbox and `on-request` approvals. Claude uses its native
  bidirectional stream-json protocol with default permissions and stdio prompts.
  Neither provider is launched with a permission-bypass flag.
- The daemon's private `orca/sessions.db` stores provider session IDs and journal
  rows. SQLite has one owner. A fresh random runtime fence and per-child
  generation reject stale operations and callbacks after restart. No persisted
  PID is used to find or signal a process.
- The UI receives Muster messages and question/approval cards, never provider
  JSON used to infer authority. Revisions update existing rows; Markdown,
  reasoning, tool calls/results, diffs and turn outcomes use typed Rust projections.
- Send is confirmed only by Codex's `turn/start` receipt or Claude's exact UUID
  echo. A pipe write alone is not delivery proof. Missing receipts stay unknown
  in Muster's existing durable operation ledger and are not automatically replayed.
- Approvals bind the shown revision and exact offered option. The journal consumes
  the callback atomically before writing a response. Unknown prompt shapes remain
  unavailable and can be interrupted; they are never silently approved.
- Remote clients need the explicit `agents:orca` scope (or the all-agent scope).
  Holding every Desktop provider scope does not grant CLI sessions. Existing
  network identity, policy and lock-state authorization remain in force. This
  adapter does not drive Accessibility and needs no desktop unlock sequence.

## Current limits and verification

Text prompts, structured questions, approvals, interruption, revisioned history,
and resumption are supported. Attachments, model selection and file browsing are
not offered by this adapter. Provider-specific session configuration stays in the
native CLI's configuration. History is bounded to 10000 items and individual
provider frames to 8 MiB. Source tests exercise both CLI protocols using a
subprocess fixture, including ambiguous delivery, persistence and expired prompts.

The ignored `installed_native_cli_handshakes_without_inference` test runs real
native CLI startup/initialization with isolated `CODEX_HOME` and
`CLAUDE_CONFIG_DIR`; it does not make model requests. A successful handshake,
source tests and an Apple UI build do not establish signed distribution,
installation, or phone-to-host inference end-to-end acceptance.

## Session presentation

The CLI timeline follows Orca's reading flow in Muster's native macOS/iOS UI:
assistant prose remains visible while reasoning and consecutive tool calls use
compact disclosures. Tool arguments and matched output remain available in full.
Provider-recorded running, completed, failed and interrupted states are projected
by Rust; output arrival alone does not prove success. Settled turns do not retain
orphaned running indicators. Row IDs stay stable as streamed content changes.

Supplied unified diffs become file cards with addition/deletion counts, actual
hunk line numbers, colored rows and Copy. Invalid or missing ranges stay plain
metadata, without invented line numbers. Previews are capped at 1200 lines per
file; Copy retains the complete supplied patch, which can itself be bounded by
the provider. File browsing is still unavailable. Approval/question action
validation is unchanged. Older shells can show the retained plain message body.

The presentation references upstream `NativeChatReasoningRow`, `NativeChatToolRun`
and `NativeChatDiffCard` at Orca revision `8a1c63b381fc6f3fb9cf756b0aea2f7c7358f0ee` (the source snapshot used for the
adapter). This is a native rendering port: no React, webview or Node runtime.

Provider errors retain their bounded user-facing message, including model-version
requirements, in the native timeline and failed-turn details. Raw transport/debug
metadata is not rendered as an error.
