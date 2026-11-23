# ADR 0291: Preserve session watch failures and distinguish terminal setup from creation

- Date: 2026-11-23
- Status: Accepted
- Related: [Native terminal hosts](0035-native-terminal-dev.md), [session reconciliation](0151-session-reconciliation-and-retry-evidence.md), [service architecture](../architecture/services.md)

## Context

Desktop logs showed repeated session subscriptions ending after history loaded. Each scheduled the shortest retry, and the client discarded the final RPC error. Separately, an iTerm2 window request reported an unknown outcome when AppleScript failed to compile the cookie request. No CreateTab request could have been sent.

## Decision

In Shared Rust, read the final RPC result when a watch closes. Keep exponential backoff across short live subscriptions and reset it after 30 seconds of continuous live time. Keep the existing eight-second retry ceiling and cancellation behavior.

Address the default iTerm2 Apple Events target by its bundle identifier so the compiler can resolve the installed scripting dictionary. Acquire the authenticated iTerm2 connection before marking window creation attempted, then submit through that same connection. Keep ambiguous failures after submission in the operation ledger.

## Alternatives

A longer fixed retry delay would also slow the first reconnect after a healthy watch. Repeating terminal creation after a lost reply risks opening a second window.

## Consequences

An ended stream can now surface the peer's reason. Repeated disconnects may take up to eight seconds to retry. Local Automation consent and the provider's API setup remain prerequisites.

## Verification

Exercise watch recovery and its final error over multiplexed RPC, check backoff across short and stable subscriptions, and compile the AppleScript against the installed dictionary without executing it. Run terminal, daemon and repository checks. Device runtime acceptance requires running an updated build.

On macOS the app-core library passed 206 tests, its chat-watch RPC suite passed 11, the terminal library passed 39, and the daemon library passed 166. Eight native terminal RPC tests passed, including a failed iTerm2 setup replay returning a definite rejection and the existing lost-receipt case remaining ambiguous. The installed iTerm2 dictionary compiled the bundle-ID script. A harmless argument-only script verified the advisory-name argument. Signing, publication, installation and live device acceptance are pending.
