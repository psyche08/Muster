# ADR 0221. Windows desktop and MSI distribution

- Status: In development
- Related: ADR 0220 and architecture §3.2, §3.7

## Context

A network-only node already runs on Windows. Desktop operation additionally
needs the shared daemon, a user interface, local client access and an installer.

## Decision

Keep the Rust device, service and application state machines. Use per-user
local IPC for desktop RPC, reuse the TPM identity, and install versioned x64
binaries through Windows Installer. Installing files must not silently enable
agent control, replace a running network identity or modify SSH policy.

## Alternatives

A network-only MSI leaves the desktop services inaccessible. An unprotected
loopback TCP listener would give other local accounts the daemon owner's rights.

## Consequences

Both ends of local RPC must agree on identity and transport. OS-specific
integrations need separate execution and device acceptance.

## Verification

Cross-build the payload, test local RPC isolation and feature behavior, inspect
the MSI payload and exercise install, launch, upgrade and uninstall on Windows.
Track device acceptance separately from successful compilation.

The desktop shell exchanges protobuf intents, views and effects through the
existing C ABI. RPC uses owner-scoped local named pipes, and each client
checks the pipe owner's SID before sending a request. A separate elevated
Wintun helper connects to a random endpoint; the daemon checks the kernel's
client PID against the process it launched. ConPTY hosts the local shell.
The installer leaves the standalone-node runtime and user data in place.
