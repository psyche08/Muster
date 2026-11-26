# ADR 0316. Verify the enrolled device identity outside the login session

- Date: 2026-11-26
- Status: Accepted for identity verification. Service integration pending.
- Related: [Windows network node](0232-windows-vnet-node.md), [Windows desktop integration](0235-integrate-windows-desktop.md), [Physical lock screen](0306-physical-lock-screen-remote.md)

## Context

After the first login of a boot, networking and Remote should remain available across logout with the original device identity, roster and grants. The Windows TPM device key belongs to the account that paired. The existing identity command can create a key and takes the network owner's lock. Neither behavior is suitable for testing another execution context while that node is online.

## Decision

Provide an independent check in the packaged Windows network executable. Supply the full device ID read from the paired node. Open the existing TPM key silently under the candidate account, compare the entire public-key ID, and sign and locally verify a fresh, domain-specific random challenge. Perform the check before opening any store, acquiring the node lock or starting any network components. Return failure if the provider needs interaction. Run the check again after logout to validate a new key open in the surviving context.

## Alternatives

Enumerating a public key does not exercise signing. Testing by starting another daemon conflicts with the active identity owner. Re-enrollment changes the device ID and invalidates the required continuity.

## Consequences

A network operator can test the account and logon type independently of the installed daemon. Success establishes local signing capability at the time of the check. Networking still needs a session-independent node with the original store, and Remote still needs the authorized physical display and input adapter.

## Verification

Use `muster-vnet.exe --verify-existing-device <full-device-id>` in each execution context. Obtain the 64-digit hexadecimal ID independently from the existing paired node. Exit status zero means the public key matched and the local signature verified.

Build the Windows target. Run the packaged command with the expected and a different full ID, and without an existing key. Compare the enrolled node and its grants before and after. Verify signing, a fresh paired RPC, vnet traffic and a physical viewer connection separately after the last user signs out. Device acceptance is pending.

The Windows x64 release binary passed seven cases in the enrolled account's existing logon session. The paired ID verified. Flipping a low-order bit outside the displayed fingerprint failed. Non-hex input, a short ID, a missing argument and conflicts with either provisioning option all returned usage errors. The live daemon PID, membership and grants matched before and after. A non-elevated attempt to register a separate S4U test task returned access denied before execution. A key-absent context, background signing and post-logout service operation still need device evidence.
