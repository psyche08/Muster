# Desktop Chats catalog (2026-09-30)

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

Implementation is in the active `src/muster` tree: `muster-agent::chat_catalog`
owns source validation, filtering, identity resolution and cache merge rules;
`musterd` orchestrates reads and persistence. `muster-app-core` supplies the
provider order (`codex`, displayed as ChatGPT, then `claude`) and initial
selection. The SwiftUI picker renders that ordered ABI field.

## Source investigation

Read-only inspection of ChatGPT 26.928.20755 on Mac-C found:

- `$CODEX_HOME/sqlite/codex.db` (production) or `codex-dev.db` (other build
  flavors). The installed app's bootstrap chooses these filenames.
- `local_thread_catalog` contains `host_id`, `thread_id`, `display_title`,
  `source_kind`, `conversation_origin`, `source_recency_at`,
  `source_created_at`, `observation_sequence`, `missing_candidate` and other
  metadata. It is the desktop application's persisted directory, including
  local Codex, ChatGPT chats and cloud Work sessions.
- The app registers account-specific `chatgpt` hosts in
  `local_thread_catalog_hosts`. `local_thread_catalog_sync_state` records
  `initial_build_complete`, update watermark and full reconciliation time.
  `local_thread_catalog_metadata.catalog_revision` changes transactionally
  with visible catalog mutations. The app applies live observations, scans
  updated history, marks missing candidates and deletes reconciled or
  explicitly removed rows. Its source synchronization supplies freshness;
  reading this file does not request a fresh cloud synchronization.
- The app's own chat/task filter treats `conversation_origin = 'tpp'` as a
  task. Muster requires `source_kind = 'chatgpt'` for the selected account and
  excludes `tpp`. `cli`/`vscode` rows are not chats. Titles never classify rows.
  On this machine the source contained 72 ordinary chats, 2 `tpp` conversations
  and 32 local Codex rows. These counts are an observation, not a requirement.
- Chromium Local Storage also holds `codex.chatgpt-conversations`: version 1
  JSON with `pageParams` and `pages` (`items`, `offset`, `limit`, `total`).
  The application replaces the saved pages when offset zero refreshes, so it
  can be only a subset. IndexedDB contains separate app caches; neither this
  subset nor HTTP cache blobs are used as a complete directory.
- `state_5.sqlite` and `session_index.jsonl` index Codex sessions, not the
  ordinary ChatGPT list. Claude's local `claude-code-sessions` files likewise
  concern Code sessions. No equivalent complete ordinary-Claude-chat file
  was established; Claude keeps its Accessibility history adapter.

## Read and update boundaries

ChatGPT reads SQLite with `SQLITE_OPEN_READ_ONLY` in a transaction, including
WAL state (never `immutable`, no migrations, writes or checkpoints). It checks
account registration, completeness and revision on each request, and rereads
rows only when that stamp changes. The cached stamp also includes path and
file modification time to invalidate replaced database files. A changed
snapshot replaces the projection, covering additions, title/order changes,
explicit deletion and `missing_candidate` changes without timestamp-watermark
or same-second-update gaps. There is no row-level tombstone feed, so a changed
revision causes a bounded local snapshot read, not a fabricated row delta.

The application-owned database is already the durable full catalog: restarting
Muster reads it again without AX history paging. An incomplete initial build
is visibly reported as a subset. The reader refuses absent/unsupported schema,
more than one database candidate, or more than one registered ChatGPT account;
it never chooses an account by title or merges accounts. Source errors clear
identity aliases. The supported snapshot limit is 100,000 rows and SQLite busy
waiting is limited to 150 ms. This is an internal app schema; incompatible
upgrades produce an explicit error rather than an untyped AX fallback.

A client probe already returns this directory, so ChatGPT avoids the second
`full_history` call entirely. New clients require `agent.chat.catalog.v1`
from the daemon before accepting ChatGPT rows. Client cache keys are versioned
to avoid restoring the former mixed sidebar list. Opening a listed ChatGPT ID not yet bound in the running daemon refreshes
the typed source and rejects removed or non-chat IDs before navigation. Existing
active chats keep their Rust-owned runtime identity (including a newly created
chat not yet synchronized to SQLite); their original AX guards still apply.
When AX exposes only a title, it resolves to a stable database ID only if that
title is unique across all visible catalog rows, including Codex/Work. Duplicate
titles remain unopenable through that title-only surface; full AX navigation
may still be needed to open an off-screen conversation. This is a navigation
limitation, not title-based classification.

Claude's fallback full catalog is stored in `musterd.db` metadata as versioned
JSON. The first load scans history; subsequent refreshes merge visible rows by
ID, updating titles/current flags and retaining unseen older rows. A complete
successful sweep replaces the catalog, including deleting absent entries and
allowing an empty result. A partial/failed scan never proves deletion. Full
reconciliation is due after ten minutes and runs on the next requested refresh;
failed/incomplete attempts back off for one minute. Corrupt/version-mismatched
caches rebuild; clock rollback cannot postpone rebuilding forever. Incomplete
scans and cached incremental views display their limitation. Off-screen
renames/deletions (and title-only identities) are only reconciled by a complete
sweep. There is no claim of real-time cloud change delivery or immediate
account-switch detection for this AX-only provider.

## Verification

Synthetic SQLite/WAL tests cover source discrimination, misleading titles,
duplicate-title navigation, revision reuse, additions, renames, deletion,
missing candidates, partial build, account ambiguity, schema loss and restart.
Cache tests cover incremental upserts, complete-snapshot deletion, persistence,
corrupt recovery and retry timing. App-core tests cover ChatGPT ordering/default,
provider retention, no second history call and rejection of older untyped servers.

An opt-in read-only test (`inspect_local_catalog`, with explicit
`MUSTER_TEST_CATALOG_HOME`) read 72 chats with `history_complete=true` on Mac-C;
first read plus cache read took approximately 6 ms. No prompts were sent by this
verification. SwiftUI compilation is checked separately from signing,
installation and mobile end-to-end acceptance.
