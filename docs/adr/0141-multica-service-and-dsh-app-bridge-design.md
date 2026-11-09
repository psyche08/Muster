# ADR 0141: Design a separate Multica service and a DSH App runtime bridge

- Date: 2026-11-05
- Status: Proposed; design only, no runtime implementation or device acceptance
- Related: [Multica design](../MULTICA_SERVICE_DESIGN.md),
  [architecture §3.6.5](../architecture/multica.md),
  [ADR 0053](0053-multica-native-workbench-and-runtime-routing.md),
  [ADR 0064](0064-multica-native-runtime.md),
  [ADR 0074](0074-deepseek-harness-plugin-control.md),
  [ADR 0094](0094-local-plugin-contract-and-receipt-recovery.md),
  [ADR 0097](0097-task-scoped-scheduler-grant-for-locked-use.md),
  [ADR 0120](0120-locked-control-without-a-per-operation-signature.md)

## Context

The owner requests a design for Multica sessions outside the Agents tab, a
Multica tab reachable through another device that explicitly offers it, and a
bridge to the existing DSH desktop App rather than an independent CLI runtime.

The current private Multica bridge supports Codex/Claude. Its workbench is
local-only. The existing DSH plugin drives App sessions and recovers prompt
receipts, but it does not expose the native creation contract this bridge needs.
Provider identity alone cannot distinguish a personal session from a scheduled
task. Connectivity alone cannot authorize exposing the host's Multica account.

## Decision

Record the proposed target and implementation gates in the linked design. This
ADR does not enable new behavior. If implemented, it supersedes ADR 0053's
local-only workbench boundary only for explicitly offered, authorized service
operations; container administration stays local. Existing accepted decisions
and their verification limitations remain in force until that implementation.

1. Shared Rust persists task/run/session ownership independently of provider,
   with exact device, service, workspace, profile, directory and native identity.
   Create reserves before acting and commits ownership from a provider receipt.
   Existing sessions require explicit local adoption. No title/cwd heuristics.
   Unknown outcomes remain unknown and are reconciled read-only, never replayed.
2. All catalog, search, notification and direct-access paths consult ownership.
   Multica sessions appear only in its workspace, including retained history.
   Personal sessions remain in Agents. Reuse the transcript renderer and store.
   Provider-native child relationships convey display ownership, not authority.
3. Adapt Multica's versioned DSH stdio protocol to the App plugin in Rust.
   Negotiate App process/epoch and capabilities under ADR 0094. Prove native
   creation, turn receipts, requested model options and task credential delivery
   before advertising them. An unavailable App never becomes a CLI task.
4. Keep private bridge authentication separate from task execution authority.
   Define a bounded Multica-specific grant using existing reservation/lease
   primitives; do not widen ADR 0097's single-task Codex/Claude grant. Missing
   verified task context or undefined grant limits block unattended scheduling.
5. Offer a default-off target-local Multica service with explicit workspace
   selection over the existing authenticated PeerLink. Propose separate view
   and control scopes; control also requires view and the existing provider
   control permission and local policy. No task data in general device metadata.
   All administrative install/login/configuration/adoption calls remain local.
6. Preserve principal identity through every internal call and reject old Agent
   APIs for task-owned sessions. Scope checks and local policy apply to requests,
   response publication and live streams. Service revocation stops access, not
   unrelated local scheduler work. Already accepted effects are not rolled back.
7. New roster scopes need a two-stage network upgrade: every roster consumer
   must understand their canonical encoding before any admin emits them. Keep
   existing enum order and signed bytes. An offline incompatible member blocks
   new-scope issuance; do not silently grant access through old Agent scopes.
8. App-core owns device selection, tab availability, stale-response rejection
   and revocation cleanup, including phone UI. Remote content is memory-only in
   the initial design and cleared on disconnect or confirmed revocation.
9. Retain current Locked Use policy. ADR 0120 removes per-operation Presence
   signatures for remote locked control; do not restore them here. Administrative
   authorization, cover, watchdog, relock and task-local scheduler limits stay.

## Alternatives

- Independent DSH CLI runtime: upstream supports it, but it does not operate the
  owner's existing App sessions and is not the selected execution destination.
- Hide rows in Swift: leaves search, old clients and direct RPC paths able to
  cross ownership boundaries and duplicates business logic in platform code.
- Expose the whole logged-in Multica CLI or plugin socket: transfers broad host
  authority and introduces a separate transport/access model.
- Infer ownership from paths, titles or task prompt text: cannot distinguish
  ordinary work, repeated runs or same-named sessions reliably.
- Add scopes immediately: older strict roster decoders may reject signed entries.
  Use staged compatibility instead of breaking membership synchronization.

## Consequences

The design separates product placement, service availability and authority. It
requires changes across Rust policy/store/protocol/daemon/app-core, a narrow DSH
plugin extension and generated platform bindings. It is not a tab-only patch.

Unverified native creation and task-credential delivery are explicit P0 gates.
Legacy hashed task directories may lack enough evidence for automatic migration;
retain a local unresolved-ownership view instead of inventing missing identities.
Network-wide scope compatibility is a deployment prerequisite. First remote
delivery is read-only; remote control remains a separately accepted later phase.

## Verification

This turn changes documentation only. Check relative links, whitespace, public
content and the ADR gate; application builds and provider tests are not evidence
for a design-only change and are not run for it. The design specifies focused
Rust/Node/real-link regression cases and separate physical acceptance for later
implementation phases, including crash recovery, old-client denial, revocation,
same-ID isolation, scheduler concurrency and actual Multica callback receipts.

No application code, signing, notarization, publication, installation, live App
mutation, real scheduler task or physical end-to-end result is claimed here.

Documentation validation: relative file links resolve; the staged whitespace
check passes; the public-content check reports 905 tracked text files and zero
findings. The committed ADR gate is required again before landing.

Later evidence (2026-11-09): [ADR 0163](0163-multica-dsh-task-sessions.md)
adds the plugin's task methods for the attended workbench, and
[ADR 0167](0167-deepseek-harness-native-session-creation.md) reuses
`task.create.v1` for `agent_native_create`, with a first message. Both are
verified on an isolated installed App host. That partly meets this design's
native-creation gate. Task credential delivery remains unimplemented, so this
ADR stays a proposal.
