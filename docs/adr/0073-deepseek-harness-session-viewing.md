# ADR 0073: View DeepSeek Harness sessions; do not drive the app yet

- Date: 2026-10-31
- Status: Accepted; viewing implemented, device acceptance and control pending
- Related: ARCHITECTURE §3.6.2 and §3.6.4 (step 5, "the next app as a
  profile"), [APP_PROFILES.md](../APP_PROFILES.md)

## Context

DeepSeek Harness (bundle `com.deepseek.dsh`, Developer ID team `NAN929V4UM`,
investigated at 0.2.1-rc.5) is an Electron desktop coding agent. Muster listed
none of its sessions. This was the first app added after the profile schema,
so it tests ARCHITECTURE §3.6.4 step 5: an app added as a profile. A profile
does not replace investigating the app's data sources and controls.

The app keeps its state under `~/.dsh` (`DSH_HOME`). Its bundled
`dsh-session` and `dsh-session-persistence-jsonl` packages describe the
format, and the installed app's logs matched that description:

- One directory per session, `sessions/--<normalized cwd>--/<session id>/`,
  holding `session.v<N>.jsonl.zstd`. The highest generation is canonical, and
  `compression: none` writes `session.v<N>.jsonl` instead. Session ids are
  `session-<uuid>`.
- The log is append-only. Each flush adds one checksummed Zstandard frame;
  the installed logs had 3 to 18 frames. A torn final frame is truncated and
  rewritten when the app next writes; it never changes committed frames.
- The first record is a header (`type: session`, `version`, `id`, `cwd`,
  optional `parentSession` and `origin: subagent`). Each event that follows
  has `type`, `seq`, `time` in milliseconds since the epoch, and `data`.
  Person and context messages are `user/message`, told apart by
  `data.source.kind`. Assistant messages are `assistant/message` with
  `data.message` (`text`, `reasoning` and `tool-call` blocks) and
  `data.usage`. Results are `tool/result`, with `data.message.toolCallId`
  and `isError`. Turns are bounded by `turn/start` and `turn/end`, and
  `session/title` records titles.
- `storages/workspace.json` lists archived sessions.
  `storages/session_projcache/sessions/<id>.json` holds the title the app
  shows.

The app's window did not expose its web content to Accessibility.
`muster debug ax-dump` read only the window frame and menus on an unlocked
console after `AXManualAccessibility` was set and retried. Computer-use access
to the app was declined, so no control was recorded. The desktop host process
runs the app executable as Node. It holds every session's `session.lock`
whether or not that session is active, so neither the process nor the lock
shows which session is running.

## Decision

Add DeepSeek Harness as a fourth desktop app (`deepseek`, "DeepSeek
Harness") that is **viewed, not driven**:

- `muster_core::apps::DEEPSEEK` records its identity and signing team, and
  `profiles/deepseek.toml` its profile. The profile declares no send,
  respond, interrupt, question cards, queue, Chat mode or model options. It
  declares `files`: a session's recorded working directory can be browsed.
- The schema allows an app without `[controls]`, `gui` and `session_key`.
  `build.rs` then rejects any capability that needs controls. `App::controls`
  becomes `Option`, and `App::controls()` answers an undriven app with
  `Controls::UNRECORDED`, whose selectors match nothing. `App::driven()`
  marks the apps that `native_app::profiles()` scans, so the Accessibility
  engine never looks at DeepSeek Harness's windows. The musterd capability
  check already refuses undeclared operations by name before any queue,
  lock, unlock or ledger entry.
- A new source, `jsonl-zstd`, is read by the one JSONL engine.
  `transcript::Tail::zstd` decodes complete frames with the pure-Rust
  `ruzstd` 0.8 decoder, which has no further dependencies, as `muster-agent`
  also builds into the mobile cores. It stops at a frame that does not
  decode yet. Message ids are positions in the decoded stream, so
  `file_transcripts()` is false. A shorter file restarts the stream as a
  rewrite. A plain `.jsonl` log is read as it is.
- The record grammar gains three general forms. `envelope` may list paths,
  and the first present wins: an assistant's message is `data.message`, a
  person's is `data`. A match condition `{ not = … }` skips every
  `user/message` whose source is not the person. A numeric timestamp is read
  as epoch milliseconds. Existing profiles are unchanged.
- `deepseek.rs` discovers sessions. It reads only the format 4 generation and
  skips a newer one. It takes the header from at most the first MiB, the
  title from the projection cache and archive state from `workspace.json`. A
  header with `origin: subagent` is a hidden child of its `parentSession`; a
  fork is its own session. The hooks take titles from `session/title`, and
  only a `user` source makes a title custom. `usage::Tracker::deepseek` sums
  a turn's requests. Cache reads and writes count as input. No price is
  guessed: DeepSeek models are not in the price snapshot. File changes are
  marked incomplete, not empty.
- The service watches `~/.dsh/sessions` and `~/.dsh/storages`, not
  `profiles/`, which holds installed packages. A receipt for a DeepSeek
  session is refused. `agents:deepseek` joins the default member scopes like
  the other agent providers.

With no controller and no runtime observation, a session reports
`process_state` and `state` as `unknown`. Transcript turn events are never
promoted to live activity (§3.6.2).

## Alternatives

- **Drive the app with guessed selectors.** Labels can be read from its web
  bundle, as the Codex composer labels were. Without a recorded tree that
  finds them, a profile would claim controls nobody has seen. Send and
  Locked Use would then rest on them.
- **An inert `[controls]` table in the profile.** It would keep the type
  unchanged, but the data would say the app has controls. `Option` with a
  fail-safe accessor keeps the profile honest. It costs a mechanical change
  of `profile.controls.` to `profile.controls()` in muster-platform.
- **Drive sessions through the app's own interfaces.** These are its local
  web server, its `dsh` CLI (`--resume`, `headless`) and its ACP mode.
  §3.6.2 makes desktop session operations Accessibility-only and removed
  provider CLI and app-server control. This decision does not reopen that.
- **The C `zstd` binding.** It would need a C build of libzstd for iOS,
  Android and HarmonyOS through `muster-app-core`. The pure-Rust decoder is
  enough for small local frames.
- **A bespoke DeepSeek parser.** It would duplicate the engine's ids,
  bootstrap, tool naming and turn rules. The two grammar additions are
  general and small.

## Consequences

Phones and desktops list DeepSeek Harness sessions with titles, working
directory, archive state and subagent links. They read the transcript
(person prompts, replies, tool calls and results, turn usage), follow
appended frames live, and browse the project's files. Reasoning blocks and
the context the app injects for the model are not shown.

Nothing can be sent, approved or interrupted. State stays `unknown`, and
cost is unavailable. A future session format (v5) is skipped until a profile
update reads it. Each scan decodes each session's first frames, which is cheap
at current sizes.

To drive the app later, a recorded Accessibility tree of its window must
expose web content (Chromium may need a different enablement). That needs
selectors proven against redacted fixtures, a resolver strategy for its
windows, and the GUI process told apart from the host process that runs the
same executable. Device acceptance follows LOCKED_USE.md. That is a separate
decision.

## Verification

- Source and tests: `profile.rs` adds the undriven-profile invariants and the
  negated condition. `transcript.rs` covers frames read as they complete,
  skippable frames, a record split across frames, a torn final frame and a
  rewrite. `usage.rs` covers DeepSeek turn sums, unknown cost, untracked
  changes and an unclosed turn. `tests/deepseek.rs` covers the service end to
  end: discovery, archive, cached titles, subagent hiding, a skipped format
  5, hidden context, tool pairs, `unknown` state, files, and live push of a
  completed frame with its usage and the model's title. The musterd and
  app-core tests cover refused operations and access defaults.
- Real data: the service, built from this branch, read a copy of the
  installed app's `~/.dsh` on the development Mac (three format-4 logs, one
  archived). It listed the two unarchived sessions with the app's titles
  and working directories. It read one person prompt and one reply, with a
  turn summary of 8,931 input tokens including 1,152 cache reads and 2
  output tokens. That input count equals the app's own context-pressure
  figure. Those logs hold no tool calls, so the tool-call and tool-result
  shapes come from the bundle's declarations (`ToolCallBlock`,
  `SessionEventMap`), not from a recorded session.
- Not done: an installed daemon reading a live DeepSeek Harness session on a
  device, signing or publication, and any Accessibility recording or
  control.
