# ADR 0216: Resolve a background Claude window for approval inspection

- Date: 2026-11-16
- Status: Accepted
- Related: [0201](0201-background-claude-card-selection.md), [0208](0208-one-window-per-native-session-operation.md), [0213](0213-bare-sidebar-badges-and-quiet-speculative-card-reads.md)

## Context

A required permission read can fail before looking at any card. ADR 0208 requires membership in AXWindows and excludes every minimized window. A background application can expose a window independently through its main, focused or direct child attributes. The selected window may also belong to another session.

## Decision

For a local Claude Code inspection, collect window metadata from the signed provider. Prefer a standard window whose document or URL identifies the requested desktop session. Otherwise use the existing focused, main and ordered window preference, with direct application window children as a fallback. Allow a minimized window for inspection when identified exactly or when no eligible visible window is available. Leave its minimized state alone. Search one window's content and require the mapped CLI UUID on the live web area before reading the card. Keep the window binding across preparation and the controller read. Ordinary writes retain their existing window policy.

## Alternatives

Changing the notification does not retrieve the card. Raising or reopening the app disrupts the local desktop. Scanning sidebars in every window can combine evidence from unrelated surfaces.

## Consequences

A provider with no accessible session window or a renderer that does not expose a complete card can still refuse inspection. Local-input admission and locked-use authorization still apply to navigation.

## Verification

Exercise target preference, alternate window sources, minimized windows, and the distinction between inspection and writes. On a real Mac, inspect a known tool permission prompt with another app in front and verify the exact session, prompt, available decisions and foreground owner.

A controller contributes its previously verified window and web area as a hint. Inspection rechecks the live mapped URL and containment. Window metadata from the current provider process can independently locate a window, and the normal sidebar and final renderer verification still run inside the chosen window. Approval inspection allows up to one second for a complete local registry scan. A cold scan on the test Mac exceeded the ordinary 250 ms budget, leaving an already open session unresolved and causing unnecessary sidebar navigation.

On an unlocked Mac, a dedicated Claude Code session paused for a Bash command that printed a unique marker. The changed controller read the exact command and both live choices, “Deny” and “Allow once”, with another application in front. The test passed once with the Claude window visible in the background and again with it minimized. In the latter case it stayed minimized throughout the read. A separate instance of the changed daemon with a fresh state directory then served AgentInspect while minimized. A client subscribed with AgentWatch received the same session UUID, marked command and two options. No decision was dispatched. The fixture window was returned to its prior size/state after the minimized runs.

The three affected Rust library suites passed 640 tests with ten ignored by default, using a 16 MiB test-thread stack as in ADR 0213. Strict Clippy passed for all targets in those crates. This acceptance used the local desktop RPC and a development build. Paired iOS display, Locked Use, installation and release were outside this run.

The workspace layer check still reports the existing muster-acp to muster-agent and muster-ffi to muster-paths edges. This change does not modify crate dependencies.
