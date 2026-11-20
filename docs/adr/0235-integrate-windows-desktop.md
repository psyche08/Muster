# ADR 0235. Integrate the Windows desktop with the current shared runtime

- Date: 2026-11-20
- Status: In development
- Related: ADR 0227, ADR 0228 and the Windows decisions in ADRs 0232 through 0234

## Context

The Windows desktop branch and main diverged after a common base. Their ADR
numbers overlap. The installed 1.1.50 desktop uses the earlier shared runtime.
Its packaging script depends on a Unix cross-build host.

## Decision

Merge both histories, retain the Windows decisions under available numbers,
and preserve the desktop upgrade code, per-user TPM keys and roster. Build
the x64 payload on Windows against the integrated Rust sources. Use the
current catalog entry to reopen a conversation after a process restart.
Keep device authorization and subscription outcomes in Shared Rust. Surface
the diagnostic returned by a failed catalog snapshot. Render the core's
agent sections, chat actions, account permissions and startup decision in
the Windows shell. Align the dependency map with two existing imports from
before the fork: ACP uses shared agent helpers and FFI uses the log directory
from muster-paths. Initialize the same bounded desktop application log on
Windows. Use native Windows controls for these interactions.

## Alternatives

Shipping the old branch again omits subsequent shared runtime changes.
Starting a fresh network would invalidate the upgrade acceptance scenario.

## Consequences

The native build needs the MSVC toolchain, .NET SDK, protobuf compiler and
WiX. Device acceptance requires the existing interactive Windows account.
Presence signing and administrator enrollment have separate prerequisites.

## Verification

Check public content, ADRs, Windows IPC, ConPTY, shared runtime startup and
catalog behavior. Build the MSI and compare the extracted payload against the
build inputs. Check the installed version, daemon, adapter and member identity
after an in-place upgrade. Record read-only chat recovery, remote catalog
access, presence verification and enrollment independently.
