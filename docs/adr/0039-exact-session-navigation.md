# ADR 0039: Recover absent Claude sidebar rows by exact session navigation

- Date: 2026-10-23
- Status: Accepted
- Related: ADR 0010; ADR 0027; ARCHITECTURE.md §3.6.2

## Context

A native preparation failure had zero candidates after a complete sidebar scan
and one successful expansion (59 to 80 controls). No further expansion control
was found. Authorization and unlock had succeeded; no draft or submission had
occurred. The catalog knew the target session, but sidebar-only navigation could
not select it. The evidence does not establish why the renderer stopped exposing
older rows, or whether their labels had changed.

The installed Claude application uses `claude://code/continue?session=<UUID>`
for its own existing-session navigation. Inspection of its URL handler shows an
exact lookup in loaded, non-archived sessions and navigation to that session's
route. Missing sessions resolve to its Code home. Feature gating can also make
this route unavailable. A successful URL dispatch is therefore not identity proof.

## Decision

Keep the current bounded sidebar search first. Shared Rust permits one exact-ID
navigation fallback only for Claude, a canonical UUID, an observed navigation
area, complete application/sidebar traversal, and zero candidate rows. Multiple
candidates, incomplete scans and other providers retain the existing rejection.
Do not use a title, the special `last` selector, or any user-controlled URL.

The macOS adapter targets the bundle path of the already signature-verified
process through LaunchServices (`open -a`), with a two-second command deadline.
The same authority, unlocked console, process generation and local input guards
apply before and after navigation. Then the existing bounded, exact-session
surface verification must succeed before controller attachment or any mutation.
No URL dispatch or navigation is retried after an uncertain result. A disabled
route, missing session, wrong view or changed input fails closed. Diagnostics
separate sidebar scanning, exact-ID dispatch and surface verification.

Release both Apple platforms as 1.0.88 (79), including the previously merged
transcript and client-diagnostic work. Preserve paired publication, notarization,
source provenance and Rust installation leases.

## Alternatives

Fuzzy title matching weakens identity. More expansions or a longer timeout do not
address a missing control. Reopening the app or replaying a send is disruptive and
unnecessary. Direct provider internals or transcript edits bypass the native UI.

## Consequences

Existing sessions absent from the visible list can be selected without guessing
row titles. Claude owns the URL contract; future changes may fail safely and need
new evidence. Navigation may foreground its existing window, as a user-initiated
selection would. No provider receipt or successful send is implied by navigation.

## Verification

Test the production routing policy with a synthetic absent-session fixture,
ambiguous/incomplete scans, non-Claude providers, malformed IDs and URL injection.
Test process-to-bundle resolution. Run agent/platform tests, macOS compilation,
layer/public-content/ADR gates. Device acceptance should navigate an existing
session without changing its draft or sending a message, verify its exact ID, and
record blocking lock/input conditions. Signing, notarization, paired OTA,
installation on three Macs and runtime network checks are separate pending gates.

### Source verification

- Agent library: 228 passed, four pre-existing ignored. Platform library:
  37 passed. New tests exercise exact-ID routing, ambiguous and incomplete
  scans, provider boundaries, malformed IDs, query/fragment injection and
  process executable-to-bundle path resolution.
- Strict Clippy for both libraries and tests passes on macOS arm64.
- Layer and public-content checks pass. GitHub Actions remain disabled;
  these are local results, not CI results.
- The affected Mac remains locked during source validation. Production-path
  navigation on the installed build and provider delivery remain unverified;
  no chat message was submitted or replayed.
