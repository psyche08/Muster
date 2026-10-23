# ADR 0032: Simplify conversation metadata and collapse file changes

- Date: 2026-10-23
- Status: Accepted
- Related: ARCHITECTURE.md §3.7; ADR 0031

## Context

The user requested removal of Cache TTL. The shared Apple conversation footer
currently shows an estimated cache-expiry row and an expandable explanation.
The user also requested that changed-file summaries start collapsed.

## Decision

Remove that row, its expandable details, local expansion state and expiry-time
formatting from the shared macOS/iOS footer. Start changed-file summaries collapsed;
explicit reader choices still survive iOS cell reuse under ADR 0031. Continue displaying turn time, model,
token usage, cache hit percentage and cost. Retain the Rust/protobuf cache-expiry
metadata for wire compatibility; this change removes a presentation feature,
not provider caching or cache expiration behavior.

## Alternatives

Hiding the row behind another setting leaves unnecessary UI. Deleting protocol
fields would broaden a small presentation request into a compatibility change.

## Consequences

The footer is shorter and no longer offers a TTL detail disclosure. Older or
current providers can still supply expiry metadata without affecting the UI.

## Verification

Update existing Apple footer rendering tests for the reduced layout and render
known, unknown and absent expiry metadata. Verify changed files start collapsed
and an explicit expansion survives reuse. Run focused native tests, public-content
and ADR checks. No new release, signing, installation or physical-device acceptance
is claimed by this source change.

### Source verification

The existing macOS footer rendering test passed. Seven focused iOS simulator
tests passed, covering the compact footer, changed-file default collapse,
explicit expansion retained across reuse, other disclosures, streaming, history
insertion, viewport behavior, interaction drafts and controller teardown. The
shared UI compiles for both platforms. This UI change is not yet packaged or
installed; installed 1.0.87 was used only for the separate ADR 0027 send acceptance.
