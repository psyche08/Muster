# Locked Use: Source Comparison with Stella

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

Continuing from the user-specified ChatGPT chat "查找Locked use项目" (Find the Locked Use project), the reference repository is
[ruuxi/stella](https://github.com/ruuxi/stella), pinned to
`7480367001e3f43793905553d6c1d20b24ec8e8e`. Only source was read; Stella was not run or installed.
The upstream license is Apache-2.0 (Copyright 2026 FromYou, LLC). Muster's comparative diagnostics use independent
C/Rust OS adapter code and do not port upstream lease files or authorization decisions.

## Directly Relevant Differences

| Stage | Stella source | Muster implementation and treatment |
| --- | --- | --- |
| Trigger lock-screen authentication | `postUnlockTriggerKey` in `desktop_automation.swift`: HID event source, keycode 36, key-down/up posted to `cghidEventTap` | Changed after device comparisons to a private event source + Session Tap Return; empty submission and the Rust authorization boundary are retained; no HID tap |
| Local input | `LockedUsePhysicalInputMonitor` uses a listen-only event tap and filters its own events by source PID | Retains the HID system input counter; measured HID posting adds 2, Session posting leaves it unchanged; no subtraction or baseline reset |
| System settings | The installer inserts a remote branch into `system.login.screensaver`, retaining the original fallback; it does not set `screenUnlockMode` | Preserves the user's requirement not to change system login preferences |
| Authorization mechanism | `StellaLockedComputerUseAuthorizationPlugin:allow`, `shared=true`, `tries=1` | `MusterLockedUse:invoke,privileged` in the root authorization host, `shared=false`, `tries=1`; the trust boundary is not changed merely to match form |
| Authorization basis | Enabled/expiry/consumed state in a JSON lease; allow/deny returned through a Unix socket | Rust owns a short-lived signed capability bound to the device and operation, one-time consumption, cancellation isolation, and a trusted plugin terminal state; these constraints remain |
| Completion | Upstream active state does not prove actual OS unlock | Acceptance requires observing plugin invocation, trusted terminal state, OS unlock, execution of the bound operation, and OS relock |

Source files:

- [Trigger and input observation](https://github.com/ruuxi/stella/blob/7480367001e3f43793905553d6c1d20b24ec8e8e/desktop/native/src/desktop_automation.swift#L10237)
- [Authorization plugin](https://github.com/ruuxi/stella/blob/7480367001e3f43793905553d6c1d20b24ec8e8e/desktop/native/src/locked_use_auth_plugin.c)
- [Installation rules](https://github.com/ruuxi/stella/blob/7480367001e3f43793905553d6c1d20b24ec8e8e/desktop/native/src/locked_use_installer.swift#L167)

## Unauthorized Control Diagnostics

Local CLI: `muster debug unlock-probe` (`unlock-key-probe` is an alias). Remote members cannot invoke DeviceDebug.
Diagnostics share the console write lock with real transactions; a prepared transaction rejects diagnostics. Without a phone signature, they neither
prepare nor consume Muster authorization. They require real Apple loginwindow, a unique writable password field, confirmed focus,
and a locked console whose identity and input count remain unchanged. After releasing the wake assertion, they clear the field and send at most one
Return pair. There is no repeated confirmation, automatic action switching, password reading, or nonempty password entry.

Insufficient OS event-posting permission, unknown/unlocked console state, changed local input, or unconfirmed focus causes immediate failure.
A submitted log entry only means the call completed; authd/plugin logs must also be checked, and submitted alone cannot be reported as unlock.
Production `request_unlock` uses the same Session Return path, with no automatic AXConfirm/HID fallback or retry.
Shared Rust supplies continuation checks for asynchronous OS operations, bound to the console, input count,
cancellation state, and deadline at Gate approval. Checks precede waiting, focusing, clearing, and sending; the baseline is not reacquired after task start.
Sending alone grants no permission; plugin consumption, trusted terminal state, and observed actual OS unlock are still required.

## Current Verification

- All 10 musterd Locked Use regressions, all 45 muster-core tests, and 2 plugin installation policy tests passed; Clippy passed. Coverage includes cancellation/input/console changes while waiting, delayed start without reacquiring the baseline, prompt cleanup after OS submission rejection, and waiting for a trusted terminal state even after authorization is consumed.
- macOS Release build and Developer ID signature verification passed; the diagnostic version was installed on mac-b. No notarization this round.
- An actual invocation while mac-b was unlocked was rejected; input generation remained 5118245 before and after, and no input was sent.
- After the user confirmed the screen was locked, HID source + HID tap Return at 11:48 triggered the plugin, which rejected the unauthorized request; input count changed from 5118683 to 5118685.
- Private source + HID tap at 11:52 also triggered the plugin; input count changed from 5118728 to 5118730. A private source alone does not solve the issue.
- Private source + Session tap at 12:00 triggered the plugin (`mechanism created`, `admitted=0`), kept the screen locked, and left the input count at 5118732 before and after.
- `screenUnlockMode` was not changed; no password was read or entered, no test authorization was granted, and the user was not logged out.
- The production authorized flow, real keyboard/mouse abort, target operation, and relock still await device acceptance; unauthorized controls do not replace these gates.

Production-path retest: the daemon at `c009fa5` was built from an independent source snapshot. Developer ID signing,
strict verification on the target machine, and installation all passed. At 12:13, mac-b's `debug unlock-probe` took 343 ms
to trigger `mechanism created` / `admitted=0`; input count remained 5118732 and the system stayed locked.
This verifies unauthorized rejection through the production adapter path; phone Presence and target-operation acceptance were not performed.

The final unified artifacts contain `c009fa5` and network fix `a473a38`. macOS / iOS Release builds,
signing, and strict verification all passed. No notarization this round. All three Macs received the same daemon, with SHA-256
`3ecd51f4759ff4d0cf3c3d339bcbcc578b2bc43d9ab7c5e0dc5241cfc3233b92`。
The iPhone was unavailable in CoreDevice and the new iOS package had not been installed; the previously installed
DoH version was not counted as this endpoint-recovery version. At completion, mac-b remained locked, input count was 5118732,
and `screenUnlockMode` remained unset.

All 15 consumer regressions passed, and Clippy had no warnings. Two protocol tests previously used the development machine's real console;
when Mac-A was locked, real policy rejected agent.view. They now use an explicit DefaultPlatform fixture
to isolate the test environment only; production policy is unchanged, and this does not replace device Locked Use acceptance.

The user later confirmed the iPhone was ready: the new iOS version was installed and launched, and RPC from the three Macs to the iPhone
succeeded in 9/9 DeviceInfo calls. The user explicitly required the new Codex test chat to select
"Don't work in a project"; the earlier mistakenly created local projectless chat was archived. The current creation interface
cannot specify a projectless target on a remote host. The user was asked to create and save a dedicated chat on mac-b, then
lock the screen and open it in iPhone Muster. Only the native_app controller of an existing business chat was observed;
no test content was sent to it, and no authorized unlock, Face ID, or target-operation acceptance was performed.
