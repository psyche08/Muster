# Debug Local Authentication Switch

On HarmonyOS, this switch is only for integration testing on physical devices. It is off by default and is not persisted. When enabled, the UI displays
`DEBUG · 本地认证已跳过 · 不用于安全验收` (DEBUG · Local authentication skipped · Not for security acceptance). It does not automatically approve pairing, bypass Shared
validation, replace hardware keys or signatures, change broker identity checks, or modify Keychain/HUKS ACLs.
If an existing key requires system authorization, the user must still handle it; this switch does not unlock the Keychain.

## macOS

macOS has removed this debug switch. Neither Debug nor Release reads or supports
`--debug-skip-local-auth`; retaining the old launch argument cannot bypass local authentication.
Owner pairing, explicit device management, Consumer enrollment, device ledger unlocking, and privacy-mask dismissal
use their original system authentication policies again. Failure or cancellation is not treated as success.

Removing the switch does not change authentication timing: normal startup, recovery of existing state, and network/storage retries for the same request
do not add authentication prompts. Only existing initial setup or explicit protected operations authenticate through the original flow, without duplicate authentication.
No identities, pairing records, hardware keys, or Keychain ACLs are modified.

## HarmonyOS

Install a Debug HAP. Fully stop the application process, then launch it with an `aa start` boolean Want argument:

```sh
hdc shell aa force-stop dev.linsheng.agenthalo.harmonyos
hdc shell aa start -a EntryAbility -b dev.linsheng.agenthalo.harmonyos --pb agenthalo.debug.skipLocalAuth true
```

The switch only reads the arguments from the process's first `onCreate`, and requires the system to report `appInfo.debug === true` for the application.
Only boolean `true` takes effect; strings do not. Authentication remains enabled if system package information cannot be read.
`onNewWant` does not change the authentication mode. The switch skips system UserAuth prompts for pairing and local secure cleanup,
but does not bypass Shared's cleanup/pairing state machines or make unimplemented cleanup functionality available.

Fully exit HarmonyOS and restart without the argument to restore original authentication. Do not use the debug switch to accept production authentication functionality.

## Historical Validation (2026-09-11)

- Mac (historical record; the switch has now been removed): all 380 Debug tests passed at the time; Release compilation and switch-specific tests passed.
- HarmonyOS: tests passed for default-off behavior in the real authentication adapter, strict boolean arguments, Release metadata, read failures,
  no reconfiguration within the process, and system authentication rejection; the full build-script regression passed.
- HarmonyOS: Debug/Release ArkTS and unsigned HAP compilation passed. Inspection of the packaged
  `app.debug` confirmed `true`/`false` respectively; the final local artifact was restored to Debug.
- This round did not sign, notarize, or install over an existing application. hdc found no connected device; physical-device prompts and cross-platform pairing were not accepted.
