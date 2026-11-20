# ADR 0240. Distribute Windows as a per-user portable package

- Date: 2026-11-20
- Status: In development
- Related: supersedes the MSI distribution decision in ADR 0233. Retains the protected Wintun boundary in ADR 0232 and session behavior in ADRs 0236 and 0239.

## Context

Replacing a machine-wide installation interrupts the daemon and tunnel even for
desktop-only changes. A full Windows distribution must support updates by its
interactive user and retain the device identity across versions.

## Decision

Build a complete x64 ZIP. Stage and verify each version under a private user
directory, then switch a fixed shortcut. Keep earlier versions while in use.
Register Chrome's native host for the current user. Updating files does not
stop processes or start the driver. A new GUI can reuse a running daemon.
Resolve the packet helper and its pipe namespace from the system Program Files
known folder. Provision the helper and pinned DLL there separately when needed.

## Alternatives

Replacing running binaries in place can leave a partial installation. Elevating
a helper from the user's writable package would cross the driver trust boundary.
The machine-wide MSI also couples application updates to administrator access.

## Consequences

An operator can prepare an update while the network remains online. Existing
processes adopt it when restarted. First-time driver provisioning and adapter
startup still require explicit administrator participation. Browser users load
the packaged extension in Chrome. Keep user data outside the release tree.

## Verification

Compare ZIP contents with the build inputs. Test first deployment, repeat
deployment, a damaged file and switching the shortcut with older binaries in
use. Verify the per-user browser registration, GUI single instance and daemon
and tunnel continuity on Windows. Report compilation, package signature and
physical network acceptance separately.

On Windows, the first test exposed the default Windows PowerShell script
policy and a missing hashing cmdlet. The ZIP includes a command launcher
using process-scoped RemoteSigned and the updater hashes with .NET. The
updated entry point deployed a full copy of the preceding version under a
standard token while the existing GUI, daemon and packet helper stayed up.

## Identify the running desktop and node

A portable desktop can be updated while an earlier daemon continues serving
the network. Windows previously displayed the daemon version beside the
platform name. Settings will label the running GUI assembly version and the
local node's reported version and commit separately. Device details will show
the selected member's reported version and commit, as on macOS. Missing fields
remain visibly unavailable. Reading the active shortcut or a package manifest
cannot identify either running process after an update.

Use the Shared Rust view for node and member metadata. No restart or inferred
upgrade status is needed to present these values. Verify a newer desktop
connected to an older daemon through the normal shortcut, compare the screen
to local RPC, and check a selected member and missing metadata independently.

The installed Windows CLI also rejected a top-level `--version`. Let its
argument parser report the portable release stamp offline. An unstamped Windows
development build reports its CLI crate version. The existing `info` command
continues to obtain the node version over RPC. Verify both commands against a
newly packaged CLI connected to a preceding daemon.

## Ship the managed terminal runtime

The Windows host from ADR 0033 can reconnect a conversation shell across
daemon restarts, but depends on a separately provisioned MSYS2 installation.
Include an executable-adjacent MSYS2 tree in the native and cross-built ZIP.
Select files from digest-pinned MSYS2 archives and include terminal data,
interactive shell file utilities and license notices. Include the matching
upstream source packages and build recipes in the outer ZIP so recipients
can obtain them offline without installing source into each running version.
Do not copy files from a developer's live MSYS2 tree.

Verify every runtime file through the existing release manifest and update
path. Test production runtime discovery without a configured external runtime,
with an ordinary Windows PATH, including a shell file command and recovery
through two separate host processes. Report installed-node activation and
Wintun separately.
