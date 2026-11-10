# Native CLI sessions

Shared Rust owns the installed Codex and Claude CLI processes and their session
journal. The runtime speaks each provider's stdio protocol directly. The
journal and parts of the presentation originated in an Orca port. Its
[MIT notice](third-party/orca-MIT.txt) is retained.

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

- The CLI runtime runs in `musterd`. The agent service calls its Rust methods
  directly to obtain sessions and history and to submit typed actions.
- Codex uses its native `app-server --listen stdio://` protocol with explicit
  `workspace-write` sandbox and `on-request` approvals. Claude uses its native
  bidirectional stream-json protocol with default permissions and stdio prompts.
  Neither provider is launched with a permission-bypass flag.
- The daemon's private `orca/sessions.db` stores provider session IDs and journal
  rows. SQLite has one owner. Exclusive journal ownership and per-child
  generation checks reject stale callbacks after restart. No persisted
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

In the native macOS and iOS timeline,
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
