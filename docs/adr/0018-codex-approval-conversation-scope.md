# ADR 0018: Scope native Codex approvals to the conversation

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](../PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

- Date: 2026-10-22
- Status: Accepted
- Related: ADR 0017; ARCHITECTURE.md Shared Rust/platform boundaries

## Context

Installed 1.0.80 on Mac-C displayed a real Calculator Computer Use card with
Always allow, Deny and Allow this conversation. Muster exposed all three but
also exposed Reject from the adjacent AIAppTargets.swift code review pane.
Before and after the permission card that unrelated button alone could keep
the conversation in waiting_approval. The recorded pane is a complementary
landmark inside the main landmark, with a tab panel beneath it.

The owner authorized the Calculator current-conversation grant. Its one
Muster answer resumed the tool and produced a final reply. The separate
Mac-B-to-Mac-A Claude answer also produced a provider answer and final reply
with the installed focused-Space implementation. These successes do not excuse
the extra approval option or prove Locked Use/physical iPhone acceptance.

## Decision

Shared Rust profiles declare approval scope. Codex requires a complete ancestry
chain to the bound web area, a main conversation landmark, and no intervening
complementary, navigation or workspace tab panel. Other provider profiles retain
their existing scope. The platform only collects bounded AX ancestry evidence;
Shared Rust decides whether that evidence belongs to the approval scope.
Use this check both when observing options and immediately before dispatch,
alongside a fresh enabled flag and exact label. Incomplete, cyclic or detached
ancestry fails closed. Preserve authorization and no-retry-after-dispatch rules.

## Alternatives

Removing Reject from the vocabulary would break legitimate permission cards.
Closing the review pane would hide the defect instead of fixing classification.
Matching only visible text without ancestry cannot distinguish these controls.

## Consequences

Review controls cannot create a pending permission interaction. A provider UI
that removes the main landmark must be re-adapted rather than silently widening
scope. No approval policy or existing task is changed by the repair.

## Verification

Add regressions for an actual conversation card, nested review panel, sidebar,
missing main landmark, incomplete ancestry, and other-provider compatibility.
Run Rust tests/Clippy, architecture and ADR gates. Publish the follow-up as a
new paired Apple version; never replace immutable 1.0.80 artifacts with new
bytes. Recheck the original Mac-C session with its review pane left open and
record source/build/publication/installation/device results separately.

### Source validation and paired follow-up

- 221 agent tests passed (4 existing ignored), 33 platform tests passed,
  and 51 daemon tests passed. Strict all-target Clippy for the three crates
  and the 17-crate layer gate passed on Mac-A.
- Build and publish version 1.0.81 (72) from the following clean commit.
  Installed positive permission/Claude acceptance above belongs to 1.0.80;
  validate this follow-up's review-pane exclusion after installing it.

### Installed 1.0.81 acceptance (2026-10-22)

- Source `c73a9b8af10938ea485e36302541daa8cfba1f67`, version 1.0.81 (72).
  An Xcode-generated scheme edit made an initial daemon build dirty; that
  build was discarded, the scheme restored and the build script rerun from
  the clean source. Published artifacts passed source/fingerprint checks.
- Both Apple Release builds passed. Apple notarization
  `c80f218c-6f8f-4972-a926-19acbd3f3513` returned Accepted; strict signing,
  Presence entitlements, staple and Gatekeeper checks passed.
- Both platforms published to the documented existing directories. An
  independent download followed the actual OTA page -> manifest -> hash-named
  IPA, verified version/build 1.0.81/72, signatures and no-store/no-cache headers.
  SHA256: macOS `ff87e69e9283af3df40da58af07e295edb977c8388644f705e3e07d0e0685f26`;
  iOS `f16862bd5666ba6c7c20d730622f1721b8604c3e522b1e0f64f1073646594f59`.
- Mac-A, Mac-B and Mac-C installed through the Rust update lease. All three
  ran daemon SHA256 `afdc1c77ca25b075835659373f900d8ccb6998c691c51485bdc5bf32ae84eb27`,
  kept their identities, retained Accessibility and reported registered relay
  and Vnet up. All six directed RPC and all six virtual-IP ICMP paths passed.
  These connectivity checks are not new fault-injection recovery tests.
- Mac-C before the repair: after Calculator completed, the provider was idle
  but agent_watch still emitted a permission interaction containing Reject.
  The native AX tree located that button in the adjacent code review panel.
  In 1.0.81, with that panel retained, repeated watch states were idle and no
  permission interaction was emitted.
- A fresh `MUSTER81-APPROVAL-1022-A` Chess card then exposed exactly Always allow,
  Deny and Allow this conversation in both AX and Muster. The review Reject
  remained visible in AX and was absent from Muster. One Deny operation using
  `ax-4b068196e790de71b037addaccaf506b-1` passed the new ancestry guard, the tool
  returned not approved, and the agent ended without retry. No Chess grant was
  created. Positive Calculator grant-and-continuation evidence is from 1.0.80
  and is recorded in ADR 0017.
- Requests originated through the installed Muster CLI/RPC; native provider
  controls and transcripts were observed. This does not claim manual clicking
  in the Mac-B Muster UI, physical iPhone acceptance, Locked Use acceptance,
  a new signed Velopack feed or TestFlight publication. GitHub Actions stayed
  disabled throughout; local checks are not hosted-CI results.
- After the Chess denial, another five-second watch emitted only idle states
  and no residual permission interaction. The original review pane was left
  open and its Accept/Reject controls were never activated.
- A new 1.0.81 Claude send preflight returned `local input is still active;
  nothing was written or sent`. No answer was attempted and no unknown send
  was retried. Final-version repeat acceptance is awaiting an owner-provided
  quiet input window; the 1.0.80 provider-receipt evidence remains distinct.
