# ADR 0243. Show session files and recorded changes on Windows

- Date: 2026-11-20
- Status: In development
- Related: ADR 0032, ADR 0240 and the shared session file views

## Context

The Windows session file screen treats every payload as UTF-8 and omits loading,
truncation, recorded changes and the link to the current file. The Git screen
does not distinguish a selected commit from the log. Directory and Git results
can arrive out of order for two requests against the same session.

## Decision

Expose recorded changes from each turn summary on demand, with the original
summary ID and path. Render the shared file and Git projections in a selectable
Windows source preview. Keep directory and file requests relative to the remote
session. Only offer the current file using the path supplied by Rust. Bind
each asynchronous worktree read to its originating request in Shared Rust.
The existing file-link confinement test fails on Windows for a remote POSIX
working directory. Interpret the target directory in file URL space, preserve
component boundaries, and send only its relative child path to the target.
Allow an explicit file link to be opened from the session's file browser.

## Alternatives

Opening local paths in Explorer would read a different machine's filesystem.
Decoding arbitrary bytes with replacement characters would conceal a failed
text preview. Matching only a path cannot distinguish repeated opens.

## Consequences

The shell handles display, selection and navigation input. The target daemon
continues to own filesystem access and the shared reducer owns which result
can update a view. Recorded patches remain readable for deleted files.

## Verification

Exercise out-of-order completions, including stale errors. Check file, binary,
error, truncated, empty and recorded-change previews with a synthetic Windows
fixture. Check diff formatting, current-file navigation and the built x64 ZIP.
Report real network checks independently of fixture coverage.

On Windows the existing POSIX session-link test initially returned
"Session directory is unavailable". It now passes alongside drive and UNC
cases. A local named-pipe integration test discovered a synthetic session
and read its directory, a truncated file, its Git log and its commit diff
against a temporary repository. A standalone WinForms fixture exercised
binary and UTF-8 rendering, selection, diff colors and the original change ID
at 192 DPI. To run the device-side file unit tests on Windows, the DeepSeek
Unix peer fixtures now compile only on Unix. Portable cases and the existing
Windows transport refusal remain covered. No live provider session is required.

The Windows checkout also exposed a public-content gate mismatch for the
reviewed remote-desktop vendor lines. Normalize CRLF line endings before
comparing their existing per-file and per-rule digests. Content, rule and
path matching otherwise stay exact.
