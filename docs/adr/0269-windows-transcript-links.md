# ADR 0269. Open Markdown links from Windows conversations

- Date: 2026-11-21
- Status: Accepted
- Related: [ADR 0243](0243-windows-session-file-previews.md), architecture §3.7

## Context

The Apple conversation opens Markdown links in the message. On Windows x64 a synthetic reply with a relative file link displayed literal Markdown in the installed plain text reader. Opening the file required leaving the transcript and re-entering its destination.

## Decision

Use a CommonMark parser for Windows presentation of links in message prose. Give the visible label a native RichEdit link range with an out-of-band destination. Rendering performs no network or file access. Activation sends session destinations through the existing OpenFileLink intent. Explicit HTTP, HTTPS and mail links use the native external URL handler after a user gesture. Keep provider file authority, path resolution and permission checks in Shared Rust.

## Alternatives

Scanning for parentheses would misidentify code, escaped syntax, nested destinations and reference links. Loading HTML in a browser would introduce a second viewer and implicit resource loading. Shell-opening a session path would resolve it on the viewing computer.

## Consequences

Both text and image conversations can present actionable links. Sessions without links or images retain the plain reader. Clipboard selection, reading position and block anchors continue to use displayed text offsets. Markdown parsing adds a Windows presentation dependency and its license to the package.

## Verification

Exercise same-label destinations, escaped and code examples, reference links, Unicode, image offsets, keyboard activation and a session change. Use the installed GUI and a software node to follow a relative link from a synthetic transcript to its actual file preview and return. Verify package, installation and production node state separately.
