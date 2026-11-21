# ADR 0279: Carry fixed GameStream engine channels within an admitted session

- Date: 2026-11-21
- Status: Accepted
- Related: [ADR 0242](0242-muster-remote-desktop-engines.md), [ADR 0277](0277-paired-desktop-host-offer.md), [ADR 0278](0278-sunshine-session-input.md)

## Context

Moonlight opens independent HTTPS and RTSP TCP connections and sends GameStream video, audio and control UDP packets. The existing session provides a single framed reliable stream and scoped lossy datagrams.

## Decision

Run a bounded multiplexer inside the established remote desktop session. The wire identifies fixed engine channels, never socket addresses or ports. Choose the engine port base locally. Only the viewer opens TCP channels. Bind to IPv4 loopback, carry media as datagrams, preserve TCP half-closes, and tear down sockets and tasks with the admitted session. The host parent provisions a session-only TLS identity, pins the child certificate, and makes the GameStream HTTPS launch request on the host. Only RTSP is bridged over TCP. The peer has no HTTP or HTTPS route to the child.

## Alternatives

A generic TCP or UDP tunnel would let the viewer pick other destinations. Treating RTP packets as a reliable stream would add head-of-line blocking during loss.

## Consequences

The handshake and viewer still need native integration. The broker is private to the session. It requires a running, authenticated local engine and can never establish membership or grant access.

## Verification

Exercise simultaneous reliable control and datagrams, invalid channels, bounded payloads, revocation, and disposal of active connections. Verify actual capture, rendering, and mouse delivery separately using the complete product.

On Windows, two concurrent fixture clients completed 24 RTSP-style TCP exchanges with half-closes over a signed PeerLink while a UDP payload made the round trip. The broker ignored a UDP sender outside the selected engine. Dropping the host offer stopped it. Windows and macOS both verified real local TCP and UDP ownership using their kernel APIs. These were synthetic engine payloads. Actual video decode and mouse acceptance remain separate.
