# ADR 0275. Give the Windows directory and conversation separate panes

- Date: 2026-11-21
- Status: Accepted
- Related: [ADR 0264](0264-windows-agent-directory.md), [ADR 0249](0249-windows-desktop-chat-sidebar.md), architecture §3.7

## Context

The Mac workspace keeps its directory full-height beside the current conversation. Windows puts the title, actions and composer across both panes and gives the workspace switcher the same visual weight as utility actions. Catalog rows concatenate titles and metadata on one line.

## Decision

Give the catalog its own column, roughly a quarter of the window. Keep the conversation title, tabs, content, actions and composer together in the detail column. Group workspace controls in one toolbar and indicate the workspace projected by Rust. Use a centered branded welcome view at the directory root, and visually distinguish section headings, session titles and metadata. Retain native accessible controls, selection identity, keyboard navigation and the shared intents.

## Alternatives

Restyling buttons alone leaves the workspace hierarchy unchanged. Replacing the shell or introducing a second navigation model would duplicate existing WinForms and Shared Rust behavior.

## Consequences

The reading and composing area stays aligned when switching between sessions. Utility commands remain accessible through the toolbar. Native high-contrast colors remain available.

## Verification

Render actual Windows forms with synthetic catalog and detail projections. Inspect screenshots and verify that the directory keeps its height beside the composer, at wide and narrow window sizes. Run existing catalog and transcript-related form checks, then check the installed portable GUI.

The real form was rendered at widths of 1160 and 760 logical pixels at 192 DPI. The directory kept the full workspace height, the composer stayed inside the detail pane, and short conversations showed their first and last characters. The Windows catalog and worktree form suites covered search and selection, formatted speaker labels, keyboard links, image rendering, code copy, changes and scroll position.
