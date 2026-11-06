# ADR 0149: Reserve exact native identities for Multica task ownership

- Date: 2026-11-06
- Status: Accepted; storage foundation and isolated App-host evidence verified
- Related: [ADR 0141](0141-multica-service-and-dsh-app-bridge-design.md),
  [service design](../MULTICA_SERVICE_DESIGN.md), ADR 0094, architecture §3.6.5

## Context

The next implementation stage needs a durable ownership authority before exposing
DSH App creation or shared task sessions. Existing native task files omit complete
service/workspace identity. Their hashes cannot supply missing identity. Installed
DSH App 0.2.1-rc.5 has a host create API that accepts a caller-provided native ID;
its existing Muster plugin has no creation or durable turn contract.

## Decision

Add a transactional Multica reservation and ownership registry to `muster-store`.
The caller supplies verified task/service/device identity, requester, operation,
run, role, profile/store, canonical cwd and a reserved native session identity.
The registry validates complete bounded identifiers and exact retry context;
operation keys include the requester. A session belongs to one task independently
of provider; equal IDs from different stores/devices/providers are distinct.

Reserve the native identity durably before any provider action. Reservations are
visible to ownership queries even before a create receipt arrives, supporting the
future catalog publication barrier. On process restart unfinished reservations
become unconfirmed. Only exact verified receipt facts bind a session. Unknown
outcomes are never replayed or removed. Conflicts retain their evidence. Ended
bindings remain task-owned. No automatic adoption, migration by directory/title,
reassignment or authority widening is added.

This slice provides the registry and an isolated App-host contract probe. It does
not yet wire an adoption RPC, catalog filtering, scheduler execution, Multica UI
or cross-device service. Those paths must consult the same authority before any
new task runtime is advertised. Existing single-task grants are unchanged.

Pin the upstream DSH JSONL contract to
`multica-ai/dsh-multica-runtime@571d7c036d15f58ee88c89fd80afb0a6c7f0023c`, protocol 1.
Treat installed App-host evidence separately from independent `dsh --stdio`
success. The probe uses a fresh profile and synthetic prompt only, without account
credentials, a production profile, business tasks or service publication.

## Alternatives

A Swift row filter leaves old clients and direct access unchanged. Reconstructing
ownership from a title/cwd or hashed task directory can bind personal sessions.
Persisting only after creation allows a directory event to escape its ownership
barrier. Repeating creation after a lost response may create another conversation.

## Consequences

The authority can survive crashes and distinguish same-ID sessions and requesters.
It establishes no remote permission by itself. P1 still requires all directory,
search, Pin, notification, direct-access and cache paths to use it. P2 still needs
verified App turn/model/task-credential contracts and a separately bounded grant.
P3/P4 require staged scope compatibility, policy and physical acceptance.

## Verification

The complete store suite passed 13 tests, including six new ownership cases.
Strict store Clippy passed. The installed App-host probe verified exact reserved
creation, prompt acceptance and read-only receipt recovery after restart; its
model turn ended in error. Detailed limits and reproducible fixture are recorded
in [P0 evidence](../validation/MULTICA-DSH-P0-2026-11-06.md).

The existing plugin suite passed all 16 tests with local socket access. Syntax,
focused Rust formatting and diff checks passed. The repository layer check still
reports the pre-existing `muster-ffi -> muster-paths` map mismatch; both files
responsible for that mismatch are unchanged from the branch base.

P0 model-settings and task-credential gates remain open. No production App, GUI,
real Multica scheduler, platform build, signing, installation, publication or
cross-device acceptance is claimed. P1 integration and P2–P4 are not complete.
