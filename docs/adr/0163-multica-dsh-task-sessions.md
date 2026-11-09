# ADR 0163: Create and reopen task sessions in the running DSH App

- Date: 2026-11-09
- Status: Accepted for attended host workbench operations; device acceptance pending
- Related: [0161](0161-multica-task-session-isolation.md), [0162](0162-multica-workbench-operations.md), [service design](../MULTICA_SERVICE_DESIGN.md)

## Context

The workbench can manage issues but cannot create or reopen their native App
sessions. The installed host exposes create, exact session observation and
resolveAgent (live or cold resume). The bundled CLI starts separate profiles.

## Decision

Allow explicit local, unlocked workbench actions against a freshly read issue.
Resolve the authenticated CLI principal to one exact workspace member ID and
bind the service identity to that ID and HTTPS service. Allocate one attended
session slot per issue; it is not an upstream scheduler run. Reserve a fresh
native ID before touching the host. Negotiate versioned task methods only with
the verified desktop plugin. Check the profile store, exact ID and canonical
directory in host facts. Reopen by the persisted binding only; never create on
resume, adopt a personal session, execute a prompt, or launch another runtime.
A lost create reply can be reconciled only by observing the exact reserved ID
on an explicit reopen. Reject ambiguous identity, mismatched receipts, locked
hosts, old plugins and missing App sessions. Use the existing operation ledger.

## Alternatives

Starting the bundled CLI opens a different runtime. Repeating create during
restore can mint a second session. Guessing identity from the issue title,
user display name, or project directory can claim another user's work.

## Consequences

Host users can prepare and reopen sessions without launching automation.
Automated task dispatch, delegated task credentials, prompts and completion
reporting remain separate; session creation never reports an upstream run.
Remote users cannot use these local administration actions.

## Verification

Exercise exact host create and cold restore, existing-ID refusal, receipt and
profile mismatch, old/offline plugin refusal, account/workspace isolation and
duplicate/unknown outcomes. Compile both Apple UIs and separate isolated host
evidence from production installation and device acceptance.

The opt-in `node src/muster/deploy/deepseek-harness/test/installed-task-host.mjs`
probe passed using the installed App's desktop-host entrypoint, a disposable
profile and no personal sessions: create returned the reserved ID, duplicate
create was refused, the restarted host had zero live agents, inspection kept
it cold, missing-ID resume failed, and exact resume attached only the original.
A separate read-only check verified the running GUI parent's signature and
launch identity. Its installed plugin did not yet offer the new methods.
The configured Multica container's authenticated status (written to stderr)
and exact workspace-member lookup were verified without exporting credentials.
Neither a production task mutation nor phone control was exercised.
