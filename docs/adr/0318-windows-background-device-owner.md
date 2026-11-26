# ADR 0318. Separate the Windows device owner from the interactive console

- Date: 2026-11-26
- Status: Accepted direction. Migration and device acceptance in progress.
- Related: [Windows identity verification](0316-post-login-device-identity.md), [enrolled network startup](0317-resume-an-enrolled-windows-network-node.md), [physical console](0306-physical-lock-screen-remote.md), [elevated updates](0310-windows-elevated-release-updates.md)

## Context

Closing an interactive Windows session ends processes started in that session. The network-only resume executable can use an enrolled identity but cannot serve the GUI or Remote RPCs. A full daemon started through the normal provisioning path can create state when it is pointed at the wrong profile.

## Decision

After the enrolled user's first login, run one full Shared Rust daemon in an independent background logon under that user's account. It owns the TPM network key, verified roster, grants, QUIC and local RPC listener. Resume by the full device ID from the installed account profile. Acquire the existing exclusive ownership lock before opening the key, verify membership before network startup and keep the same signer throughout. Keep initial enrollment available to a new interactive installation.

The interactive GUI attaches using the existing per-account local IPC. The network helper and the Remote host run under SCM with separate, narrowly scoped operations. The network helper holds the Wintun adapter. The Remote service follows the physical console and starts capture only for authorized sessions. End capture after the last viewer's 30-second idle window. Session-bound UI automation needs a separate interactive adapter before switching the GUI's default launch path.

## Alternatives

A second network-only node cannot serve desktop RPC. Giving each service its own key or policy creates competing identities and authorization decisions. Starting the full daemon under an administrative token would give remote application and terminal operations that token's privileges.

## Consequences

Roll out the full daemon's strict startup before changing the installed startup path. A supported background launch must demonstrate the actual non-elevated token and successful networking after sign-out. Keep the interactive startup available until the service and account lifecycle are validated. Remote follows ADR 0306 for supported surfaces and operating-system authentication.

## Verification

Test invalid full IDs, missing state, ownership contention, mismatched membership and retention of grants. Exercise an authorized and denied request through the full daemon. Device acceptance additionally requires an independent account logon, GUI reconnection, authenticated peer traffic after true sign-out, and observation of the physical console with an authorized viewer. Record installation, runtime and viewer results independently.

The first Windows implementation adds the full daemon resume entry and a demand-start Wintun service. The updater pins the service to the protected version and original Windows account. The daemon verifies the pipe client PID against SCM. The helper checks the pipe owner and the server image before taking the interface plan. A missing service prevents background vnet startup. Interactive installations without SCM retain the previous UAC path.

Windows x64 release linking passed for the daemon and helper. Isolated process tests covered malformed IDs, conflicting directories, a missing profile and a held owner lock. A local pipe owned by the right account but served by the test binary was rejected. A synthetic signed roster resumed with its policy intact. A different identity was refused before pending-operation recovery. Six unrelated Multica store cases failed on both the candidate and its source baseline on Windows.

Service installation, adapter traffic, ordinary-token activation after logoff, and Remote frame and pointer acceptance remain unverified. The GUI continues to use its existing startup path during this phase.

Integration with [ADR 0306](0306-physical-lock-screen-remote.md) retains both Windows checks. Background daemon startup checks its process token, while the current Remote host checks its physical console before admitting and servicing a viewer. Verify both on the same Windows revision, and build the shared daemon and test the Apple client on macOS from that revision.

The integrated Multica store tests use an absolute workspace on each host platform so Windows exercises reservation ownership and parent traversal with the same assertions as macOS.
