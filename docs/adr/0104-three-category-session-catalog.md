# ADR 0104: Three-category session catalog

- Date: 2026-11-03
- Status: Accepted; source verified, device acceptance pending
- Related: [ADR 0102](0102-session-sections-hierarchy-search.md), [ADR 0103](0103-visible-tabs-and-provider-aggregation.md)

## Context

The owner replaces provider aggregation with exactly three top-level categories:
Pin, Section and Recent. There must be no additional device, provider, date or
native-section subdivisions. Native section membership and source identity still
matter for navigation and for the separately requested native organization actions.

## Decision

Supersede ADR 0103's catalog grouping for both aggregate and single-device views.
Shared Rust projects three stable, initially expanded categories, including empty
categories. Pin takes precedence over native custom/project membership; Section
contains grouped conversations; Recent contains the remainder. Search uses the
same categories. Native section names, device and provider are row metadata only.
Native section collapse settings do not hide rows inside these categories.

Read Codex's native pinned flag and Claude's scoped pinned membership independently
of native custom section membership. Carry explicit pin and section-kind metadata
through the ABI; never classify by a user-editable title. Older peers may lack pin
metadata; do not invent pinned status. Preserve device/provider/UUID identity and
Claude family rendering within a category. A child in another category stays
independently visible in its own category instead of disappearing under its parent.

Native pin, section creation/deletion, membership and archive mutations remain a
separate implementation obligation; this projection change neither writes native
stores nor claims those actions are implemented. Their eventual results must feed
this same native metadata projection, not a client-only organization overlay.

## Alternatives

Provider/device/date grouping adds categories the owner has explicitly rejected.
Treating every native section as custom incorrectly puts Claude's default Sessions
bucket in Section. Classifying by labels confuses user-created sections called
Pinned or Recents. Direct native database writes are not needed for display.

## Consequences

A native group is shown on its conversation rows rather than as another header.
Empty native groups have no conversation row; future section-management controls
must retain their native IDs independently of these three display categories.
Pin metadata is display-only and grants no write or control authority.

## Verification

Test all three categories, pin precedence, empty/search/single-device projection,
duplicate labels and IDs across devices, native recent sections, family members in
different categories and native metadata compatibility. Run focused Rust tests,
protocol generation, Apple UI compile and repository policy checks. Signing,
publication, installation and physical acceptance are separate delivery states.

### Source verification

The native metadata readers and Shared Rust three-category projection are
implemented, with additive protobuf fields and regenerated Apple bindings.
Both affected Rust unit suites pass; focused cases cover native pin membership,
old schemas, account boundaries, category precedence and independently pinned
children. Strict Clippy, layer checks, public-content checks and protocol
regeneration pass. macOS and iOS simulator MusterUI compile successfully.
Native organization write actions remain unimplemented; no provider store was
modified for this change. Signed installation and physical UI acceptance are
tracked separately from this source verification.

### Subsequent group decision

[ADR 0110](0110-native-groups-paging-and-session-entry.md) supersedes the combined
Section bucket with individual native Section and Project groups and paged reads.
