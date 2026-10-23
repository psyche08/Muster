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

### Installed navigation acceptance (2026-10-23)

On installed 1.0.89 (80), after the owner paused desktop input, production native
navigation found exactly one row for the originally failing session and used
`select_press`. Exact opened CLI-session identity verification passed. Selection
including verification took 1,159 ms; the complete native preparation took
1,766 ms. Before/after user-frame snapshots were unchanged. No message, answer
or draft was submitted by the acceptance operation.

An earlier attempt on the same release was rejected by the existing local-input
protection before clicking; user frames were unchanged. The successful run used
the sidebar row, so it does not constitute installed acceptance of the corrected
exact-ID fallback. That fallback retains source regression coverage and a checked
live registry mapping. Provider delivery and physical iPhone acceptance were not
performed for this release.

### Release and runtime verification

- Clean release source: `5b104142ab551abf62b2b31d0b4637fb5dea4d2d`;
  source fix merged in PR #27. Local daemon, both Apple FFI targets, macOS Release
  app and iOS archive builds passed.
- macOS Developer ID signatures and source/Presence ancestry checks passed.
  Apple notarization accepted request `ceb0179f-48be-4e1a-a87a-1f7ec33a2c48`;
  staple validation, strict signatures and Gatekeeper assessment passed.
- Paired macOS 1.0.89 and iOS build 80 OTA were published. Independent public
  downloads verified the actual install link, version/build, artifact digests and
  macOS signing/notarization. macOS ZIP SHA-256:
  `f45c515491803ea8d246b6c326100abc22af2c06fe32e2ec326a53a28429ec96`;
  iOS IPA SHA-256:
  `04822c917d77a405359e1e30bcfeac77089cfd4e9a5e6727d1091a186cb05a7b`.
- All three Macs installed through the Rust update lease and retained their
  device identities. Each reports 1.0.89 (80), Accessibility granted, registered
  relay and active Vnet. Runtime source stamps match the release commit; each
  installed daemon SHA-256 is
  `8c6579510c7356fa27cfb8a8ee9450198ed2d18bc976698e191297d931f24985`.
  Installed strict signatures, staples and Gatekeeper assessments passed.
- All six directed RPC info paths and all six directed virtual-IP ping paths
  passed after installation. Peer-unreachable warnings during the other Mac's
  replacement interval recovered; shutdown I/O errors precede the new daemon
  source stamp. These observations are not new network fault-injection tests.
  One Mac remains signed out of Claude Cloud; that is a provider credential
  prerequisite, not evidence that native sidebar selection failed.
- GitHub Actions remained disabled: local gates passed, no hosted CI pass is
  claimed. No Xcode Cloud build was started or historical OTA file deleted.
  iOS OTA publication does not establish installation on a physical phone.
