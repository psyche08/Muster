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

Pending: release build, signature and Presence checks, notarization, guarded
installation, unchanged device identity, service recovery, native page rendering
and read-only query acceptance. Source tests are recorded in ADR 0053.
No native provider prompt, scheduler execution or physical phone acceptance is
claimed. No public macOS or iOS publication is planned.
