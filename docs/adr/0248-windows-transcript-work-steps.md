# ADR 0248. Inspect conversation work steps on Windows

- Date: 2026-11-20
- Status: Accepted
- Related: [ADR 0031](0031-transcript-disclosure-state.md), [ADR 0160](0160-turn-pages-and-work-on-demand.md), [ADR 0246](0246-desktop-chat-history-availability.md), architecture §3.6.2

## Context

Windows prints block titles and message text. A completed turn with unloaded
steps has no Windows action to fetch them. Even when present, projected tool
rows and their output are missing. An empty conversation also lacks the
loading indicator or transcript notice available on macOS.

## Decision

Display the shared transcript in order. In the chat toolbar, offer a Work steps
menu indexed by turn and the preceding prompt. Retain the reader's expansion
choices within the open conversation, keyed by block and nested tool identity.
Honor the Rust expansion default until the reader chooses. Expanding an
unloaded group or retrying a failed group sends the existing AgentLoadWork
intent. Render the Rust-provided activity rows, status, detail, output, loading
state and errors. Use the projected message count and notice for empty views.

## Alternatives

Eagerly loading every completed turn defeats bounded history. Reconstructing
tools from raw messages duplicates pairing and status decisions in Rust.

## Consequences

The shell retains only disclosure choices. Reading results uses the same
history and authorization path as the Apple client. The WinForms menu can be
operated from the keyboard. Rich Markdown and viewport anchoring have separate
presentation requirements.

## Verification

Exercise the native menu with synthetic projections, including delayed updates,
retry, duplicate tool IDs across turns, navigation to another device and a
return to the original chat. Check loading, unavailable, empty and populated
transcripts. Build a portable x64 release and check the ordinary-user runtime
and network independently of the UI fixture.

The native WinForms checks passed at 192 DPI. A separate isolated Windows
daemon with a software identity served a synthetic Codex history through its
named pipe. The real GUI opened the catalog entry, expanded an unloaded turn,
displayed the matching call's result and collapsed it again while preserving
the unsent draft. The failed-load and stale-menu cases were exercised with
projected fixtures. This run did not send a provider message.
