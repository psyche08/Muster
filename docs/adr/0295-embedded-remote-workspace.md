# 0295. Embed remote desktop in the Windows Remote workspace

- Status: Accepted
- Supersedes the Windows window decision in [ADR 0293](0293-desktop-host-idle-and-viewer-window.md)
- Related: [Desktop takeover](0294-desktop-takeover-confirmation.md)

## Context

Desktop viewing should share the application's main navigation. A separate top-level viewer makes it difficult to alternate between remote work, Agents and Terminals.

## Decision

Place Remote immediately after Terminals in the Windows tab group. Shared Rust projects eligible paired desktop devices, selection, connection availability and any pending takeover. The shell mounts the native media child in a stable panel in the main window. Selecting a device never connects by itself. Switching tabs hides the panel while keeping an established stream. Disconnect and application shutdown end its lease. The device details shortcut opens the same workspace with the peer selected.

## Alternatives

A separate viewer adds an unwanted application window. Recreating the video surface on each render or tab switch would interrupt a passive stream and lose the native parent handle.

## Consequences

The workspace can return to a live stream without another host grant. Desktop access remains enforced on the host.

## Verification

Check selection, explicit connect, occupied and cancel states, tab switching, stable parent handle, disconnect and host expiration. Verify native embedding in the installed Windows application independently of reducer checks.
