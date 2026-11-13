# ADR 0197: Show recorded reasoning and tool descriptions inline

- Date: 2026-11-13
- Status: Accepted
- Related: [0031](0031-transcript-disclosure-state.md), [0086](0086-dsh-session-chrome-and-trace.md), [0160](0160-turn-pages-and-work-on-demand.md)

## Context

The conversation initially hides reasoning and tool descriptions. The trace
uses the tool name as its title. A command stored as plain text with its
output on later lines can therefore appear simply as “shell”.

## Decision

Show recorded reasoning and individual tool rows by default in visible work.
Use the provider's description when available, then a bounded excerpt of the
command, path or query. Use the same tool title in the trace. Keep full
arguments and output in the row details. Respect a reader's explicit
disclosure choice and load completed historical work on demand.
Read the recorded Codex summary and the displayable Claude thinking block
when projecting desktop transcripts.

## Alternatives

Expanding raw output would bury the descriptions in command results. Parsing
arguments in each native view would make the descriptions platform dependent.

## Consequences

Active work takes more vertical space. Missing descriptions fall back to the
tool name. Records without displayable reasoning supply no new text.

## Verification

Exercise historical and live input shapes, summary-only reasoning, preserved
call/result pairing, disclosure defaults and layout. Track build and physical
device verification separately.

Local verification covered four provider scan tests, 24 timeline tests and two
trace tests. The macOS Swift suite passed 21 trace and rendering tests, including
phone and desktop layout widths. Swift linked an existing FFI archive and the
new Rust projection ran in the Rust tests. The iOS Simulator, Android and
HarmonyOS builds and installed-device checks remain unverified.
