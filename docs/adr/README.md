# Architecture Decision Records

Every repository change carries a numbered ADR or an update to the ADR for the
same decision. See [AGENTS.md](../../AGENTS.md#architecture-decision-records-adr).
The target architecture remains [ARCHITECTURE.md](../ARCHITECTURE.md); an ADR
records a specific decision and does not revive code on its delete list.

| ADR | Status | Decision |
| --- | --- | --- |
| [0001](0001-require-adr-for-changes.md) | Accepted | Require an ADR for every change |
| [0002](0002-provider-limits-and-bounded-relay-probes.md) | Accepted | Provider pagination and loss-tolerant bounded relay probes |
| [0003](0003-confirmed-conversation-workbench.md) | Accepted, retrospective | Confirmed terminal execution and origin-bound result return |
| [0004](0004-transport-recovery-and-stable-vnet-owner.md) | Accepted | Socket failure recovery under a stable Node and Vnet owner |
| [0005](0005-isolate-desktop-rpc-requests.md) | Accepted | Isolate desktop RPC forwarding and retain desktop authorization |

| [0006](0006-desktop-presence-and-console-routing.md) | Proposed | Desktop Presence enrollment and console routing |

| [0007](0007-desktop-manual-locked-use.md) | Accepted | Desktop manual Locked Use and selection observation |

| [0008](0008-complete-delayed-vnet-startup.md) | Accepted | Complete delayed Vnet startup independently of callers |

| [0009](0009-task-contract-and-runtime-manifest.md) | Proposed | Provide Muster as a Multica runtime behind a shim command |

| [0010](0010-native-sidebar-metadata.md) | Accepted | Recognize bounded native sidebar metadata |

| [0011](0011-linux-ci-artifact-budget.md) | Accepted | Keep Linux CI within platform and storage limits |

| [0012](0012-pairing-presence-promotion.md) | Accepted | Promote the verified pairing link to online |

| [0013](0013-ci-must-be-askable.md) | Accepted | CI has to be runnable on demand, and "no checks" is not a pass |

| [0014](0014-recovery-test-races.md) | Accepted | Fix the recovery tests' own races |

| [0015](0015-offering-a-task-runtime-is-opt-in.md) | Accepted | Offering this machine as a task runtime is opt-in |
| [0016](0016-suspend-hosted-actions-budget.md) | Accepted | Suspend hosted Actions at the owner's budget request |
| [0017](0017-claude-question-submit-activation.md) | Accepted | Activate Claude Submit through verified keyboard focus |

| [0018](0018-codex-approval-conversation-scope.md) | Accepted | Scope native Codex approvals to the conversation |

| [0019](0019-submit-focus-sampling.md) | Accepted | Sample Submit focus immediately before activation |

| [0020](0020-submit-window-focus-proof.md) | Accepted | Verify Submit focus within the addressed application window |

| [0021](0021-desktop-startup-retry.md) | Accepted | Retry complete desktop startup through Shared Rust |

| [0022](0022-sanitize-current-public-tree.md) | Accepted | Sanitize the current public source tree |

| [0023](0023-bootstrap-sanitized-repository.md) | Accepted | Bootstrap a sanitized repository with one commit |

| [0024](0024-cloud-external-testing-opt-in.md) | Accepted; hosted configuration pending | Make Cloud external testing opt-in |

| [0025](0025-ignore-the-scheduler-token-in-the-working-tree.md) | Accepted | Ignore the scheduler token that sits in the working tree |

| [0026](0026-cloud-trust-and-native-diagnostics.md) | Accepted | Platform Cloud trust and bounded native diagnostics |

| [0027](0027-native-send-scan-reuse.md) | Accepted | Reuse native send scans and batch control metadata |

| [0028](0028-codex-async-card-completion.md) | Accepted | Preserve live asynchronous Codex cards after turn completion |

| [0029](0029-bounded-conversation-projection.md) | Accepted | Bound initial conversation layout with Rust-owned history paging |

| [0030](0030-ios-recycled-conversation.md) | Accepted | Recycle iOS conversation rows while retaining Rust pagination |

| [0031](0031-transcript-disclosure-state.md) | Accepted | Retain transcript disclosure choices across cell reuse |

| [0032](0032-remove-cache-ttl-display.md) | Accepted | Simplify conversation metadata and collapse file changes |

| [0033](0033-managed-terminal-compatibility.md) | Accepted | Recover managed terminals and attach existing desktop clients |

| [0034](0034-transcript-window-continuity.md) | Accepted | Preserve transcript continuity across paging and tabs |

| [0035](0035-native-terminal-dev.md) | Accepted | Select native terminal hosts and isolate local development |
| [0036](0036-terminal-simulation-coverage.md) | Accepted | Preserve native terminal events and bound cancelled requests |

| [0037](0037-apple-recycled-history.md) | Accepted | Recycle desktop transcripts and load larger history pages near the top |

| [0038](0038-client-send-diagnostics.md) | Accepted | Export content-free client send phases and error codes |

| [0039](0039-exact-session-navigation.md) | Accepted | Recover absent Claude sidebar rows by exact session navigation |

| [0040](0040-desktop-navigation-identity.md) | Accepted | Map Claude navigation to its desktop identity |

| [0041](0041-macos-transcript-geometry.md) | Accepted | Fix desktop transcript measurement and wheel-driven history |

| [0042](0042-history-anchor-during-scroll.md) | Accepted | Preserve reading anchors while history arrives during scrolling |

| [0043](0043-ordered-terminal-input.md) | Accepted | Serialize and bound remote terminal input in Shared Rust |

| [0044](0044-ghostty-creation-under-locked-use.md) | Accepted | Bind Ghostty creation to a one-shot terminal-scoped unlock |

| [0045](0045-native-send-fast-path.md) | Accepted | Bound native send scans and settle receipts after relocking |

| [0046](0046-send-recovery-and-quiet-admission.md) | Accepted | Recover send outcomes without replay and wait for quiet before navigation |

| [0047](0047-transcript-reading-anchor-parity.md) | Accepted | One reading-anchor policy for both Apple transcripts |

| [0048](0048-admitted-send-acknowledgement.md) | Accepted | Acknowledge a send against the conversation, not only its outcome |

| [0049](0049-web-window-onto-one-origin.md) | Proposed | A web window onto one origin, fetched by the person's own browser |

| [0050](0050-paired-release-1-0-94.md) | Accepted | Land the outstanding branches and release them as 1.0.94 (85) |

| [0051](0051-multica-management-and-live-catalog.md) | Accepted | Manage Multica containers and subscribe to session catalogs |

| [0052](0052-targeted-multica-deployment.md) | Accepted | Deploy Multica management and relocate the existing workspace root |

| [0053](0053-multica-native-workbench-and-runtime-routing.md) | Accepted | Native read-only Multica workbench; desktop execution still pending |

| [0054](0054-native-multica-targeted-installation.md) | Accepted | Targeted installation of the native Multica workbench |

| [0055](0055-clear-every-input-before-a-submission.md) | Accepted | Clear every input before a submission; hold a quiet cover |

| [0056](0056-bounded-scans-skip-transcripts-and-retry-vanished-nodes.md) | Accepted | Session scans skip transcripts; control scans retry a vanished node |

| [0057](0057-targeted-deployment-1-1-0.md) | Accepted | Targeted deployment of 1.1.0 (90) and device acceptance |

| [0058](0058-duplicate-views-use-the-main-window.md) | Accepted | Several views of one conversation use the main window's view |

| [0059](0059-first-public-apple-release.md) | Accepted; iOS review pending | First public Apple release and PR-only main |

| [0060](0060-multica-runtime-recovery.md) | Proposed | Recover native runtime execution after release upgrades |

| [0061](0061-send-receipt-format-and-manual-resolution.md) | Accepted | Provider URL formatting and explicit send resolution |

| [0062](0062-scoped-composers-and-send-preparation-outcomes.md) | Accepted | Scoped composers and complete send-preparation diagnostics |

| [0063](0063-queue-receipts-use-provider-text-matching.md) | Accepted | Provider-aware queued-message confirmation and display matching |

| [0064](0064-multica-native-runtime.md) | Accepted; acceptance pending | Desktop-first Multica runtime adapters (draft ADR 0055) |

| [0065](0065-native-session-identity-and-warm-creation.md) | Accepted; acceptance pending | Native session identity and warm Claude creation (draft ADR 0056) |

| [0066](0066-native-codex-project-identity.md) | Proposed | Selected project identity (draft ADR 0057) |

| [0067](0067-paired-release-1-1-4.md) | Accepted; release pending | Integrate runtime recovery and publish Apple 1.1.4 |

| [0068](0068-send-retry-and-ignore.md) | Accepted | Retry and ignore unconfirmed-send notices |

| [0069](0069-composer-return-key.md) | Accepted | Enter sends and Shift+Enter inserts a newline |

| [0070](0070-paired-release-1-1-5.md) | Accepted; release pending | Publish paired Apple 1.1.5 |

| [0071](0071-small-change-execution.md) | Accepted | Bound the execution cost of small changes |

| [0072](0072-multica-install-dialog.md) | Accepted; device acceptance pending | Guided Multica installation with live logs |

| [0073](0073-deepseek-harness-session-viewing.md) | Accepted; driving superseded by 0074; viewing revised by 0106 | View DeepSeek Harness sessions; do not drive the app yet |

| [0074](0074-deepseek-harness-plugin-control.md) | Accepted; installed daemon and phone remote control accepted on 1.1.8; locked-screen transaction pending | Drive DeepSeek Harness through a Muster plugin inside the app |

| [0075](0075-claude-registry-entries-without-cli-session.md) | Accepted; fix installed in 1.1.8 on three Macs; phone send acceptance pending | A Claude conversation without a CLI session hides no other |

| [0076](0076-paired-release-1-1-6.md) | Accepted; published and installed on one Mac | Publish Apple 1.1.6 to the OTA page and this Mac |

| [0077](0077-multica-runtime-state-and-manual-upgrade.md) | Accepted; device and end-to-end acceptance pending | Record the managed Multica bridge, read-only version and migration state, manual upgrade boundary |
| [0078](0078-multica-daemon-detection-without-pid-file.md) | Accepted; device probe passed, end-to-end acceptance pending | Detect the Multica guest daemon without a pid file |
| [0079](0079-install-multica-daemon-detection-fix.md) | Accepted; installed on one Mac, publication pending | Install the Multica daemon-detection fix as desktop 1.1.7 (97) |
| [0080](0080-session-list-scan-prunes-conversation-views.md) | Accepted; shipped in 1.1.8; refined by 0091 for apps whose sidebar shares the conversation view | The desktop session-list scan prunes conversation views |
| [0081](0081-paired-release-1-1-8.md) | Accepted; published and installed on three Macs; provider send acceptance pending | Publish Apple 1.1.8 (98) and install it on the three Macs |
| [0082](0082-progressive-architecture-documentation.md) | Accepted | Progressive disclosure: ARCHITECTURE.md split into parts; release, field and deployment records moved to docs/validation/ |
| [0083](0083-idle-compaction.md) | Accepted, amended by [0087](0087-idle-compaction-on-a-locked-or-busy-mac.md); device acceptance pending | Opt-in `/compact` for Claude Code conversations idle 30-60 minutes |
| [0084](0084-keep-awake-menu.md) | Accepted; released in 1.1.9 (99) and running on three Macs | Keep Awake and Keep Screen On in the Mac app menu, through `caffeinate` |
| [0085](0085-paired-release-1-1-9.md) | Accepted; published and installed on three Macs | Publish Apple 1.1.9 (99) and install it on the three Macs |
| [0086](0086-dsh-session-chrome-and-trace.md) | Accepted; macOS/iOS builds and synthetic rendering verified, device interaction pending | DSH-style session header/tabs, a turn-grouped trace page with time overview and inspector, and a third DeepSeek conversation style |
| [0087](0087-idle-compaction-on-a-locked-or-busy-mac.md) | Accepted; device acceptance pending | Idle compaction on a locked Mac through an unattended Locked Use lease, and with a top-right notice while the Mac is in use |
| [0088](0088-unattended-desktop-updates.md) | Accepted; signed feed published with 1.1.10, unattended update pending | Unattended desktop updates through the signed Velopack feed, with a new feed signing key |
| [0089](0089-paired-release-1-1-10.md) | Accepted; published, on GitHub and installed on three Macs | Publish Apple 1.1.10 (100) and install it on the three Macs |
| [0090](0090-reuse-transcript-heads-between-rescans.md) | Accepted; implementation verified, installed acceptance pending | Reuse parsed transcript heads between catalog rescans, so the session list updates without a 2 s scan |
| [0091](0091-prune-conversation-views-only-where-separate.md) | Accepted; device acceptance passed on desktop 1.1.11 | Prune conversation views only in apps that expose them apart |
| [0092](0092-paired-release-1-1-11.md) | Accepted; published, installed on two Macs; third-Mac delivery via signed feed pending | Publish Apple 1.1.11 (101) and install it on the three Macs |

| [0093](0093-app-and-cli-capability-control-design.md) | Proposed; design only | Separate App and CLI sessions, route typed capabilities through plugins or AX, and recover operations without replay |
| [0094](0094-local-plugin-contract-and-receipt-recovery.md) | Accepted; local App host verified | Bind local App plugin operations and recover native send receipts without changing device interfaces |
| [0095](0095-shared-ax-navigation-and-preparation.md) | Accepted; installed Codex phone-send baseline verified; broader App acceptance pending | Share bounded AX navigation and reuse verified preparation |

| [0096](0096-question-editor-preparation.md) | Accepted; source verified, installed question acceptance pending | Rebind and refresh question editors during preparation |
| [0097](0097-task-scoped-scheduler-grant-for-locked-use.md) | Accepted; source and tests verified | Let one local task's scheduler grant run bounded desktop operations under this Mac's own Locked Use lease |

| [0098](0098-focused-question-field-deletion.md) | Accepted; installed candidates failed; see 0101 | Delete a proven question-editor selection through the application |

| [0099](0099-local-claude-identity-and-permission-cards.md) | Accepted; device acceptance pending | Local Claude UUID mapping and fresh permission cards |

| [0100](0100-locked-claude-permission-cards.md) | Accepted; verification pending | Observe and retain local Claude permission cards across lock |

| [0101](0101-question-placeholder-projection.md) | Accepted; device acceptance pending | Recognize the empty optional question reply projection |

| [0102](0102-session-sections-hierarchy-search.md) | Accepted; source verified, device acceptance pending | Native session sections, Claude hierarchy and conversation search |

| [0103](0103-visible-tabs-and-provider-aggregation.md) | Accepted; source verified, device acceptance pending | Show every session tab and aggregate All devices by provider |

| [0104](0104-three-category-session-catalog.md) | Accepted; source verified, device acceptance pending | Aggregate sessions into Pin, Section and Recent only |

| [0105](0105-l4forwarder-addon.md) | Accepted; deployment superseded by 0107; TCP device acceptance passed, UDP partly verified | Forward loopback-only member services with the l4forwarder addon |

| [0106](0106-deepseek-harness-session-adaptation.md) | Accepted; source and rendering verified, device acceptance pending | Adapt DeepSeek Harness sessions on every surface |

| [0107](0107-l4forwarder-in-muster-app.md) | Accepted; polling superseded by 0108; release and device migration pending | Ship l4forwarder in Muster.app and let musterd run it |

| [0108](0108-watch-l4forwarder-configuration.md) | Accepted; source verified, release pending | Watch the l4forwarder configuration instead of polling it |

| [0109](0109-uuid-session-refresh-and-permission-lifetime.md) | Accepted; verification pending | UUID metadata reconciliation, message recency and permission lifetime |

| [0110](0110-native-groups-paging-and-session-entry.md) | Accepted; verification pending | Native group pages of ten main sessions and immediate session metadata |

| [0111](0111-native-organization-ax-and-file-confirmation.md) | Accepted; verification pending | Native organization with AX controls and file readback |

| [0112](0112-silent-native-catalog-read-failures.md) | Accepted; verification pending | Ignore native catalog read failures in the interface |

| [0113](0113-pin-only-session-catalog.md) | Accepted; verification pending | Show only Pin and the ordinary session list |

| [0114](0114-restore-unlanded-1-0-21-field-fixes.md) | Accepted; source verified, release pending | Restore two 1.0.21 field fixes that never landed |

| [0115](0115-target-scoped-local-input.md) | Accepted; source and tests verified, device acceptance pending | Count only local input that could reach the target app |

| [0116](0116-failure-screenshot-analysis.md) | Accepted; source, tests and device acceptance verified | Look at a failed native send with the user's own LLM |

| [0117](0117-bounded-view-cache-and-empty-pin.md) | Accepted; source verified, device acceptance pending | Bound the view cache on disk and drop an empty Pin |

| [0118](0118-sweep-past-an-unreadable-worktree.md) | Accepted; source and tests verified | Sweep past a worktree that cannot answer for itself |
| [0119](0119-ignore-loose-credential-files.md) | Accepted; source verified | Ignore the credential file types the fleet actually keeps loose |
| [0120](0120-locked-control-without-a-per-operation-signature.md) | Accepted; source and tests verified | `locked.control` authorizes Locked Use without a per-operation signature |
| [0121](0121-generated-xcode-project-follows-project-yml.md) | Accepted; source verified, fleet build pending | `project.yml` is the Mac project's only version, and drift is reported |
| [0122](0122-wait-for-the-app-to-exit-before-replacing-it.md) | Accepted; source verified, install verified | Wait for Muster to exit before replacing its bundle |
| [0123](0123-the-daemon-leaves-the-dock.md) | Superseded by 0127 | The daemon leaves the Dock instead of moving out of the bundle |
| [0124](0124-prompt-for-screen-recording-and-move-terminal-authorization.md) | Accepted; partly superseded by ADR 0125 | Prompt for Screen Recording at startup, and move terminal authorization to the Terminals page |
| [0125](0125-screen-recording-is-the-bundles-and-only-a-restart-takes-it.md) | Accepted; source and tests verified, device evidence recorded | Screen Recording is the bundle's grant, and only a daemon restart takes it |
| [0126](0126-multica-bridge-follows-the-container-gateway.md) | Accepted; source and tests verified, installation and runtime pending | The Multica runtime bridge follows the container gateway |
| [0127](0127-the-daemon-stays-out-of-launch-services.md) | Accepted; premise corrected by 0131 | The daemon stays out of Launch Services instead of leaving the Dock |
| [0128](0128-the-panel-helper-keeps-its-own-identity.md) | Accepted; source verified | The panel helper keeps its own identity |
| [0129](0129-ask-ax-for-windows-by-name.md) | Accepted; premise withdrawn by 0131 | Ask the Accessibility API for windows by name, not by walking children |
| [0130](0130-mount-the-multica-data-image-before-the-container-starts.md) | Accepted; mount step superseded by 0133 after the install acceptance failed | Mount the Multica data image before the container starts |
| [0131](0131-a-locked-screen-explains-the-missing-windows.md) | Accepted; measured locked and unlocked on three apps | A locked screen explains the missing windows |
| [0132](0132-restore-the-daemons-background-registration.md) | Accepted; timeline measured, install acceptance passed | Restore the daemon's background registration, and the Dock tile goes |
| [0133](0133-mount-the-multica-data-image-as-the-user.md) | Accepted; source and tests verified, install acceptance passed | Mount the Multica data image as the user, not through diskutil |
| [0134](0134-activate-claude-next-and-back-by-keyboard-focus.md) | Accepted; installed in 1.1.22 on three Macs, live card acceptance pending | Activate Claude's Next and Back through keyboard focus |
| [0135](0135-give-session-watches-the-forwarding-budget.md) | Accepted; installed in 1.1.22 on three Macs, runtime acceptance pending | Give session watches the forwarding budget and report why they ended |
| [0136](0136-settle-relay-carrier-switches.md) | Accepted; installed in 1.1.22 on three Macs, switch counts pending | Settle relay carrier switches |
| [0137](0137-keep-the-desktop-apps-log-in-a-file.md) | Accepted; installed in 1.1.22, the log file verified on three Macs | Keep the desktop app's log in a file |
| [0138](0138-paired-release-1-1-22.md) | Accepted; published and installed on three Macs | Publish Apple 1.1.22 (112) and install it on the three Macs |
| [0139](0139-cache-macos-transcript-row-measurements.md) | Accepted; installed acceptance pending | Reuse unchanged macOS transcript row measurements |
| [0140](0140-resolve-cargo-output-through-cargo-target-dir.md) | Accepted; macOS, iOS and Android builds verified in both modes | Resolve Cargo output through CARGO_TARGET_DIR |

| [0141](0141-multica-service-and-dsh-app-bridge-design.md) | Proposed; design only | Separate Multica session ownership, DSH App bridging and opt-in remote service |

| [0142](0142-session-history-and-live-push-latency.md) | Accepted; source verified, installed acceptance pending | Keep session history and live pushes independent of catalog work |

| [0143](0143-paired-release-1-1-23.md) | Accepted; published and installed on three Macs; cleanup verified; iPhone deferred | Publish Apple 1.1.23 (113) from main and install the fleet |

| [0144](0144-local-release-and-independent-installation.md) | Accepted; three Mac installations verified | Local Apple release and independent installation jobs |

| [0145](0145-isolate-slow-subscriptions-and-bound-chat-entry.md) | Accepted; source verified, release pending | Isolate slow subscriptions and bound chat entry history |

| [0146](0146-request-streams-over-the-quic-peer-pool.md) | Accepted; verification pending | Requests own service streams over the QUIC peer pool |

Copy [template.md](template.md), choose the next unused four-digit number and a
short descriptive filename, and add it above. Commit the decision alongside its
implementation. Add later evidence without erasing earlier acceptance limits;
superseding decisions get new numbers and bidirectional links.

| [0147](0147-correlated-session-load-diagnostics.md) | Accepted | Correlate session entry, transport, history and native model application |

| [0148](0148-per-device-provider-catalog-pages.md) | Accepted | Load ten conversations per device/provider on entry and continuation |

| [0149](0149-multica-ownership-reservations.md) | Accepted; foundation verified | Durable Multica ownership reservations and isolated DSH App contract probe |

| [0151](0151-multica-task-session-isolation.md) | Accepted; device acceptance pending | Enforce registered task ownership and expose scoped Multica browsing |
