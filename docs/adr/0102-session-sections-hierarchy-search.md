# ADR 0102: Native session sections, Claude hierarchy and conversation search

- Date: 2026-11-03
- Status: Accepted
- Related: ADR 0099, ADR 0075, ARCHITECTURE.md §3.6.2, §3.7

## Context

The owner requests Codex/Claude sidebar sections, Claude-style parent/child
conversation presentation, and search over titles and conversation content.
The current list is flat and limited to recent sessions. Codex records section
and project membership in its local SQLite store. Claude records desktop/CLI
identity and spawned relationships in its registry, with sidebar preferences
in Chromium local storage. Titles are labels, never identity.

## Decision

Read provider-owned grouping metadata without modifying provider stores.
Shared Rust joins membership to known local session UUIDs, scopes section keys
by device/provider, and projects ordered sections and cycle-safe hierarchy rows.
Claude children appear indented beneath their root with disclosure controls and
parent navigation, retaining the distinction between side sessions and read-only
subagents. Native views only draw the Rust projection and send intents.

Search runs in Shared Rust on the host holding the transcripts, under the same
provider visibility permissions as reading those sessions. Search title and
provider-parsed user/assistant text, returning bounded snippets and exact UUIDs.
Do not search arbitrary paths, tool output or hidden system prompts. Debounce
client requests, cancel superseded work and reject stale/device-mismatched
results. Report partial search coverage rather than implying completeness.

Claude remains local-only. Remote Control metadata does not introduce sessions
or authorize native input. Group membership and search matches are display-only.
Codex project membership prefers native section/project UUIDs and scoped local
assignment migration metadata. An exact, unique full project root is a display-only
fallback; explicit projectless or remote memberships are excluded. Never match
project basenames or titles.

No direct writes to Codex SQLite or Claude local storage, and no provider sends
or approvals are needed for this implementation.

Codex current local data has migrated most rollout files into
`state_5.sqlite` and `thread_history_1.sqlite`. Extend discovery and the read-only
viewer for this provider-recorded schema, preferring paginated SQLite history
and retaining JSONL for older existing rollouts. Search typed user/agent items;
exclude guardian/subagent rows from the top-level catalog and Muster-owned CLI
records from this native adapter. Bound reads and disclose unavailable formats.

## Alternatives

Title-based grouping/navigation confuses duplicate labels. Client-only filtering
of 200 recent rows cannot search older conversations or their contents. Native
Swift filtering duplicates portable business state. Scraping historical LevelDB
strings can resurrect deleted preferences; read a bounded consistent snapshot
with the database engine instead, entirely in memory.

## Consequences

Provider schema changes can temporarily make native grouping unavailable; retain
ordinary sessions and surface the limitation. Search is bounded and may report
partial coverage on large or unavailable transcripts. Reading sections does not
claim editing/synchronizing provider-owned sections. No provider DB is migrated,
checkpointed or written. Snapshot data is ephemeral; only section metadata is
retained in the discovery cache.

## Verification

Test duplicate section names, UUID/account/provider boundaries, missing parents,
cycles, multi-level Claude children, collapsed sections, search Unicode and stale
responses, provider authorization and bounded/partial transcript search. Verify
current native metadata read-only. Run affected Rust suites, Clippy, protocol
regeneration, Apple UI build, layer/public-content/ADR checks. Signing,
notarization, publication, installation and physical UI acceptance remain
separate from source and build results and will be recorded honestly.

### Implementation evidence

The Shared Rust readers, catalog projection, search RPC and Apple UI are implemented.
Read-only native probes resolved Codex custom/project sections and migrated SQLite
conversations, plus Claude scoped sections and multi-level side conversations.
Search returned title/content matches with continuation. The native provider stores
were not modified. Legacy title-navigation and catalog mocks were aligned with the
already-shipped UUID-only navigation and streamed catalog protocol.

macOS and iOS simulator `MusterUI` package builds pass. The full affected
`muster-agent` and `muster-app-core` unit/integration suites pass, including
320 agent and 132 app-core unit tests. Provider-scoped RPC search (`musterd`
access integration), strict Clippy across affected targets, layer and public-content
checks also pass. The ADR gate is run on the committed branch before landing. This record does not claim signed installation or physical
UI acceptance. Target-device deployment and manual acceptance are separate delivery
steps; source verification is not a substitute for them.
