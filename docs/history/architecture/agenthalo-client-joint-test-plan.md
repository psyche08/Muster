# AgentHalo unified client manual test plan

Scope of this round: macOS owner and iOS / Android / HarmonyOS consumers; Windows is out of scope.
Current Shared source uses C ABI 1.48 (adding non-authoritative issuance-error classification); the consumer deadline interface remains compatible with ABI 1.47.
Tests must check the actual ABI of each installed package separately. A Mac update does not mean mobile clients or Relay were updated. Pairing deadlines, recovery, and completion use Shared typed interfaces;
privacy cover retains its separate interface. Record builds, signing, installation, actual execution, and physical-device pairing separately, as specified in
[Pairing Release Gates](agenthalo-pairing-release-gates.md).

## Behavior testable in this round

- iOS: read-only completion details for a Shared-verified directory; explicit Resume after recovery network failure; restart continues using the existing
  checkpoint / Acked journal. Directory details are not connection credentials.
- Android: recovery interrupted by backgrounding during startup resumes on return to foreground; local
  Cancel cannot destroy a saved pending-approval flow; completion no longer offers an unusable “Scan again.” Ordinary network failures while waiting for the owner preserve recovery state.
- HarmonyOS: show pairing transport capability separately; correctly show completion after directory verification; recover from saved checkpoints
  after confirmation-upload failure; late responses received while backgrounded cannot overwrite current state.
- macOS: ordinary startup opens the workspace directly without authorization; the global shortcut manually activates the cover, and pressing it again requests Touch ID to exit;
  quitting requires Touch ID only while privacy cover is active, with no prompt for an ordinary uncovered session. OS window implementation and limitations are described below.

Parts of the mobile remote listener / live peer connector remain disconnected. Successful scanning, ACK, and verified directory
can establish pairing and recovery; they do not establish that remote Agent session reading, sending, terminal, or Computer Use
works. Mark those items separately as BLOCKED, without bypassing them using temporary direct addresses.

## Pre-installation record

Build from the same final commit and retain artifact SHA-256, ABI, target architecture, and signing-check results. An iOS Simulator `.app`
cannot install on a physical device; a device `.app` without a valid provisioning profile is not an installable package either. Distinguish Android
debug signing from release signing, and a signed HarmonyOS HAP from an actually connectable device.

This round prepares packages only; the user will perform the coordinated manual tests. Upgrade installation should preserve existing app data. If old pairing must be cleared, first record the error and
stage, then use the app's authenticated cleanup entry point. The previous macOS cleanup removed old orchestration/federation state,
but retained owner identity, Muster ledger, and configuration; do not record it as a full device reset.

## Repeat on each mobile platform

| Step | Expected behavior and evidence |
| --- | --- |
| Cold start / background and foreground | Shell opens; recovery does not automatically repeat local authentication prompts; saved flows recover first |
| Retry after camera denial | Clear permission explanation; scanning works after reauthorization, with no manual-payload or short-code fallback |
| Scan a fresh Mac QR code | Shared invitation valid for two minutes; phone and Mac each display verification information for human comparison |
| Phone confirmation / Mac approval | Only explicit actions invoke local authentication; cancellation or mismatch never enters paired state |
| Background / foreground caused by system authentication | Successful phone-authentication callback remains valid through the system authentication screen; only explicit cancellation/closure invalidates old callbacks |
| Retry immediately after cancellation | Scan a new code if no state is persisted; with a checkpoint, recover only the original request without destroying recovery evidence |
| Mac authorization cancellation / approval timeout | Preserve the current request while still approvable; offer pairing again after expiry; compare SAS again for a new request |
| Network loss while waiting for owner | Preserve checkpoint; resume after network/foreground recovery without creating another authorization or repeating authentication |
| Recovery, signing, or persistence crosses confirmation deadline | Follow the Shared typed step: retransmit, receive an existing delivery only, or preserve evidence and repair; never upload late |
| Background or process termination near delivery / ACK | On restart, Shared recovers from exactly persisted and read-back state; UI never infers success |
| Expiry again after prior issuance and failed delivery | Recover with the same hardware identity; cover expiry and restart after checkpoints containing 1–6 successor signatures, without reverting to first-time pairing |
| Historical issuance / different recovery identity rejected | Show the actual Shared stage and security-error class; do not clear the ledger, replace keys, or treat a new QR code as recovery authorization |
| Completion and directory | Evidence is the Shared Acked journal and verified owner directory; record service entries without treating them as connection readiness |
| Normal restart after completion | Pairing and directory recover; no unusable scan-again entry point |
| Old code / expired code / damaged state | Explicit failure or re-pairing requirement; no second authorization bypass |

Use a distinct run ID for every physical-device run. Record only source commit, package hash, platform/OS, Shared stage, journal generation,
Relay ACK/directory state, and directory revision. Do not copy raw QR payloads, SAS, keys, signatures,
bearer capabilities, SPKI, or opaque handles into reports.

## macOS privacy cover

Default shortcut: Control + Option + Command + B; Settings can select P or K with the same modifiers. Ordinary startup
neither covers displays nor requires authentication to enter the workspace. Explicitly saved startup-cover settings for existing users are retained; enabling
that option requires authentication to remove the cover on workspace entry. Shared generates the settings bytes, which are committed
after exact disk readback. A successful cover exit is not persisted as authentication-free authorization for the next startup.

Reactivating the shortcut only requests Touch ID to exit; cancellation, failure, or unavailability keeps the cover. There is no timed automatic exit,
password fallback, system lock, logout, Keychain unlock, or input filtering. Shared directly approves quitting an ordinary uncovered session;
quitting during privacy cover requires successful authentication. Cover windows are nonactivating black NSPanels laid out across actual screens, with mouse-event passthrough and exclusion from window
sharing. These properties require separate acceptance checks for each actual capture path.

| Test | Acceptance |
| --- | --- |
| Default startup / hotkey | Ordinary startup shows workspace with zero authorization prompts; manual hotkey covers every physical display; repeat requests Touch ID |
| Touch ID success, failure, cancellation, unavailability | Only success reveals the desktop; failure/cancellation preserves every cover window |
| Quit App / Cmd-Q | Zero authentication for an ordinary uncovered session; authentication required with cover active, cancellation preserves cover |
| External displays, hotplug, resolution, Spaces, fullscreen | Cover all current displays; record any exposed areas or window-level problems |
| Codex Computer Use | Actual screenshots remain readable during cover; AX and screenshot-coordinate clicks and keyboard input can all be read back |
| Login session and Keychain | Observe actual session/Keychain state read-only; user separately confirms other apps remain logged in |
| Restart | Start according to Shared-saved startup settings; never reuse the previous authentication result |

An independent prototype on 2026-09-05 verified Codex screenshots and both click/input paths during cover on one built-in display;
read-only sampling of the default Keychain remained unlocked. The prototype did not actually perform Touch ID exit or test external displays.
This evidence only establishes feasibility of the tested API combination; behavior must be revalidated after production integration. Privacy cover is not a secure OS lock screen:
local input still passes through, and an administrator or forced process termination can remove the windows.

## Test result template

```text
Run ID / platform / OS:
Source commit / ABI / artifact SHA-256:
G0 software tests:
G1 hermetic virtual-phone loopback:
G2 production cross-adapter contract / distinct subject and target:
G3 build / signing / provisioning:
G4 install / hardware auth / protected readback / restart:
G5 running Relay revision and broker:
G6 correlated owner + consumer terminal journals and directory:
Remote sessions / send / interrupt / approval:
Privacy cover (macOS):
Result: PASS / FAIL / NOT_RUN / BLOCKED
```

## Required startup-authorization and production-adapter regressions

- Record required first-install OS initialization authorization separately from device-management authorization; do not add prompts on platforms that need no initialization authorization.
- After initialization, three consecutive cold starts, background/foreground transitions, and network recovery must not proactively request UserAuth/Touch ID/Keychain UI.
- macOS cover activates only through the global shortcut (default Control-Option-Command-B), never automatically at startup. Legacy `start_covered=true`, damaged/unreadable settings, and Shared initialization failure must not flash a cover, request fingerprints, or start keep-awake. Shared preserves valid old shortcuts and forbids re-enabling startup cover.
- Run `/usr/bin/caffeinate -dimsu` only after cover activation. Place panels before requesting keep-awake (`-u` may wake displays). Authentication cancellation/failure preserves cover and keep-awake; revealing stops it; authorized exit retains it until process termination. During settings loading, ordinary quit still uses Shared bootstrap; if no Shared session exists yet, do not block App exit.
- Keep-awake tests must cover repeated activation idempotence, assertion renewal beyond the default five seconds for `-u`, visible startup/refresh failures, stop, and parent-exit cleanup. Never terminate another app's caffeinate. Power assertions do not prevent manual lock, forced sleep, or bypass OS security policy. Actual multiple-display/Touch ID/idle-lock behavior needs separate physical-device acceptance.
- If Secure Enclave/Keychain resources cannot be accessed noninteractively, return explicit unavailability; do not automatically delete keys, rebuild identity, or loop authentication.
- Device-management unlock, pairing approval/rejection/revocation, and explicit cleanup still use their respective Shared/OS authorization. Noninteractive startup is not a session-level authorization cache.
- Harmony production `.ets` bindings must participate in tests: successful authentication through system-triggered background/foreground, immediate re-pair after cancellation, slow authentication crossing deadlines, and upload/recovery conflicts.
- Mac production subprocess transport must cover concurrent stdout/stderr, output limits, deadlines, cancellation, late responses, and permanent failures; a URLSession fake alone is insufficient.
- G2 uses distinct phone subject and owner target through the actual production ABI; fakes must not ignore critical arguments. A software binding smoke test is not evidence of physical HUKS/UserAuth success.
- `make pairing-platform-adapter-gate` must rebuild current Apple Shared libraries, invoke the Mac production issuer for a fresh invitation, then verify through Harmony `.ets` / C++ NAPI / Rust. Missing fixtures must not degrade this into a direct Rust smoke test.
- G6 requires a redacted run ID spanning both endpoint journals and the live Relay. Without G2/G4/G5/G6, package preparation must not be recorded as working pairing.
