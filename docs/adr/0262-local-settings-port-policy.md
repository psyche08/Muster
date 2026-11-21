# ADR 0262. Save the Settings port policy on the device that supplied it

- Date: 2026-11-21
- Status: Accepted
- Related: [ADR 0252](0252-windows-network-member-details.md), [target-local policy](../architecture/identity-and-trust.md)

## Context

Settings displays the policy read from the local daemon. The selected member
persists across navigation. Saving a policy previously used that selection
even inside Settings. Windows also omitted the port list and editor that macOS
presents under Open ports.

## Decision

In Shared Rust, route a Settings policy save to the local daemon. Keep the
selected member for navigation and retain the existing selected-device
administration flow outside Settings. The Windows dialog edits one port rule
per line. Save submits the edited list with the latest reported values of the
other policy fields. If the reported list changes during editing, the user
can reopen the editor to review the new list. The daemon parses and commits
the complete policy.

## Alternatives

Clearing the selected device on entry to Settings would affect Back navigation.
Adding a Windows port parser would duplicate the daemon's policy language.

## Consequences

Saving an empty list removes the configured port rules. Cancel discards the
draft. The operating system's firewall and service configuration are separate
from Muster's rules.

## Verification

Exercise a Settings save with a different member selected on an isolated
backend and read the resulting local policy. Verify that the daemon rejects
an invalid list without a partial write. Check draft editing, Cancel and
other policy fields in the Windows form. Exercise the packaged GUI against
an isolated node and use read-only checks against the installed node.

On 2026-11-21, the installed 1.1.94 Windows GUI saved one TCP and one UDP
rule on an isolated 1.1.70 node. After both processes closed, a 1.1.94 node
opened the same database. The fixed shortcut reopened Settings with the
original fingerprint and the new node version and commit. Both rules appeared
in the editor, and the full policy matched the value saved before restart.
The production daemon kept its process ID and policy throughout.
