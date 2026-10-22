# ADR 0011: Keep Linux CI within platform and storage limits

- Date: 2026-10-22
- Status: Accepted
- Related: ADR 0010 integration; `.github/workflows/muster.yml`

## Context

PR 168 run 39698443697 passed Linux formatting, layer, Clippy and workspace
unit/integration tests. At the following network-namespace acceptance step,
GitHub reported `System.IO.IOException: No space left on device`, including
failure to write the runner's diagnostic log. The job failed and its logs could
not be retrieved. The macOS full build/tests passed. This is an observed runner
storage failure, not evidence of a failing send or network assertion.

## Decision

Disable debug symbols and incremental compilation for disposable Linux CI dev
and test artifacts using job-scoped Cargo environment variables. Keep all tests,
strict linting and the privileged vnet namespace acceptance. The namespace
script receives the same environment through the existing `sudo -E` call.
Local developer and signed release profiles stay unchanged.

Once the generated Swift binding gate passed, run 39699645359 exposed an
unconditional MarkdownUI dependency in the Apple package: its NetworkImage
transitive dependency imports SwiftUI, unavailable on Linux. The MusterUI
sources already guard Apple UI with `canImport(SwiftUI)`. Give the MarkdownUI
product dependency the corresponding macOS/iOS platform condition. Keep the
existing Linux core tests and both Apple platforms' UI dependency intact.

## Alternatives

Blind retries retain the same artifact budget. Removing tests weakens acceptance.
Deleting unrelated runner SDKs hides the dependency on runner image contents.
Changing the workspace's developer/release profiles affects unrelated users.

## Consequences

CI artifacts consume less disk; symbolic debugging of those ephemeral binaries
is reduced. Panic/assertion diagnostics remain, and local builds retain their
original debug settings. A passing namespace step is still required; the earlier
storage failure does not count as acceptance.

## Verification

Review job scope and Cargo variable propagation. Validate the Swift package
manifest and its macOS/iOS dependency condition with `swift package dump-package`. Run all four CI jobs on the
final commit, including Linux workspace tests and vnet namespaces. No device
runtime, signing, installation or physical network acceptance is implied.

The next Swift build exposed the same missing platform boundary for the Mach
stack sampler. Scope its C target dependency, Swift sampling implementation
and native-thread tests to macOS/iOS. Keep the portable counter/delta policy
and its three tests available on Linux. No fake Linux stack samples or Mach
stubs are introduced, and the Apple implementation is unchanged.

Complete the existing `canImport(SwiftUI)` boundary for the three unguarded UI
files (BrowserView, ChatStyle, ChatComposer). Linux still compiles portable UI
presentation helpers and all applicable core tests. The Apple code is unchanged.

A macOS CI run also observed the browser-host test reading socket mode 0755
between `bind` and the immediately following `chmod(0600)`. Production admits
requests only after chmod. Move the unchanged exact-0600 assertion after the
host routes the test Ping, which witnesses completed initialization, before
answering it. No permission check is removed and no timing sleep is introduced.

Local validation: all eight browser bridge tests pass after the readiness
change; the three guarded UI files pass Swift syntax parsing. Package manifest
inspection confirms both OS-only dependencies are limited to macOS/iOS.
