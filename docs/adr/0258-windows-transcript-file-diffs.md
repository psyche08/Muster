# ADR 0258. Inspect file diffs in Windows conversations

- Date: 2026-11-21
- Status: Accepted
- Related: architecture §3.6.2, ADRs 0243 and 0248

## Context

The shared timeline supplies a per-file patch, bounded preview lines, counts
and file identity for a CLI diff. The Mac conversation displays these by file.
Windows printed the entire raw tool result and offered no per-file copy action.

## Decision

Render collapsed file summaries in timeline order. Let the reader expand a
file or copy its supplied patch from the conversation toolbar. Use the line
numbers, change kinds, counts and truncation flag supplied by Shared Rust.
Keep disclosure local to the device, conversation and displayed file. Check
the live projection before acting on an open menu. Preserve the existing text
presentation for messages without projected file diffs.

## Alternatives

Parsing the unified diff in WinForms would duplicate the shared parser.
Reading the current working tree could show different content from the
recorded conversation.

## Consequences

The visible preview stays bounded. The clipboard can contain a longer patch
than the visible rows because the user explicitly selected the file to copy.

## Verification

Exercise multiple files, line numbers and kinds, truncated previews, exact
copy including original newlines, and stale choices after navigation or file
replacement. Verify the Windows control against the packaged GUI separately
from provider ingestion and production node activation.

The Windows worktree controls passed at 192 DPI. The diff menu test used a
memory clipboard callback and compared the entire supplied patch, including
mixed line endings. A separate GUI opened a synthetic CLI journal through
the Agents catalog on an isolated software node. Shared Rust projected two
files, including one with 1,205 additions. The Windows GUI showed both
summaries, expanded the bounded lines and truncation notice, offered Copy
diff and collapsed the file again. This run preserved the system clipboard.
