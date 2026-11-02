# ADR 0094: Bind local App plugin operations and recover send receipts

- Date: 2026-11-02
- Status: Accepted; implementation and local App host verified
- Related: [ADR 0093](0093-app-and-cli-capability-control-design.md),
  [ADR 0074](0074-deepseek-harness-plugin-control.md), ADR 0046

## Context

The owner authorizes implementing the plugin design and validating it locally,
while keeping device-to-device interfaces unaffected. The existing DeepSeek
Harness bridge drives the App's sessions but generates a fresh native request
ID for each send, cannot recover an acknowledgement lost after acceptance, and
does not carry the operation's guard into its blocking socket worker.

Inspection of the installed host confirms that accepted prompts retain their
request ID as `source.rpcId` in the live inbox and session events. Reading these
facts does not require resuming or sending to a session. This makes a read-only
receipt route possible without replay or changing the remote Agent RPC.

## Decision

Implement a versioned, additive local App-plugin contract with explicit App
domain, instance epoch, capability declarations and bound operation replies.
Keep the existing local protocol-1 methods for installed daemons and validate
the additional contract before using it. An unsupported or malformed contract
never falls back to CLI or AX. CLI ownership, process lifecycles and scopes are
untouched; this plugin controls the original desktop App session only.

Shared Rust derives native prompt IDs from the original ledger operation and
session, validates plugin facts, and decides acceptance/recovery. The plugin
only projects host inbox/event evidence and forwards host calls. No plugin
business-state journal or device-facing protobuf/RPC/ABI changes are added.
Absent evidence remains unknown. A reconnect/status query does not resubmit.

Bind writes to the verified process and negotiated plugin epoch; retain live
authorization and worker-lifetime tracking through the blocking socket call.
Validate response correlation and treat malformed/ambiguous post-write replies
as unknown. A host method returning without an explicit accepted receipt is
not success. Lookup is read-only, bounded, session/request-bound and separately
authorized through the existing send-status path and ledger body check.

Retain the existing send, interrupt, approval and question functionality and
Locked Use rules. Do not claim model choices, native attachments, queue editing,
or other missing host capabilities merely because the plugin is connected.
Do not change the App/AX profile exclusivity in this implementation slice.

## Alternatives

Automatic retries after transport errors can duplicate messages. A plugin reply
cache cannot bridge a host/plugin crash boundary. A new remote status API would
unnecessarily affect other devices. Replacing an App session with a CLI runtime
violates the design's ownership boundary.

## Consequences

New daemons gain explicit local capability/epoch validation and read-only send
recovery when the installed plugin provides it. Existing device callers retain
the same requests, scopes, operation ledger and result types. Older plugins
retain their existing control semantics without falsely advertising recovery.
Host versions without the needed evidence shape return unknown.

## Verification

Use the existing Node and Rust suites for malformed envelopes, epoch changes,
wrong domain/session/operation, reply loss, native receipt lookup, cancellation,
approval races and preserved legacy methods. Exercise existing device RPCs over
an authenticated in-process link and compare protocol/schema files to main.
Validate locally against the app runtime and report live App acceptance separately
from synthetic fixtures. Run format, focused Clippy, architecture/public-content
and ADR checks. Record builds, signing, installation and physical acceptance
separately; no publication or paired-platform release is requested.

### Local evidence, 2026-11-02

The installed, signed DeepSeek Harness App runtime loaded the source plugin
in an isolated temporary profile. The opt-in `musterd` acceptance test passed:
authenticated existing device RPC accepted a synthetic prompt, native receipt
lookup found its exact request, a repeated operation returned duplicate, status
returned applied, and interrupt was accepted. Receipt observation may lag host
acceptance; the test waits boundedly without another send. No model credentials
were supplied, so this does not assert model completion.

The regular daemon integration test drops a host acknowledgement, keeps the
result unknown while evidence is absent, rebuilds the plugin link and device
connection, and recovers the original ledger row once evidence appears. Its
host send count stays one. Wrong-body and absent-ledger status requests do not
query the plugin. Node tests cover all existing operations, legacy requests,
binding validation, host exceptions, read-only lookup and concurrent answers.
Rust tests cover malformed replies, revoked authorization before socket write,
unknown state and existing locked-control/send-status/unconfirmed behavior.

No device-facing schema, generated bindings or ABI files changed. This delivers
the local plugin contract and reference-adapter portion of ADR 0093; its broader
AX migration, general route migration and additional host capabilities remain
separate work. The production desktop profile was not modified. No Apple app
packaging, signing/notarization, release publication, physical phone acceptance
or locked-screen physical acceptance was performed in this change.
