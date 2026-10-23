# ADR 0010: Recognize bounded native sidebar metadata

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](../PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

- Date: 2026-10-22
- Status: Accepted
- Related: ADR 0007; ARCHITECTURE.md §3.6.2 and §4

## Context

Mac-A 1.0.78 logs on October 22 at 08:03 CST show successful Locked Use
admission followed by zero sidebar candidates before the operation ledger.
The requested Claude title exists as an exact static child, but its row name
adds `Open · 3 pull requests`. The existing whitelist rejects that child label.
The same capture contains `Running`, delegated provenance plus `SSH`, and a
workspace `· Offline` badge. These are observed UI shapes; only the PR row is
confirmed to have caused these send failures. Another operation selected its
session successfully but expired before provider submission; its remaining
latency is not explained by the current logs.

## Decision

Extend the existing Shared Rust sidebar grammar for the observed metadata.
Require one exact standalone title, a unique catalog title, a complete bounded
label read, and recognized metadata. Composite metadata must reproduce the
entire row name in observed visual order; never accept arbitrary extra text,
substring titles, nested controls or ambiguous rows. The selected surface's
exact session ID remains mandatory before any draft mutation or submission.

Add operation/session correlation and phase timing around native preparation.
Keep original queue, authorization and mutation budgets. Do not retry sends or
navigation presses, renew authorization silently, or change Locked Use ownership.

## Alternatives

Substring matching or ignoring every non-title child is less brittle but accepts
unrelated/ambiguous labels. A larger timeout does not fix a deterministic zero
candidate result. Replaying failed or unknown submissions risks duplicate sends.

## Consequences

Known sidebar badges no longer prevent exact session selection. Unknown formats
still fail closed and need evidence before extending the grammar. Timing makes
queue, mode navigation, selection and controller attachment distinguishable;
this does not claim to fix all authorization-expiration causes.

## Verification

Replay the captured PR row through the production label reader and matcher,
including delegated/SSH/running and offline forms. Test ambiguous titles,
unknown badges, malformed counts, provider boundaries, unreadable/over-budget
labels, and unrelated composite names. Run agent and daemon tests, macOS Rust
build checks, layer checks and ADR gate. Record results below before landing.
Signing, publication, installation and real provider delivery are separate;
source replay is not device send acceptance. Do not replay the user's messages.

### Source verification (Mac-A, 2026-10-22)

- Captured PR-row regression fails on the original matcher, then passes after
  the fix. All nine sidebar tests pass, including negative/ambiguous cases.
- `cargo test -p muster-agent -p musterd --lib`: 217 agent tests pass
  (four pre-existing ignored), 51 daemon tests pass.
- Strict Clippy for both crates with tests passes on macOS arm64.
- `cargo build -p musterd` builds and links the macOS arm64 debug daemon.
  This is not a signed distribution build.
- The layer gate passes for all 17 crates after repairing the previously merged
  ACP map omission, documented as a follow-up to ADR 0009.
- No service replacement, signing, publication or new provider submission was
  performed during this source repair. All three devices still ran 1.0.78 at
  investigation time; real locked/unlocked delivery on the new code is pending.
- The separate 08:01:43 authorization expiration remains unresolved. The
  existing logs show successful navigation, then expiration before submission;
  newly added phase timings must be collected before assigning a cause.

### Release plan: 1.0.79 (70)

Publish the merged sidebar correction and phase diagnostics as paired macOS and
iOS artifacts. The source also includes ADR 0012's verified-pairing presence
correction. Preserve the existing signing, notarization, provenance and Rust
update-lease gates. Install on Mac-A, Mac-C and Mac-B without interrupting
protected console sessions. Verify each installed source stamp, identity, relay
and Vnet independently. Device installation does not establish native-send or
physical-iPhone acceptance. The separate Codex approval-card investigation has
no source fix in this release. Record completed and blocked gates after rollout.

### Published and installed 1.0.79 (2026-10-22)

- Clean release source `f04592ac657e3511f4706dc179ed5ab488e027ed` contains
  PR #168. macOS arm64 daemon/FFI/app and iOS arm64 FFI/Release archive built
  successfully; iOS OTA is version 1.0.79, build 70.
- Apple notarization `52f755c6-0e5f-44e2-9898-70796cc17bf2` was Accepted.
  Strict signatures, Presence provisioning, source/fingerprint checks, staple
  validation and Gatekeeper passed. Published via `${MUSTER_RELEASE_HOST}:${MUSTER_RELEASE_SSH_PORT}`.
- Independent public downloads, including the page's actual installation link,
  verified versions, hashes and no-store/no-cache headers. macOS ZIP SHA-256:
  `3230907a4031c8f801b2d3f795506e620fa01eae77b1e55fa5f954a1a0bce450`;
  iOS IPA SHA-256:
  `f1e6fed37b5509a446b0f64b17eb503bcbf72a94e8945ac6a19009577bd8e4b2`.
- Mac-A, Mac-C and Mac-B each acquired the Rust update lease and installed
  the same verified app without retaining an old app backup. Device identities
  remained unchanged. All report 1.0.79, Accessibility granted, relay registered
  and Vnet up. Installed daemon SHA-256:
  `3dbc108bfdb942c9ce52ff47405bb04e87eae3344ba2234c6f6699e4e6243a2b`.
  Each installed bundle passed strict signature, staple and Gatekeeper checks.
- All six directed RPC and virtual-IP ping paths passed. Four virtual-IP SSH
  paths targeting Mac-C/Mac-B passed. Explicit port-22 checks from both devices
  to Mac-A returned connection refused, as did Mac-A localhost; its SSH service
  was not enabled or modified. Mac-C's initial SSH alias used port 2222 and
  timed out; the explicit port-22 follow-up separates that configuration issue.
- The user separately authorized deletion of exactly three superseded 1.0.78
  OTA files. Their IPA now returns 410; old install/manifest links resolve to
  build 70. Latest artifacts and aliases remain intact.
- No physical iPhone install, installed native-send/approval-card acceptance,
  original network-fault reproduction, signed Velopack feed or TestFlight
  publication is claimed. The Macs remain locked; this rollout did not unlock
  them or submit any provider message. Codex's missing approval option remains
  a separate unresolved investigation.
- Local evidence: `build/releases/Apple-1.0.79-70/` contains build, notary,
  public-download, installation and network-matrix results. These ignored
  artifacts are not part of the source commit.

### Merged pull-request metadata follow-up (2026-10-22)

A later device log from the installed release shows the exact catalog title as
one passive child, alongside `Awaiting input` and `Merged · 5 pull requests`.
The complete row name is the title followed by that merged badge; it is rejected
because the grammar recognizes only the `Open` prefix. This is a deterministic
selection failure before ledger admission, not proof of a failed submission.

Continue the same bounded-metadata decision by accepting the observed `Merged`
PR state with the existing positive canonical count and singular/plural checks.
Keep exact composite-name reconstruction, catalog uniqueness, passive child
boundaries, and opened-session identity verification. Do not infer support for
other unobserved states or relax unknown metadata. Use synthetic fixture titles.

Verification plan: replay the captured label shape through the production label
reader and matcher, demonstrate failure before the change, and cover malformed
merged counts, unrelated composite names and non-unique titles. Run the Rust
sidebar suite, relevant agent tests, public-content check and ADR gate. Packaging,
notarization, OTA publication, installed versions and native send acceptance must
be recorded separately; no user message is replayed by the regression test.

Source replay failed before the grammar change, then all ten production sidebar
tests passed. The full agent library suite passed 224 tests (four pre-existing
ignored). Public-content and whitespace checks passed. Release preparation bumps
both Apple platforms to 1.0.83 (74); signed builds, publication, installation
and native-send acceptance are still pending at this source commit.

### Published and installed 1.0.83 (2026-10-22)

- Clean source `e5ec0e85afc3ce4d6f20b26d23e5c55a70dfdc1c` from PR #3
  built locally for macOS arm64 and iOS arm64, version 1.0.83 (74).
  The source replay and 224 passing agent tests above establish the parser
  correction; they do not establish a new native provider receipt.
- Apple notarization `b12050f1-7df7-4e35-b30e-46cc1cc05835` was Accepted.
  Strict signatures, staple validation and Gatekeeper passed. An initial
  signing attempt failed because a timestamp was unavailable; a bounded retry
  succeeded without removing the timestamp requirement.
- Both platforms were published. Independent public downloads, including the
  installation page's content-addressed iOS asset, verified 1.0.83 (74).
  macOS ZIP SHA-256:
  `780d2ee23b69700597709084f4b1666e1573fca9280188e7319f29eea9a6ee22`;
  iOS IPA SHA-256:
  `fa284b3132aa234eb860c7fa4a7adc393370a1e35359d0e5a79d576dfedc3df3`.
- All three test Macs acquired the Rust update lease and installed the release.
  Device identities remained unchanged. Each reports 1.0.83, Accessibility
  granted, relay registered and Vnet up. Installed daemon SHA-256 is
  `f66585b86efad2266ffd1b25f70d645e09764899ad4a9d663b6d9677cbdb8603`
  on all three devices; strict signature, staple and Gatekeeper checks pass.
- One installation initially left no daemon socket: Launch Services reused an
  existing GUI process invoked with a lowercase executable spelling and a CLI
  argument. Starting a new GUI instance with normal `open -n` restored the
  ServiceManagement job and Vnet. The existing process and terminal session
  were not terminated. Preserve the initial failed installation result and
  subsequent recovery evidence separately. This is operational recovery, not
  a new source fix or acceptance of the startup-retry fault scenario.
- After installation, all six directed Mac RPC paths and all six directed
  virtual-IP ping paths passed. SSH reached the two remote test Macs over
  their virtual addresses. The other Mac's port 22 refused connections both
  locally and over Vnet during this inspection; its SSH service was not changed.
- Other log findings remain open: two pre-ledger Codex multi-composer refusals,
  transient Claude question-panel lookup timeouts, and intermittent Codex Cloud
  request errors, including an error after this rollout. Already-answered
  question rejections retain their protection. These observations do not yet
  establish a source defect or justify replaying a provider operation.
- No chat was replayed for this inspection. Installed native-send acceptance,
  physical iPhone installation, phone log inspection, and reproduction of the
  original kernel/network fault remain unverified. iOS OTA publication is
  independently verified. GitHub Actions remained disabled; no Xcode Cloud or
  external TestFlight testing build was started. No historical OTA files were
  deleted.

### Merge-conflict badge regression (2026-10-23)

Unlocked acceptance on installed 1.0.88 exposes the actual target as an exact
passive title plus `Merge conflicts · 7 pull requests`. Both application and
sidebar traversals complete, but the metadata grammar rejects this new state.
The exact-ID fallback added in ADR 0039 dispatches once but does not change the
resolved surface; its verification correctly rejects before any draft or send.
This establishes the current zero-match cause. It does not prove why the
provider's URL route was ineffective or reconstruct the earlier renderer state.

Continue the existing metadata decision: admit exactly the observed
`Merge conflicts` prefix with the same canonical positive count, noun agreement,
full composite-name reconstruction and unique catalog title checks. Do not
accept arbitrary PR states or weaken exact-session verification. Preserve the
safe failure of unavailable exact-ID navigation.

Before implementation, add a synthetic capture regression using the production
label reader and matcher and show it fails. Cover singular/plural counts,
malformed numbers, injected suffixes, non-unique titles and unrelated composite
names. Then run the agent suite, platform compile checks and repository gates.
Build the corrected installed version and repeat navigation without sending.
Record source, release and actual navigation results separately. If a macOS
update is published, publish iOS with it under the existing release policy.
