# ADR 0031: Retain transcript disclosure choices across cell reuse

- Date: 2026-10-23
- Status: Accepted
- Related: ARCHITECTURE.md §3.7; supersedes the disclosure-reset limitation in ADR 0030

## Context

The recycled iOS transcript discards SwiftUI row-local state when a hosting cell
is reused. Expanded reasoning, tool output and file changes therefore revert to
their defaults when the reader scrolls away and back.

## Decision

Keep presentation-only disclosure overrides in a lightweight store owned by the
conversation collection controller. Key choices by the Rust-provided root block
ID, disclosure kind and component ID; scope tool rows under their activity group.
Observe each individual choice, rather than invalidating the entire timeline.
Until the reader chooses a state, follow the supplied rendering default, including
changes to a working group's default during streaming. Retain choices through
reuse, projection updates and history insertion, and release them with the opened
conversation. A newly opened conversation starts with defaults; this is not disk
persistence or a cross-session cache. Existing non-recycled views retain local
state. Rust pagination, provider state and answer submission remain unchanged.

## Alternatives

Retaining every hosting controller defeats recycling. Persisting view choices in
Rust or on disk adds durable product state that this presentation fix does not
require. Index-based keys move under history insertion; unscoped tool IDs can
collide across groups. One published dictionary would redraw unrelated rows.

## Consequences

Only small choice objects survive offscreen. Their count is bounded by the
visited disclosure controls in the open conversation, not by retained view trees.
Interaction drafts keep their separate retained host from ADR 0030. Returning to
a conversation after closing it may reset presentation choices intentionally.

## Verification

Exercise real recycled rows with expanded and collapsed choices, nested tools,
streaming updates, prepends, separate session controllers and teardown. Run the
existing simulator layout/keyboard/navigation suite and source gates. Record
local Apple builds, macOS notarization, paired OTA and installed Mac checks
separately. Physical iPhone scrolling acceptance remains a separate check.

### Source verification

The complete iOS 27 simulator suite passed 25 tests, including six real collection
view regressions. Presentation choices were changed through the retained state
used by controls, then verified against rendered row geometry after reuse,
streaming and prepends. All transcript disclosure types, nested tool ID isolation,
separate session controllers, retained answer input and store/controller teardown
passed. This is simulator integration evidence, not physical touch acceptance.

An initial run exposed two over-specific geometry assertions: Markdown margins
settled after expansion, and an estimated scroll target was not the first visible
row. The disclosure test now checks retained expanded content; the existing
prepend test measures the actual visible reading anchor. The final complete run
passed without changing viewport or Markdown layout behavior. Rust runtime and
pagination sources are unchanged. Architecture and public-content checks passed;
prepare Apple 1.0.87 (78), with release and device evidence recorded separately.

### Paired release and installed runtime evidence

Apple 1.0.87 (78) was built locally from
`1e59d5985bced94ec1c41d18cff293e06c9a638f` after PR #15 merged. macOS Release
and iOS archive passed, along with clean source ancestry and Presence signing
checks. Apple notarization `218a88eb-c4fa-4df2-83d1-8e425edb9e92` was Accepted;
staple validation and Gatekeeper assessment passed. The simulator command also
finished successfully after its diagnostic collection, with 25 passing tests.

Paired desktop and iOS OTA publication passed public readback of the current
page and manifest, versions, cache headers, hashes and signatures:

- macOS ZIP SHA-256: `16eb747417a9530430b5b6285c1b21e281e010c7ce1ed9d6ed2451768dd75c8f`.
- iOS IPA SHA-256: `5c10406d13e66ad68daee233d796013b28114ec45340b8c08a910a07f1fdf33e`.
- Installed daemon SHA-256: `50538b7a1beb27eda25869e8b654c8b079cf8530c9f659d1f9e0aa999bf354d3`.

Mac-A, Mac-B and Mac-C installed through the Rust update lease, retained their
identities and Accessibility permission, registered relay and brought Vnet up.
Startup logs confirmed helper completion on all three hosts. All six directed
RPC paths and six virtual-address ping paths passed. Each installed bundle passed
strict codesign, notarization staple, Gatekeeper and expected daemon hash checks.
Peer disconnects during sequential updates were followed by successful checks;
they do not establish a persistent transport defect.

No provider message was sent for this UI-only change. The physical iPhone was
unavailable through the direct installation channel, so its installation and
scrolling/touch acceptance remain unverified despite successful OTA publication.
GitHub Actions stayed disabled; no Cloud build or old OTA deletion occurred.
