# ADR 0054: Targeted installation of the native Multica workbench

- Date: 2026-10-26
- Status: Accepted
- Related: [ADR 0052](0052-targeted-multica-deployment.md), [ADR 0053](0053-multica-native-workbench-and-runtime-routing.md)

## Context

The owner scoped the native Multica page to lists, details and runtime status,
and previously requested installation on one desktop. ADR 0053 implements the
read-only page but has not yet been installed there. The desktop-first execution
request remains separate: the existing Kiro-compatible bridge executes Codex
CLI, and changing its label would not implement desktop session creation.

## Decision

Build and install 1.0.96 (87) on the requested desktop, using the guarded,
Developer ID signed and notarized portable-bundle process from ADR 0052.
Keep the existing migrated container and Documents/Multica root. Replace only
the application bundle, with no old bundle backup. This is a targeted install;
do not publish or alter the public desktop/iOS release feeds.

Supersede ADR 0051's compatibility-link relocation: a Settings save must never
retain or create the old data path. For an existing container, accept a changed
root only after an attended offline migration has removed the old path and
updated every managed bind mount. Reject a save before any filesystem or
configuration change otherwise. Do not generically rewrite Apple's private
container metadata; ADR 0052's migration was specific to a verified runtime.

Keep the workbench read-only and preserve the existing container login.
Do not advertise native execution or CLI fallback as implemented. Calibration
must honor native input guards; a detected local-input conflict stops that
operation without automatically retrying it or sending a prompt.

## Alternatives

Reusing the previous version obscures source provenance. Publishing a fleet
update exceeds the requested installation. Renaming the ACP compatibility alias
would misrepresent provider behavior. Restoring the historical native-create
candidate without current selectors, receipt and task-credential validation
would not establish a working runtime.

## Consequences

The target receives the native workbench independently of the public release
pair. Desktop-first execution remains incomplete and must be reported as such.
An idle target is required for controlled native-provider verification; installed
applications or a protocol handshake alone do not establish that capability.

## Verification

Release build, signature and Presence checks, notarization, guarded
installation, unchanged device identity, service recovery, native page rendering
and read-only query acceptance are recorded below. Workbench source tests are
recorded in ADR 0053. The operator's native-provider diagnostic is separate from
the deployed product. No scheduler execution or physical phone acceptance is
claimed, and no public macOS or iOS publication occurred.

### Target acceptance on 2026-10-26

- Source: `13801519f122914d4a64840e376ee987b52914b7`, version 1.0.96 (87).
  Eight Multica tests passed (one operator-only integration test ignored),
  including rejected legacy symlinks, missing/mismatched mounts, unchanged
  source data on rejection and valid adopted mounts. Daemon library/test
  Clippy passed with warnings denied. Public-content and ADR gates passed.
- Build: daemon, CLI, browser helper, Apple FFI and macOS Release passed.
  The provenance gate refused an earlier build after source changed; rebuilding
  the final committed source passed clean-tree, ancestry and fingerprint gates.
  Developer ID, nested signatures and Presence entitlements passed verification.
- Package: Apple notarization `189edc71-9317-499d-a0d9-408124ef21d5` was Accepted;
  stapling and Gatekeeper passed on both build and target hosts. Portable ZIP
  SHA-256 is `6504ece701ff86229bf41a3de960a422031225e90530ef642930d589c6e5ef57`;
  installed daemon SHA-256 is
  `7d5e89805920d98ec02ba32254047f97840cbbbc2ae59cefa576345c00e6f656`.
- Installation: the target acquired the Rust update lease and replaced only
  its application bundle. Version/build, unchanged device identity,
  Accessibility, Relay and Vnet recovered. No old application backup was made.
  The installer's final daemon start raced with the supervisor and returned
  nonzero; subsequent read-only status confirmed Multica 0.3.56 already running
  with its four agents and two workspaces. No installation retry was needed.
- Storage: the Documents/Multica directory and matching configured mount root
  remain in place; the old root is absent and is not a symlink. Keep-alive stays
  enabled. No further data migration or credential changes were performed.
- Installed UI/API: AX inspection confirmed the native workspace picker,
  task list, runtime selector, running indicator and pagination, with no embedded
  web area. The installed typed query returned two workspaces, 50 issues,
  54 agents and 66 runtime records; a requested issue detail matched its ID.
  These are observations of the target, not fixed product limits.
- Scope: no public desktop feed, iOS OTA or TestFlight publication occurred.
  The native page is deployed; desktop-first scheduler execution is not.

### Separate native-provider calibration

The owner explicitly authorized isolated desktop test sessions with a fixed
no-file-change prompt. Claude 3.6556.2 opened a separate new-session window.
Its new page and completed-session page both exposed `about:blank`, so the
historical `/epitaxy/new` URL rule cannot accept it. Exact full cwd was exposed
by the folder control's description/help, and local mode by its environment
control. A native file panel can own AX focus as a sheet attached to that
window; the parent/child identity must be verified, not a window index.

A temporary operator diagnostic, not the shipped create/runtime adapter,
verified signed process identity, full test cwd, local mode, empty composer,
retained AX objects, physical-input counters and a provider desktop lease. The
first draft read was too early; inspection proved it was an unsubmitted draft.
After its exact asynchronous echo was verified, a durable attempt record
preceded one Send. The provider catalog returned one new session with the exact
cwd, and its transcript contained the requested prompt and `MUSTER_NATIVE_OK`.
The test does not establish Multica scheduler integration or task credentials.

The existing controller did not bind that standalone `about:blank` surface.
A separate existing-session send through the installed daemon was rejected
before delivery because the target was locked or no longer at its console.
No unknown outcome was retried. Codex creation, existing-session acceptance,
no-window/relaunch behavior, Locked Use and scheduler execution remain pending.
No native runtime or fallback capability is advertised by this release.

### Resumed console check and entrypoint review

The owner subsequently confirmed an unlocked target. Live observations showed
an unlocked active console, but a manual Locked Use transaction owned by a
different device. A new existing-session test was refused by the console-write
barrier before delivery. Unlocking the screen does not transfer that
transaction's authorization to a local CLI or another requester. No transaction
was cancelled, no direct AX action was used to bypass it, and no native send or
CLI fallback was performed. Continue GUI acceptance only after the owner ends
that manual transaction and establishes a free, unlocked console.

While that external prerequisite remained pending, read-only inspection of the
actual target bundles confirmed native new-page routes in ChatGPT
26.1021.31345 and Claude 3.6556.2. The findings and their unverified boundaries
are recorded in the [native creation review](../NATIVE_SESSION_CREATION_REVIEW.md).
This is diagnostic documentation only: no shipped runtime routing, provider
capabilities, signing, installation or release version changed in this follow-up.
