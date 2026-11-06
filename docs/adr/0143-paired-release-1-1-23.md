# ADR 0143: Publish Apple 1.1.23 (113) from main and install the fleet

- Date: 2026-11-06
- Status: Accepted; published and installed on three Macs; iPhone installation pending
- Related: ADR 0144 (supersedes physical-path-only installation); ADR 0138; ADR 0139; ADR 0140; ADR 0142; DESKTOP_UPDATES.md

## Context

The owner requests installation on all devices and publication to the OTA page,
explicitly built from main. Session-loading and live-synchronization changes
(ADR 0142) and the earlier transcript-layout change are merged but not installed.
The existing release is Apple 1.1.22 (112).

## Decision

Bump both Apple targets to 1.1.23 (113), regenerate the Xcode project, and land
this decision on main before building. Build only a clean checkout of that main
merge commit. Verify the daemon's embedded commit, release ancestry and shipped
fingerprints. Preserve the shell's thread-specific Cargo output directory and
keep release artifacts and evidence outside transient build directories.

Build and sign the macOS bundle and paired registered-device iOS app. Package the
macOS portable and full update with Velopack 1.2.158, notarize/staple, validate
signatures and Presence entitlements, and sign the desktop feed. Publish packages
before aliases/feeds/pages to the existing private release destination. Verify
public HTTPS package hashes, feed signature, manifest, versions and cache policy.

Install the same verified portable on the three enrolled Macs with the guarded
Rust update lease, preserved identity, and scheduler-idle checks. Use physical
network paths for remote installation. Install the paired iOS app on the available
registered physical iPhone and read back its version; OTA publication alone does
not prove installation. Do not submit to TestFlight or external testing.

## Alternatives

Building from the former fix branch does not meet the explicit main requirement.
Reusing 1.1.22 would hide changed bytes and prevent normal updates. Installing
plain unsigned bundles would lose the established signing/update contract.

## Consequences

Installation restarts Muster and its daemon after protected operations quiesce.
Unavailable devices or locked signing credentials remain explicit pending steps;
Keychain unlocking is interactive and belongs to the user. Deployment and runtime
checks do not by themselves establish a real provider conversation latency result.

## Verification

Check generated version consistency, ADR and public-content rules before landing.
Record the main commit, source gates, build, signing/notarization, package/feed
hashes, public readback, per-device version/identity and physical iPhone result
separately after execution. No release or installation result is claimed yet.

### Execution evidence (2026-11-06)

- Source: clean main merge `f0f7f0833eac1157a1cbcd51c7b564c23af73c39`,
  containing ADR 0142 and the paired version bump. Both Apple targets are
  1.1.23 (113). Release ancestry, shipped fingerprints and daemon stamp passed.
- Build: local release Rust binaries and macOS/iOS FFI completed; macOS Release
  build and iOS archive passed with Xcode 27.2. The shell-provided Cargo output
  directory was preserved. Artifacts and evidence are outside transient caches.
- Signing: local Developer ID, Presence provisioning, deep strict signatures,
  notarization, stapling and Gatekeeper passed. Apple accepted submission
  `a43ae1cb-1112-454c-a88c-c6c0bd0ee696`. Velopack 1.2.158 full-package and
  portable CDHashes match. The feed key stayed on its existing host; the returned
  signature verifies against the compiled release public key.
- Publication: fourteen files published. Public HTTPS readback verified every
  hash, the page's installation link through its manifest to the exact IPA,
  desktop feed signature, and no-store/no-cache/must-revalidate/max-age=0 headers.
  macOS portable SHA-256:
  `a4cdbbf47b0b009e1b2e3fbc9bfe62a07db4c648efd4e699aabcd6695d1ddf18`.
  iOS IPA SHA-256:
  `15efc755ad2840e2034b9ce8b4184c876c767c92b5a9c21b08bf137ed941d7ec`.
- Installation/runtime: three enrolled Macs independently installed the same
  verified portable under Rust update leases. All report 1.1.23, the expected
  daemon commit/hash, unchanged Secure Enclave identity and fingerprint, granted
  Accessibility, registered relay and active Vnet. One managed scheduler was
  stopped only after an idle check and recovered; an already stopped container
  stayed stopped. No old program backup was retained. See ADR 0144.
- iPhone: signed registered-device IPA published (profile valid through
  2027-10-25), but direct installation could not begin: CoreDevice error 4016,
  unavailable trusted device connection. The operator was asked to connect and
  unlock the phone. No physical iPhone installation or runtime success is claimed.
- Retention: the guarded cleanup preview identifies 45 superseded public files.
  Automatic approval review rejected deletion because explicit cleanup approval
  was absent. Those files remain; no latest-only or retired-URL acceptance is
  claimed. New release aliases and signatures are verified independently.
- No TestFlight/external distribution, App Store submission, GitHub release,
  unattended updater acceptance or real-provider Mac/iPhone latency acceptance
  was performed. Existing source regression evidence remains in ADR 0142.
