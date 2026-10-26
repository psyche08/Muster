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
