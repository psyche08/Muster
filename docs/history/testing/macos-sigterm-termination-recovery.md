# macOS SIGTERM Termination Deadlock

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](../../PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

On 2026-09-12, while continuing the mac-c installation, the old App PID `11332` was found unable to exit normally.
This round observed the normal workspace through the user's already logged-in screen-sharing session, with no exit authorization prompt; the Keychain was not operated on.

## Evidence and Fix

A two-second thread sample at 14:57:56 showed the main thread still inside the main dispatch queue callback for the earlier SIGTERM, entering `_shouldTerminate`'s nested event loop after calling `NSApplication.terminate`. The asynchronous MainActor termination task could not continue inside a main dispatch queue callback that had not returned.

The signal coordinator was extracted into a separate source file. The signal callback submits the termination request through `RunLoop.main.perform(inModes: [.common])`, then returns to the main dispatch queue first; AppKit subsequently enters the normal termination flow. `applicationShouldTerminate`, Shared `requestExit`, OS authentication, and termination approval are unchanged. The fix does not exit the process directly or bypass rejection.

## Validation

- An independent, windowless AppKit test directly compiles the production signal coordinator and runs with real SIGTERM, `terminateLater`, the main dispatch queue, and the AppKit event loop. Only the termination approval result is a test double; it does not access product identity, user state, or the broker.
- Before the fix, it timed out after five seconds (exit code 124); asynchronous approval could not execute.
- After the fix, normal approval and exit passed; rejecting the first termination request and allowing the second also passed.
- The regression entry point is `src/native/apple/AgentHaloMacApp/Tests/TerminationHarness/run.sh`, now included in the official App build script before packaging and signing.

## On-site Boundaries

The source fix cannot release an existing process already trapped in the old code's nested loop. The user subsequently explicitly authorized “允许，仅强退这个 GUI 进程” (“Allowed; force-quit only this GUI process”). After checking its path and start time, only old GUI PID `11332` was force-quit, then the signed package was installed. This authorization did not extend to any other processes or identity data.

At 15:04 on 2026-09-12, mac-c had installed the Release App containing this fix from `99fd8fe55e20310da2a14a0f0b1264906f44c74b`, Developer ID Team `89LGY6BD53`; deep strict signature verification passed, with no notarization. The old App was retained at `/Applications/AgentHalo.app.backup-20260912-150403`.

On the physical machine, new GUI PID `51653` exited normally after an ordinary SIGTERM, already gone at the first 0.1-second check. It was then reopened from Finder as GUI PID `52376`. Backend PID `50531`, Muster agent PID `50699`, and bridge PID `50724` remained unchanged throughout and were checked again at 15:23. This validates normal termination of the real App; asynchronous rejection coverage comes from the independent AppKit regression above.

Only mac-c was deployed in this round; mac-b was not operated on. Device-ledger system authentication, hardware identity registration, and complete phone pairing remain separate acceptance items.
