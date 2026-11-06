# ADR 0150: Create desktop app sessions from another device

- Date: 2026-11-06
- Status: Accepted; source, tests and builds verified; device acceptance pending
- Related: ADR 0044, ADR 0045, ADR 0065, ADR 0097, ADR 0120; LOCKED_USE.md

## Context

Muster could open, read and continue Codex and Claude desktop (Code mode)
sessions, and it could create CLI sessions (Orca) on a member device. It could
not start a new desktop session. `AgentNativeCreate` existed for the local
scheduler bridge (ADR 0097), but the wire handler treated it as local-only, so
no member, not even the owner's phone, could ask a Mac to open one. A user who
wanted a new Code session in the Claude or Codex app had to walk to the Mac.

The desktop route is the Accessibility submission in
`muster-platform::macos::native_create`. It is not idempotent by itself; its
receipt is the unique new session in the canonical directory whose first user
message equals the prompt. Locked Macs need a Locked Use lease, whose lifetime
(`MAX_TTL`, 15 s from the challenge) also covers any authentication the
requesting device performs.

## Decision

`AgentNativeCreate` becomes a member operation:

- **Access.** It needs `Agent { provider, control: true }` for its provider and
  writes the console. It is no longer local-only. The daemon advertises
  `agent.native_create.v1` in `DeviceInfo.services` on macOS when it offers
  agents; clients check that service string first. An older Mac is told to update.
- **Unlocked Mac.** The existing `native_create::create` path runs unchanged:
  the provider lock and console guard are held, one submission is ledgered per
  `op_id` as `agent.native_create`, and the receipt is awaited for up to 15 s.
- **Locked Mac.** `LockedUseOperation` gains `native_create = 15`, so the
  request is bound into the challenge digest with its `op_id`, provider,
  directory and prompt. Like every Locked Use transaction, it also needs
  `locked.control` and the Mac's local locked-control switch (ADR 0120). Before
  any unlock, `native_creation` validates the
  request: the `op_id` is at most 128 bytes, the provider has a supported app,
  that app's bundle is in this device's `ax_allowlist`, and the directory exists.
  `native_provider` and `require_native_operation` reuse the same check, so it
  runs again for every OS call. Inside the lease, `native_create::scheduled`
  submits once and looks for the receipt for `IN_LEASE_RECEIPT` (3 s).
- **After relock.** `ReceiptWait` settles the creation without the console. It
  holds the directory's sessions captured before the run as a baseline. For up
  to `AFTER_RECEIPT` (45 s) it reads transcripts every 500 ms. A unique new
  session showing the exact first message confirms the creation's own pending
  or unknown ledger row, and the reply becomes `Applied` with that session id.
  More than one match binds nothing. Every poll re-checks the member's
  permission. This path never touches the desktop, the controller or
  `ledger.begin`.
- **Client (Shared Rust).** App intent `NativeCreate` (field 136) validates the
  request:
  - the Mac is reachable and is a desktop;
  - the app is Claude or Codex;
  - the directory is absolute;
  - the first message is not blank.

  The intent then sends one `op_request_from("native_create")` with a fresh
  `op_id`, through the usual Locked Use prepare/authenticate/execute flow.
  `AgentsView` gains these fields:
  - `native_create_hosts`;
  - `native_create_apps`;
  - `native_creating`, which allows one creation at a time;
  - `native_create_draft`.

  The outcome decides what happens next:
  - **Applied** opens the new session's chat after the Mac lists it.
  - **Rejected** keeps the draft so the person can correct it.
  - **Unknown** drops the draft, reloads the catalog and says the session may
    exist. A transport error after sending also counts as Unknown. Nothing is
    ever resubmitted.
- **Apple shell.** The agent list gains "New desktop session". It opens a sheet
  with a Mac picker, an app picker, an absolute directory field and a first
  message editor. Create stays disabled until all four are filled. A refused
  draft is prefilled. The shell holds no creation logic of its own.

## Alternatives

- **Keep creation local and point users to CLI sessions.** CLI sessions are
  not the desktop app's sessions, and they do not show in that app.
- **Open the app through a URL scheme.** Neither app offers a documented link
  that starts a Code session in a chosen directory with a first message. Such
  a link would also have no receipt to prevent a duplicate.
- **Extend `AgentSend` with a "new session" flag.** That would mix a
  conversation's delivery receipt with a creation's receipt and ownership rules.
- **Hold the lease until the transcript appears.** The lease bound is a
  security invariant, so this was rejected. Waiting read-only after relock gives
  the same receipt without keeping the Mac unlocked.
- **Retry an unknown creation.** The app may already have the session. Retrying
  could create a second session with the same first message, so the outcome is
  reported and nothing is submitted again.

## Consequences

- A member with Agent control of a provider can start that provider's desktop
  sessions on a Mac, unlocked or locked. The app must be in the Mac's
  `ax_allowlist`, and the directory must already exist. There is no remote
  directory picker.
- On a locked Mac, the requester's authentication and the submission share the
  15 s lease. Slow authentication expires the challenge, and nothing is sent. After relock the
  RPC can stay open for up to 45 s while the receipt is read from transcripts.
- An unknown outcome asks the person to check the session list. The ledger
  keeps the row, so the same `op_id` can never submit twice.
- Old Macs keep refusing remote creation with a clear update message. Old
  clients are unaffected.
- The Android and HarmonyOS shells and their generated protocol files are not
  updated. They do not offer this form yet.

## Verification

- Source and tests (2026-11-06, macOS arm64):
  - `muster-proto`: the locked operation round-trip, its turn, and digest
    changes for each bound field.
  - `musterd` lib:
    - Prepare refuses every invalid creation before any unlock.
    - A valid prepare binds a challenge.
    - The read-only receipt settles on the unique new transcript, excludes the
      baseline, and confirms the ledger row.
    - An un-ledgered operation stays unknown.
  - `musterd` access test: Agent control of the provider is required.
  - `muster-app-core`:
    - Validation happens before sending.
    - One creation runs at a time.
    - A rejection keeps the draft; an unknown outcome drops it.
    - Cancelled authentication clears the busy state.
  - The full workspace test run, clippy (`-D warnings`), the layer check and
    the public-content check.
- Builds: `swift build` and `swift test` (111 tests) for the Apple package
  against `libmuster_ffi`. `xcodebuild` builds `MusterUI` for the iOS
  Simulator. The Swift protocol files were regenerated with protoc 36.2 and
  swift-protobuf 1.38.1.
- Not run: the tests submit no real desktop session. Validation tests use an
  empty prompt or stop at prepare. Device acceptance is pending:
  - creating a Claude session and a Codex session on an unlocked Mac;
  - creating one on a locked Mac within the lease;
  - a late receipt confirmed after relock.
- Signing, publication and installation are not part of this change.
