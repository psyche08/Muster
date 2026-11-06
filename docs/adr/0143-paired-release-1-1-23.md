# ADR 0143: Publish Apple 1.1.23 (113) from main and install the fleet

- Date: 2026-11-06
- Status: Accepted; release execution pending
- Related: ADR 0138; ADR 0139; ADR 0140; ADR 0142; DESKTOP_UPDATES.md

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
