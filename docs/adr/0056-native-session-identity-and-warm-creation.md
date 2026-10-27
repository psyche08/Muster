# ADR 0056: Native session identity and warm Claude creation

- Date: 2026-10-27
- Status: Accepted; duplicate scheduler project names remain an acceptance blocker
- Related: ADR 0055, PR 52; supersedes its new-window-only rule for the Claude main shell

ADR [0057](0057-native-codex-project-identity.md) supersedes name-only
project resolution for repeated scheduler directory basenames.

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

Session discovery must scan page identities across every window without
descending into each Codex page's conversation controls. The current signed
Codex layout identifies each session at its `app://-/index.html` web area.
Prune only that page's descendants during identity discovery; still visit all
sibling windows, reject duplicate target pages, and fully scan controls inside
the uniquely selected page. Introduce an explicitly branch-pruning traversal
instead of interpreting the old stop-on-false walker as pruning. Sidebar
discovery uses the same distinction when retaining navigation roots. Keep
node/time bounds and fail closed on unreadable/truncated child lists.

Apply creation's existing-draft check separately to every retained window,
using each window's unique composer. Never combine all independent windows
under one form's node budget. Require each complete window scan to succeed;
unreadable, ambiguous or nonempty composers still refuse navigation.

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

The signed `6a0cec8` candidate passed Codex creation and Claude cold creation,
continuation and warm creation. Codex continuation instead exposed an unrelated
long-window scan exceeding its bound, before sending. The identity scan must
avoid conversation descendants while still proving all page candidates were
examined. Add regressions for later sibling duplicates, cycles, pruning,
unreadable children and bounds before redeployment; do not replay that alias.

Those regressions and the affected agent/platform suites passed (320 tests,
five ignored); strict Clippy passed. A read-only target diagnostic using the
new production walker visited all five Codex page roots, completing in 26 ms
over 302 shell nodes. This confirms scan completeness on the current layout,
not a successful continuation. The real warm Claude scheduler task on
`6a0cec8` also completed on its first attempt with a ready native binding and
the expected marker. Rebuild and repeat continuation after installation.

On installed `203e03e`, a later creation was refused before any new window:
its global draft-preservation scan aggregated five unrelated forms and hit
4096 nodes. Read-only diagnosis confirmed every individual window completed
in 67–80 ms (827–1153 nodes, one composer each), while the aggregate hit the
node limit. Partition only this existing-draft scan by the already-retained
window identities; keep all per-window completeness and draft checks.

### Latest installed evidence

Targeted desktop 1.0.98 (89) is installed from source
`826d522b002962d9ca21e873c07f3329b2e2fe71`. Its daemon SHA-256 is
`71a4da6e28e5d884ba15ad8f4adb45f72606ba866f613b06f4b844fa5e18ed1a`.
Apple notarization `a1c07d97-88e9-44b7-a010-5299b64a9449` was accepted;
signature, stapling, target Gatekeeper, identity, Accessibility, Relay and Vnet
checks passed. No old application backup was retained. The selected Documents
data root remains active with keep-alive enabled; the previous root is absent.
Public desktop and iOS feeds remain unchanged.

On this installed source, Codex created a new session alongside existing
windows and returned the expected marker. Repeated synthetic prompts produced
duplicate generated titles; continuation correctly refused those ambiguous
sessions with a complete scan. A fresh test session was given a unique test
name through the desktop app's title API. The same native binding then returned
the expected marker on its second turn. This verifies rename handling,
multi-window discovery, first creation and continuation without relaxing
duplicate-name rejection or replaying failed aliases.

Affected source checks passed 377 library tests and 42 integration tests,
with seven operator/environment cases ignored overall. The final per-window
draft change repeated all 37 platform tests and strict Clippy successfully.
Public-content and ADR gates passed. No CI checks were reported on the PR.

Claude cold creation, continuation, running-zero-window scheduler execution,
warm creation and warm scheduler execution passed on the earlier installed
candidates above. Repetition on the latest candidate encountered renewed input
changes; a subsequent 45-second admission check could not obtain 15 quiet
seconds and started no desktop test. Retain failed aliases unchanged, preserve
the deployed version and request a quiet console before final repetition.
Codex cold/zero-window checks remain explicitly deferred by the operator.

### Final scheduler repetition and requested cleanup

The operator subsequently deferred Claude cold repetition to preserve an
unrelated running task. Its window and task were not stopped. The installed
daemon hash still matched `826d522` above.

A new Codex scheduler issue failed both scheduler-managed attempts at its
30-second no-progress deadline. The exact native-create ledger entries were
rejected when console authorization ended; both bindings remained attempted
with no provider identity. These attempts were not manually replayed. A fresh
direct test using a uniquely named project returned the real provider marker.
Scoped project-state inspection found two projects named `workdir`, including
the new scheduler directory. The existing exact-label-to-single-root proof
therefore cannot establish which directory is selected. This is an unresolved
runtime limitation for ordinary repeated scheduler directories; do not relax
the path proof, claim acceptance, or merge the PR until it is fixed.

The operator then requested removal of test tasks and directories. Four
synthetic scheduler issues were permanently deleted through the authenticated
service API, and a follow-up GET returned 404 for each. Their four work roots,
the dedicated directory containing 25 direct test directories, the earlier
temporary acceptance root, and 30 test-only runtime binding namespaces were
removed. Two test-owned REPL processes were terminated before removing their
working directories. Seven identified Codex test conversations and the two
agents created for these tests were archived through their supported APIs;
archival is not a claim of permanent conversation deletion. The pre-existing
agent, business tasks, provider history outside that archive set, container
login, application data root and installed candidate were retained. Temporary
screen keep-awake was ended. No new test tasks were created after cleanup.

Keep PR 52 unmerged with the duplicate-project limitation visible. The recorded
earlier receipts remain historical evidence; deleting test artifacts does not
turn the final failed scheduler repetition into a passing result.
