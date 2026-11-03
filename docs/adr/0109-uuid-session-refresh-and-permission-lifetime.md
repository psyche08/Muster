# ADR 0109: Reconcile session metadata by UUID and preserve permission lifetimes

- Date: 2026-11-03
- Status: Accepted; verification pending
- Related: ADR 0099, ADR 0100, ADR 0105, ADR 0107, ADR 0108

## Context

The owner requests completing Claude permission-card and locked-display handling,
fast entry refresh, consistent native titles and UUID-aligned session metadata.
List timestamps must represent the last conversation message, never file mtime or
an unrelated native metadata update. Section/Project presentation remains disabled.
Inspection finds that a Claude-specific row omits timestamps, catalog updates do
not reconcile an open chat, an idle file status can override an observed AX card,
and transcript polling performs file I/O while holding the global catalog mutex.
Codex migrated history also uses thread updated_at rather than message timestamps.
The reported local Claude session has a running UUID-bound worker but no native
controller; the old waiting-only inspection trigger cannot discover its AX card.

## Decision

Keep the implementation in Shared Rust. Reconcile metadata using the full source
identity (device, provider, UUID), preserve search-specific projections, and never
bind by title. Native titles remain preferred, with freshness checks so old scans
cannot replace a newly read title. Refresh metadata independently of history.
Move transcript polling I/O out of the global catalog lock while retaining a
stable visible snapshot and discarding results for a replaced source/viewer.

Use provider conversation-message timestamps for recency and display them on
Claude rows too; filesystem stamps are only change-detection inputs. Tool and
metadata records must not advance the last conversation-message time. Respect
separate child-transcript scope and keep duplicate UUID/source handling explicit.

A live, UUID-bound AX permission card takes precedence over file activity hints.
Transient session state alone does not resolve a Claude permission request;
explicit interaction resolution or fresh complete provider observation does.
On entry, permit one existing protected read for a local Claude session with a
verified running worker even when its file hint does not say waiting. This
supersedes ADR 0100's waiting-only trigger, not its authorization. Do not rearm on
repeated running/idle frames; a new view or a later distinct waiting episode may
inspect again. Locked retention remains display-only. Responding still requires fresh guarded
AX observation and existing Presence/Locked Use controls; no cached card grants
authority, no provider command is approved as part of read-only acceptance.

The disposable device test exposed a changed Claude saved-environment form.
An experimental account/preset/name/full-path join permitted test submission,
but Claude created it in a scratch directory despite the selected preset. The
existing exact-directory receipt correctly remained unknown; it was not replayed.
Withdraw this experimental form admission from the final candidate: a saved
preset alone does not prove the actual destination. Keep existing form guards
and report modern preset creation as unresolved; do not weaken directory checks.

## Alternatives

Title-based joins and filesystem mtime reorder unrelated sessions. Holding the
catalog lock through transcript parsing serializes independent metadata reads.
Clearing cards on an idle hint confuses runtime activity with request lifetime.
Remote sessions, injected scripts, native APIs and direct provider-store writes
remain excluded. Section/Project controls are not re-enabled.

## Consequences

A retained card is the last confirmed observation while locked, not continuous AX
visibility. A protected cold read may need existing Presence authorization. Last
message time is unknown when no supported conversation message can be read.
All files remain read-only provider evidence. Device acceptance must distinguish
observed display, cold attachment, lock retention and actual provider responses.

## Verification

Test UUID/provider/device boundaries, metadata alignment and search snippet
preservation, rename versus scan races, message time versus metadata/file changes,
child messages, slow polling without catalog blocking, replaced-source rejection,
and explicit interaction lifetime across idle/unknown/locked status. Use bounded
read-only probes for entry latency and exact native/file title comparison. Verify
permission cards without approving existing work commands. Run Rust and Apple
checks; commit source, then sign and install all three owner-selected Macs and
publish only iOS OTA. No GitHub/TestFlight publication or notarization is requested.

## Implementation verification

Candidate 1.1.18 (108): agent unit tests (348 passed, four ignored), process-state
integration tests (six passed), app-core unit tests (141 passed, one ignored),
and the added slow-poll/replaced-viewer regression passed. Strict Clippy passed
for agent, platform, daemon and app-core targets. Public-content and diff checks
passed. These are source checks; Apple builds, installation, new-session AX
acceptance and locked permission display remain pending at this commit.

## Device evidence and final scope

The 1.1.18 test candidate on one authorized Mac produced a real disposable local
Claude command-permission card through AX. A fresh subscription while the OS was
confirmed locked returned the same interaction ID, prompt and Deny/Allow once
options. No command was approved. Two UUID-bound sessions matched native titles
and last user/assistant message timestamps; first state took 15–17 ms, targeted
refresh 17–18 ms, and recent history 14–24 ms on the host (not phone latency).
Platform tests also passed (39). These measurements do not prove cold discovery
of a never-observed card while locked, nor iPhone visual acceptance.

Final candidate 1.1.19 (109) retains the session/card fixes and withdraws only the
unproven new-environment form admission. Its Apple build and three-Mac / OTA
delivery are tracked separately from the above runtime observations. Modern
Claude environment-based new-session creation still requires further work.

Final UI acceptance also exposed a Codex title precedence defect: the native
session_index.jsonl had the generated name, while threads.title still held the
full original prompt. Prefer the UUID-matched latest nonempty title index record
in targeted refresh, catalog metadata and migrated history, matching JSONL
provider discovery. Test fallback, rename and wrong-UUID rejection. A Mac client
visually rendered the remote permission card while its source Mac was locked.

## Reconnection acceptance follow-up

A real daemon upgrade reproduced duplicate permission cards in a Mac client:
reconnection replayed the current card with a new native ID alongside its old
cached ID. On the authenticated first UUID/provider-matching state of each new
subscription, retire prior stream interaction display handles before replay.
Do not clear them on transport failure or ordinary running/idle status. Preserve
queued-send state and outstanding/unknown response receipts; this is resync,
not a claim that the provider resolved or executed a command. Cancelled or
foreign device/provider/session callbacks cannot clear another chat. Version
1.1.20 (110) carries this follow-up, with focused reducer and stream tests.
