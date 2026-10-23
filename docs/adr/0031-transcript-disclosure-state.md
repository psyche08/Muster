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
