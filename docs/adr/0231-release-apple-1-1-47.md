# ADR 0231: Publish Apple 1.1.47 (137)

- Date: 2026-11-19
- Status: Accepted; versions prepared, build and publication pending
- Related: [0219](0219-release-apple-1-1-46.md),
  [0220](0220-background-only-native-creation.md),
  [0223](0223-locked-use-blackens-displays-with-a-gamma-table.md),
  [0224](0224-claude-sidebar-status-in-row-name.md),
  [0225](0225-multica-run-grant-closes-after-its-first-turn.md),
  [0226](0226-task-local-scopes-are-plain-functions.md),
  [0227](0227-system-launchagent-startup-gate-and-full-restart.md),
  [0228](0228-hourly-redacted-log-upload.md),
  [0229](0229-log-collector-behind-nginx-on-8443.md),
  [0230](0230-claude-environment-by-main-folder.md)

## Context

The operator requested a paired release that brings ADR 0230 to the Macs
that create native Claude sessions: a Claude environment with extra folders
is picked by its main folder. The last Apple build was 1.1.46 (136), built
from #240. Since then #241 through #250 have merged on main.

## Decision

Build macOS and iOS 1.1.47 (137) from the release merge on main. It includes
#241 through #250:

- background-only native creation and Locked Use displays blackened with a
  gamma table (#242, #244);
- Claude sidebar rows whose name includes their status word (#243);
- a Multica run's grant closing after its first turn (#245);
- task-local scopes as plain functions (#246);
- musterd under a system LaunchAgent, started before the main page, and a
  restart that restarts every Muster process (#247);
- hourly redacted log upload to the collector behind nginx (#248, #249);
- Claude environments picked by their main folder (#250);
- ADR 0156 evidence only (#241).

Publish as for 1.1.46:

- the signed and notarized Apple Silicon portable on GitHub;
- the signed desktop update and the registered-device iOS OTA on the download
  page;
- a separate App Store export for TestFlight.

External beta submission follows the operator's choice for this build
(ADR 0024).

## Alternatives

- Reusing build 136 would make the update ambiguous.
- Waiting for the log collector before releasing would hold back ADR 0230,
  which pending Claude PR reviews need now.
- Disabling the upload for this build would need a code change that ADR 0228
  does not provide for release builds.

## Consequences

- Release builds upload redacted logs to `muster-logs.linsheng.dev:8443`
  (ADR 0228, 0229). On 2026-11-19 that name did not answer from two of the
  owner's Macs. Until it does, desktops retry hourly and lose nothing.
  ADR 0228 lists the manual prerequisites before a release ships with
  uploads on: the collector installed, the endpoint reachable and the updated
  privacy page published. The operator decides whether to complete them
  before this build is published.
- The update moves musterd to the system LaunchAgent of ADR 0227. Its
  remaining acceptance (the next login, an update and the restart button) is
  first exercised by this update.
- Each updated Mac picks Claude environments by main folder. A PR review
  through Tusk works once Tusk asks for the environment's main folder.

## Verification

- Prepared in this change: `MARKETING_VERSION` 1.1.47 and
  `CURRENT_PROJECT_VERSION` 137 in `project.yml` and the generated project.
- Pending, on the release Mac: verify merge ancestry and the embedded daemon
  revision, then signatures, notarization, the signed feed and public package
  hashes. Developer ID signing needs the owner's unlocked login keychain.
- Pending, separately: the OTA manifest, App Store Connect processing and
  beta group state.
- Pending, per Mac: the installed version on each Mac, including the one that
  runs PR reviews, and a live native Claude creation into a multi-folder
  environment (ADR 0230).
