# ADR 0112: Ignore native catalog read failures in the interface

- Date: 2026-11-03
- Status: Accepted; verification pending
- Related: [ADR 0110](0110-native-groups-paging-and-session-entry.md), [ADR 0111](0111-native-organization-ax-and-file-confirmation.md)
- Renumbered from 0107 when [ADR 0107](0107-l4forwarder-in-muster-app.md) landed on `main` first;
  the decision and its evidence are unchanged.

## Context

Transient native Pin, Section and Project metadata read failures repeatedly appear
in the session list. The owner requests that these reads be silently ignored.
This supersedes only ADR 0110's unavailable-notice presentation decision.

## Decision

Shared Rust keeps native read failures internally, preserving the last complete
provider metadata snapshot when available. Missing initial metadata contributes
no grouping information. Do not project those notices into catalog responses or
promote catalog notices from older peers into UI errors. An older peer lacking
native pagination also falls back silently. Preserve actual transport errors,
search coverage notices and mutation rejection/unknown-result reporting.
Native mutations still require current readable metadata and file confirmation;
a cached display snapshot does not authorize a change.

## Alternatives

Repeated warnings obscure the usable catalog. Dropping the previous valid
snapshot makes transient reads move sessions between groups. Suppressing all
errors would hide failed mutations and transport failures. Retrying native UI
operations would violate the existing unknown-outcome rule.

## Consequences

A temporarily stale group may remain visible without an alert. A user-requested
mutation can still report that current metadata is unavailable. This is a Rust
presentation policy shared by desktop and mobile, with no platform fallback.

## Verification

Exercise repeated catalog responses containing read notices and retained group
metadata; ensure they do not add UI errors. Verify transport errors still appear.
Existing provider retention and fail-closed mutation tests remain required.
Record source checks, builds, device installation, OTA publication and native
operation acceptance separately. No device acceptance is implied by unit tests.

Source verification: 523 agent/app-core/platform library tests passed, with five
existing ignores; strict Clippy passed. The initial sandbox run could not create
Unix sockets; the same suite passed when local sockets were permitted. Mac/iOS
builds, installation and OTA publication remain pending for this candidate.
