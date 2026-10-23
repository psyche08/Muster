# ADR 0040: Map Claude navigation to its desktop identity

- Date: 2026-10-23
- Status: Accepted
- Related: Supersedes ADR 0039's navigation-ID decision; ADR 0010; ARCHITECTURE.md §3.6.2

## Context

Unlocked 1.0.88 acceptance reveals two concrete defects. The target sidebar row
has a `Merge conflicts` PR badge not admitted by the existing grammar (ADR 0010).
The fallback then dispatches a CLI UUID. Claude logs `code entry link invalid
?session`; its installed handler accepts `local_...` desktop IDs for this route,
and its registry maps these IDs to a different `cliSessionId`. ADR 0039 incorrectly
assumed the CLI UUID was the navigation identity. Its final surface check did
prevent any write or submission to the wrong conversation.

## Decision

Resolve the CLI session to exactly one active desktop registry entry in Shared
Rust before constructing a fallback route. Require a canonical `local_<uuid>`
filename and a valid CLI UUID, reject ambiguous mappings (including accounts),
archived entries, incomplete/malformed reads and oversized files. Do not invent
a desktop ID by prefixing the CLI UUID. Ignore unrelated non-session metadata.
Re-read for each navigation; resuming can change the mapping.

Retain ADR 0039's complete-zero-candidate gate, one guarded dispatch to the signed
running bundle, bounded verification, exact opened CLI-session check and no send
retry. Normal row selection remains first. Correct the observed PR badge in ADR
0010 rather than forcing fallback for an otherwise identifiable visible row.

## Alternatives

Prefixing the CLI UUID still identifies the wrong object. Guessing by title or
choosing the first registry match risks selecting a different conversation.
Removing fallback entirely avoids the ID bug but leaves truly absent rows
unsupported. A provider feature gate or future route change can still reject;
that failure must remain safe and observable.

## Consequences

The two identity domains stay explicit. Registry absence or uncertainty preserves
the zero-match refusal. Missing or disabled provider routes still fail closed.
The fixed visible-row path and corrected fallback require distinct device evidence.

## Verification

Replay the captured badge through the production label reader/matcher, showing
failure before its one-prefix correction. Exercise the production inverse mapping
with synthetic distinct IDs, archived/duplicate/corrupt/malformed records, account
ambiguity and changed mappings. Reject CLI UUIDs and injected navigation IDs.
Run agent/platform/daemon checks and repository gates. Use installed production
navigation without typing or sending; record which path actually ran. Paired
release 1.0.89 (80), signing/notarization, OTA, installation and runtime checks are
separate from navigation and provider-delivery acceptance.

### Source verification

The synthetic merge-conflict capture failed on the previous matcher, then passed
with the one-prefix correction. Agent library tests pass 231 cases (four existing
ignored); platform tests pass 37. Strict Clippy including tests passes for agent,
platform and daemon. Layer/public-content/whitespace checks pass. The live local
registry contains exactly one active mapping for the affected session, with a
desktop ID distinct from its CLI ID. No provider message was sent or replayed.
Installed corrected navigation and release gates remain pending at this commit.
