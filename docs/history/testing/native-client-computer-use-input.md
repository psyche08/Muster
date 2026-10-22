# Original-Client Computer Use Input and Independent Output Observation

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](../../PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

## Goal and Current Delivery Boundary

The user confirmed this division: Computer Use inputs into the original Codex/Claude client; an independent observation path reads output; Shared Rust decides authorization, operation state, and duplicate prevention. Resuming the same session in another runtime no longer simulates original-client control.

The user further confirmed **Codex desktop, Claude desktop's Code interface, and Claude Code terminal** as targets. Shared requests bind distinct client types, preventing the same operation ID from switching terminal to desktop; mismatched client/provider types reject. On 2026-09-12 a Claude desktop AX implementation was added but not assembled into product startup. Existing iTerm2 Claude CLI sessions now have explicit local Terminal-page input, detailed in the final section. Codex OS adaptation and remote original-client input remain incomplete.

The initial 2026-09-11 increment lives in Shared Rust backend `native_ui_input`. Early sections retain each stage's acceptance boundaries; latest progress is in the final section. **Local CLI compilation/fixture tests are not original-client physical E2E or a completed remote authorization chain.** Existing provider input is unchanged and undeployed; startup does not automatically send or take over original clients.

## Implemented Shared Core

- Requests explicitly bind device, Codex/Claude provider, native session ID, operation ID, and exact text. No automatic approvals, arbitrary key scripts, or extra JSON fields; reject blank/oversized text and controls other than newline/Tab, retaining valid leading/trailing whitespace.
- Accepts `ResolvedRemoteAuthorization`, not constructible from JSON, requiring both `computer-use.execute` and `agenthalo.session.write`, matching operation type, resolved target, and provider/session/operation interaction binding. Checking Grant target sets alone is insufficient; current authorization is reacquired before preparation and actual input.
- One serialized input channel per desktop exclusively opens a fixed private journal; existing `OperationLedger` persists pending before actual OS input. Only request fingerprints persist, not text. Input/output Debug implementations also omit bodies. Completion-sync failure only retries the retained original record, never preparation/input.
- Operation identity includes owner, subject, device, and client operation ID; renewing Grants creates no new deduplication scope. Same ID/body returns durable results; changed body/session rejects.
- Successful keyboard calls do not mean native acceptance. `Accepted` requires an exact new-input event from an independent observer matching target, stream generation, increasing sequence, native input ID, and exact-text digest. Events already confirming another operation cannot be reused; anti-reuse survives restart.
- Errors after OS submit, failed observation, and mismatches conservatively remain unknown, with no app-server fallback/resend. Any pending/unknown blocks new input IDs on that desktop, preventing new-ID deduplication bypass; status/output remain independently available.
- `status` reads exact journal state only, without clicking/input/implicit recovery. `read_output` requires current session-read permission and validates target/cursor/size; it neither acquires the input lock nor updates journals or converts output into authorization.

## Required OS Adapter Contract (Implementation and Physical Validation Pending)

1. Native binding must identify device, process generation, window/control handles, and session ID; matching window names/screenshot text is insufficient. Preparation is read-only and cannot obtain binding by starting/resuming another session.
2. Prefer precise native accessibility operations; stop if session, focus, permissions, lock screen, or existing draft is uncertain. Never blindly press Enter, overwrite user input, or auto-dismiss approvals. Recheck binding/draft at send and submit exact text directly.
3. OS adapters own permissions/native handles; Shared Rust owns business decisions. Screens, OCR, and JSON display fields cannot grant permission.
4. Independent observers must attribute new input exactly to this operation; a user manually typing identical text must not be mistaken for delivery. Otherwise return uncertainty, never fabricated receipts. Transcript reads, streaming output, and complete input receipts are distinct capabilities.
5. Desktop control needs deadlines, cancellation, conflicts with manual user actions, and visible target/send status. Accessibility/screen-read permissions under normal authentication and actual client behavior need separate acceptance.

## Subsequent Wiring Order

1. Complete/persist native bindings and observation anchors for evidence-based late receipts/read-only restart recovery. Unknown remains closed, with no clear/resend/manual-confirm interface; deleting journals is not product recovery.
2. Implement original-window input and independent output adapters for Codex/Claude. Dedicated test sessions must prove exactly-once delivery, continued original-client use, output synchronization, and no automatic approval bypass.
3. Add Shared interaction authorization issuance, MST2 operation parsing, backend routes, then required ABI/UI. This increment does not enable existing-route `ComputerUseExecute` or use request booleans to bypass authorization.
4. After local OS acceptance, validate complete remote authorization→original input/output on mac-c; mac-b waits for mac-c completion. No changes to Muster virtual-network configuration/identity.

## Validation

Targeted tests cover both providers, exact text, durable deduplication, renewal, conflicts, restart unknowns, new-ID/session blocking, native-receipt reuse, authorization scope/target/expiry/interaction binding, revocation during preparation, lock/draft/permission failures, persistence failures, read-only status, independent output, size/cursor constraints.

Actual results:

- 22 new targeted tests passed, covering three client bindings and no resend after completion-sync failure.
- `cargo clippy -p agenthalo-backend --lib --tests -- -D warnings`, new Rust-file rustfmt, and `git diff --check` passed.
- Initial full sandbox run failed 31 local-socket tests due to permissions; host rerun passed. A full run after expanded tests had 428 passed, 14 ignored.
- Final-source full rerun: 427 passed, 1 failed, 14 ignored. Sole failure: existing `remote_operation_recovery::tests::another_peers_receipt_cannot_complete_same_client_id`, with `503 provider dispatch capacity is unavailable`. It then passed in isolation; neither test nor scheduler changed and intermittent root cause is not claimed fixed. All 22 new tests passed in that final full run.

Tests use fake OS input/native-event observers and real private-file journals. They prove Shared coordination rules, not actual window binding, keyboard delivery, or bidirectional original-client synchronization. Rust compilation passed; Mac/HarmonyOS product packages, signed installation, normal authentication, and physical E2E remain incomplete.

### 2026-09-12 Native Window Input Smoke Test

Codex Computer Use operated a real local window, **without AgentHalo `NativeUiInputAdapter`, Shared authorization, journal, or independent observer**. Results are OS-adapter feasibility evidence only, not AgentHalo E2E acceptance.

| Client | Actual result | Evidence and boundary |
| --- | --- | --- |
| Claude desktop Code interface | Single-input smoke passed | Created a separate No folder test session, pasted two Chinese lines/unique marker into an empty composer; AX draft readback retained newlines, Chinese, and `✓`. Clicked Send once; one user message/reply with matching marker appeared, composer emptied. |
| Codex desktop | Tool permission blocked; untested | `getApp("com.openai.codex")` returned `Computer Use is not allowed to use the app 'com.openai.codex' for safety reasons.`; no alternative control route attempted. |
| Claude Code terminal | Tool permission blocked; untested | `getApp("com.googlecode.iterm2")` returned the same access-denied category; no commands launched or sent to existing terminals. |

Claude test session: `local_fac48064-6f21-411e-9e3b-70e933ac6253`, title “AgentHalo 原生窗口输入测试” (“AgentHalo native-window input test”), marker `AH_UI_20260912_CLAUDE_DESKTOP_01`. The message explicitly requested no tools/files/commands/network, only the marker and “中文 ✓” (“Chinese ✓”). Final AX showed `Claude finished the response`, and screenshots showed user/reply messages. AX spaces in the reply differed from the draft, so byte-exact output fidelity/native-event digests are not claimed verified. Session retained for manual review; existing work sessions/drafts untouched.

No repeat-submit deduplication, crash/restart recovery, concurrent manual input, approvals, continuous streaming synchronization, or remote wiring was tested. No builds/signing/installation/deployment/virtual-network changes. The other two clients require a permitted Computer Use environment; app-server input cannot substitute for original-client acceptance.

### 2026-09-12 Claude Executor Increment (Product Integration Incomplete)

Added `claude_native_input.rs` implementing Shared execution order for two Claude frontends, rather than exposing generic terminal writes as remote operations:

- Accepts only locally bound native handles with native session, process-start generation, window/terminal identity, connection generation; rejects duplicate targets/surface aliases. Unbinding invalidates old prepare handles, not durable history.
- prepare read-only checks an empty editor/independent observation baseline. Before submission rechecks identity/focus/lock/draft/user-input generation; after insertion reads exact text back and sends once only if equal.
- Desktop uses distinct set-draft/press-Send actions; CLI uses bracketed paste without Return, checks original-editor equality, then submits once. Rejects embedded ESC/CR controls; never clears/reconnects-resends/falls back to SDK/app-server after failure.
- `composer=None` means missing complete editor evidence and must reject, not infer empty drafts from the terminal's last line. Snapshot Debug omits draft bodies.

Added `macos_claude_native_input.rs` with actual desktop OS calls: verifies running Claude's signature/PID-start generation, retains AX web area/Prompt/Send by exact Code-session URL, requires focused composer, uses AXValue/AXPress, not global keyboard/clipboard/AppleScript/app launch/automatic permission requests. AX traversal has count/time bounds; native user-input counter changes block submission. **This code only compiled/passed basic tests; it was not invoked on real windows this round. The preceding CUA test does not accept this implementation.**

Backend adds `NativeUiRuntime` installation and `handle_native_ui_with_resolver` integration with strict submit/status/output parsing. Callers retain a live Shared resolver, resolving current authorization twice before input; one-shot old proofs, JSON auth fields, or original local-admin routes cannot call it. `/send_prompt` unchanged. Integration tests use the real durable service with fake native ports/observers.

Incomplete and not usable yet:

1. Product startup has not installed runtime; App binding UI, cross-process/ABI wiring, production live resolver bridge, ComputerUse interaction issuance missing. `/native_ui/*` are not already-open HTTP endpoints.
2. Independent native delivery observer unassembled; AX/terminal success is not a receipt, and identical-text history cannot fabricate Accepted. Unknown conservatively blocks later input.
3. CLI has Shared execution/native-port contracts only, no usable iTerm2 Claude-editor port. Existing `Iterm2Adapter` session snapshot/sendInput does not bind Claude drafts/process sessions. Whether the first version accepts original CLI sessions launched/bound by AgentHalo needs confirmation; arbitrary existing terminals are not auto-taken over.
4. Existing client installations, authentication, pairing, virtual network unchanged; no signing/deployment/new-executor physical E2E.

Validation: 7 Claude execution tests, 26 native-UI/route tests, 2 macOS CF text/process-generation basics passed. Testing found missing mandatory operation header in new routes; fixed with regression. Final backend **441 passed, 14 ignored**; Clippy `--lib --tests -- -D warnings` passed. Ignored real-CLI tests were not run.

### 2026-09-12 Reusing Existing iTerm2: Local Claude CLI Wiring

This increment neither launches, resumes, nor duplicates Claude processes. After AgentHalo Terminal connects to existing iTerm2, the user selects a terminal and clicks “Bind current Claude CLI.” New ABI 1.86 registers the existing native adapter as narrow Shared callbacks; subsequent page sends enter the Rust executor. **The Agent session-chat page's legacy provider send is unchanged; this is not completed remote/MST2 execution.**

Implemented:

- Rust binds iTerm2 foreground job PID/TTY to the current user's Claude CLI registration file, validating process signature `com.anthropic.claude-code` / `Q6L2SF6YDW`, startup generation, terminal foreground process group, native-session UUID. Registration directory/files check ownership/write permissions/symlinks/size/times; adjacent key files are not read, and names/cwd/status/IPC metadata do not authorize.
- Swift returns only native screen/coordinates/code-point mappings/process variables. Rust's bounded conservative Claude TUI reader accepts only complete single lines with clear top/bottom borders, no wrapping, cursor at body end. `[Pasted text…]`, newline/Tab, existing drafts, lock screen, changed target/user-input generations reject. Other CLI versions/layouts lack real-device validation; full compatibility is not claimed.
- Bracketed paste omits Return; one Return is sent only after exact readback. If paste acknowledgment precedes refresh, the still-empty editor may be briefly reread, without repasting/tolerating another draft. Native callbacks reread/compare captured state in the same iTerm2 transaction, then send to the exact session with explicit `suppressBroadcast=true`. One-shot capture is consumed before possible writes.
- Explicit local Send uses fixed private journal `state/native-ui-local/input.jsonl`, persisting pending before any native mutation; same operation returns original result, changed body rejects. Uncertainty blocks later local CLI input IDs across restart while output observation continues; no clear-history/automatic resend entry. This journal does not implement unassembled remote authorization; local ABI must not be exposed to HTTP/MST2.
- `SubmittedToTerminal` means only iTerm2 confirmed transport of draft/Return, **not Claude Accepted or task completion**. Existing independent snapshot watcher continues original output. No native Claude reception observer proves this input's causality yet; screen text cannot create Accepted.
- App asynchronously waits for Rust worker; callbacks copy data/bound length/wait. Timed-out tasks cannot use borrowed C pointers. Invalid CLI bindings do not auto-fall back to raw terminal input; clearly unsent preflight failures permit editing, unknown requests retain IDs. Explicit exit from CLI input mode is allowed without pending UI requests, but never deletes Shared journals.

Usage and limits:

1. Run Claude CLI in original iTerm2, keeping its editor empty. Local input adds no pairing flow.
2. Explicitly connect iTerm2 in AgentHalo Terminal, select the matching terminal, click “Bind current Claude CLI.” Existing native authorization unchanged, no TCC bypass/automatic permission requests.
3. Start with a short single-line test message. Exceeding visible line width fails exact readback; no extra Return, draft clearing, or automatic resend follows. Inspect the original client manually; reliable unknown-state recovery remains unimplemented.
4. No signing/installation/release to mac-c/mac-b, virtual-network/pairing/authentication changes. Existing native CUA prohibitions for iTerm2/Codex remain; new native code/other tools did not bypass them for real control.

Test layers: Rust input/persistence and FFI use fake ports/real temporary journals; Swift transactions use fake protocol connections to check revalidation/send order/exact sessions/no broadcast/one-shot capture; Mac App regressions check product compilation/existing behavior. Actual Claude CLI registration binding/editor layouts/bidirectional synchronization **still need physical acceptance in a permitted environment**. Claude desktop assembly, Codex OS executor, remote live resolver/interaction issuance, and native reception observation remain incomplete.

This round's results:

- Backend **452 passed, 14 ignored** (real-CLI tests disabled), including 9 new iTerm input and 2 delayed-refresh/timeout tests.
- Rust FFI **97 passed**, including 3 new callback-boundary/retain-release tests.
- Swift TerminalAdapters **48 passed**; Mac App **385 passed**, including no fallback after CLI failure, mode retention across terminal switches, continuous independent output.
- CoreClient XCTest **68 passed**, Swift Testing **12 passed**. Initial legacy ABI 1.85 assertion failure passed after updating to 1.86.
- Clippy `--lib --tests -- -D warnings`, changed Rust formatting, `git diff --check` passed.
- Rust static libraries/XCFramework built for macOS, iOS devices, simulator; Mac C ABI link smoke and owner-directory / pairing-scan / remote-consumer C ABI smoke passed. Final-XCFramework Mac App rebuild/new UI regressions passed. Sandbox Xcode warned simulator service unreachable, but XCFramework generation succeeded; no simulator-runtime acceptance claimed.
- No signing/notarization, installation/deployment; no actual iTerm2/Claude E2E pass evidence for AgentHalo's new executor.
