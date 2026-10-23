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

### Release evidence and integration handoff

Release source `d928cf77d98a4962d90f0b9963815fdab40a83e2` built macOS
and iOS 1.0.85 (76). Both Rust FFI targets, desktop binaries, the macOS build
and iOS archive passed. Four OTA packaging tests passed. The Mac bundle was
signed on the existing device holding the matching G2 identity after local
Xcode signing exposed a profile/certificate mismatch; no private key was
exported or Keychain unlocked. Presence provisioning and clean-source
ancestry/fingerprint verification passed. Apple notarization
`a3e04266-3d36-48f2-b258-756b57090469` was Accepted; stapling, strict/deep
signature verification and Gatekeeper assessment passed.

Both downloads were published through the operator-provided SSH destination.
Independent public downloads verified the page, normal and retry manifests,
fixed and content-addressed IPA, versions, no-store headers and matching hashes:

- macOS ZIP: `849608295ead3316bd140cb27810d419e28aba508bff8a934451dcb6ff96e0aa`
- daemon: `0d859ab899a90f36398d800839d24f74b39f3a855f1e84c689831a44101a6a99`
- iOS IPA: `ac8c4f437f507af489007243352bc916ba7785690ae04b23ae97cd107ea347a2`

The iOS registered-device profile covers three devices and expires 2027-09-30.
OTA is <https://linsheng.dev:8443/muster/>. No signed update feed, TestFlight
build or Xcode Cloud run was published; hosted Actions remains disabled.

At the owner's request to merge before integrated testing, Mac-A had installed
this exact artifact under the Rust desktop-update lease. Its identity stayed
unchanged; signature, staple, Gatekeeper, Accessibility, relay and Vnet checks
passed, including remote device-info after restart. Mac-B and Mac-C deployment
and the six-direction network matrix were still pending at this handoff.
Physical iPhone installation, fresh native-send receipts and comparative stage
timing remain unverified. Release logs and signed artifacts are retained locally
under `build/releases/Apple-1.0.85-76/`; they are not committed.
