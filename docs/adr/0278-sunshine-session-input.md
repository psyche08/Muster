# ADR 0278: Start the downstream Sunshine child with one session certificate

- Date: 2026-11-21
- Status: Accepted
- Related: [ADR 0242](0242-muster-remote-desktop-engines.md), [ADR 0277](0277-paired-desktop-host-offer.md)

## Context

The loopback-only Sunshine snapshot still registers GameStream pairing endpoints and reads persistent upstream client certificates. A view-only grant must also constrain packets a modified Moonlight client might send.

## Decision

Start the downstream host with a bounded bootstrap on an inherited pipe. The parent supplies the authorized viewer's session certificate and chooses view or mouse after local consent. Keep that certificate in process memory, remove GameStream PIN pairing and configured application launches, limit launch to desktop capture and stop when the parent closes the pipe. The engine consumes only the mouse device class for a mouse grant. The Rust session owner remains responsible for stopping the child on lock, revocation or disconnect.

## Alternatives

Persisting a paired client in a Sunshine state file could leave the next process trusting a previous viewer. Passing a PIN through the web interface would add another listener and pairing flow.

## Consequences

Upstream interactive startup is unavailable in this downstream build. Local development requires the same pipe contract as the installed host. A terminal, TCP endpoint or command line cannot grant an engine client access through the product.

## Verification

Exercise absent, malformed and oversized bootstrap records, view and mouse modes, refused PIN routes, and forbidden process launches. Compile the downstream host and verify real frames and mouse with the integrated viewer independently.

An arm64 Mac compiled and linked the downstream Sunshine executable. The native bootstrap and Moonlight loopback tests passed. The host process accepted its selected client certificate, refused a second certificate and a configured-app launch, and exited after its owner closed stdin. The executable used a temporary development library prefix. No stream, screen pixels, or mouse movement was exercised by this handshake test.
