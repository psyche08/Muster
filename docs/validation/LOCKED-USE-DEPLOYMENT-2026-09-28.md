# Locked Use Device Deployment Record, 2026-09-28

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](../PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

Section 7 of [LOCKED_USE.md](../LOCKED_USE.md), moved here unchanged: the first
device deployment and the follow-up fixes found during it (through 2026-10-02).

The new Muster App and `MusterLockedUse.bundle` were installed on mac-b. The installer read back
`system.login.screensaver`: the Muster branch was first, while the existing AgentHalo branch and normal-login branch
remained. Transferred files had matching SHA-256 values at both ends; strict code-signature verification passed on the target. launchd started
the new daemon, and the plugin-only socket was 0600; a real ordinary local process's consume request was rejected.
OS diagnostics confirmed the console was locked, Accessibility was available, and policy allowed Locked Use.

The signed iPhone Release version containing this round's cancellation-race fix was successfully installed.
Rust regressions: 71 passed, 1 existing test skipped. Clippy, plugin denial-path tests, and two installation
policy tests passed. macOS and iOS Release builds passed; no notarization this round.
This record does not establish end-to-end acceptance. After the user completes Face ID on the iPhone, verify:

1. Initiate one operation from a session visible in the target native App; the phone displays Presence authorization.
2. The daemon records successful signature verification, plugin `consume admitted=true`, and trusted terminal state
   `allowed=true accepted=true`, admitting the bound operation only after actual OS unlock.
3. The operation ledger reports Applied, the target session shows the result, and the cover is removed after confirmed locking.
4. Local input, authorization cancellation, and timeout terminate actions and complete safe cleanup; separately verify all-display coverage, Spaces,
   and display hotplug on real devices.

Logs do not record signatures, nonces, input content, or credentials. If any actual observation is missing, signing, installation,
or passing tests must not be presented as full feature acceptance.

## 2026-09-28 Follow-up: System Wake and Credential-Free Diagnostics

Before identifying the unique secure input field in real Apple loginwindow, Shared Rust's macOS adapter
holds a wake assertion through `IOPMAssertionDeclareUserActivity(kIOPMUserActiveRemote)`;
after identification, it explicitly releases the assertion, focuses, writes an empty value, and performs one AXConfirm. Cancellation releases the assertion.
It does not read the password-field value or enter test passwords or nonempty markers. New local `debug lock-field` reports only
roles, identifiers, and available actions; `debug unlock-probe` shares the transaction write lock and cannot run alongside prepared authorization.
Plugin logs report only lifecycle and success/failure, without nonces, signatures, usernames, or credentials.

This macOS and iOS version was built, signed, and installed; daemon SHA-256 matched across all three Macs.
All selected Rust regressions: 96 passed, 2 existing tests skipped; final network regressions: 22 passed, 1 skipped.
Clippy, plugin denial-path tests, and two installation policy tests passed. No notarization this round.
An actual empty submission on mac-b took 71 ms; the system remained locked afterward, with no plugin invocation logs.
Successful wake and AX calls therefore do not establish that system authorization was triggered; Locked Use E2E had still not passed.
`screenUnlockMode` was unset; system login preferences were not changed and the user was not logged out.

For the itemized Stella comparison and unauthorized Return diagnostics, see [LOCKED_USE_STELLA_REVIEW.md](../LOCKED_USE_STELLA_REVIEW.md).

## Session Return Trigger Fix

`AXConfirm` returned success without starting system authentication. Actual comparisons confirmed that one Return pair posted from a private event source through
`kCGSessionEventTap` triggers the Muster plugin without increasing the HID input count.
The production adapter now uses this single-attempt path, retaining empty fields, real loginwindow identity, confirmed focus, Rust
continuation checks, and the independent watchdog. HID posting increases the counter and is therefore not a fallback.
Cancellation, input changes, or console changes during asynchronous waits stop submission; send failure immediately returns control to Owner
for cleanup instead of waiting for the entire lease timeout. See the Stella comparison record for evidence and outstanding device gates.

## 2026-09-28 AX Reference Refresh for Consecutive Sends

In mac-b's projectless Codex test chat, the iPhone request at 12:50 passed Presence, plugin
consume/terminal, and actual OS unlock, then sent successfully: the ledger showed Applied and the target chat displayed a new reply,
followed by relock and cover release. Two subsequent requests at 12:56 also authenticated and unlocked, but both ledger results were
Rejected with `unavailable: the session view changed`; both relocked and cleaned up normally.

The cause was a stale AX input reference retained by NativeApp controller after an Electron update; the operation arrived before
the once-per-second background refresh. Operations now synchronously rediscover the unique surface for the same process and session
before acting, then recheck process generation and lock state. The original controller's unknown-write latch
and approval identity remain; reattachment does not clear state. Draft reads and control snapshots share the same lock.
Codex titles resolve only on a unique match; last-modified time no longer guesses among identically titled sessions.

Rust regressions: 27 passed, 1 existing FSEvents test skipped, including real directory scans for
duplicate-title rejection and rename recovery. Clippy reported no warnings; macOS Release build and strict signature
verification passed. Installed on mac-b at 13:06, with daemon SHA-256
`ad19f9845c824cec15f422bbf2298139248e6a70fe36dbe61f09b12f6cce5de9`；
After service recovery it reconnected to the iPhone, with the OS still locked. No plugin or system login preferences changed,
no new notarization was performed, and the iPhone was not updated. Two consecutive user-authenticated sends after the fix still awaited device retesting.

## 2026-09-28 13:08 Device Sends Passed; Message Display Gap Fixed

AX-refresh retest: operations at 13:08:06 and 13:08:33 were both Applied; the second explicitly logged
`native_app refreshed session view`. Both passed Presence, trusted plugin terminal state, OS unlock,
actual send, relock, and cover removal. After retesting, the OS remained Locked and input generation was unchanged.
Server session history contained both input/reply pairs in full, with user message IDs 147890 and 163380.
However, the iPhone showed the first reply only after returning home and reopening the chat, and omitted the second user message.
The send execution chain was therefore verified, but message display still failed end-to-end acceptance.

Shared Rust fixed chat subscriptions: on initial connection and reconnection, wait for the server's first subscription frame, then fetch history and process
queued events, eliminating the gap between history reads and subscription. After stream end, reconnect automatically at 1/2/4/8 seconds;
foreground restoration and network changes proactively replace the subscription, with cancellation tokens rejecting late history/events from old subscriptions.
Only reads retry; sends are not replayed. Merge history by message ID, preserving duplicate text and correct order;
a nonoverlapping history tail replaces the old fragment and exposes pagination to fill the middle gap. Consume events while fetching history
so the 256-item RPC queue cannot block the history response on the same connection. Skip already-covered queued messages by history overlap position;
if order cannot be established, fetch again. The server explicitly ends subscriptions on event loss instead of silently skipping events.

Regressions cover message recovery while remaining on the current page after real RPC stream loss, consecutive foreground/network callbacks, duplicate text
retaining distinct IDs, history gap filling, nonoverlapping pagination entries, and ordering of queued messages longer than the tail page.

An additional independent real-RPC test confirms that 300 events arriving before the snapshot response do not deadlock; after foreground restoration,
late old snapshots cannot overwrite new messages, and the old subscription actually receives cancellation. All 18 related Rust tests
passed, with no Clippy warnings; macOS/iOS Release builds and strict signature verification passed, without new notarization.
Installed on the iPhone and mac-b at 13:21, with identity and pairing still usable; mac-b daemon SHA-256 was
`f50a91028dbf7d408819a893771cae095f40feb98285cae1366ec5108f2517b2`。
Logs confirmed direct connectivity restored on both ends, with mac-b still Locked. The user still needed to verify that two consecutive user inputs and replies
appeared automatically without leaving the chat page; other Macs had not received this chat-display fix.

Retests at 13:22:57 and 13:23:11 both logged Applied, Relocking → Idle, and cover removal.
The user confirmed that both appeared automatically without returning home; read-only session history also confirmed the last four messages were
user / assistant / user / assistant, matching the two tests. This passed current normal-path acceptance for
two consecutive locked sends and message display on the current page. A later OS diagnostic showed Unlocked,
with input generation changing from 5120218 to 5120220. That later state is not represented as the state at transaction completion, and the system was not
actively locked again for acceptance. Cancellation, local-input interruption, and multiple-display failure scenarios still require the separate gates above.

## Evening of 2026-09-28: AX Session Selection and Dual-Display Watchdog

mac-c's target `01a0e75d` (full v18 audit, training, and testing under the new decision) was in the sidebar,
but the main view showed `01a0ce56`; the old send path discovered only the current view and returned session not visible.
All NativeApp operations now serialize AX selection and input: match a stable URL in the application's navigation area,
or a full title unique in the local directory; perform AXPress only once and wait at most 3 seconds to verify the target web area
and unique composer. Codex sidebar handling supports AXIdentifier / AXDOMIdentifier, and Claude uses the
navigation landmark. Recheck process, session, control ownership, and user input generation before input and clicking Send.
Duplicate titles or entries outside the current AX navigation fail explicitly; search outside the sidebar is not implemented.
Local `muster debug ax-select-session <unique-ID-prefix>` verifies selection without sending text.

Three failures on mac-b at 22:25–22:26 received valid Presence signatures but stayed Armed until expiry,
without plugin consumption or message execution. The independent watchdog began repeatedly locking about 200 ms after startup.
Dual-display testing showed that NSPanel's screen parameter interprets the contentRect origin relative to that screen:
the second display's origin (0,1117) was added twice, producing (0,2234) and persistent health-check failures.
After initializing at screen-local (0,0) and explicitly calling setFrame with global screen.frame, the same device
passed 10 consecutive checks with healthy=1. The system remained locked throughout diagnostics, with no input or authorized unlock.
Rust now also distinguishes watchdog startup from termination; termination immediately revokes permission with an explicit error,
rather than making the user wait for the entire challenge to expire.

This round's 12 musterd tests, unique-title regression, and Clippy passed. macOS / iOS Release
builds and strict signature verification passed, without new notarization. Dual-display coverage does not establish complete phone-send acceptance;
user authentication, send, and relock after these fixes still require phone testing.

Installed over existing versions on mac-c/mac-b at 23:07; both TCP/TLS and UDP/QUIC Relay authentication recovered.
mac-c's local selection diagnostic succeeded; the subsequent AXWebArea title confirmed target `01a0e75d`, without
writing or sending text. Claude was not running, so no real-device Claude selection acceptance occurred this round.
System SMAppService reported the background item disallowed; musterd was temporarily running through user launchd.
The user was asked to approve Muster in both machines' system login items; production autostart and the new root helper still awaited confirmation.
The existing authorization plugin stayed in place; screenUnlockMode was unchanged and no Agent application was restarted.

## Signed read-only question inspection

`muster --target <device> inspect <session>` asks the local daemon to sign only
an `AgentInspect` operation for an enrolled remote device. The challenge binds
the target, requester, session, operation ID, nonce, console and 15-second TTL.
The target still requires its app allowlist, the provider's Agents scope and
`locked.control`. This inspection accepts the requester's enrolled device key
on desktop as well as phone. Desktop write operations retain their existing
Presence requirement; this is not a generic unattended signing endpoint.

An inspection cannot carry answers or authorize a provider queue. It selects
only the bound session and reads its question AX tree, then uses the same
watchdog, cover and confirmed-relock cleanup as other Locked Use operations.
The CLI returns only after the target operation finishes its cleanup. Keep the
JSON output local: it can contain text visible in the selected conversation.

## Covered display lifetime (2026-10-02)

The independent Rust watchdog owns a `PreventUserIdleDisplaySleep` assertion
for the remaining signed transaction lifetime, with an OS-enforced release
at the same deadline. The brief lock-screen wake assertion is still released
before authorization. Display idle prevention stays active across the covered
AX operation and is dropped on stop/release/error; it never suppresses explicit
lock requests or replaces console, input, TTL, permission or cover checks.

The read-only question diagnostic now samples across five seconds to expose
mid-operation display-sleep failures. It inherits the caller's live check and
work-queue permit, performs no form actions, and returns initial/final form
metadata plus elapsed time. Final-page answer dispatch retains a separate
Submit action when the actual form exposes one, even if initially disabled;
control readiness is distinct from selected-value or provider-receipt waits.

## Refreshing covered Chromium question controls

Ordinary AX attribute reads use Chromium's browser-side cache. When the next
question or enabled Submit is not found, the native adapter makes a read-only
`AXUIElementCopyElementAtPosition` query scoped to the verified application and
the intersection of the session web area and its window. Chromium routes this
query to the renderer, updating layout and serializing dirty AX objects. The
result is discarded and never used as an action target. Existing session,
live-authority, input-generation and three-second deadline checks remain.
No click is replayed, no selection echo or receipt is required, and the opaque
full-screen cover remains unchanged. Read-only inspection reports whether the
renderer query was accepted; acceptance alone does not prove answer delivery.
