# ADR 0222. Windows desktop chat control and presence

- Status: In development
- Related: ADR 0006, 0220, 0221

## Decision

Drive signed provider windows in the interactive session through UI Automation.
Share conversation resolution, input fencing and send receipt handling with the
macOS adapter. Obtain presence signatures separately from the daemon identity,
using a persisted, non-exportable TPM P-256 key with mandatory interactive key
protection. Refuse registration or use when the provider permits silent signing.
Keep the existing presence wire format and administrator enrollment process.

## Verification

Exercise the adapters on Windows, including unavailable and canceled operations,
and verify the installed MSI and running network member after upgrade.
