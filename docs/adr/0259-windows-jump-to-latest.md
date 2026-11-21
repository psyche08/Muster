# ADR 0259. Jump to the latest conversation messages on Windows

- Date: 2026-11-21
- Status: Accepted
- Related: [ADR 0160](0160-turn-pages-and-work-on-demand.md), [ADR 0248](0248-windows-transcript-work-steps.md), architecture §3.7

## Context

On macOS a reader can jump from an earlier history page or from higher in the
visible transcript straight to the latest message. Windows has a one-page
Newer messages action and separate plain-text and inline-image readers.

## Decision

Use the existing Shared Rust latest-history intent. Offer a keyboard-accessible
button when the reader can scroll down or the shared projection has newer
messages. Scroll the active transcript and complete a page jump when the same
conversation projects its latest page. Clear a pending jump on navigation.

## Alternatives

Repeatedly paging forward takes multiple user actions. Inferring the latest
page from displayed message IDs would duplicate the shared history window.

## Consequences

The Windows shell owns only viewport position and focus. Reading the latest
page uses the normal history path and leaves the composer draft intact.

## Verification

Exercise native text and image controls with a long transcript, a short
transcript, an earlier page and a change of conversation before the reply.
Check the generated intent, the displayed last line and the button visibility.
Verify the portable GUI independently of these control checks.

## Reading during updates

The installed Windows reader opened a long conversation at its first line.
While reading above the tail, an appended reply and loading an earlier page
both reset the first visible line to the top. The macOS collection remembers
a projected block and its offset while reading, and follows the tail when
opening or already reading the latest page.

Keep this presentation state local to each open device and session. Capture
the visible block and selection before replacing the transcript. Restore them
against the new rendered offsets. When an older page is prepended, keep the
reader on the same block even if the previous viewport was at the end.
Continue to honor an explicit jump. Absolute scrollbar positions alone change
meaning when the shared history window grows or slides. Verify long plain
and image histories, asynchronous image layout, paging, selection and session
changes using native controls, then append synthetic records on an isolated
node while reading in the installed GUI.
