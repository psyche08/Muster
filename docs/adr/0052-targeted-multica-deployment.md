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

### Device acceptance on 2026-10-26

- **Source/build:** clean source `31a2fa52f995b3b0deacd951d2c1005dafd056e3`;
  Rust daemon/CLI/browser and macOS FFI Release builds passed. The generated
  Xcode project carries 1.0.95 (86), matching its spec; macOS Release passed.
  The initial provenance gate correctly refused the uncommitted generated
  project; it passed after committing and rebuilding. No application logic
  changed in this deployment turn. Public-content and ADR gates passed.
- **Signing/package:** Developer ID and Presence entitlements verified, clean
  source stamp and all release ancestry/fingerprints passed. Velopack 1.2.158
  retained updater metadata. Apple notarization
  `90a618c3-1bcf-44fb-b891-41e16930c437` was Accepted; stapling, strict deep
  signature verification and Gatekeeper passed on both build and target hosts.
  ZIP SHA-256: `b44250b53b85536dbeabf7eed75f438cb75b192bbabfc77d16d7552be61ccc88`.
  Installed daemon SHA-256:
  `f1fe00b463ac92888c6a32e07c0a2c9650ac3dbd0a6f56bd8832ae3b7e952eb7`.
- **Installation:** one target desktop acquired the Rust update lease, exited
  normally, replaced only the application bundle and launched through Launch
  Services after ten seconds. No old application backup was made. Its device
  identity remained unchanged; Accessibility, relay registration and Vnet
  recovered on 1.0.95. Multica scheduling was paused during daemon replacement.
- **Data migration:** the existing container was idle before the offline move.
  On installed Apple container CLI 1.4.1, stopped the container and runtime,
  preserved the writable root disk, and changed only the workspace bind source
  and destination in `config.json` and `runtime-configuration.json`. Fresh
  inventory after runtime restart confirmed both ends at `~/Documents/Multica`.
  All 320 files (776,720,148 bytes before path edits) were hash-checked; exactly
  four JSON metadata files received the root-path substitution. The final total
  is 776,720,020 bytes. Nine Git repositories passed connectivity checks. The
  old host path and empty guest mountpoint are absent, with no compatibility
  symlink. Guest daemon identity, login configuration and tool hashes matched
  before and after the migration. Existing two workspaces and four agents remain.
- **Supervision:** the installed local-owner RPC adopted the existing container,
  persisted the new data/mount root and keep-alive in mode-0600 configuration.
  After deliberate idle stops, the daemon recovered in 4.79 seconds and the
  container plus daemon in 16.73 seconds, retaining the daemon identity. The
  first probe stopped on the supervisor's expected busy response; the revised
  read-only polling tolerated that response and completed both tests.
- **Bridge:** container-to-host ACP `initialize` returned protocol version 1
  and Muster identity after migration and restart. The probe kept stdin open
  until the response; immediate EOF closes the legacy forwarding connection.
  No session was created and no prompt was sent.
- **Installed UI:** read-only AX inspection of the installed window showed the
  segment order `Agents`, `Terminals`, `Multica`, with no `Chat` segment.
- **Scope:** no public desktop feed, iOS OTA or TestFlight publication occurred.
  No provider task/prompt was submitted; scheduler task execution, WebKit login
  and physical-phone acceptance are not claimed. Prior conversation transcripts
  are historical records and were not rewritten by the directory migration.

Local ignored evidence is retained under `build/deploy95/`; it is not public
source content. The attended metadata migration is specific to the inspected
runtime schema and is not a new generic application migration API.
