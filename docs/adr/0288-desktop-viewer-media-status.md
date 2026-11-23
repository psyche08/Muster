# 0288. Report native viewer failures to the desktop session

## Context

An authorized paired viewer completed RTSP negotiation and opened its media sockets. It closed the sockets before displaying a frame. The receiving Mac then reached the initial media ping timeout. The viewer daemon had only a first-paint marker and reported a later timeout.

## Decision

Have the packaged Moonlight adapter report the failed startup stage or connection termination code over its existing private status pipe. Keep the first-paint event tied to a decoded frame drawn in the viewer. Shared Rust interprets the bounded status records and controls session teardown. Exclude media, peer addresses, credentials and general engine logs.

## Alternatives

Increasing the frame timeout would hide early terminations. Forwarding unrestricted engine logs could include private session material.

## Consequences

Session startup and post-paint termination can be distinguished at the normal Windows entry point. The authenticated transport and host permission checks continue to govern every connection.

## Verification

Check native build, bounded status parsing and actual paired device behavior. Successful media acceptance requires observing a Mac test window and its receipt of a click through the Windows viewer.
