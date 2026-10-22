# ADR 0027: Reuse native send scans and batch control metadata

- Date: 2026-10-22
- Status: Accepted
- Related: ARCHITECTURE.md §3.6.2; ADR 0020; ADR 0026

## Context

Native sends repeatedly traverse the same conversation to find the composer,
find submit controls and produce diagnostics. Each selector also reads role and
labels separately through synchronous AX IPC. Validating cached submit controls
adds traversal even though sending resolves them again. Larger conversations
therefore do more work before dispatch. Startup-only caching would not address
these repeated reads or controls replaced after drafting.

## Decision

Read role, labels, children and diagnostic flags in one AX batch per node.
Preserve IPC and per-attribute errors: unsupported/no-value attributes may be
absent, but stale or unreadable nodes invalidate the scan. Keep selector,
composer-branch and actionable-submit decisions in existing Shared Rust code.

Carry all submit candidates and bounded, redacted diagnostics with each fresh
surface scan. Reuse that scan during draft readiness and diagnostic reporting.
Do not validate unused cached submit references during identity checks. Retain
fresh exact-session resolution, composer uniqueness, renderer refresh, exact
draft echo, live authorization/input checks, final pre-press control rescan,
enabled checks, one dispatch and provider receipt reconciliation. No cached
coordinate or historical enabled state authorizes a press.

Record elapsed time for each native operation stage, including failure exit.
Diagnostics reuse observed control metadata and never read transcript values.

## Alternatives

An unconditional startup snapshot becomes stale after navigation, process
restart or an editor rebuild. Removing final validation or reducing readiness
deadlines would weaken correctness without removing repeated work. Parent-path
validation and event-driven prewarming remain possible follow-ups after this
smaller change has device timing evidence.

## Consequences

Each readiness iteration avoids its duplicate controls traversal and the
additional diagnostic traversal on ready/failure. Matching and child discovery
use one IPC per node. Final validation and receipt latency remain; no wall-clock
speedup is claimed from source inspection alone. Unsupported attributes remain
normal, while failed batch reads cannot hide another composer or submit control.

## Verification

Test batched metadata against existing selectors, empty-title description
fallback, missing attributes and AX error values. Run agent/platform regressions
(including ambiguous submits and stale disabled duplicates), strict Clippy,
formatting, architecture, public-content and ADR gates. Preserve the existing
bounded composer diagnostics added by ADR 0026.

Hosted Actions remain disabled under ADR 0016. No signing, publication,
installation or end-to-end send is implied by local source tests. Fresh device
timing and provider acceptance require a dedicated idle conversation and an
installed build; those remain unverified in this source-only optimization.

### Local source validation

Agent tests passed 224 unit and 41 integration cases (five existing ignored
cases across those suites). Platform tests passed 36 cases, including the new
batch metadata and per-attribute AX error regressions. The first sandboxed run
could not bind the CatDesk test socket; rerunning with local socket access
passed. Strict all-target Clippy for `muster-agent` and `muster-platform`,
formatting, the 17-crate architecture check and public-content checks passed.
These checks ran against the current sanitized main branch, preserving its
renderer refresh, focused draft write and final submit-candidate revalidation.
No release build, signing/notarization, publication, installation or live send
was performed. Hosted CI was not enabled or dispatched.

### Paired release plan: 1.0.85 (76)

Publish this optimization in matching macOS and registered-device iOS builds.
Build from a clean committed source tree, verify the desktop source lineage
and Presence provisioning, sign with Developer ID, notarize and staple the
portable Mac bundle. Validate the signed iOS archive and OTA package, then
verify public page, manifest and package bytes. Install the verified desktop
artifact on Mac-A, Mac-B and Mac-C under the existing Rust update lease;
check signatures, device identity, Accessibility, relay and Vnet recovery.
Record completion evidence separately. Physical iPhone acceptance and native
send timing remain separate from release and installation verification.
