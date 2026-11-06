# ADR 0153: Enforce registered task ownership and expose scoped Multica browsing

- Date: 2026-11-06
- Status: Accepted for the implementation below; full service and device acceptance pending
- Related: [ADR 0141](0141-multica-service-and-dsh-app-bridge-design.md),
  [ADR 0149](0149-multica-ownership-reservations.md),
  [Multica architecture](../architecture/multica.md)

## Context

The durable ownership registry exists, but ordinary Agent catalogs and direct
RPCs do not consult it. UI-only separation leaves cached IDs, searches, watches,
receipts and notifications exposed. The workbench cannot select a remote service.
The deployed customized Multica CLI does not expose the public version's account
identity command; its exact account and scheduler contracts remain unverified.

## Decision

Shared Rust enforces existing registry entries at ordinary Agent boundaries.
Native provider discovery supplies canonical store identity and provider-declared
ancestry. Catalog/search results are filtered before pagination, nested children
are projected recursively, direct session and managed-terminal requests are
checked, asynchronous reads are rechecked before publication, and active watches
are terminated on ownership changes. Missing discovery can only deny a known
owned ID, never establish a binding. No classification uses title, prompt or cwd.
Clients clear revoked catalog/message projections and invalidate outstanding
requests. Cached history is not shown before an authorized initial watch event.

Add default-off, host-local sharing configuration with explicit workspace IDs.
Remote browsing uses independent `multica.view` authorization and filters the
workspace list before selecting or querying a workspace. Recheck policy and
roster before returning data. The descriptor contains no host paths, installation
logs or credentials. Mac and iPhone render a Rust-owned device selection; offline
and revoked targets clear content, and backgrounding invalidates remote results.
Only the host may administer its sharing configuration.

Append `multica.view` and `multica.control` to the existing canonical Scope order.
Every active roster member must prove support on its authenticated PeerLink before
new permissions are issued; offline and old members block issuance. A bounded,
fixed-payload `roster-scopes` metadata service performs this check, including on
phones that cannot accept general daemon RPCs. No new port or phone daemon service
is introduced. Initial enrollment cannot include the new scopes. Control requires
view in grants, but the current descriptor always reports control unavailable:
no task operation endpoint is delivered in this change.

## Alternatives

Swift-only filtering leaves direct APIs exposed. Forwarding as a local owner loses
the remote principal. Agent or admin scopes grant broader access than intended.
Build strings are not proof that all roster consumers understand new encoding.
General phone RPC admission would broaden the phone's service surface merely for
a version check. Guessing deployed account identity or replaying unknown App
creation could bind or modify the wrong personal session.

## Consequences

This is a partial implementation of ADR 0141, not acceptance of P1–P4. Registry
writers, verified legacy migration/quarantine, explicit existing-session adoption,
and the binding-scoped task transcript path remain pending. Consequently, the
isolation enforcement applies to registered sessions; existing scheduler bindings
are not automatically imported or claimed to be separated. Ordinary sessions with
no ownership entry retain their existing behavior.

The remote workbench exposes selected-workspace task/agent/runtime metadata only.
DSH App create/resume scheduling, task credentials and callbacks, approvals,
interrupt, execution arbitration and remote task control remain pending. The
existing Codex/Claude bridge is not changed or reused as a DSH CLI fallback.
Completing these paths requires the deployed customized CLI/daemon source or an
exact authenticated API contract. The user has been asked for this prerequisite.

## Verification

- Source: focused store, policy, scope-codec, daemon browse and app reducer tests;
  real QUIC authorization tests cover independent grants, phone compatibility,
  offline-member issuance refusal, workspace boundaries, local-only administration,
  parent/child ownership, direct-ID denial and idle-watch termination. Background,
  disconnect, revocation, stale-response and cache invalidation regressions are
  included. Existing DSH controller, terminal, queue and receipt regressions pass.
- Static checks: strict Clippy for the seven changed Rust crates, public-content
  and ADR gates. The pre-existing layer-check discrepancy (`muster-ffi` depending
  on `muster-paths`) is outside this change; those files are unchanged.
- Builds: shared MusterUI target compiled for macOS and arm64 iOS Simulator.
- Signing/notarization, publication and installation: not performed.
- Runtime/device acceptance: no physical Mac-to-Mac/iPhone task session or DSH
  scheduling acceptance is claimed. Prior isolated App evidence remains in ADR
  0149 and does not establish the missing deployed Multica contract.
