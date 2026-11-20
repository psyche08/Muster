# ADR 0252. Display the selected network member on Windows

- Date: 2026-11-21
- Status: Accepted
- Related: architecture §§3.5 and 3.7, ADR 0240 and ADR 0251

## Context

Windows displayed the local node identity at the top of every member detail.
Two devices could thus appear on one detail page. Member rows omitted the link,
and selected phones received service shortcuts in the Windows shell. The
Network navigation button requested Home, which the shared core maps to Agents.

## Decision

Route the Network shortcut to Settings and focus its member list.
Open member details by ID from the roster with a click or keyboard activation.
Present only the selected member on that page. Draw link text, build details,
grouped access and service availability from the shared view. Keep navigation
and operations scoped to the ID that produced the displayed detail.

## Alternatives

Using the local identity as a fallback would attribute its metadata to a
different device. Deriving host capability from kind or scopes in WinForms
would duplicate the Rust decision.

## Consequences

The roster can distinguish the local device and show the reported link.
Missing and removed members can be presented without substituting another
record. Link telemetry can change while a keyboard user is selecting a row.

## Verification

Check duplicate names, changing link text, missing metadata and the service
host projection. Compare a local and a remote member with the normal installed
GUI. Check Back and keyboard activation. Access and service navigation can be
checked without modifying a grant or sending a provider request.

The Windows form was exercised with same-name synthetic members, a link text
update during keyboard selection, missing build metadata, a removed member
and shared service/access projections. The selected identity and action
visibility matched each input. In an isolated network, mouse activation and
Enter each opened the local member; Back restored the roster both times.
