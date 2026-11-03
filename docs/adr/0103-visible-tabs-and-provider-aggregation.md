# ADR 0103: Visible session tabs and provider aggregation

- Date: 2026-11-03
- Status: Accepted; source and rendering verified, device acceptance pending
- Related: [ADR 0086](0086-dsh-session-chrome-and-trace.md), [ADR 0102](0102-session-sections-hierarchy-search.md)

## Context

The owner requests direct Subagents, Sessions and Terminal tabs rather than the
macOS overflow menu, and provider-first aggregation in All devices. Native section
support remains read-only display, disclosure and title/content search.

## Decision

Supersede ADR 0086's secondary-view overflow choice: render Conversation, Trace,
Subagents, Sessions and Terminal directly in a horizontally scrolling tab strip. Empty related
views remain visible. Terminal stays visible but disabled when Shared Rust reports
it unavailable. Availability and control decisions remain in Rust.

Refine ADR 0102's catalog projection: All devices aggregates conversations under
one section per provider, ordered by recency across devices. Each row carries a
Rust-projected origin label with its device and native section when available.
Selecting one device retains its native sections and disclosure preferences.
Search uses the same provider aggregation. Session keys, parent relationships and
navigation remain scoped to device/provider/UUID; same titles or UUIDs on different
devices are never merged into one session. No provider-store writes are added.

## Alternatives

The owner selected horizontal scrolling on narrow windows instead of wrapping.
A More menu requires an extra action before choosing a destination; a scrollable
strip keeps direct tab controls and a stable header height.
Merging same-name native sections across devices would infer identity from labels;
keep their origin on rows and their full organization in the single-device view.

## Consequences

All devices emphasizes provider rather than machine. Native section ordering is
used in the single-device view; aggregate ordering is recency. The selected tab scrolls into view when changed. This does not auto-open terminals or authorize new operations.

## Verification

Verify Rust aggregation across devices/providers, duplicate session IDs, same-name
sections, search and single-device projection. Build Apple UI and run existing
synthetic chrome rendering at wide and narrow widths. Keep source verification,
signing/notarization, installation and physical acceptance distinct.

### Source and build evidence

- App-core unit suite: 133 passed, one existing ignored test; aggregation covers
  provider grouping, recency, duplicate IDs on different devices, search and
  single-device sections. Strict Clippy passes for all app-core targets.
- Existing session chrome rendering suite: three tests pass. Wide (1180pt) and
  narrow (420pt) light/dark header images and the integrated session rendering
  were inspected. The tab strip retains its height and has no More menu.
- macOS SwiftUI and iOS simulator MusterUI compile successfully. Layer and public
  content checks pass; the ADR gate runs after committing.
- Signed device installation and physical interaction acceptance are separate
  delivery steps, not implied by these build/rendering results.
