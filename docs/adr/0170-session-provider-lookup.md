# ADR 0170: Read the session provider without projecting its descendants

- Date: 2026-11-10
- Status: Accepted
- Related: ADR 0148, ADR 0161, ADR 0168, ADR 0169

## Context

A sampling profile of a desktop daemon serving catalog pages showed visibility checks spending most of their samples in descendant activity aggregation. Each row, child and referenced parent triggered full detail projection just to read its provider.

## Decision

Resolve the session's provider in the Shared Rust agent service using the same native catalog, cloud record, desktop chat identity or Orca entry as detail projection. Use this lookup for provider authorization in request admission and catalog filtering. Continue consulting current grants and task ownership on each check. Keep the page contents, live update interval and nested filtering rules.

## Alternatives

Caching visibility across requests would require invalidation on grant and ownership changes. Further slowing catalog watches would also delay real changes and leave initial reads expensive.

## Consequences

Provider checks no longer aggregate descendant activity or enumerate children. Actual detail requests still produce the full projection. Package the change as desktop build 1.1.30 (120) for direct installation.

## Verification

Compare provider resolution for native, cloud, desktop chat and unknown sessions. Exercise provider revocation and ownership changes during open watches. Compare installed daemon CPU time before and after on three desktops and inspect the local session list. Signing, installation and runtime measurements are separate from source checks.

In a debug build, four alternating measurements over the same 201 synthetic child records took 101–105 ms with detail projection and 123–127 µs with provider lookup. Every lookup returned the expected provider. These times cover only provider resolution.
