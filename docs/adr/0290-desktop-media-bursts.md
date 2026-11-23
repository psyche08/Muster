# 0290. Buffer video packet bursts in the paired desktop broker

- Date: 2026-11-23
- Status: Accepted
- Related: [Desktop engines](0282-source-built-desktop-engines.md), [desktop session lifetime](0289-desktop-host-session-lifetime.md)

## Context

A paired Windows viewer painted the initial Mac desktop and delivered mouse clicks. The Mac acknowledged the clicks at two different coordinates but the displayed image remained unchanged. Reconnection could also end before the first complete frame. The capture engine groups up to 64 video packets per send. The Shared Rust receive route held 32 packets and the video socket queue held 16. A single producer turn can overflow either queue before its consumer is scheduled.

## Decision

Size the bounded session route and video channel to absorb multiple engine batches. Keep separate small control and audio socket queues. Keep delivery nonblocking so a full video queue cannot stop control or virtual network dispatch. Record queue saturation with counts only, without logging packet contents. Continue checking the active session and local socket owner at delivery.

## Alternatives

Increasing startup timeouts cannot recover missing fragments. An unbounded queue would retain stale video under sustained overload.

## Consequences

The receiver can absorb a burst while checking socket ownership. Sustained overload can still drop packets and requires measurement on the actual link.

## Verification

Exercise a multi-batch burst with a paused consumer and check that overflow remains bounded. Run the existing paired transport tests. Repeat live color changes and pointer input within one session and check reconnection using the persistent grant.

The Windows unit test delivered 257 fragments to a paused consumer, retained the first 256 in order and recorded one overflow. Four paired transport integration tests passed, including revocation and reconnection. The standalone manual two-host harness was skipped.

The complete x64 portable package was installed and both the GUI and daemon reported the new version. With the same signed Mac host, the open Windows viewer showed a new blue test window and its change to green after a Windows click. The host independently recorded the click. After closing the viewer and reconnecting through the paired device details, the viewer displayed the green window and the host recorded a second click at different coordinates. The saved view-and-mouse grant remained active. The Windows virtual adapter was up and both test pings to the peer succeeded. This was a device acceptance installation.
