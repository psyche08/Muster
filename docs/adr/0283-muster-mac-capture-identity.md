# ADR 0283: Authorize the enclosing Muster app for paired desktop capture

- Status: Accepted; installed device verification pending
- Date: 2026-11-22
- Related: [ADR 0125](0125-screen-recording-is-the-bundles-and-only-a-restart-takes-it.md), [ADR 0280](0280-native-paired-desktop.md). Supersedes the macOS helper bundle layout in [ADR 0282](0282-source-built-desktop-engines.md).

## Context

A separate Sunshine application bundle inside Muster gave macOS a second privacy identity. The owner should install and authorize only Muster.

## Decision

Compile the checked-in capture engine as a private Mach-O tool beside musterd in the enclosing app's Contents/MacOS. This is the bundle layout used by Muster's existing TCC-dependent daemon. Give the tool no application bundle, embedded application plist or independent permission prompt. The foreground device view requests Screen Recording on a share action and Accessibility only for mouse control. The tool checks its effective access before use. Shared Rust still owns peer admission, input grants and termination. Keep the media engine in its private process with its own license, notices and corresponding source.

## Alternatives

Renaming the nested app retains a separate bundle identity. Linking the whole engine into the live UI introduces process-wide signal, working-directory and lifecycle changes.

## Consequences

Development and release builds both remove the obsolete nested app from the bundle they are assembling. Development uses its own outer application identity. Production approval is granted to Muster. A permission change may require reopening the app.

## Verification

Build twice including over a bundle with the old layout. Inspect the enclosing identity from inside the packaged Mach-O, signatures, runtime dependencies and absence of a nested capture app. Exercise permission denial and the selected-client handshake. On a signed, installed build verify System Settings shows Muster as the sole required grant, read-only viewing without Accessibility, mouse delivery with Accessibility, and revocation.

An arm64 Mac built the development app over the former nested bundle and
rebuilt it in place. Both packages passed strict deep ad hoc signature
verification and contained no Sunshine.app. The capture executable reported
the outer development bundle identifier at runtime and had no embedded
application plist. Its 20 load commands referenced macOS libraries. The
bundled daemon reported host availability over its local RPC. The packaged
host accepted the selected TLS client, denied another certificate, pairing
and other apps, and stopped on owner EOF. The console was locked and Screen
Recording was unavailable. Installed Developer ID permission attribution,
live frames, mouse input and user-initiated revocation remain unverified.
