# ADR 0310: Install Windows release files with UAC

- Date: 2026-11-25
- Status: Accepted. Deployment acceptance pending.
- Related: supersedes the per-user executable destination in [ADR 0240](0240-windows-portable-distribution.md). See [ADR 0306](0306-physical-lock-screen-remote.md) for the independent physical-console work.

## Context

The Windows ZIP updater places executables and libraries under a user-writable directory. Windows releases now require administrator approval for updates. A future machine service needs a machine-managed installation.

## Decision

Keep the full ZIP and the existing update entry point. Install each release in an administrator-controlled version directory under the x64 Program Files known folder. The invoking user reads the package and sends the bytes through a one-way local pipe. The elevated reader connects with anonymous impersonation, verifies the pinned manifest and file digests, and publishes the verified directory by rename. It accepts no executable path or destination from the client. The invoking user then switches their existing shortcut and registers their Chrome native host. The optional shortcut location remains per user. Keep development builds unprivileged.

## Alternatives

Elevating the old per-user copy would leave installed code writable by the account. Running the entire updater as the approving administrator could update the wrong user's shortcut and registry. An MSI would replace the existing distribution format.

## Consequences

Updates require a local administrator. Existing processes may finish using their previous version and everyday launches use the interactive user's identity. Executable installation does not grant remote access or install a capture service. The operator verifies the published ZIP digest before running it. Package content hashes detect changes during installation.

## Verification

Exercise the package entry point, manifest pin, staged file hashes and fixed machine destination. Check that a cancelled elevation leaves the old per-user activation intact. Test a reapplication, a corrupted payload and rollback separately from service installation and remote desktop acceptance.

Windows packaging ran with previously built x64 application binaries and pinned terminal inputs. A standard-user harness exercised the actual pipe transfer, verified staging, same-version reapplication and rejection of a corrupt payload or an altered installed version. Simulated UAC cancellation and manifest tampering left per-user activation untouched. The production ACL check accepted the tested Program Files parent and rejected the user-owned daemon. Actual UAC consent, alternate administrator credentials, production activation and rollback remain to be exercised on the next device update.
