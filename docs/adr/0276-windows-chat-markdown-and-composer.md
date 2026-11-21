# ADR 0276. Render conversation Markdown and distinguish user input on Windows

- Date: 2026-11-21
- Status: Accepted
- Related: [ADR 0275](0275-windows-workspace-layout.md), [ADR 0269](0269-windows-transcript-links.md), architecture §3.7

## Context

The Windows reader handles links but exposes other Markdown punctuation as ordinary text. The Mac shows assistant Markdown, separates user messages from replies, and groups the editor and submit control inside a rounded composer.

## Decision

Parse provider prose with the existing Markdig library. Project text, style ranges and actual link destinations from the same parse. Keep user prompts and code literal, preserve receipt-driven drafts and reuse native RichEdit selection and links. Place the native editor and the normal Send action inside one rounded surface. Distinguish user turns from assistant replies visually.

## Alternatives

Rendering provider HTML in an embedded browser would introduce another navigation surface and bypass the current native reading anchors. Replacing the editable control would duplicate IME and keyboard behavior.

## Consequences

Formatting changes only the Windows presentation. Markdown links continue to require an explicit user action through the existing intent. No externally hosted image or raw HTML runs during display.

## Verification

Test headings, emphasis, list nesting, quotes, inline and fenced code, adjacent links, Unicode offsets, and literal user text. Verify the real form, IME and Return behavior, reader selection and composer bounds at high DPI.

On Windows at 192 DPI the form fixture rendered a heading, bold text, inline code, a list, a literal prompt and the rounded editor at two widths. The worktree reader suite exercised keyboard and mouse link activation, image offsets, viewport restoration and code copy. These checks used synthetic sessions. Live provider delivery is separate.
