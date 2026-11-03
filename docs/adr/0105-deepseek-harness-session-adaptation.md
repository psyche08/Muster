# ADR 0105: DeepSeek Harness session adaptation on every surface

- Date: 2026-11-03
- Status: Accepted; source and rendering verified, device acceptance pending
- Related: [ADR 0073](0073-deepseek-harness-session-viewing.md),
  [ADR 0074](0074-deepseek-harness-plugin-control.md),
  [ADR 0086](0086-dsh-session-chrome-and-trace.md),
  [ADR 0102](0102-session-sections-hierarchy-search.md),
  [ADR 0103](0103-visible-tabs-and-provider-aggregation.md),
  [ADR 0104](0104-three-category-session-catalog.md)

## Context

The owner reported that DeepSeek Harness (DSH) sessions were not adapted, and
asked that Subagents and the other secondary tabs not be hidden. They selected
four areas: the iOS session page, the conversation content, the session list,
and DSH session behavior.

- **iOS session page.** ADR 0103 made the macOS session tabs direct. The phone
  still used its own three-destination bar (Chat, Activity, Terminal), with
  Trace, Subagents and Sessions nested under Activity.
- **Conversation content.** The DeepSeek conversation style changed colors only.
  Process rows, tool bodies and reasoning kept the generic layout, unlike the
  DSH Chat view (`dsh-client-ui-chat` 0.2.1). Tool bodies took their full height
  cap even when the output was one line.
- **Session list.** ADR 0102 and ADR 0104 group sessions by native sections and
  families, but DSH contributed neither. Its workspaces, pins and forks were
  ignored, and content search skipped its Zstandard logs.
- **Session behavior.** DSH 0.2.1 names a subagent's session and directory with
  a bare UUID; only a person's session uses `session-<uuid>`. The reader
  rejected bare ids, so subagents and their links were missing. In a local
  survey of 25 logs (shapes only, no content), 68 turns ended: 54 completed,
  11 with a provider error, 2 stopped by the person and 1 at the output limit.
  Every turn read as completed, `isError` tool results looked successful, and
  recorded reasoning was not shown. Every session the app had not loaded showed
  `unknown` state even while the plugin of ADR 0074 was live.

## Decision

### iOS session page

- The iOS conversation uses the shared session chrome (ADR 0086, ADR 0103) in a
  compact form: the same header facts with phone metrics, and the direct tabs
  Conversation, Trace, Subagents, Sessions and Terminal in a horizontally
  scrolling strip. Empty views remain visible, and Terminal stays visible but
  disabled when Shared Rust reports it unavailable.
- The Activity destination and its navigation bar are removed.
  `docs/CONVERSATION_WORKBENCH.md` is updated to match.

### Conversation content (DeepSeek style)

- The Worked group, activity groups, tool rows and reasoning rows use DSH's row
  anatomy. Each row is a quiet single line with a 16pt glyph box, a label, an
  optional status and a trailing chevron. Rows sit 6pt apart. An open row's
  body is indented under the label on the code-block surface.
- A completed row carries no mark; failed and interrupted rows stay explicit.
- A tool body sizes to its content and scrolls only past its cap: 130pt for
  input and 260pt for output.
- The person's bubble is capped at 525pt, which is DSH's 70.2% of its 748pt
  column.
- These are presentation-only changes. Titles, statuses, grouping and expansion
  defaults still come from Shared Rust, as they do for the other styles.

### Session list

- `sections::deepseek` reads `storages/workspace.json` (unit `workspace`,
  version 2) read-only.
  - Each workspace in `global.workspaceIds` order becomes a `deepseek` section
    `workspace:<id>`. Its title is the workspace title, else the folder name.
  - Members keep their slot in `sessionIds`, and the first workspace that
    claims a session keeps it.
  - `pinnedSessionIds` become pins, separate from membership. Archived sessions
    are not listed.
  - Only `session-<uuid>` ids are accepted. An unreadable or other-version file
    yields a notice and ungrouped sessions.
- A DSH fork (a person's session whose header names `parentSession`) joins its
  parent's family in the catalog, as Claude forks do. Subagents are never
  catalog rows; they stay under their parent's Subagents tab.
- Content search reads DSH logs through `transcript::FrameReader`, which
  decodes complete Zstandard frames in bounded chunks. It searches prompts and
  replies only: never reasoning, tool output or injected context.

### Session behavior

- **Subagent ids.** A bare `<uuid>` is a valid id when the header says
  `origin: "subagent"`. The header id must equal the directory name. The parent
  link comes from `parentSession`, and the relationship is `subagent`. A
  person's session with a parent is a `fork`. Subagents stay hidden from lists.
- **Reasoning.** The record grammar gains `reasoning_blocks`. It requires
  `block_ids = "all"`, so reply and tool-call ids do not move. The DSH profile
  declares its `reasoning` blocks. They become assistant text with
  `phase = "reasoning"`.
  - Never marked as the final answer.
  - Never streamed as reply text over ACP.
  - Never searched.
  - The timeline shows them as collapsed Reasoning rows inside Worked.

  This supersedes ADR 0073's "Reasoning blocks ... are not shown".
- **Tool outcome.** `Message` carries `tool_status`. In a grammar that declares
  a result's error field, a tool result is `failed` when the field is true and
  `completed` otherwise. Without that field the status stays empty, which reads
  as unknown. This also applies to Claude transcripts, whose grammar declares
  `is_error`. Trace spans and activity rows already render these statuses.
- **Turn outcome.** A new `Hooks::outcome` lets a provider add a
  `turn_status` message before the turn's usage summary. The DSH hook covers
  `turn/end` reasons other than `completed`:
  - `error` becomes `Turn: failed` with its code and message (status `failed`).
  - `aborted` becomes `Turn: interrupted` (status `interrupted`).
  - `max-tokens` becomes `Turn: interrupted`, noting the output limit.
  - An unknown reason becomes `Turn: ended (<kind>)` with no status.

  The text follows Orca's `turn_status` rendering, so ACP's existing
  `turn_outcome` parser settles these turns correctly.
  - The non-CLI timeline shows `turn_status` as a notice after the turn's work,
    without displacing the final reply.
  - The Trace summary span of such a turn reads `Turn failed` or
    `Turn interrupted`, not a hard-coded `completed`.
- **Live state.** The plugin's `state` reply lists the app's live agent
  registry. When that list is read in full, a session missing from it is one the
  app has not loaded, so no turn runs in it there. Muster then shows `idle`.
  - An entry that cannot be read keeps every absent session `unknown`.
  - An unread or exited launch keeps every session `unknown`.
  - Transcript events are still never promoted to live activity (ADR 0073).

### Invariants

- Nothing writes to `~/.dsh`.
- Hidden subagents still never inherit a controller's state.
- Session keys stay scoped to device, provider and id.
- No protocol fields are added: `tool_status` and `turn_status` already exist in
  `AgentMessage`.

## Alternatives

- **Keep Activity on iOS and add Subagents to it.** The owner asked for direct
  tabs. One chrome on both platforms also keeps tab availability in one place.
- **Infer liveness from the log, `session.lock` or the process.** ADR 0073
  found that the app holds every session's lock and that one process serves all
  sessions. A log's open turn is history, not liveness. The app's registry is
  the only live evidence, so a session is idle only when the full registry
  omits it.
- **Show agent relays as messages.** `agent-message` relays, `subagent-settled`
  and `tool-jobs` notices, and wrapped `schedule` prompts are machine-written
  envelopes. Showing them well needs a sender-aware relay row. They stay
  skipped, as ADR 0073 decided, and are follow-up work.
- **A typed turn-outcome field.** ADR 0009 already records this as a protocol
  follow-up. Reusing the `turn_status` text keeps one parser for Orca and DSH.

## Consequences

- Phone and Mac session pages show the same tabs. The DeepSeek style reads like
  DSH's own chat.
- DSH sessions are grouped by workspace, with pins and fork families. Their
  content is searchable, and subagents of every depth link to their parents.
- Failed tools and failed or stopped turns are visible in Chat and Trace,
  including in Claude transcripts. Recorded reasoning is visible but collapsed.
- With the plugin live, sessions the app has not loaded read `idle` rather than
  `unknown`.
- A session run by the `dsh` command line outside the app is not in the app's
  registry, so it reads `idle` until the app loads it.
- Still open:
  - Subagent live state: subagents do not get a plugin controller.
  - Relay messages.
  - Compaction records.
  - Sub-second timing.
  - Linking a spawn tool call to its subagent.

## Verification

### Source and test evidence

- `muster-agent`:
  - 372 tests pass (328 unit), with 5 ignored.
  - The DSH integration test covers:
    - bare-UUID subagents and nested subagents;
    - forks, workspace sections, pins and archived members;
    - the reasoning phase and id stability;
    - a completed result, an `isError` result marked `failed`, and an `error`
      turn followed by its usage summary;
    - frame pushes.
  - Unit tests cover:
    - `FrameReader` over a 3 MiB multi-frame stream;
    - DSH search excluding tool output and context;
    - workspace order, pins and archived members;
    - a full plugin registry against an unreadable entry.
- `muster-app-core`: 165 tests pass, with 1 ignored. `muster-acp` (24) and
  `musterd` (124, 3 ignored) also pass. App-core tests cover:
  - A DSH fork joins a family; a subagent and other providers do not.
  - Reasoning is a collapsed row inside Worked.
  - A failed turn's notice follows its reply.
  - A Trace turn keeps its failed outcome.
- Clippy passes for `muster-agent`, `muster-app-core` and `muster-acp` with all
  targets. Changed Rust files are rustfmt-clean.
- Session chrome rendering tests pass on macOS. A compact 390pt header with
  direct tabs, and DeepSeek process rows (reasoning, completed and failed tools,
  an expanded tool body, bubbles), were rendered in light and dark and
  inspected.
- MusterUI compiles for the iOS simulator.
- `MusterMobileTests` was run on an iOS 27 simulator (iPad mini, explicit test
  sizes) with the README's linker flags. 30 of 32 tests pass, including the
  phone conversation, tool-row and composer layout tests that cover the
  compact header.
  - `KeyboardAndLiveChatTests.testBuiltInKeyboardWaitsForWindowAndCanSwitchWhileFocused`
    failed: the switched-back keyboard had no window.
  - `HighCPUStackTests.testHighCPUAutomaticallyReachesExportedLog` failed: no
    high-CPU interval was sampled.

  Both exercise only the text editor and the stack monitor, which this change
  does not touch.

### Not yet verified

- No signed build was installed. Physical iPhone and Mac acceptance against a
  live DeepSeek Harness, including the plugin's registry, is still pending.
