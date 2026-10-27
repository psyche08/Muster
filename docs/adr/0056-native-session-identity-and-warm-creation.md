# ADR 0056: Native session identity and warm Claude creation

- Date: 2026-10-27
- Status: Accepted; installed continuation verification pending
- Related: ADR 0055, PR 52; supersedes its new-window-only rule for the Claude main shell

## Context

Native Codex creation and actual scheduler execution returned real receipts.
Continuation then failed because an already-open transcript viewer retained
the initial prompt as its title after Codex published its generated name in
the session index. A fresh scan read that name but native lookup and list
projection still preferred the stale viewer.

Claude cold creation, continuation and a real scheduler task from a running
zero-window baseline also passed. A subsequent task with a main window already
open was refused: Claude intentionally routes code/new into that main window.
Requiring every task to create another window prevents ordinary warm use.

## Decision

Keep Codex's scanned title authoritative in Shared Rust. A viewer may refine a
generic placeholder when no useful catalog name exists, but cannot overwrite a
current indexed name. List projection, native title resolution and navigation
use the same current name, and duplicate names remain ambiguous.

Before Claude navigation, identify at most one main window through its signed
bundle's exact main-renderer URL. Shared Rust may select that retained opaque
window when navigation creates no new one. Preserve existing-draft checks.
Before writing into a reused shell, require its exact new-session page URL,
the visible New session marker, full requested directory, local mode and empty
composer. Recheck these proofs before draft mutation and sending. A session
page, stale form, replaced window or nonempty draft cannot qualify. Continue
requiring a newly discovered provider transcript with the exact first prompt.
No failed or uncertain submission is replayed.

## Alternatives

Keeping the old restriction makes each later Claude task depend on manually
closing its window. A separate popout plus native folder-picker automation
adds more OS interaction while the app already supports an explicit new-session
route. Reusing an arbitrary foreground window or trusting its title would lose
the identity and new-session checks and is rejected.

## Consequences

Warm Claude creation can navigate the verified main shell after drafts have
been preserved; its prior conversation remains in the provider history. Codex
still creates a new window. Neither change bypasses console, input, signing,
directory or receipt protections. Unsupported layouts fail before sending.

## Verification

Extend the service regression to open Codex viewers before index rename and
duplicate-title changes, then append a prompt and ensure polling does not
restore the stale name. Test opaque window selection, including stale or
ambiguous reusable candidates. Run affected Rust checks, public-content and
ADR gates. Build, sign, notarize and deploy before repeating real continuation
and warm-creation receipts. The operator has deferred Codex zero-window/cold
tests that would interrupt an unrelated running task; retain that boundary.

The affected agent/platform libraries and integration tests passed 318 tests
with five operator/environment cases ignored (excluding a nested test-process
duplicate). Strict Clippy passed after a style correction. The index-rename
regression passed with both transcript viewers already open. Public-content
and ADR checks passed. These are source checks; deployment and the new warm
Claude behavior remain pending.

On the previously installed `035152c` candidate, Claude cold launch plus a
second turn returned the expected marker. A separate real scheduler task from
a verified running zero-window baseline completed on attempt one. Its native
alias bound to a ready provider transcript, which contains task-scoped CLI
calls and a final assistant response with the expected marker. Warm creation
then failed before submission because no new window appeared; that failed
alias is preserved and will not be replayed.
