# ADR 0152: Install Apple 1.1.26 (116) for desktop session acceptance

- Date: 2026-11-06
- Status: Accepted; installation and acceptance pending
- Related: [ADR 0150](0150-remote-desktop-session-creation.md) (desktop
  session creation), [ADR 0151](0151-session-reconciliation-and-retry-evidence.md),
  [ADR 0144](0144-local-release-and-independent-installation.md) (local build
  and independent installation jobs), [ADR 0143](0143-paired-release-1-1-23.md),
  [DESKTOP_UPDATES](../DESKTOP_UPDATES.md)

## Context

The operator asked for desktop session creation (ADR 0150) to be deployed and
tested on the devices. No device runs it: the three Macs and the published feed
are on 1.1.24 (114).

`project.yml` already says 1.1.25 (115). Two 1.1.25 candidates were built and
notarized from earlier commits (`7fd7480` and `c5a8ff6`) for session-loading
acceptance. Neither was installed or published, and neither contains ADR 0150
or ADR 0151. A third, different build under the same number would make the
installed version ambiguous. The installer replaces any bundle whose daemon hash
differs, so two installers could swap each other's 1.1.25 without notice.

When this was decided, the two remote Macs had been unreachable for three hours
over the virtual network, their physical addresses and the reverse tunnel. The
operator's own Mac and the iPhone were reachable.

## Decision

- **Version.** Set `MARKETING_VERSION: 1.1.26` and `CURRENT_PROJECT_VERSION:
  116` in `project.yml`, and regenerate the committed Xcode project with
  xcodegen 2.46.0. The 1.1.25 candidates are superseded and must not be
  installed over 1.1.26.
- **Source.** Build only a clean, detached checkout of the merge commit that
  lands this change. The daemon's build stamp must equal that commit.
- **Build and signing.** These follow the ADR 0143 process:
  - release Rust binaries;
  - the Apple FFI;
  - a Developer ID macOS build with the Presence provisioning profile;
  - `verify-release.sh` and the Presence signing check;
  - a Velopack 1.2.158 portable;
  - notarization, stapling, a strict signature check and Gatekeeper.
- **Installation.** Use the ADR 0144 one-shot job in the user's GUI launchd
  domain. It does the following:
  - holds the Rust update lease;
  - checks the scheduler is idle;
  - exchanges the bundle atomically and keeps no copy of the old program;
  - verifies that identity, version, daemon hash, Accessibility, Relay and Vnet
    all recover.

  Start with the operator's Mac. Install on each remote Mac only once it is
  reachable by a route that does not depend on Muster.
- **Not published.** No feed, OTA page, iOS build, TestFlight or App Store
  change. The published 1.1.24 stays the current download.
- **Acceptance.** On an installed Mac, create one desktop session in each
  provider's app, Claude and Codex. Use a scratch directory and a first message
  that asks for a one-word reply. Use the bundled CLI's `rpc` command:
  - **Local.** Send `agent_native_create` to the Mac's own daemon.
  - **From another Mac.** Once a second Mac runs 1.1.26, send it from that Mac
    with `--target`, to the same Mac unlocked, and to a locked Mac through
    `locked_use_prepare` and `locked_use_execute`. ADR 0120 needs no signature,
    only `locked.control`.

  For each creation, record:
  - the reply;
  - the ledger row;
  - the new session id;
  - that its first message matches;
  - that exactly one session was created.

## Alternatives

- **Rebuild as 1.1.25.** This keeps the number but gives one version three
  different builds, so it was rejected.
- **Install a plain Xcode build with `install-mac.sh`.** That loses the
  Velopack metadata the feed needs (ADR 0138).
- **Publish 1.1.26.** Publishing was not asked for. It would also need a paired
  iOS build that cannot yet be tested on a phone.
- **Wait for the remote Macs.** That would leave nothing deployed. The
  operator's Mac can host local acceptance at once.

## Consequences

- An installed Mac runs a build newer than the published feed. The feed will not
  offer it an update until a release above 1.1.26 is published.
- The session-loading work must rebuild above 1.1.26 instead of installing its
  1.1.25 candidate.
- Acceptance creates real sessions in the operator's Claude and Codex apps.
  While Accessibility drives an app, that app comes to the front.
- Testing from another Mac and on a locked Mac stays pending until a second Mac
  is reachable and updated.

## Verification

Before landing, run the ADR check and the public-content check, and confirm the
generated version fields.

After landing, record each of the following separately:

- source commit and build;
- signing, notarization and Gatekeeper;
- each Mac's installation, identity and runtime health;
- local creation in each provider's app;
- creation from another Mac and on a locked Mac.

No installation or acceptance result is claimed yet.
