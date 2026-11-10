# ADR 0173: Connect CLI sessions directly to the native provider protocols

- Date: 2026-11-10
- Status: Accepted, installed-device acceptance pending
- Related: [ADR 0093](0093-app-and-cli-capability-control-design.md), [ADR 0064](0064-multica-native-runtime.md), [architecture](../ARCHITECTURE.md)

## Context

Both desktop and headless CLI sessions are product entry points. The CLI host
already speaks Codex app-server and Claude stream-json over stdio. Its clients
nonetheless reconstruct tabs, paged history, fingerprints and operation receipts
through a second JSON RPC interface inside the same daemon.

## Decision

Remove that internal RPC interface. Shared Rust calls the owning native runtime
for inventory, stored history, creation, send, interrupt and prompt replies.
The runtime owns its SQLite store and child processes and checks the native
turn-start receipt or user-message echo on send. The interrupt path waits for
the native control reply. A prompt response consumes its callback once and
reports write failure as an unknown outcome. Prompt answers bind to the pending item revision
and the current child generation. The daemon operation ledger owns replay.

Preserve the deployed CLI session namespace, capability, scope, creation RPC
and journal location. Existing saved sessions, older peers and delegated tasks
continue using their original identifiers. The desktop entry point keeps its
provider-specific control path.

## Alternatives

Removing the CLI entry point would remove headless operation. Replacing the
store would require migrating provider resume IDs and existing operation
bindings. The existing journal is sufficient for direct native integration.

## Consequences

Each CLI session resumes through its provider's own thread or session ID.
The agent service projects journal changes into the existing conversation
timeline. Old wire and storage names remain stable compatibility identifiers.

## Verification

Exercise both native stdio protocols with the subprocess fixtures. Check
restart, history, stale prompts, receipts, interruption, environment handling
and daemon permissions. Verify the architecture and public-content gates.
Installation and interactive provider acceptance require separate evidence.

The installed Codex and Claude CLIs both completed their native handshake
using temporary provider homes. No model turn was submitted. The upgrade
fixture opens the old journal, reads its history, expires the old callback
and checks the provider thread ID on the subsequent resume. Provider actions
after a live model turn and installed-app acceptance remain unverified.

The public-content check passed. The dependency layer checker reports the
existing `muster-ffi` dependency on `muster-paths`, which is missing from its
allowlist on the base revision as well.
