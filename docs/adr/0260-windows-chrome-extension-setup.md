# ADR 0260. Expose Chrome extension setup and connection checks on Windows

- Date: 2026-11-21
- Status: Accepted
- Related: [ADR 0240](0240-windows-portable-distribution.md), architecture §3.6.2 and §3.7

## Context

The Windows Browser page shows a message and accepts JSON. When the native
host is registered but Chrome has not connected, the daemon tells the user
to check the connection. The Windows page lacks that action and the local
folder and extensions-page shortcuts available on macOS. It also opens with
an empty request even when the shared core supplied a draft.

## Decision

Render the shared BrowserView, including the target, draft, busy state,
connection result and setup guidance. Offer a status check on either target.
For the local target expose the packaged extension folder and Chrome's
extensions page. On Windows the per-user package updater registers the native
host. Make its recovery instructions visible when registration is missing.
Keep installation and Chrome's approval in the user's hands. Provide the same
ping and tabs request presets as the Apple UI. Choosing a preset only edits
the request field. An explicit Run uses the existing browser intent.

## Alternatives

Polling the registry or checking Chrome processes in the shell would duplicate
the daemon's setup state. Running a request when a preset is chosen would turn
an editing action into a browser operation.

## Consequences

The Windows shell opens local system UI and renders results. Shared Rust
continues to perform target selection, authorization, operation validation and
execution. The existing host registration survives GUI-only upgrades through
the ordinary-user updater. Loading an unpacked extension in a Chrome profile
remains an interactive Chrome setup step.

## Verification

Exercise local and remote projections, busy and disconnected states, draft
preservation, preset selection, the explicit request and the local-only
shortcuts. Verify the installed page against the node's read-only setup RPC.
Record native host registration, actual Chrome connection and packaging
separately.

On the Windows acceptance machine, the registered native host and the packaged
extension connected a temporary Chrome for Testing profile to an isolated
software node. A ping through the node returned the extension capability and
version. The ordinary browser profile reported a registered host and a
disconnected extension. These are separate observations.
