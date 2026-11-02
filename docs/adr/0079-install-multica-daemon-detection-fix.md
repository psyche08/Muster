# ADR 0079: Install the Multica daemon-detection fix as desktop 1.1.7 (97)

- Date: 2026-11-02
- Status: Accepted; installed and read-only acceptance passed on one Mac,
  publication and end-to-end task acceptance pending
- Related: [ADR 0077](0077-multica-runtime-state-and-manual-upgrade.md),
  [ADR 0078](0078-multica-daemon-detection-without-pid-file.md),
  [ADR 0052](0052-targeted-multica-deployment.md),
  [ADR 0054](0054-native-multica-targeted-installation.md);
  [DESKTOP_UPDATES](../DESKTOP_UPDATES.md)

## Context

ADR 0078's daemon-detection fix merged to `main` at `071e54de`. The dedicated
acceptance Mac still ran desktop 1.1.5 (95), which predates both the fix and the
`version` / `migration_required` projection from ADR 0077. The published 1.1.6
(96) predates the fix as well, so installing a published artifact would not
deliver it, and rebuilding different source under the published 1.1.6 number
would obscure provenance (the same reason ADR 0052/0054 used a new number).

Read-only acceptance also showed the guest Multica CLI could not resolve its
server: the container resolver pointed at the container gateway, whose DNS
queries were refused. The operator then configured the guest resolver to the
site DNS server; the read-only workspace query succeeded. That is container
environment state, not a Muster product decision.

## Decision

Build and install **desktop 1.1.7 (97)** on that one Mac from source based on
`071e54de` plus this version bump, as a targeted installation:

- Bump `MARKETING_VERSION` to 1.1.7 and `CURRENT_PROJECT_VERSION` to 97 in
  `MusterApp/project.yml` and regenerate the committed Xcode project.
- Build the release daemon, CLI, relay, browser helper and macOS FFI archive;
  build the app bundle, then sign it with the team's Developer ID identity,
  the Presence provisioning profile and the repository entitlements (app and
  nested `musterd`, CLI, browser helper, Locked Use plugin and NetHelper).
- Stop only the running app and its LaunchAgent, replace only
  `/Applications/Muster.app` on that Mac, launch through Launch Services, and
  keep no old bundle afterwards.
- Do not publish a feed, GitHub release, OTA page or TestFlight build, and do
  not change the public macOS/iOS pair. No notarization or stapling is claimed
  for this local transfer: the bundle is Developer ID signed and validated
  locally, not distributed.
- Keep the container and its login state. The guest resolver change is the
  operator's environment action; Muster does not manage container DNS, and a
  recreated container may need it applied again.

Data-root migration, container recreation, installation/uninstall of Multica
itself and scheduler configuration are out of scope.

## Alternatives

- **Install the published 1.1.6 (96).** Rejected: it lacks ADR 0078's fix, so
  the acceptance Mac would still report `running=false` while the daemon runs.
- **Wait for the next public release.** Rejected by the request to verify the
  fix on the acceptance Mac now.
- **Replace only `musterd` inside 1.1.5.** Rejected: it invalidates the app
  signature and the Presence/TCC identity the daemon relies on.
- **Recreate the container with a different DNS flag.** Rejected: it risks the
  guest login and installed tools; the resolver is environment configuration.
- **Notarize and publish 1.1.7.** Rejected: this is a targeted installation,
  not a release, and no notarization profile is claimed here.

## Consequences

The acceptance Mac now runs the fix and the read-only projection from ADR 0077.
Its device identity, Accessibility grant and container state were preserved.
The local 1.1.7 build is unpublished and not notarized; the public feeds remain
at 1.1.6. The guest resolver setting lives inside the container and is outside
Muster's management; a container rebuild would require repeating it. Host and
guest Multica CLI versions can still differ (host copy 0.3.54, guest 0.3.57)
until the documented manual upgrade is performed.

## Verification

- Build/sign: release builds completed; the bundle reports 1.1.7 (97);
  `codesign --verify --deep --strict` passed; the Presence access group,
  team identifier and profile authorization passed
  `check-presence-signing.swift`; the app and nested components carry the
  repository entitlements.
- Installation: only `/Applications/Muster.app` was replaced after stopping the
  app and `dev.linsheng.musterd`; the daemon returned in about 12 seconds; the
  old bundle was not retained.
- Identity/OS: `muster info` device `id` and `fingerprint` matched before and
  after (hashes compared without printing them); Accessibility reports
  `granted`.
- Runtime, read-only: `MulticaStatus` reported `supported=true`,
  `installed=true`, `running=true`, `migration_required=false`, guest
  guest `version` reported `multica 0.3.57`; bridge files
  (`bridge-token`, `runtime-codex`, `runtime-claude`, `runtime-command`) were
  mode 600; the workbench returned 2 workspaces, a 50-issue page, 58 agents and
  77 runtime records.
- Container DNS: the guest resolver pointed at the site DNS server and
  `multica workspace list` returned both workspaces; without it the query
  failed with a server-address error.
- Not performed: notarization/Gatekeeper quarantine assessment, public
  publication, reboot and app-upgrade recovery, terminal `multica login`, a
  real scheduler task, a real provider turn, and any iOS build or device run.
