# ADR 0158: Queued message actions and Codex paginated runtime state

- Date: 2026-11-07
- Status: Accepted; source and tests verified, installation and runtime acceptance pending
- Related: [ADR 0063](0063-queue-receipts-use-provider-text-matching.md), [app profiles](../APP_PROFILES.md), [Locked Use](../LOCKED_USE.md)

## Context

Four problems were reported against the queued-message card and session
state in the phone app:

1. A Claude Code prompt sent while the previous turn was thinking stayed in
   the card after the next turn had already processed it.
2. A queued message could not be sent at once from Muster for Claude Code,
   only for Codex.
3. A queued message could not be deleted from Muster in either app.
4. Codex desktop sessions that were running showed as idle or unknown. Every
   Codex session other than the one open in the desktop app showed as
   unknown.

Claude Code records its queue as `queue-operation` records in the session
transcript. An enqueue carries the prompt text, except for a prompt that
holds only an image. A dequeue carries no content. A remove carries the text
and sometimes a reason (`absorbed_mid_turn`). Claude dequeues its most urgent
slot first and is first-in, first-out within a priority. Wrapped notifications
and peer prompts (text that starts with a `<tag>`) enqueue as `later`; a
person's prompt enqueues as `next`. The reader removed the oldest slot on
every dequeue. A queued prompt behind an earlier notification therefore
stayed in the card after Claude had dequeued it. A remove whose text matched
no slot, which happens when an image-only slot is removed with text, also
left its slot behind. Send now in the desktop app writes a dequeue and then
enqueues the same text again at `now` priority. The reader took that dequeue
for the oldest slot, so an older prompt left the card while the sent one
stayed. A replay of 880 local transcripts left 10 sessions showing stale
queued prompts.

In the Claude desktop Code tab, a queued prompt is an unread message in the
conversation. Its "Message actions" toolbar offers Send now, which stops the
current step so Claude reads the message next, and Cancel and edit message,
which takes the message off the queue and puts its text back into the
composer. Prompts that cannot be edited offer Remove queued message instead.
This was recorded on 2026-11-07 from the live accessibility tree, with probe
prompts sent through `muster send`. Codex rows carry Steer or Send now beside
Delete queued message.

Codex's paginated history (`history_mode = 'paginated'` in `state_5.sqlite`)
lists such threads from the shared `thread_history_1.sqlite` store. The
catalog path of those threads is that store. The app-server worker still
keeps the thread's own rollout JSONL open and still writes `task_started` and
`task_complete` there. Runtime observation matched the worker's open files
against catalog paths only, so it never matched a paginated thread.

## Decision

**Claude queue replay.** On a dequeue, `queue::read_claude` removes the
oldest slot of the most urgent priority. Slots whose text is a wrapper are
`later`; every other slot, including an image-only one, is `next`. An
enqueue right after a dequeue is a Send now when its text is the text just
dequeued or the text of a slot still waiting. It is `now` priority. In the
second case the waiting slot with that text is the one Send now took, and
the slot the dequeue retired goes back in its place. A remove retires the
slot with the same text. If no slot has that text, it retires the oldest
slot without text.

**Codex runtime evidence.** `observe_runtime` matches each open rollout
against a paginated thread whose id equals the UUID in the rollout's file
name. Activity is read from that rollout with the existing turn grammar. A
thread whose rollout no worker holds open stays unknown, as before.

**Queue actions are interaction options.** A queued message's interaction
offers `Send now` when the provider can send it now and `Delete` when it can
be deleted (`queue::Action`). `respond_in_session` accepts either option for
a `queued:` interaction. It re-selects the message for that action and calls
`Controller::queued_action`, which replaces `send_queued`. The daemon still
gates `queued:` interactions on the profile's queue capability. Codex items
are deletable when they hold text and are queued. Claude items are offered
both actions only when `require_desktop_input` accepts the session, so a
command-line session shows its queue without controls.

**Claude declares its queue.** `profiles/claude.toml` gains
`[controls.queue]`: the row and delete selectors name Cancel and edit message
(with and without the attachment warning) and Remove queued message;
`send_now` names Send now. The new profile key `delete_returns_text = true`
says that a delete may put the message back into the composer. Declaring a
queue also limits Claude's sends to the composer's own branch, as for Codex,
so a queued row's Send now is never taken for the composer's Send.

**Deleting.** The macOS controller finds the delete control in the one row
that shows the message's text, by the same row rule as Send now. Before the
press it re-reads the queue and the local input generation, and the press is
marked as a possible delivery for Locked Use, as for Send now. The delete is
confirmed when no queue item carries the message's identity any more
(`queue::Removal`). If the send receipt for the message's text arrives first,
the turn took the message, and the delete is reported as a conflict. If the
queue does not confirm within ten seconds, the outcome is unknown. When
`delete_returns_text` is set, the delete is refused unless the composer is
empty. After the removal is confirmed, the composer is emptied only if it
holds exactly the returned text (`queue::returned_to_composer`) and no local
input has arrived since the request began. Any other text is left to the
person.

**Phone apps render the options.** The iOS and Android cards show one button
per option the core offers, under the option's name. Delete is shown as a
trash icon. Which options exist and whether they apply stay in Shared Rust.

## Alternatives

- Keep Claude queue items read-only and send a fresh prompt instead of
  pressing Send now. That would duplicate the message, because the queued
  original would still be delivered.
- Delete a Claude item by clearing the composer after any delete without
  checking it first. That could erase a draft the person was writing in the
  desktop app.
- Tell a delete from a delivery by the remove record's reason. The reasons
  Claude writes are not documented and differ between paths. The send
  receipt already recognizes every delivery form Muster accepts for a send.
- Match paginated Codex threads by their shared store path. That path is
  the same for every paginated thread and cannot name one.

## Consequences

- Claude Code prompts leave the card when Claude takes them, including
  prompts queued behind notifications and image-only prompts.
- Codex sessions loaded in the desktop app's server show running and idle
  again. Threads no worker has loaded stay unknown, which is accurate.
- Claude Code and Codex queued messages can be sent at once or deleted from
  the phone. A Claude delete needs an empty composer in the desktop app.
- Claude's composer send now uses the queue-aware branch rule. Recorded
  Claude trees confirm that each composer still reaches exactly its own Send.
- Two queued messages with the same text still cannot be acted on, because
  the row is located by its text.

## Verification

- Source and tests: `cargo test -p muster-agent` passes, including the
  Claude priority replay, Send now requeue, image-slot retirement, removal
  identity, desktop-versus-command-line options, and paginated Codex runtime
  tests.
  `muster-platform` tests pass, including Claude's recorded toolbar structure
  for Send now and Cancel and edit message. `musterd` and `muster-app-core`
  tests pass.
- The same 880-transcript replay with these rules leaves no session with a
  stale queued prompt, down from 10. It recognizes 24 Send now requeues.
- Live evidence: the Claude desktop accessibility tree and transcript records
  for two probe prompts, recorded on 2026-11-07.
- `swift test --filter AgentRenderingTests` passes on macOS, including the
  queued card with both options.
- Not done: the Android card was not built (no Android SDK on the build
  Mac), and neither phone app was run on a device. Signing, installation,
  and runtime acceptance of Send now and Delete against the live Claude and
  Codex apps are pending.
