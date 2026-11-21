# ADR 0282: Build the paired desktop engines from Muster source

- Date: 2026-11-22
- Status: Implemented; live screen and mouse acceptance pending
- Related: [ADR 0242](0242-muster-remote-desktop-engines.md), [ADR 0280](0280-native-paired-desktop.md)

## Context

The Windows viewer calls the vendored Moonlight C API. The macOS package accepts a separately supplied Sunshine.app and can finish without including a host. A development build also resolved libraries from its builder's temporary installation.

## Decision

Build the macOS host from the checked-in downstream Sunshine code. Materialize the pinned build dependencies outside the vendored source tree. Use the same source build entry point for the native application and the development bundle. Check runtime dependencies before embedding an engine. The Windows native packager always compiles the viewer linked to our vendored Moonlight C code. Both media roles use the owner-limited private process boundary of ADR 0280. Shared Rust continues to own pairing, per-session permission, transport and revocation.

## Alternatives

A preinstalled upstream application has a separate configuration and authorization model. Copying a discretionary prebuilt host lets a release silently omit the feature or include a binary built from different code.

## Consequences

Building the desktop product needs host build tools and the native dependencies. Repeated builds can reuse a build cache. The installed product should obtain both roles from its own package. Screen capture and input still require the host user's OS permissions.

## Verification

Build both roles from the checkout. Verify packaged files, runtime dependency resolution, signature and failure on missing components. Exercise a private host handshake and the client decoder. Verify actual capture, input, and revocation separately on a consenting unlocked host.

The native Windows packager compiled the Moonlight viewer, passed all three
CTest checks, and produced a complete unsigned x64 ZIP. The per-user updater
verified 59 payload hashes and switched the shortcut. The existing GUI and
daemon continued running their previous versions.

An arm64 Mac built a complete MusterDev.app with the capture host on two
successive builds. The repeat build exercised replacement of read-only Swift
resource files. Both the app and nested host passed strict ad-hoc signature
verification. All 20 host load commands referenced macOS system libraries.
The bundled daemon reported host availability over local RPC. A private
handshake accepted the selected viewer, denied another certificate and pairing,
restricted launch to the desktop, and stopped the host on owner EOF. The
matching source archive was about 651 MiB. The macOS Xcode project parsed and
its capture phase matched the specification. A Developer ID archive,
notarization, and publication were not performed. The console was locked and
capture permission was absent, so real pixels and mouse delivery remain pending.
