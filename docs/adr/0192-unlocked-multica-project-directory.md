# ADR 0192 – Run an enrolled Multica project in its registered Codex directory

- Date: 2026-11-12
- Status: Proposed
- Related: [0066](0066-native-codex-project-identity.md), [0187](0187-trusted-multica-dispatch.md), [0190](0190-background-desktop-admission.md), [0191](0191-native-multica-receipts.md)

## Context

Multica allocates a new working directory per run. Assigning a project in the
service does not give Codex that project's local directory. The executor needs
an explicit local binding to reuse a working window while the owner types.

## Decision

Allow the owner to associate a Multica project UUID with a canonical local
directory in the existing owner-private dispatch enrollment. For an unlocked
initial direct assignment, independently verify the service issue, enrolled
principals, runtime, live run and original per-run directory before selecting
the mapped directory. Submit only the issue text obtained from the service.
The normal Codex selected-root, window admission and unique receipt checks
apply. Keep the bridge, delivery ID and task credential bound to the run.

## Alternatives

Treating the directory in an issue description as routing authority would
let remote text choose local files. Registering every run through the folder
panel changes window order. Moving the pointer back after a UI action would
hide a disruption from an endpoint-only measurement.

## Consequences

The optional `unlocked_projects` object in the host enrollment maps service
project UUIDs to absolute directory strings. An owner choosing the same
directory for successive issues grants those issues access to shared project
files. Unmapped projects use their ordinary run directories. Locked creation
uses its original per-run authorization.

## Verification

Exercise mismatched principals, directories and projects, receipt matching,
and unchanged behavior without a mapping. Sample a foreground editor from
before service assignment through both final receipts, including system-routed
input, window IDs, pointer coordinates and mouse events. Distinguish observed
cursor movement from the source of an event.
