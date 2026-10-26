# ADR 0052: Deploy Multica management and relocate the existing workspace root

- Date: 2026-10-26
- Status: Accepted
- Related: [ADR 0051](0051-multica-management-and-live-catalog.md), [deployment runbook](../DESKTOP_UPDATES.md)

## Context

The owner requested a local build from the existing repository, installation on
one desktop, and migration of its existing Multica container data to the default
Documents/Multica directory. The owner explicitly rejects retaining the old path
or a compatibility symlink. The existing container holds login state and tools
in its writable filesystem, separately from the bind-mounted workspace data.

## Decision

Build version 1.0.95 (86) from clean committed source and deploy only to the
requested desktop through its physical network and the Rust update lease.
Keep the Developer ID identity and presence entitlements. This is a targeted
installation, not a public feed publication; the published macOS/iOS pair stays
unchanged.

Perform an attended, offline migration of the existing container and workspace
root. Preserve its writable disk, daemon identity and credentials. Verify idle
execution before stopping it. Move the data atomically, update both bind-mount
ends and workspace references, and require the old path to be absent. Do not
invoke ADR 0051's compatibility-link relocation for this operation. Enable the
Rust supervisor only after configuration and startup checks succeed. Any
version-specific container metadata adjustment must be checked against the
installed runtime and verified by a fresh inventory after service restart.

## Alternatives

A compatibility link was rejected by the owner. Reinstalling the container would
risk discarding its login and tools. Reusing 1.0.94 for different source would
obscure deployed provenance. Fleet publication would exceed this targeted
installation request.

## Consequences

The requested desktop advances independently of the published pair. Existing
workspaces must be validated at their new absolute root; credentials stay on
that device. No old application bundle is retained. Other containers and other
devices are outside the migration scope.

## Verification

Pending: signed build, notarization, source provenance, guarded installation,
unchanged device identity, data manifest comparison, new host/guest mounts,
absence of the old path, daemon identity and supervisor recovery. The earlier
source tests are recorded in ADR 0051. No provider task is submitted by this
migration. Publication and physical phone acceptance are outside this request.
