# ADR 0021: Retry the complete desktop startup through Shared Rust

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](../PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

- Date: 2026-10-22
- Status: Accepted
- Related: ARCHITECTURE.md thin platform shells; ADR 0019/0020 release acceptance

## Context

The owner reported Mac-A displaying "Muster could not start" with a missing
musterd.sock; Try again did not recover, while restarting the GUI did. During
the third unpublished 1.0.82 candidate installation, the update lease completed
but daemon startup was not confirmed. A read-only launchctl lookup found no
musterd job and the socket did not accept connections. This is local startup
failure, not evidence of a relay/peer network fault.

The macOS App's initial SwiftUI task registered/repaired the service and waited
for its socket. MusterModel.retry only recreated the core with the same socket;
it never repeated that preparation. Startup orchestration was also duplicated
outside Shared Rust. Initial preparation and manual retry must share one path.

## Decision

Shared Rust owns a serialized, bounded desktop preparation operation exposed
through one narrow ABI entry point. Probe the socket first; a healthy service
needs no registration action. Otherwise request native registration, wait up to
five seconds, request one stopped-service recovery, then wait up to ten seconds.
Manual/custom daemon directories never invoke service-management callbacks.
Busy, approval-required, native errors and exhausted readiness return explicit
errors. Cancellation releases the Rust single-flight admission.

The macOS adapter performs only SMAppService and launchctl operations requested
by Rust. It never stops a running job: an existing stopped job is kickstarted
without -k; an absent job may have its stale SMAppService registration repaired.
Required user approval is reported, never bypassed. No daemon/GUI periodic
restart, Keychain operation, or change to other applications is introduced.

The UI stores the preparation closure as lifecycle wiring. Initial start and
Try again invoke it off the main thread, then construct the core. While that
operation is in flight it renders Starting and rejects duplicate starts. iOS
and the development client retain their existing direct core startup.

## Alternatives

Calling only core construction again repeats the missing socket error. Adding
another Swift retry loop would keep business recovery policy outside Rust.
Unconditionally unregistering/restarting the daemon could interrupt existing
operations; recovery must inspect native job existence/running state.

## Consequences

Try again can restore an absent service without restarting the GUI. Startup is
bounded and the UI remains responsive while preparing. Running-but-unreachable
services are diagnosed rather than killed. Native registration errors remain
visible and may require the owner's normal Login Items approval.

## Verification

Inject missing/delayed sockets, failed registration, recovery success/exhaustion,
custom-directory isolation, concurrent preparation and cancellation. Test the
C ABI and build both Apple apps. Device acceptance must separately verify
Mac-A startup/retry, the pending Claude answer receipt, installation on all three
Macs and iOS OTA. Earlier source/signature checks do not satisfy these gates.

### Source verification (2026-10-22)

- Four socket fault/concurrency tests and three C ABI tests passed.
- Swift `DesktopStartupTests` passed: repeated clicks are coalesced and a
  failed preparation is invoked again by Try again.
- Strict Clippy for app-core and ffi (all targets), the 17-crate layer gate,
  and whitespace checks passed. The local Swift debug linker reports cached
  object deployment-target warnings; this is not an Apple release build result.
- Native service-registration behavior, Apple release builds, signing and
  device startup/answer acceptance remain pending at this source commit.

The first macOS release compilation rejected a static-method reference as a C
function pointer. The callback now uses a noncapturing literal closure; the ABI
and recovery policy are unchanged. Rebuilding both Apple targets is required.


### Reproduced deployment trigger and device evidence (2026-10-22)

The final candidate installation reproduced the missing socket with a concrete
cause: its release driver exported MUSTER_DIR for local CLI routing, and `open`
passed that variable into the GUI. A narrowly filtered process-environment
inspection confirmed the production path was present. The app deliberately
treats an explicit MUSTER_DIR as a manually managed daemon, so it skipped
registration and ended its custom-socket wait at 5001 ms. Normal GUI relaunch
without that variable recovered the service; this explains this reproduced
failure, not every historical missing-socket report.

Keep custom-directory isolation intact. The release driver now removes only
MUSTER_DIR from the GUI launch environment (CLI routing remains explicit), and
waits for the old GUI to exit before replacing the bundle. The repository's
production `install-mac.sh` launch also removes MUSTER_DIR. No GUI trust policy,
Login Items approval, console protection or Keychain boundary is bypassed.

- App source 366a199fa346e6288578023f79d96c0dc365730b built macOS Release
  and iOS archive successfully. Public 1.0.82 (73) uses that source; the later
  installer/docs evidence commit does not change the application binaries.
- Notary 6a38fa3f-6cc6-4304-a124-1840bb3b0881 was Accepted. Strict signature,
  Presence entitlements, stapling, Gatekeeper and clean-source fingerprint
  verification passed before publication and again on the public download.
- Mac-A installed the final daemon, then recovered after normal GUI relaunch
  with MUSTER_DIR absent. Its actual window showed the session list. Mac-B and
  Mac-C installed with the corrected driver and started automatically. All
  three retained identity and Accessibility, registered relay and brought Vnet up.
- All six directed RPC checks and six virtual-IP ping checks passed. This is
  post-install connectivity, not a new kernel/socket network fault injection.
- The retry fault/coalescing path has source/Swift regression coverage. The
  production daemon was not deliberately stopped again solely to force a Try
  again screen; an installed managed-service retry fault test is still unclaimed.
- The pending Claude card C received its answer and continued on the final
  Mac-A build; see ADR 0020. No unknown-outcome chat/answer retry occurred.
- GitHub Actions remains disabled. Physical iPhone installation/UI, new Locked
  Use acceptance, and a new Mac-C consent grant were not performed in this
  release; the preceding approval acceptance remains versioned in ADR 0018.
