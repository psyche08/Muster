# ADR 0083: Compact idle Claude Code conversations once, opt-in

- Date: 2026-11-02
- Status: Accepted; source and tests verified, device acceptance pending
- Related: [ADR 0007](0007-desktop-manual-locked-use.md),
  [ADR 0045](0045-native-send-fast-path.md),
  [ADR 0063](0063-queue-receipts-use-provider-text-matching.md),
  [ADR 0068](0068-send-retry-and-ignore.md);
  [AGENT_INTERFACE.md](../AGENT_INTERFACE.md#idle-compaction-2026-11-02)

## Context

A desktop Code conversation that has been quiet for a while still carries its
whole history. When the person comes back, the next turn sends that history
again. Inside the provider's one-hour prompt cache that read is cheap. After
the cache expires the whole conversation is billed as fresh input. Compacting
while the cache is still warm replaces the history with a short summary at
cached-read cost. The person asked Muster to type `/compact` into a Codex or
Claude conversation that is not running and has had no input for 30 minutes.

The two desktop apps differ:

- **Claude Desktop** passes a typed `/compact` from its prompt to Claude Code.
  The transcript then records a `compact_boundary` system record, an
  `isCompactSummary` user record, a `<local-command-caveat>` meta record, the
  command itself (`<command-name>/compact</command-name>` with empty
  `<command-args>`), and `<local-command-stdout>`. Claude writes all of these
  only after compaction finishes (about 49 s in one local transcript), so the
  10 s send receipt budget expires before the command record appears.
- **Codex desktop** offers Compact only as an entry in the composer's slash
  suggestion menu, which requires an empty composer. Pressing Send with the
  text `/compact` submits it to the model as an ordinary prompt. Codex records
  a real compaction as a top-level `type: "compacted"` record.

## Decision

Add an opt-in idle-compaction loop for Claude Code conversations on the Mac
that runs the desktop app.

- **Decision lives in shared Rust.** `muster-agent::idle_compact` decides which
  sessions are due. A session is due when all of these hold:
  - its provider runs a typed command (Claude only);
  - mode is `code`, and it is neither archived nor read-only;
  - `state` is `idle`, `process_state` is `running`, and there is no
    descendant activity;
  - its newest message is between 30 and 60 minutes old.

  The 60-minute ceiling matches the cache lifetime. It also leaves old
  conversations alone after a daemon restart.
- **One attempt per idle period.** An in-memory tracker keys each attempt by
  the session's newest message time. A failed or unconfirmed attempt is never
  repeated in the same period, and new activity opens a new period. Across
  restarts, `idle_compact::compacted` reads the transcript tail. A session
  whose newest conversation record is a compaction is skipped. Command
  wrappers and metadata records do not count as conversation records.
- **This Mac's facts stay in the daemon.** `musterd::idle_compact` runs on
  macOS at most one conversation per 60 s tick. It skips the tick when:
  - the feature is off;
  - the screen is locked;
  - Accessibility is not granted;
  - local keyboard or mouse input happened in the last 2 minutes;
  - a Locked Use transaction holds the console.

  After taking the session's provider queue (`hold_session`), it checks those
  facts again. It then uses the same navigation (`ensure_native`) and native
  write path as a send.
- **Drafts are never replaced.** `send_prompt` takes a draft policy. An
  explicit send keeps `Draft::Replace`. An automatic write uses `Draft::Keep`
  and refuses with a conflict when the prompt holds a different non-empty
  draft.
- **The provider's record confirms the command.** For Claude,
  `receipt::user_text` normalizes a user record made only of `command-name`,
  `command-args` and `command-message` tags into `/name args`. The typed
  command therefore matches its own record. Because Claude writes that record
  after compacting, the attempt usually returns an unknown outcome. The
  existing unconfirmed-write latch then clears when the record appears.
- **Codex is refused, not approximated.** `NativeAppController::compact`
  returns `Unavailable` for any provider without typed commands, and nothing
  is sent.
- **Local opt-in, no ABI change.** The setting is the local store key
  `agent.idle_compact`, absent meaning off. It is toggled with the local-only
  `muster debug idle-compact on|off|status`. No protobuf field is added.

## Alternatives

- **Send `/compact` to Codex through Send.** Rejected: it reaches the model as
  a prompt, spends a turn, and does not compact.
- **Drive the Codex slash suggestion menu.** Deferred. It needs a recorded AX
  tree of the suggestion popup and live acceptance on a desktop app the person
  is using.
- **Compact every idle conversation regardless of age.** Rejected: after the
  cache expires, compaction costs a full re-read, and a restart would compact
  many old conversations at once.
- **Retry until confirmed.** Rejected: a repeated `/compact` would summarize a
  summary. An unknown outcome follows the existing latch rules instead.
- **Network or phone toggle.** Rejected for this change: it would need new
  protobuf fields, and the Swift bindings cannot be regenerated on every
  build Mac. The feature is local and off by default.
- **Replace a draft like an explicit send.** Rejected: an automatic write must
  never discard text the person left in the prompt.

## Consequences

- An opted-in Mac pays a cache-read compaction for each idle Claude Code
  conversation instead of a full re-read when the person resumes after the
  cache expires. The person's next turn starts from Claude's summary, which
  loses detail by design.
- The desktop app visibly selects the conversation being compacted. The local
  2-minute idle rule and the screen-lock check keep this away from someone
  using the Mac, but a person who returns mid-compaction sees it happen.
- Until Claude writes the command record, about a minute, the session's
  unconfirmed-write latch refuses other writes, as after any unknown send.
- Codex conversations are not compacted.
- The setting is per Mac and survives restarts. Nothing is visible on phones.

## Verification

- Source and tests: `cargo test -p muster-agent -p muster-platform -p musterd
  --no-fail-fast` passed with no failures. New tests cover:
  - the due window and its eligibility rules;
  - one attempt per idle period;
  - transcript compaction detection for Claude (in the record order Claude
    Desktop writes) and for Codex;
  - slash-command receipts;
  - draft preservation for automatic writes.
- Lint: targeted Clippy (`-p muster-agent -p muster-platform -p musterd -p
  muster-cli --all-targets`) reported no warnings. The touched files are
  rustfmt-clean; repository-wide `cargo fmt --check` was already failing on
  main in untouched files.
- Builds: `cargo build -p musterd -p muster-cli` passed. No app build,
  signing, publication or installation was done.
- Device and provider acceptance: **not run.** No `/compact` has been typed
  into a live Claude Desktop conversation by this loop, so these are
  unverified on a device:
  - selecting the conversation;
  - keeping a draft;
  - the late receipt clearing the latch;
  - the transcript check after a real compaction.
