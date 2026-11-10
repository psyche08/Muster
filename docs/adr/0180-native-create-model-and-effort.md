# ADR 0180: Choose a new desktop session's model and effort before its first message

- Date: 2026-11-10
- Status: Accepted; source and tests verified, the Claude form read on a Mac
  without sending. The effort keys, a daemon-to-app creation run, release,
  installation and device acceptance are not verified.
- Related: [ADR 0097](0097-task-scoped-scheduler-grant-for-locked-use.md),
  [ADR 0111](0111-native-organization-ax-and-file-confirmation.md),
  [ADR 0115](0115-target-scoped-local-input.md),
  [ADR 0116](0116-failure-screenshot-analysis.md),
  [ADR 0150](0150-remote-desktop-session-creation.md),
  [ADR 0167](0167-deepseek-harness-native-session-creation.md),
  [DESKTOP_CHAT_MODE.md](../DESKTOP_CHAT_MODE.md#model-and-reasoning-effort)

## Context

`AgentNativeCreate` (ADR 0150) opens a new Code session in the Claude or Codex
desktop app and sends its first message. The session always started on
whatever model and reasoning effort the app last used. A caller that needs a
particular model, such as a scheduled task that must not run on a small
model, could only change the session afterwards. By then the first message
has already been answered on the wrong model.

The existing options path (`AgentChatSetOptions`) works on an existing
session's composer. It is scoped to that session's controller, and it never
runs during a creation. The new-session form has the same picker, but there
is no session or controller yet.

Facts about the form, read on Claude 4.23118.1 in Code mode:

- The composer's picker is an `AXPopUpButton` described as
  `Model: <model> <effort>`, for example `Model: Opus 5.5 Extra`.
- Pressing it opens an `AXGroup` titled like the picker, not an `AXMenu`. Its
  models are `AXMenuItem`s. The current model carries `AXValue` 1 and the
  check mark `✓`, not `AXSelected`.
- After the models come `Effort <level>`, `More models`, and a paid
  `Enable fast mode` toggle.
- The effort levels are a submenu of the `Effort <level>` entry that opens on
  hover. `AXPress` leaves it closed. `AXShowMenu` on the entry opened the web
  view's own context menu (Copy, Select All) instead.
- The picker names the effort as the app's own picker does: `High` for
  `--effort=high` and `Extra` for `--effort=xhigh`.

The Codex composer shows one combined popup named by model and Power level
(`GPT-6 Astra High`). Its panel states the level as `High, 3 of 5.`, and the
slider takes arrow keys on its focused `Power` item. `AXPress` on its track
clicks the centre, so Accessibility alone can only reach the middle level
(DESKTOP_CHAT_MODE.md).

ADR 0111 already opens an Electron submenu whose trigger ignores `AXPress`:
it focuses the exact menu item, reads back that the app's focused element and
window are that item and window, and posts one application-directed Right
Arrow with `AXUIElementPostKeyboardEvent`. ADR 0115 refuses such posted keys
while a modifier is held.

## Decision

- **Request.** `AgentNativeCreateRequest` gains `model = 5` and `effort = 6`.
  Each is named exactly as the app's own picker names it (`"Opus 5.5"`,
  `"High"`). Empty keeps the app's current choice. Each must be at most
  128 bytes, with no control characters and no surrounding spaces.
  `native_creation` checks this before any unlock, and `well_formed` checks it
  again for every OS call. Because both are fields of the request, the Locked
  Use digest (`native_create = 15`) binds them without other changes.
- **Service.** On macOS the daemon advertises
  `agent.native_create.options.v1` next to `agent.native_create.v1`. A caller
  that sets `model` or `effort` must first see this service in `DeviceInfo`.
  An older daemon decodes the request over protobuf and drops the unknown
  fields, so it would create the session on the app's current model. An older
  `muster rpc` parses its JSON with `deny_unknown_fields`, so it refuses
  `model` or `effort` locally with an "unknown field" error and sends nothing.
- **Order (platform).** `native_create::submit` runs these steps under the
  existing creation guards:
  1. Open the new form and check that its composer is empty.
  2. `native_options::prepare_new` chooses the model through the form's
     picker. It presses an entry only if the picker does not already show
     that model.
  3. It chooses the effort on fresh reads, because the model can change the
     offered levels and Codex's picker label.
  4. It reads both back: the model from the picker's label or a fresh menu
     read, the effort from the Power status, the menu's selection, or
     Claude's `Effort <level>` entry.
  5. It drafts the first message and finds the send button.
  6. Just before pressing send, `still_shown` re-reads the picker's label. A
     label that changed since step 4 refuses the creation.

  The same `native_options` code serves existing sessions and the new form
  through a `Scope` trait. `SessionScope` keeps the session checks. The form's
  scope reuses the creation's own guard (unlocked console, idle input,
  process identity, exact window, page, directory and empty form) and its web
  area and application element.
- **Effort keys.** An effort that `AXPress` cannot reach is chosen with
  application-directed Accessibility keys, the route ADR 0111 uses for
  submenus. Each key:
  1. sets `AXFocused` on the exact control inside the form's web area;
  2. waits until the app's `AXFocusedUIElement` is that control, its
     `AXFocusedWindow` is the form's window, and the control reads
     `AXFocused`;
  3. is refused while a modifier is held (ADR 0115);
  4. re-checks the scope, then posts one key down/up pair to the app with
     `AXUIElementPostKeyboardEvent`.

  No pointer event, no global or HID-level input, and no key outside that
  application. Every step re-checks the creation guard, so local input
  anywhere stops the walk.
- **Claude's effort.** If the panel's `Effort <level>` entry already names the
  request, nothing is pressed. Otherwise the entry gets one Right Arrow. The
  levels are the enabled menu entries that the panel did not list before the
  key and that are not the entry itself. They count once two reads agree.
  The level named exactly like the request is pressed, and the panel is
  closed by toggling its picker. A name the levels do not have is refused
  with the levels in the message. Levels that do not open within 2 seconds
  refuse the creation as unavailable.
- **Codex's effort.** Power is walked one level per key: Left Arrow lowers,
  Right Arrow raises. After every key the panel's status is read again, and
  the key must move exactly one level, within 2 seconds. Because the app
  states only the level it shows, the walk goes down to the lowest level
  first, then up, and stops on the first level with the requested name. A
  name no level has walks back to the starting level and is refused with
  every level's name. A Codex form whose effort picker is a plain menu has
  the level pressed as on existing sessions.
- **DeepSeek Harness.** The plugin path (ADR 0167) has no model picker. A
  non-empty `model` or `effort` is refused before the ledger, the plugin or the
  App host is involved.
- **Refusal.** Nothing is sent when a name cannot be chosen or confirmed. The
  ledgered creation settles as `Rejected` (status 2). Each message starts
  with `The new session's model and effort were not set: ` after the error
  kind, and ends with `; nothing was sent`.
  - Final (`invalid:`), refused again if asked again unchanged:
    - `model "X" is not offered (the app offers "A", "B")`
    - `effort "X" is not offered (the app offers "A", "B")`
    - `effort "X" cannot be chosen: the app shows no effort for this model`
    - `the app did not confirm model "X" (it shows "Y")`
    - `the app did not confirm effort "X" (it shows "Y")`
    - `the model picker changed from "A" to "B" before the first message`
  - Retryable later (`unavailable:`), when the app did not respond as read:
    - `effort "X" could not be chosen: the app did not open its effort levels`
    - `the Power slider moved from 3 to 1 of 5 on one key`
    - `the Power slider stayed at "High"`
    - `the Power slider did not settle on a level`
    - `the Power panel closed`
  - `DeepSeek Harness sessions take no model or effort from Muster` is
    returned before the ledger as an `Invalid` RPC error.

  A malformed name is refused before the ledger with
  `invalid: Native creation model and effort must be names of at most 128
  bytes, without control characters or surrounding spaces`. Guard refusals
  (local input, lock, identity) keep their existing `conflict:` messages.
- **Model list.** The paid fast-mode toggle is never offered as a model.

The scheduler grant (ADR 0097) and the apps' "New desktop session" form leave
both fields empty and keep the app's choice.

## Alternatives

- **Change the model after the first message.** This is what callers could
  already do. The first answer would still come from the wrong model.
- **Check Claude's effort and refuse any other level.** This was the first
  version of this decision. It left callers unable to choose an effort other
  than the one last used in the app.
- **Synthetic pointer input.** A posted mouse move over the `Effort` entry
  (`CGEventPostToPid`) or a click would open the levels. It depends on window
  coordinates and visibility, and ADR 0116 allows pointer input only as a
  later stage with its own idle, stop-on-input and restore rules. The focused
  key needs neither coordinates nor a visible pointer.
- **Global keyboard events (`CGEventPost`).** They would reach whatever app is
  frontmost. The application-directed key reaches only the app whose focused
  element was read back.
- **Codex Power by pressing the track.** `AXPress` clicks the centre, so it
  reaches only the middle level.
- **Map effort names to CLI flags (`xhigh`).** The picker names are what the
  person sees and what Accessibility reads back. Mapping would add a second
  vocabulary that the app does not show.
- **Best effort: send on the current model and report it.** This was
  rejected. A creation that asked for a model must never run on another one.
- **Refuse unknown names in an older daemon.** That is impossible, because
  protobuf drops the unknown fields. The service string is the guard.

## Consequences

- A caller can pin a new Claude or Codex Code session to any model and
  effort the app offers before the first message.
- Choosing on the new form can also change the model and effort the app
  proposes for later new sessions, as choosing them by hand can.
- Walking Codex's Power can pass through other levels on the form before it
  settles. Nothing is drafted or sent meanwhile. If local input stops the
  walk, the form keeps the level it reached, and the creation is refused.
- Callers must check `agent.native_create.options.v1`. Without the service,
  they must not send the fields.
- `invalid:` refusals are final. `unavailable:` refusals may be retried later
  by callers that retry that kind.
- The existing-session path (`AgentChatSetOptions`) keeps its behaviour: it
  still cannot choose Claude's effort, and Power still only keeps a level or
  raises it to `Middle`.

## Verification

- **Source and tests.**
  - `muster-proto`: the Locked Use digest test now also changes `model`,
    `effort` and clears `effort`, and each change alters the digest.
  - `muster-agent`: fixtures modelled on the Claude 4.23118.1 form cover:
    - reading the shown model and effort, without offering fast mode;
    - finding Claude's effort levels once they open, and refusing a name
      they do not have;
    - walking Power to every level by name, walking back before refusing an
      unknown name, and stopping on a key that moves more than one level;
    - name validation and the refusal messages.
  - `musterd`: tests cover the request shape check and a DeepSeek Harness
    refusal with no ledger row and no plugin call.
- **Mac read (2026-11-10).** A read-only probe opened and closed the form's
  picker on Claude 4.23118.1. It confirmed the tree above, including that
  `AXPress` and `AXShowMenu` leave the effort levels closed. No message was
  sent, and the model was not changed.
- **Not verified.**
  - That Claude's focused `Effort` entry opens its levels on the Right Arrow,
    and what the levels are named. ADR 0111's submenu key is also still
    pending device acceptance.
  - That Codex's focused `Power` item moves one level per arrow key on the
    new-session form.
  - A daemon-driven creation with `model` and `effort` on Claude or Codex,
    unlocked or in Locked Use.
  - Signed build, release and installation.
- **Device acceptance (pending).** This needs an unlocked Mac and the
  person's approval to send one test message from a new session in a scratch
  directory:
  1. Through a development daemon with its own state directory, run
     `muster rpc '{"device_info":{}}'` and check that it lists
     `agent.native_create.options.v1`.
  2. Request a Claude creation with a model and an effort other than the
     current ones. Expect `Applied`, a new session whose picker shows both,
     and a transcript with the first message.
  3. Request a Claude creation with an effort the app does not offer. Expect
     `Rejected` with the levels in the message, and no new session.
  4. Repeat steps 2 and 3 for Codex, with a Power level two levels away.
  5. Repeat step 2 on a locked Mac through Locked Use.
  6. Restore each app's previous model and effort.
