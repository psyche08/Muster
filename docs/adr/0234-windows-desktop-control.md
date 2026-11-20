# ADR 0234. Windows desktop chat control and presence

- Status: In development
- Related: ADR 0006, 0232, 0233

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

## Inspecting the local key

The original Windows toolbar action registered a candidate and always suggested
administrator enrollment afterward, including when the binding already existed.
Make the toolbar action a read-only query. Keep explicit TPM registration in
Settings. Both operations use the bundled Rust client's existing DevicePresence
RPC and display its response. Rust formats the candidate and signed-roster
enrollment fields independently. Preserve the CLI's default output for scripts.
A live status query supplies no signature challenge or verification evidence.

Verify the CLI report for absent, pending and enrolled candidates. Compare the
installed toolbar result with a read-only CLI query on the same node, then
record signature verification and any administrator enrollment separately.
