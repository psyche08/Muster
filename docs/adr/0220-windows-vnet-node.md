# ADR 0220. Run a network-only node on Windows

- Date: 2026-11-16
- Status: Accepted for development
- Related: ADR 0004, ADR 0008, architecture §3.5 and P4

## Context

Windows needs the virtual network before the desktop application integrations.
The ordinary daemon starts those integrations and its local control plane.

## Decision

A foreground Windows executable composes the shared roster, QUIC node and vnet
packet loops with Wintun. It refuses application and forward streams at stream
admission. An enrolled node signs with a persistent per-user P-256 key in the
Microsoft Platform Crypto Provider. Enrollment uses the existing invite and
administrator approval. Store accepted roster updates and stop the adapter if
its signed local membership or address changes.

## Alternatives

Starting the complete desktop daemon brings services outside this port. A
separate Windows packet router would duplicate the shared authorization logic.

## Consequences

Wintun requires elevation. Run the executable from an administrator-owned
directory with the signed Wintun DLL alongside it. There is no system service
installation in this first port. Restart the process after an address change.

## Verification

Build the Windows executable, exercise shared networking checks and verify the
driver, address and route on Windows. Enrollment and peer traffic require an
independent member. Verify these separately from an adapter startup.

## Running the first Windows build

From `src/muster`, build with `cargo build --locked --release -p muster-vnet`.
Place the executable and the matching architecture's signed `wintun.dll` from
[Wintun](https://www.wintun.net/) together in an administrator-owned directory.
In an elevated PowerShell, run `.\muster-vnet.exe --identity` to obtain the
device ID. Use a private invite file for first enrollment:

```powershell
Get-Content -LiteralPath $inviteFile -TotalCount 1 | .\muster-vnet.exe --join
```

Match the printed ID to the pending device on the network administrator, then
approve that member. Use `.\muster-vnet.exe` to resume the same network under
the same Windows account. Ctrl+C stops the interface. This first configuration
permits replies to outbound flows and ICMP under the shared packet policy.
Unsolicited inbound TCP and UDP ports start closed.
