# ADR 0327: Build the desktop release from a shared main commit

- Date: 2026-11-27
- Status: Accepted. Apple packages published. Windows public distribution deferred pending signing.
- Related: [Windows portable distribution](0240-windows-portable-distribution.md), [console selection](0326-select-console-by-session.md), [desktop updates](../DESKTOP_UPDATES.md)

## Context

Several development hosts held different local main references. The original console worker commit had already landed through a cherry-pick followed by fixes. The predecessor and the integrated commit had different identifiers.

## Decision

Join the original worker lineage to main with an unchanged source tree. Build Apple 1.1.48 (138) and Windows x64 1.1.128 natively from the same merged commit. Keep the Apple version in the XcodeGen project and regenerate the committed project. Use the established desktop and registered-device release channels.

## Alternatives

Shipping packages from separate feature branches would make the installed source harder to reproduce. Reapplying the old worker patch would discard subsequent integration fixes.

## Consequences

Both desktop builds can report the same source revision. Packaging, publication and installation have independent acceptance results. The existing physical-login-screen and disconnected-viewer keyboard limitations still apply.

## Verification

Check the ancestry of development branch heads, the embedded commit, package versions, platform signing and release artifacts. Read back published hashes and verify the running version and identity on each installation target.

## Release verification

PR #359 landed the common source at `45e088cc9789663af668303438f9edc05ac7e38d`. All three development hosts' branch tips at the release cut are ancestors of that commit. The ancestry merge for the original worker retained the integrated source tree byte for byte. GitHub reported no CI checks for the release PR.

- Apple Silicon macOS 1.1.48 (138) and the registered-device iOS OTA were built on the Mac build host. Apple accepted the Developer ID portable for notarization. The stapled app passed Gatekeeper. The Velopack full update contained matching executable bytes, and the feed signature verified with the embedded update public key. The GitHub release tag resolves to the build commit.
- Windows 1.1.128 was built on the Windows host. The portable package reused verified media components whose source was unchanged. The installer checked all 99 manifest entries. The installed daemon contained the full release commit. The GUI and daemon ran in the existing user session, the automatic stream service ran in session zero, and the enrolled identity matched. Public Windows distribution awaits Authenticode signing.
- Three signed-in Macs ran the release GUI and daemon. The update resume argument `--muster-updated` let the app restart its existing system LaunchAgent. Each daemon reported the build commit and its original Secure Enclave identity, registered relay, and active Vnet.
- A fourth Mac received the same signed and notarized bundle at the login window. The installed daemon hash and bundle signature matched the release. Runtime and identity verification on this Mac require a user login.
- This release run verified installation and startup. Physical remote-input acceptance and on-device iOS installation were outside the release run.

The macOS portable SHA-256 is `16987165d6f1bc3379c44cf975aba02d486e9edf8b8d554ce345272c4cf511bb`. The registered-device IPA SHA-256 is `53561b8c7dc3d840a3f4d37bfa204641513d547a29d8d370dd52507798572b22`. The local Windows portable SHA-256 is `369a28bc5463af5a061359e11787051d9f0e483441764c1a37902ca0200a48fe`.
