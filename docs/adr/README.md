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

Copy [template.md](template.md), choose the next unused four-digit number and a
short descriptive filename, and add it above. Commit the decision alongside its
implementation. Add later evidence without erasing earlier acceptance limits;
superseding decisions get new numbers and bidirectional links.
