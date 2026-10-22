# Virtual-Network Disconnect Caused by a Local Fragmented Packet: Fix and Validation

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](../../PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

On 2026-09-12, mac-c experienced an approximately 100-second disconnection from `13:20:13.348` to `13:21:53.236`.
The direct error in production logs was `vnet read local endpoint: vnet: malformed IPv4 packet: fragmented packet (flags/offset=0x2000)`.

## Scope of the Fix

The fix corrects the Go network process's use of the existing OS packet-read contract: after each UTUN read consumes one datagram, `ErrMalformedPacket`, or `ErrShortFrame` for a missing address-family header, means only that the datagram was rejected. The egress read loop now increments `DropMalformed` and continues instead of exiting the entire network plane. Logging is limited to once every ten seconds and does not record packet contents.

No packet validation, fragment forwarding, routing, identity, authorization, or peer decisions were added or changed; platform state does not replace Shared business state. Genuine device-read errors and buffer-configuration errors still propagate, and close/cancel still exits. The incorrect comment “MTU 1280 prevents normal fragmentation” was corrected; this version still does not support fragment forwarding, and affected large datagrams may be dropped.

## Automated Evidence

- The new regression drives production `UTUN.ReadPacket` through a local Unix datagram socket, without creating a real utun or accessing the privileged helper.
- Before the fix, the regression failed with the exact same `flags/offset=0x2000` network-loop exit error as on site.
- After the fix, first fragments, later fragments, wrong versions, over-MTU datagrams, and incomplete address-family headers are rejected in sequence, followed by successful normal packets on the same peer stream; all five rejections are counted. Cancellation unblocks the real reader.
- `EIO` and `io.ErrShortBuffer` still terminate the read loop and are not counted as packet drops.
- `go test -race ./internal/vnet ./internal/agentd -count=1` passed. This validates the host production-reader/datagram-adapter contract, not physical mac-c acceptance of the fix.

## Initial Deployment Blocker (Resolved)

At the start of this round, mac-c still ran `0fbe1f8f`, old GUI PID `11332` had not exited, and screen sharing remained at the `192.168.13.13` login window. The earlier installer had stopped before replacing the App because normal termination failed; the backend had recovered.

For this fix to take effect, a complete signed App package must be deployed and only mac-c's corresponding Muster user service restarted. Merely producing/uploading a package or current automatic network recovery must not be treated as deployment of the fix. Do not change mac-b, network configuration, or identity data, and do not force-kill the old GUI.

## Initial Artifact (Superseded by the Later Combined Fix Package)

- Fix commit: `3dc1272b2273a903f69eb2867b33b86b8fcdabc3`, with a clean worktree during the build; the Muster executable was recompiled and a complete Release App produced.
- Developer ID Team `89LGY6BD53`; deep strict signature verification, App/static-library Shared 1.86 runtime comparison, and broker software self-tests passed; no notarization.
- mac-c received `/private/tmp/AgentHalo-mac-c-3dc1272b.zip`; SHA-256 on both ends was `f829682182794f2d51208365a14d6337eb18888f93ae1a5dba9980e5786d47b1`.
- Packaged `muster-agent` SHA-256: `e6625ea2448df947a277198e06f876bc594cb955fe1f2e3f7484af894f066490`.
- After upload, the installed commit remained `0fbe1f8f`; this round did not retry backend removal or replacement of the running App, and no target service was restarted. Screen sharing still required the user to log in directly and inspect the old App's termination prompt.
- Continued interface operation after production fragment drops, real SSH/screen-sharing recovery, and complete phone pairing had not yet been accepted.

## Physical Deployment and Acceptance at 15:04 on 2026-09-12

- The final signed package came from clean commit `99fd8fe55e20310da2a14a0f0b1264906f44c74b`, combining the vnet fix above and the macOS SIGTERM termination-scheduling fix. Package: `/private/tmp/AgentHalo-mac-c-99fd8fe.zip`; local and mac-c SHA-256 both `8ae99ffcf3f549d84dd3fc624f8598aba776828bbd07dbea5b19a86e9555d007`.
- Installation completed after the user specifically approved force-quitting old GUI PID `11332`, retaining the old App backup. The installer reported `deploy_exit=0`; installed App executable SHA-256: `27670d2e75f2e5343c1141699d6adc459cf21b17067ccc90764a760657749dc6`; backend: `0a885a1922830d9e42cb23b491c87903fb9f275d66a2ffe541d76bbdeaa8917b`.
- Developer ID Team `89LGY6BD53`, deep strict signature verification, and installed Shared `1.86.0` reporting passed; no notarization. The actual signed App completed a protected-state read from the broker; an ordinary same-user process was rejected with `ok=false reason=backend identifier mismatch`. This clears only the OS transport boundary, not Shared authentication.
- The user runtime retained the existing canonical Muster layout, with `migrated=false`, valid configuration, and matching pre-/post-installation configuration hashes. Identity and pairing state were not cleared; mac-c's fingerprint remained `EXAMPLE-PEER`. mac-b, DNS, TUN, and Relay configuration were unchanged.
- At 15:04:43, one harmless 2049-byte test datagram was sent on mac-c from `192.168.13.13` to `192.168.13.11:9/UDP`, causing the kernel to fragment it on the actual MTU 1280 link. Production logs showed `rejected local packet; keeping interface up`, with the exact same `flags/offset=0x2000` error, but bridge PID remained `50724` before and after the test.
- Five pings after the test had no loss and averaged 11.185 ms; SSH and screen sharing continued working. At 15:23, backend `50531`, Muster agent `50699`, and bridge `50724` still had start times 15:04:03, 15:04:08, and 15:04:08 respectively, without restart.
- This accepts the fix for network exit triggered by fragmentation; fragment forwarding remains unsupported. Hardware Owner identity, device-ledger authentication, and real Mac↔HarmonyOS pairing/sessions are not yet accepted, so complete migration cannot be declared.

## Separate Blocker for Continued Phone Acceptance

At 15:22, clicking `Unlock device ledger` in Muster Network on mac-c opened the macOS login-password authentication window `Unlock the Muster device ledger`, awaiting direct user completion. No password was obtained or entered, and the Keychain was not operated on.

hdc recognized physical HarmonyOS device `5NC0225C10020352`. The installed `dev.linsheng.agenthalo.harmonyos` reported version `0.1.0`; that field and existing logs alone cannot prove its Shared ABI.

This worktree built HarmonyOS from the same source as the Mac package. Platform-adapter regressions, Shared Rust/NAPI compilation, and unsigned HAP packaging passed. Unsigned package: `src/native/harmonyos/entry/build/default/outputs/default/entry-default-unsigned.hap`, SHA-256 `71e14145a0bac27be9f28c5192d45ed00279ce614af8d03fe52964efccf22ad6`.

At 15:26, signing was attempted with the existing local development signing configuration. `SignHap` rejected an expired certificate: `The certificate has expired! NotAfter: Fri Sep 11 19:58:39 CST 2026`, exit code 255. The original signing configuration's hash was unchanged before and after; this worktree was restored to an independent unsigned configuration. No unsigned package was installed, and no phone data was uninstalled or cleared. DevEco currently shows `Sign in`; the original developer account must sign in and renew signing before physical phone installation and pairing acceptance can continue.
