# ADR 0192 – Run an enrolled Multica project in its registered Codex directory

- Date: 2026-11-12
- Status: Accepted
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
the mapped directory. Recheck the owner enrollment and canonical path while queued and at the action boundary. Submit only the issue text obtained from the service.
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

The signed and notarized macOS executable completed two successive real
assignments in the already registered test project. Each had a distinct run
and one root Codex session with one user turn. Both exact model answers
matched the completed service results. The second creation matched the live
AX window retained for the first. The observed window ID list was identical
before and after both creations.

The foreground editor received all 195 system-routed tagged characters in
order. Its final text matched. All 713 editor samples had the same foreground
owner, key window, first responder, pointer position and normal-window order.
A separate observer compared AppKit and Quartz in 2,122 samples, with zero
position transitions, mouse movement events or drag events. Sampling covered
both assignments through their service receipts. Before the test, eight
seconds of ordinary desktop activity without a task produced 497 system
mouse events and 208 coordinate transitions. The earlier pointer-only record
did not retain event sources and cannot establish who moved the pointer.

The temporary local service-project mapping was removed after acceptance.
