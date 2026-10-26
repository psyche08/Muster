# Native Codex / Claude session creation: historical review

Reviewed 2026-10-26 against main 2b2266d and the local historical archive.
The historical candidate was commit `7416c4f5a6747cc74986ad26a1715050c116da28`
(2026-10-02, `Add candidate native session creation with workspace selection`).
Its own `docs/NATIVE_SESSION_CREATION.md` explicitly called it an implementation
candidate: native selectors, provider receipts and installed-device acceptance
were still pending. It is absent from the current main tree.

The candidate's sequence was: choose target/provider/directory, inspect Git and
optionally prepare an isolated worktree in Shared Rust, reserve an operation ID,
enter the existing provider queue and Locked Use flow, preserve any unsent draft,
switch to Code mode, locate a unique New session/task/thread control, verify the
new-page route, select and verify the full working directory, enter the initial
prompt once, then correlate a new transcript with cwd, first prompt and native
window identity. Unknown submission outcomes were not automatically replayed.

Reusable decisions are the Rust workspace preparation, stale form-generation
rejection, provider queue, full request binding to the operation ledger and Locked
Use, and receipt-based return of the real provider session ID. Native selectors
and page routes must be recalibrated against current Codex and Claude separately.
A matching title alone is insufficient identity evidence; reuse the newer exact
Claude identity mapping from ADRs 0039/0040 and the send admission/recovery rules
from ADRs 0046/0048.

The current `muster-acp` → `orca.create` path creates **CLI** Codex/Claude sessions.
It supplies Multica's execution runtime but does not create a desktop app chat.
The current `native_chat` New Chat action creates Chat-mode conversations and is
also not proof of Code-mode creation. Hiding the Chat tab leaves these internals
intact. No historical implementation is restored by this change, and no real
Codex/Claude prompt was sent for this review.

Recommendation: port the historical native-create protocol and workspace logic
as a separate decision, then accept each provider with a unique native route,
full cwd verification, one submit and provider receipt, both unlocked and through
Locked Use. Do not expose the action as supported until those gates pass.

## Current calibration and window requirements

ADR [0054](adr/0054-native-multica-targeted-installation.md) records a separate
operator diagnostic against a current Claude desktop build. An isolated native
first turn received its provider response, but the standalone window continued
to expose `about:blank`. This is not a shipped create adapter or a scheduler
acceptance result. The existing-session test was stopped by the console guard.

The owner requires explicit distinctions between the main window, a newly
created standalone window and the target session window. Window position or
title alone must never authorize input. Bind creation to a verified new window
and page, revalidate after close/recreation, preserve drafts, and check session,
full directory and composer immediately before sending. Native file sheets must
be bound to their parent window. Existing-session sends and first-message
creation need separate acceptance tests.

An installed application with no windows is not absent. Running with all
windows closed and not running at all require separate launch/window recovery
paths and observations; neither condition permits CLI fallback while the
application is installed. Test those cases without closing or replacing the
owner's unrelated working windows. Desktop-first routing also still needs a
task-scoped credential channel and the scheduler's actual provider protocol.

## Native new-page routes observed in installed bundles

Read-only inspection on the target found the following routes in ChatGPT
26.1021.31345 and Claude 3.6556.2. These are observed implementation details,
not a claim of a stable API or a completed native-create adapter. No route was
invoked during this review because another device owned the console.

| Provider | Observed native entry | What the implementation establishes |
| --- | --- | --- |
| Codex in ChatGPT.app | `codex://new?mode=codex&path=<encoded-absolute-directory>` | The parser accepts a new-thread route, explicit app mode and path. The handler checks that the path is a directory, resolves or creates a local project, then navigates to the new-page composer. |
| Claude Code in Claude.app | `claude://code/new?folder=<encoded-absolute-directory>` | The installed app's own Finder service constructs this form. The Code URL handler forwards the folder into its native new-page flow. |

Codex also accepts a `prompt` query field, which the inspected handler projects
as `prefillPrompt`; that is not a Send or a provider receipt. Claude's Code
handler accepts `q`/`prompt` and forwards it as `q`; whether that flow merely
prefills or takes any further action still needs attended verification. Do not
put scheduler tokens, task credentials or private environment values in either
URL. Prefer directory-only navigation and the existing guarded draft/echo/Send
path once the exact new form is bound.

These entries may replace menu and native folder-panel traversal in a future
adapter, but they do not answer which window will be reused, whether a current
draft is preserved, or how a closed-window application recovers. Verify those
properties separately. Dispatch to the verified application bundle instead of
the default scheme handler; percent-encode directory values, reject unsupported
providers and invalid paths, and recheck the full selected directory and local
execution mode after navigation. A successful `open` is only dispatch evidence.
In Codex, resolving a new directory may create a saved project; that side effect
must be accounted for in the product flow and acceptance fixture.

The resumed existing-session test established a separate prerequisite: a
manual Locked Use transaction can own an otherwise unlocked console. Its owner
must release it before a local diagnostic operates the desktop. A direct AX
script, alternate deep link, different requester or CLI fallback must not be
used to evade that console barrier. At this point the prior single Claude
operator test remains the only confirmed native first-turn result; production
new-session routing, existing-session acceptance and both launch/no-window
cases are still outstanding.
