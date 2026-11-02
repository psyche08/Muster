# ADR 0083: DeepSeek-Harness session chrome, trace page and a DeepSeek conversation style

- Date: 2026-11-02
- Status: Accepted; macOS/iOS builds and synthetic rendering verified, device interaction pending
- Related: [ADR 0073](0073-deepseek-harness-session-viewing.md) and
  [ADR 0074](0074-deepseek-harness-plugin-control.md) (DeepSeek Harness as a
  viewed and plugin-driven app); [AGENT_INTERFACE.md](../AGENT_INTERFACE.md)
  conversation styles; [CONVERSATION_WORKBENCH.md](../CONVERSATION_WORKBENCH.md)

## Context

The owner uses DeepSeek Harness (DSH) as the main desktop coding-agent app and
asked Muster's session screen to look and work like DSH's session view: a title
row with session metadata and actions, a compact tab row with an underline for
the active view, and a Trajectory page that shows execution evidence as a
turn-grouped ledger with a time overview. Muster's Mac session screen currently
uses the window toolbar title, a native segmented picker for Conversation /
Tracing / Subagents / Sessions, and a tracing tab made of `DisclosureGroup`
rows. The conversation-style preference exists (Codex and Claude) but has no
visible control in the current shell, although the interface document still
describes one.

The trace data is a Rust-owned projection, not the provider's raw trajectory:
`TraceSpan` carries `kind` (`tool` | `turn` | `reasoning` | `status`), `title`,
a bounded `detail` (at most 2,000 characters per source message), `status`,
`started_unix`, `ended_unix`, `duration_known` and `duration_seconds`. There are
no assistant-message timing records, structured tool arguments, attachments,
turn identifiers or sub-step boundaries, and timestamps have second precision.
The UI may group and lay out what Rust projected, but it must not invent
durations, payloads or turn metadata that the projection does not contain.

The installed DSH 0.2.1-rc.5 client exposes its own design tokens and compiled
view CSS in its web bundle. Reading them is the same kind of investigation that
ADR 0073 recorded for its session data: it defines the observed baseline
(palette values, header geometry, tab metrics), not a runtime dependency.

## Decision

This is a presentation-only change in the Apple shell. Shared Rust and the
protocol are unchanged; the session screen keeps rendering ViewModels and
sending Intents.

- **Design tokens.** `Sources/MusterUI/SessionChromeStyle.swift` records the DSH
  alias palette (surfaces, four label levels, four border levels, business /
  success / warn / error states, interaction washes, radii, geometry, type
  sizes) exactly as read from the installed client's CSS, resolved for light and
  dark system appearance. It is used by the new session chrome and the trace
  page.
- **Mac session chrome.** The Mac `ChatView` gets a custom header
  (`SessionChrome.swift`) instead of the window-toolbar title and segmented
  picker: a title row with an optional parent breadcrumb, session utility chips
  (live state, provider, device, subagent count, sub-session count) and icon
  actions (style, stop, model, files, git), then a tab row (Conversation,
  Trace, and an overflow menu with Subagents, Sessions and Terminal) with a
  business-blue 2pt underline on the active view. The header is 76pt high,
  with a 0.5pt bottom divider and DSH's spacing. The session actions that were
  in the Mac toolbar move here; the window toolbar keeps only app-level
  navigation. iOS keeps its native navigation and workbench (CONVERSATION_WORKBENCH.md).
- **Trace page.** `TraceModel.swift` is a platform-neutral projection over the
  existing spans: it groups records into user turns (`User message` starts a
  turn, records before the first turn are "Before first turn"), classifies the
  kind tag, and derives a three-lane timeline (turn / reasoning / tools) in
  equal-width sequence mode or in recorded-time mode when spans carry timing.
  `TraceScreen.swift` renders a DSH-like page: a toolbar with duration mode,
  expand/collapse and search; a 50pt time overview with turn boundaries; a
  30pt-row ledger with kind tags, turn rails and labels; and an inspector for
  the selected record (macOS: 340pt right pane; iOS: sheet). Missing timings
  stay unavailable, running records show their start rather than a fabricated
  duration, and details remain Rust-truncated as received.
- **Third conversation style.** `ConversationStyle` gains `deepseek`, so the
  shell offers Codex, Claude and DeepSeek styles (the owner asked to keep
  Codex). Its palette, radii and composer use the DSH tokens; typography stays
  the system font. The style is still a local rendering preference: it does not
  select an agent/model, send an intent, reload history or change
  authorization. The new header exposes the style menu (paintbrush), restoring
  the visible control the interface document already specified; the stored
  preference and the environment plumbing are unchanged, so existing Codex /
  Claude choices survive.

## Alternatives

- **Embed the DSH web frontend (WebView).** It would copy another product's UI
  wholesale, introduce a second rendering runtime, and put protocol-shaped
  state into the shell, contrary to the native-UI and Rust-ownership rules.
- **Clone DSH's virtualized table completely, including drag-range zoom,
  ttft/decoding split, raw JSON trees and attachments.** Those features need
  data Muster's projection does not carry; building empty placeholders would
  misrepresent unavailable evidence.
- **Restyle the existing segmented picker only.** It does not produce DSH's
  title row, metadata chips or underline tab row, which is what the owner
  referenced.
- **Add a fourth agent provider/profile for DSH.** DSH session support already
  exists (ADR 0073/0074); this change is about the shell's presentation and
  does not reopen protocol, authorization or controller semantics.
- **Remove Codex style.** The owner chose to keep it as a third style.

## Consequences

- The Mac session screen looks like DSH's session screen while remaining a
  native SwiftUI view of Rust-owned state. Session actions are concentrated in
  the header, so the window toolbar no longer duplicates them.
- The trace tab becomes useful for longer histories: turn grouping, the time
  overview and the inspector make tool activity and failures easier to read
  than the previous flat disclosure list. Timings are still approximate
  (second precision) and details are still bounded by Rust.
- The DeepSeek style is a new local rendering option. No migration is required:
  the stored `conversationStyle` values `codex` and `claude` remain valid, and
  no default changes.
- The DSH palette is pinned to the observed release. A future DSH redesign
  needs a new token update and evidence; it does not change data or protocol.
- Not implemented from DSH: range-selection focus over the timeline, zoom/pan,
  assistant ttft/decoding timing, structured tool payload views, image
  attachments in the ledger, and virtualized per-row measurement beyond what
  SwiftUI's lazy stack provides.

## Verification

Separate source/tests, build, rendering and device acceptance states:

- Source: `swift build --target MusterUI` (macOS, debug, against the local
  `libmuster_ffi.a`) succeeds with the new chrome, trace page and third style.
  The trace model is pure Foundation + MusterCore, so tests can run without
  SwiftUI.
- Tests: the macOS `MusterCoreTests` suite ran on 2026-11-02 with 103 tests and
  0 failures, including the new `TraceModelTests` (20 cases: turn grouping,
  before-first-turn section, search/filtering, sequence and timed timeline
  layout, missing/duplicate timings, turn boundaries, error/running flags and
  duration/start-time formatting) and `SessionChromeRenderingTests` (renders the
  chrome and trace page in light and dark and asserts the header's height).
  The existing conversation-style rendering test now also renders the DeepSeek
  palette in light and dark, and the mobile composer layout fixture iterates all
  styles.
- Rendering: `/private/tmp/muster-session-chrome-{light,dark}.png`,
  `/private/tmp/muster-session-trace-{light,dark}.png` and
  `/private/tmp/muster-session-integrated-{light,dark}.png` were rendered from
  synthetic fixtures and inspected against the DSH baseline (76pt header,
  underline on the active view, metadata chips, 50pt three-lane overview,
  30pt ledger rows, kind tags and turn grouping; the integrated image shows the
  header over the DeepSeek conversation style and composer).
- iOS: `xcodebuild -scheme MusteriOS -sdk iphonesimulator` (Debug, arm64,
  ad-hoc signing, local `libmuster_ffi.a`) built successfully on 2026-11-02;
  that build compiles the shared trace page, the iOS sheet inspector and the
  iOS ChatView path. This is a build, not an installation or interaction test.
- Not verified here: physical-device interaction, VoiceOver/accessibility
  traversal of the new header and ledger, every window width, an installed
  iOS build, and Live provider data in the trace page.
