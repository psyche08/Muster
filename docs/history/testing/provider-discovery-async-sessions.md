# Provider Auto-Recovery and Asynchronous Session Loading

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](../../PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

## Observed Problem

On 2026-09-11, the local App showed `Provider discovery: local: unavailable`, although the running backend's `/providers` returned HTTP 200 in approximately 0.3 ms, with both Codex and Claude available. Shared Local ABI reads and provider aggregation through the same backend also succeeded. Clicking Refresh in the App cleared the notice, followed by 363 local sessions; no restart or data clearing occurred.

The confirmed mechanism defect was that transport/cancellation/parsing errors in discovery requests were swallowed as unavailable, and `refresh_if_needed` scheduled no further reads after failure. The precise trigger for the first failure cannot be determined because old logs are missing. In addition, the original UI waited sequentially for options, sessions, and inbox under one loading flag; multidevice session aggregation displayed results only after every device returned.

## Fix

- Shared Rust retains attempt counts and monotonic-clock backoff state per provider source. Each explicit discovery allows at most three attempts: wait 250 ms after the first failure and 1000 ms after the second; stop after exhaustion, with manual refresh opening a new budget.
- Automatic retries do not reread healthy sources or those with requests in flight, and do not restore availability from stale display data. Rebinding clears the attempt budget and invalidates old tokens.
- The existing provider display ABI adds an optional `retry_after_ms` projection; null means stop. Mac executes Shared's plan without reimplementing retry decisions in Swift.
- Each provider and session source result is aggregated through Shared and incrementally projected to the UI. The first provider result can start an independent session task without waiting for provider discovery on slow devices.
- Session loading has an independent indicator; options/inbox run concurrently and no longer delay the session list. Source updates, workspace replacement, and exit cancel old session tasks, with generation checks rejecting late display results.
- Logs use the `workspace-read` category and `providers.transport / providers.apply / sessions.transport / sessions.apply` phases. Local errors record status codes, HTTP status, and transport type; Shared errors record status and kind; other errors record only type, without response bodies, credentials, or session content.
- Authorization, pairing data, Relay, and the backend protocol are unchanged; caches remain display data only.

## Validation

- Rust client: 118 tests passed, including 6 provider tests; Clippy `-D warnings` passed.
- Rust FFI: 94 tests passed.
- Swift CoreClient: 68 XCTest and 12 Swift Testing tests passed.
- Mac: 384 tests passed (an explicit complete rerun with `swift test --skip-build --no-parallel`).
- New Mac tests cover automatic recovery after the first failure, stopping after permanent failure, showing local sessions while remote providers block, incremental display while remote sessions/options block, and rejecting late provider results after exit.
- The first full concurrent run failed two existing timing tests: subprocess timeout and backend keepalive. Both passed in isolation, followed by a successful full nonparallel regression. Neither timing test was changed, and their concurrency sensitivity is not claimed to be fixed.
- Apple XCFramework build and Mac Swift compilation succeeded. XCFramework packaging emitted CoreSimulatorService connection warnings but succeeded; no iOS simulator was launched or accepted in this round.
- Repository-wide formatting checks exposed extensive existing formatting differences. Unrelated files were not mass-formatted; rustfmt ran only on Rust files modified in this round, and `git diff --check` passed.

## Deployment Boundaries

This round did not sign, notarize, or replace the installed App locally or on mac-b/mac-c, and did not repeat HarmonyOS physical-device tests. The fix requires rebuilding and installing a complete Mac package containing the new Shared core and UI; replacing only the backend is insufficient. Existing phone-session raw-stream errors and remote Owner initialization issues are not validated outcomes of this fix.

## Subsequent Local Replacement Installation (2026-09-11)

- The user subsequently authorized replacing the local installation. Shared Apple XCFramework, backend, Broker, and Debug App were rebuilt completely from clean source `800c8dd6f98e1d4424a980fc8ae9dca7933e8826` and signed with Developer ID Team `89LGY6BD53`; no notarization.
- `/Applications/AgentHalo.app` was replaced and AgentHalo's backend/Broker restarted. The original Debug launch argument `--debug-skip-local-auth`, pairing, and identity data were retained; the Muster tunnel and original Codex/Claude clients were not restarted.
- The installer retained the old package at `/Applications/AgentHalo.app.backup-20260911-225208`. New main executable SHA-256: `75ebb6fa9fc75b7bef3847f7030505a7b63bffb57ba8043fdd339020f5b424fb`; backend: `9b19f748b1b8f545be65f7b54ce385cacc7379aeb65c6338e5fd149d1c91ab11`.
- `codesign --verify --deep --strict` passed in the host environment. The signed App's Broker read returned `read_completed`; an ordinary same-user process returned `ok=false reason=backend identifier mismatch`. Signature-service calls failed inside the restricted sandbox, which is not evidence of a damaged installed-package signature.
- The actual UI after startup displayed the Debug authentication-bypass notice and asynchronously loaded `Local, 364 sessions`, without provider unavailable. No control recovery, message send, or phone end-to-end operation was performed; these results do not establish normal authentication or two-end synchronization acceptance.

## Separate Gaps in Remote Sessions and Original-Client Synchronization

- After installation, the original SSH aliases connected to mac-b and mac-c again. Both backends' latest blocker was `owner broker: existing hardware identity file is unavailable`, with no TCP 8765 listener found. Virtual-network reachability does not imply an authorized AgentHalo service; target identity, Owner registration/publication, and the consumer authorization directory/connection chain still need completion. No Owner identity was automatically initialized or copied.
- Mac's `AgentHaloAuthenticatedPeerManager.swift` creates remote clients from an authenticated directory and performs certificate binding and health reads; it does not add a network device IP as a session source merely because it is reachable.
- `AgentHaloWorkspaceModel.needsLocalSessionResume` offers control recovery for entries with only a resume ID and no current control session ID. The backend's `codex_app_server.rs` launches a separate `codex app-server --stdio`; `codex_control.rs` resumes the thread and binds it to that AgentHalo process generation, rather than attaching to the original client's active interaction channel.
- Current history reads and explicit resume do not mean AgentHalo and the original Codex/Claude client synchronize input/output for the same active runtime. The Claude control path likewise uses an AgentHalo-managed SDK subprocess. Hiding buttons or automatically resuming cannot supply event, approval, interruption, and concurrent-input synchronization.
- Two-end synchronization still requires runtime integration/bridging, input scheduling, output event broadcast, and disconnected-identity recovery designed for each original client form. These shared semantics must live in Shared Rust; platforms only adapt the necessary native interfaces. This round did not claim the capability was implemented and did not operate on original-client sessions.
