# ADR 0297: Select a Codex project from the background new-chat form

- Date: 2026-11-23
- Status: Proposed
- Related: Amends rule 2 of [ADR 0220](0220-background-only-native-creation.md). See also [ADR 0203](0203-restore-native-navigation.md).

## Context

The Codex sidebar can group conversations under Pinned and Recents without exposing a project-specific new-chat button. On an unlocked Mac this caused requests from an old session to stop at background admission. The operator observed that the sidebar New chat button and the new form's searchable project list responded to background Accessibility actions.

## Decision

Prefer a verified empty form, then a project-specific sidebar action. When another application owns the foreground, allow a unique New chat button in a safe window as a third route. Resolve a unique project label for the requested single root from the provider's local project registry. On the new home form, open its project selector and search for the exact label. Press only a unique matching project in the picker's list. Reuse the selected project ID, root, local mode and composer proofs before submission.

A covered Codex creation observes the actual foreground owner before any activation. When another process owns the foreground it uses the same background routes under its existing Locked Use authority. Pin that foreground owner and keep the lease's system-wide physical-input guard. A takeover, ambiguous owner, or missing safe route refuses the held operation before activation. The existing covered route remains available for a foreground provider or a cold launch.

Keep the foreground and physical input guards throughout navigation and cleanup. If selection fails, clear the search this operation wrote and close its own picker while it still owns the window. A user takeover ends writes, including cleanup. An unlisted root retains the existing Invalid response.

The web project list can expose its own AXGroup with the AXApplicationDialog subrole. Before pressing the selector, require a dialog-free home window. After the press, pin the direct dialog parent of the unique Search projects field on that page. During selection and recovery, allow only that exact dialog. A pre-existing dialog, a replacement, any additional dialog, or a sheet still blocks the operation. Verify this boundary with a regression test and a device request that switches away from the default project.

## Alternatives

A directory basename can refer to a different project. Opening a folder sheet or using a URL would change the established background navigation contract.

## Consequences

A flat sidebar can start a task from an existing conversation with an empty composer. An ambiguous project, unreadable form or open dialog prevents navigation. Changing the visible conversation follows the existing restoration policy.

## Verification

Exercise covered admission, foreground changes, system-wide input scope, route priority, exact root and label selection, ambiguous and missing choices, picker cleanup and takeover. For device acceptance, check all provider composers before sending a single request from an old chat, observe foreground ownership throughout, and verify the resulting session directory independently.

On macOS the agent and platform library suites passed (446 and 59 tests respectively, seven opt-in cases ignored), and the development daemon built. A read-only check on the local Mac found both New chat buttons, the primary button sharing a group with the Codex mode switch, and no project-specific action. The Codex and Claude composers contained only a placeholder or whitespace. A chat preview exposed as an application dialog prevented the initial acceptance run, and the Mac subsequently locked. The new branch has no physical creation receipt yet.

The covered-route regression exercises another foreground process, takeover, unknown focus, a foreground provider and cold launch. It checks that covered background work retains global input scope and refuses a held request when a safe route is missing. The macOS agent and platform suites passed (447 and 59 tests, seven opt-in cases ignored). All four daemon scheduled-lease tests passed, including revocation and relock.

On November 24 an isolated signed daemon at revision `1a5760bb` ran on an unlocked Mac with another app in front. Creation from an old conversation succeeded for the default project. The provider database confirmed the returned session's directory, and the foreground observer recorded no activations. Requesting a different single-root project exposed the web dialog described above: the old guard rejected it before writing the search and left the empty picker open. That second request did not submit a prompt.
