# ADR 0167: Create DeepSeek Harness sessions through Muster's plugin

- Date: 2026-11-09
- Status: Accepted; source and tests verified, the plugin sequence verified on
  the installed App host in an isolated profile. No daemon-to-App run,
  release, installation or device acceptance.
- Related: [ADR 0074](0074-deepseek-harness-plugin-control.md),
  [ADR 0094](0094-local-plugin-contract-and-receipt-recovery.md),
  [ADR 0097](0097-task-scoped-scheduler-grant-for-locked-use.md),
  [ADR 0141](0141-multica-service-and-dsh-app-bridge-design.md),
  [ADR 0150](0150-remote-desktop-session-creation.md),
  [ADR 0163](0163-multica-dsh-task-sessions.md),
  [APP_PROFILES.md](../APP_PROFILES.md),
  [plugin README](../../src/muster/deploy/deepseek-harness/README.md)

## Context

The owner wants a local task runner to start DeepSeek Harness (DSH) sessions
through Muster. The runner calls `agent_native_create`, as it already does
for Codex and Claude. That RPC accepted only `codex` and `claude`, and it
creates sessions by driving the app's window through Accessibility. DSH
exposes no web content to Accessibility. Muster drives its sessions through
`muster-dsh-bridge`, a plugin inside the app (ADR 0074). ADR 0141 recorded
the missing piece: the plugin had no creation contract. It also ruled out
widening the provider check so that DSH would go down the Accessibility path.

ADR 0163 has since given the plugin versioned task methods for the attended
Multica workbench. The plugin offers them only inside the desktop App's host,
and the workbench also checks that the plugin's process is a child of the
signed App. `task.create.v1` creates exactly the named `session-<uuid>` in a
canonical directory and the expected profile store. It does so only when no
session of that name exists, live or stored. The methods never send a
prompt. The workbench reserves a random ID in its own store before it
touches the App.

A second gap concerns the session's answer. A task runner knows a turn is
answered when it sees a `final_answer` message. Muster's app timeline and
ACP host use the same marker. DSH records no answer phase, so its sessions
never show one.

## Decision

1. **`agent_native_create` accepts `deepseek` on a direct request.** The
   validation that the scheduler (ADR 0097) and Locked Use (ADR 0150) share
   still accepts only Codex and Claude. It refuses DSH with a named reason,
   so neither path creates DSH sessions or drives DSH through Accessibility.
2. **Muster names the session.** `plugin_control::created_session(op_id)`
   derives `session-<uuid>` from the ledger operation by SHA-256 under its
   own domain tag. The UUID carries the version-4 form that `task.create.v1`
   accepts.
   - A retry of the same operation can only name the same session.
   - The first message's request id is `prompt_id(session, op_id)`. ADR
     0094's receipt lookup therefore confirms it exactly.
   - Unlike ADR 0163, nothing else is stored. The ledger operation is the
     reservation, and the name can be derived again after a restart.
3. **The plugin's code is unchanged.** Creation reuses ADR 0163's contract:
   `task.create.v1`, then the existing `prompt` operation, both bound to the
   same ledger operation. No other creation method is added to the plugin.
4. **Preconditions come before the ledger,** so a refused operation can be
   retried:
   - The Mac is unlocked. As in ADR 0074, DSH meets the same locked-Mac rule
     as Claude and Codex. A locked Mac is refused with "Native creation
     requires an unlocked desktop". No Locked Use transaction may own the
     console.
   - The app is on this Mac's allowlist.
   - The directory exists. Its canonical form is sent and must be UTF-8.
   - The plugin is live and offers ADR 0163's task methods.
   - The plugin's process is a child of the signed DSH desktop App. This is
     ADR 0163's check, now `Platform::signed_app_child`, which the workbench
     shares.
   - Accessibility is not required.
5. **The creation**
   - takes the app's provider queue and desktop lock;
   - records `agent.native_create` in the ledger;
   - checks the unlocked Mac and the desktop App again;
   - sends `task.create.v1` with the session, the directory and the DSH
     store, and requires the App to confirm exactly those;
   - sends the prompt as the first message;
   - waits up to 15 s for the transcript to list the session in the
     directory, with the prompt as its first user message.

   The outcomes:
   - **Applied.** The App acknowledged the first message. This holds even
     before its transcript shows. After a lost reply, the receipt lookup or
     the transcript showing it also counts.
   - **Rejected.** The plugin or the App refused before it confirmed the
     session. Or the first message was definitely not sent after the session
     existed; the reason then names the blank session left behind.
   - **Unknown.** The create reply was lost or named something else, or the
     first message's reply was lost and no receipt shows it. The operation
     is never replayed.
6. **The scheduler grant is unchanged.** ADR 0097's grant still names only
   Codex and Claude. The Multica workbench in ADR 0163 keeps its own
   reservations and never prompts.
7. **DSH marks its answers.** A new `[source.turn]` field,
   `final_without_calls`, marks the text of an assistant message that calls
   no tool as `final_answer`. The DSH profile sets it, because its turn ends
   on the step that calls no tool.

## Alternatives

- **Scripting the app's window.** DSH exposes nothing to Accessibility.
- **Running DSH's CLI runtime (`dsh --stdio`).** It runs outside the app's
  sessions. ADR 0141 and ADR 0163 already rejected it.
- **A plugin `create` operation that creates and prompts in one call.** An
  earlier draft of this decision added one. It duplicated ADR 0163's
  creation with weaker checks: it refused only live sessions, so a stored
  blank session of that name could be reused, and it checked neither the
  store nor the canonical directory. It also needed another plugin release.
- **Letting the App mint the id and report it.** After a lost reply, Muster
  could not prove which session was its own.
- **A random reserved id, as ADR 0163 stores.** It would need its own store
  to survive a daemon restart. The name derived from the ledger operation
  needs none.
- **Marking the answer on `turn/end`.** That record carries no text. Marking
  an earlier record would mean revising messages already published.
- **Creating sessions on a locked Mac.** That needs its own Locked Use
  decision. The caller retries once the Mac is unlocked.

## Consequences

- Local callers can start DSH sessions without Accessibility. ADR 0150 made
  `AgentNativeCreate` a member operation, so a member with control of DSH
  can too, on an unlocked Mac. Muster's apps still offer only Claude and
  Codex in their creation form.
- Each Mac needs two things:
  - Install plugin 0.3.0 with ADR 0163's task methods into the app's desktop
    profile, then restart the app.
  - Put `com.deepseek.dsh` on the Mac's allowlist.

  An older plugin is refused before the ledger, with "Update the DSH
  desktop profile's Muster plugin".
- Creation is two plugin calls, not one. If the first message is definitely
  refused after the session exists, a blank session stays behind. The
  rejection names it. This needs the Mac to lock, the request to be
  cancelled or the plugin to restart between the two calls.
- A lost create reply stays unknown. Nothing here reopens or reconciles it.
- A step without tool calls that is followed by another step still has its
  text marked as an answer. This happens, for example, when a message is
  queued mid-turn. Session state still follows `turn/start` and `turn/end`.

## Verification

- **Source and tests.**
  - Plugin: its code is unchanged; its README names the new caller. Its 17
    node tests pass.
  - muster-agent: `Link::create` runs against a socket stand-in. The tests
    check the task contract, then the prompt, with their bindings and
    request id. They also check these cases:
    - a refusal before creation sends nothing more;
    - a lost or mismatched create is unknown;
    - a refused first message names the session;
    - a lost first message is settled only by the receipt lookup.

    The derived id has the version-4 form, and `prompt_id` is unchanged.
    The DSH reader test asserts `final_answer`.
  - musterd: the DSH integration test checks three things:
    - refusals before the ledger (allowlist, the App not running, a plugin
      outside the desktop App) leave no row and reach no plugin;
    - one `agent_native_create` creates the named session once, with one
      prompt, and a repeat is answered from the ledger;
    - a locked Mac is refused.

    A unit test checks that the shared scheduler and Locked Use validation
    refuses DSH.
- **Installed App host.** The installed App's desktop host was run with the
  plugin under a scratch `DSH_HOME`, with no model credentials. It used a
  derived session id and request id. The sequence:
  - `task.create.v1` returned exactly the session, directory and store;
  - `prompt` was accepted;
  - `prompt.lookup` confirmed the receipt;
  - a second `task.create.v1` was refused as not started;
  - the transcript had the session's header in that directory, then the
    prompt as its first user message, with the request id as `rpcId`.

  The user's `~/.dsh` profiles were not touched.
- **Not done.** A daemon-to-real-App run, a release, installation on the
  Macs and a task runner end-to-end run.
