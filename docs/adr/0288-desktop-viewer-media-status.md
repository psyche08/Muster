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

A loopback RTSP fixture returned 503 to the packaged native viewer. The viewer reported the failure immediately, but its process stayed alive while the parent retained the input pipe. Closing the pipe released the pending CRT stream read and allowed process exit. Read the private control pipe through Win32 and test process exit while the parent still holds its writer. This exercises the failure path independently of Mac capture.

On Windows x64 the same packaged-viewer test timed out after 15 seconds on the CRT version. The Win32 pipe version returned the expected RTSP 503 status and exited with the writer open. The loopback, bootstrap, decode and paint checks also passed. A real paired display and pointer receipt remain pending.
