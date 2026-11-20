# ADR 0244. Select the terminal host on Windows

- Date: 2026-11-20
- Status: Accepted
- Related: ADR 0035, ADR 0043, ADR 0240 and architecture §3.6.3

## Context

The Windows terminal list omits device selection. In the aggregate view Rust
offers no terminal creation sources. On a Windows device with an empty local
catalog this makes a working ConPTY source hard to discover. The list also
renders loading and empty results as the same blank page. Two reachable peers
returned an explicit terminal view scope denial during device acceptance.

## Decision

Use the existing projected service devices and the typed OpenTerminals intent
for device selection. Keep partial device errors visible alongside sessions,
show loading and empty states, and display the session's source and dimensions.
Keep each row bound to its device ID and opaque session ID. Use the Rust
creation sources and busy flag for the New action. Display the projected
session status in the attached view.

## Alternatives

Using the network member list directly would include hosts that do not offer
the service. Automatically switching to the local host would conceal remote
sessions. Reusing the Agents device picker keeps the Windows interaction
consistent across both lists.

## Consequences

A user can open Terminals, select this PC and create its advertised built-in
terminal. The Windows shell presents the shared model and returns intents.
Rust owns reachability, access control and session lifecycle.

## Verification

Exercise the device menu with opaque IDs and repeated labels. Check the
ordinary Windows catalog entry, empty view, source action, session identity and
reported dimensions. Verify local create, input, resize and close against a
fresh ConPTY session. Record remote permission denials separately.
