# ADR 0153: Bound repeated work for large session histories

- Date: 2026-11-06
- Status: Accepted; device acceptance pending
- Related: ADR 0142, ADR 0147, ADR 0151; architecture/agent-halo.md; architecture/runtime.md

## Context

Small RPC pages do not bound cold source parsing, synchronous cache decoding,
repeated prefix hashing, or timeline projection after loading older history.
The operator requests paged caching, incremental validation, provider indexing,
and bounded display work before further device acceptance.

## Decision

Keep all history state and validation in Shared Rust. Cache a bounded recent page
separately from older pages; load disk-backed history away from the reducer and
reject stale completions using watch cancellation and authoritative readiness.
The recent cache holds at most 200 records; older pages have independent keys.
Before reusing an older page, validate its existing byte-exact prefix proof up to
an exclusive `before_id` bound. Changed pages and older peers fall back to a plain
page request. Old whole-session cache keys are not loaded. Preserve durable drafts;
new small history entries participate in the existing 64 MiB disk eviction budget.
Reuse ID indexes and SHA-256 prefix checkpoints for immutable source generations;
content rewrites invalidate them. Keep the existing byte-exact prefix proof wire
contract and same-ID reconciliation protections.

Maintain disposable SQLite projection indexes for plain JSONL sources in the
daemon's private state folder (128 MiB budget per provider). Bind them to provider,
source identity/size/mtime/ctime, running binary identity, and schema version.
Position/ID indexes select bounded pages without projecting the whole transcript.
Missing, invalid, or changed indexes use the authoritative provider reader. Reader
initialization is serialized per session, with duplicate background starts suppressed.
Never tail-seek JSONL without parser context: first indexing and background reader
initialization still parse the complete source. An unchanged indexed source can
serve its page first; any change across that snapshot/reader gap emits Reset so
the existing watch resynchronizes. Raw provider SQLite databases, compressed logs,
cloud and desktop-only sources keep their existing readers; this decision does
not invent a compatible parser checkpoint for them. The persistent fast path is
Unix-only until other platforms expose equivalent source identity evidence.

Rust owns the transcript display window and navigation. Native shells render
explicit earlier/newer affordances and return intents. Keep a bounded window even
when older history is loaded. Adjacent 200-record windows overlap by 16 records
to preserve boundary anchors; explicit oldest/latest navigation can jump. Reuse
turn boundaries for unchanged prefixes, retaining the complete containing turn
for group identity, completion and provider-card semantics. Already loaded history used by receipt accounting is not
silently discarded; it is separate from the bounded native projection.

## Alternatives

Longer timeouts, more streams, or smaller message counts do not remove these local
costs. Blind tail parsing loses provider state. Treating IDs or cached metadata as
proof of unchanged contents weakens reconciliation. Native-side history state
violates the shared-core architecture. Byte-budgeted lazy tool payloads remain a
separate change and are outside this decision.

## Consequences

Source indexes and page caches are disposable and never authorization inputs.
First-ever JSONL indexing, background reader initialization and historical receipt
accounting retain linear work. Re-entry can display a bounded cache/index page
before that work completes. Projection skips completed turns outside the window;
a single exceptionally long turn still requires its grouping context. Loaded
receipt history is not memory-bounded by this change. Index failure must retain
readable authoritative history. Large individual messages remain possible.
Apple, Android and HarmonyOS forward the same Rust navigation intents. Regenerating
HarmonyOS bindings also catches up existing schema additions omitted by earlier
commits; no platform-side pagination state machine is introduced.

## Verification

Use synthetic long histories, in-place edits, truncation, stale cache completions,
same-ID streaming revisions, indexed page boundaries, repeated catch-up proofs,
and earlier/newer navigation during live output. Check work counts as well as
contents, preserving existing race and receipt regressions. Run focused Rust
suites, strict Clippy, protobuf generation checks, native compile checks where
needed, public-content and ADR gates. Report source/build separately from signing,
installation, runtime, dual-Mac and physical iPhone acceptance. No OTA publication.

### Implementation evidence

- Added content/work-count checks for 10,000-record proofs: first validation
  projects 10,000 records; repeating the same proof projects only the mutable
  tail; advancing by one record projects two. Source-generation reset rejects an
  in-place edit under the same ID.
- Added indexed-source tests for 1,000-record keyset pages, same-sized rewrites,
  and a deterministic 300-record snapshot/reader race that must emit Reset.
- Added indexed/unindexed timeline equivalence and bounded projection tests,
  stale cache completion rejection, earlier/newer/live window checks, and updated
  the 1,300-record cache reopen integration to verify a two-record delta.
- Apple `MusterUI` target compiles without signing. Swift and HarmonyOS bindings
  are regenerated. Android/HarmonyOS device compilation and all physical device
  acceptance remain pending.
- The system disk filled during development. Only this chat's regenerable Rust
  incremental cache was cleared; subsequent Rust checks disable incremental
  caching without changing `CARGO_TARGET_DIR`. Swift products use the external
  development disk. Release artifacts, installations and device identity data
  were not changed.

### Verification results

- Final unit suites: muster-agent 378 passed / 4 existing ignored; app-core
  165 passed / 1 existing ignored; muster-proto 18 passed. A prior concurrent
  run hit the existing two-second polling-test timeout; its isolated run and the
  final four-thread suite both passed.
- Chat watch integration: all 10 passed. Strict Clippy with all targets and
  warnings denied passed for muster-proto, muster-agent and muster-app-core.
- Apple MusterUI build passed. Source formatting, public-content and ADR checks
  are required before landing. No signing, notarization, installation, dual-Mac
  runtime test, physical iPhone test or OTA publication was performed for this
  source revision; the previously notarized candidate does not contain it.
