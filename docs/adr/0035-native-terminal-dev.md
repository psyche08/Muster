# ADR 0035: Select native terminal hosts and isolate local development

- Date: 2026-10-23
- Status: Accepted
- Related: [ADR 0033](0033-managed-terminal-compatibility.md), ARCHITECTURE §3.6.3

## Context

Automatic terminal selection exposes only one native application. New-window
requests cannot choose a host, and managed terminals offer only copied attach
commands. The development executable also defaults to production state and keys.

## Decision

Shared Rust registers both installed native adapters, namespaces their public
session IDs, aggregates their inventories and reports per-source failures.
Explicit new-window requests select an advertised source; unknown sources fail.
Legacy unqualified IDs still address only the primary adapter. Live iTerm2
connections report transport closure directly; input no longer preflights with a
session-list round trip. Disconnect ends idle output/list watchers without replay. Ghostty lock and
lease checks resolve the source from the exact ID, including secondary adapters.

Native creation opens a real window. A managed terminal may be attached by
starting its generation-checked command as that new window's initial command,
never by typing into an existing shell. Native windows remain independent views;
closing a view does not terminate the managed shell. Initial commands and host
selection are Rust-owned. This supersedes ADR 0033's copy-only attachment entry
point; its viewport policy and manual copy commands remain supported.

The local development launcher uses a separate bundle, software identity and a
fresh private data/socket directory. It never installs over Muster, starts a
production launch agent, or publishes an update. Native application permissions
and iTerm2 API setup remain explicit user-controlled prerequisites.

## Alternatives

Selecting a single auto adapter hides other running applications. Inferring
session identity from titles or directories can address the wrong shell.
Typing attach commands into arbitrary existing windows risks program input.
Reusing production state makes a development test mutate the installed service.

## Consequences

Existing native sessions keep their adapter capabilities. No live program is
migrated into tmux. Managed shell persistence remains on the dedicated tmux
socket. Creating native windows is not retried after an ambiguous failure.
No new network port, permission bypass or production release is introduced.

## Verification

Source and local verification:

- The initial full terminal/daemon/app-core suite passed 206 tests. Subsequent
  targeted runs passed 169 library tests and both native RPC integration tests,
  covering secondary Ghostty lease routing, partial inventory failure, final
  watcher errors, opaque IDs, deduplicated creation and idle stream cancellation.
- Four opt-in real-tmux tests passed on private sockets. A simulated native host
  received the exact managed initial command once across an RPC replay; stale
  handles were rejected before opening another window. Separate PTY clients
  executed regular, iTerm2 control-mode and native initial commands against the
  same shell and verified disconnect persistence.
- The iTerm2 WebSocket/protobuf fixture observed one inventory request followed
  by two inputs and one creation request, with no input preflight scans.
- Swift bindings were regenerated; MusterUI and the isolated MusterDev app built.
  Strict Clippy, Rust formatting and layer checks passed before final integration.

Local runtime: the dev bundle started its own software-identity daemon on a
private socket; read-only RPC observed both native sources and their setup
errors. No production app, launch agent, virtual interface or release was changed.
The operator chose simulation-only acceptance for this turn: real iTerm2 API
and Ghostty Automation/Accessibility acceptance remain pending. A downloaded
external iTerm2 test copy is local build material only, never repository content.

Signing: ad-hoc local bundle only; certificate signing/notarization not run.
Publication/installation: neither macOS nor iOS was published or replaced.
Physical-device/GUI/latency acceptance: pending; simulated results do not prove
those paths. Hosted Actions stays disabled under ADR 0016.
