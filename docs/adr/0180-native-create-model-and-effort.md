# ADR 0180: Choose a new desktop session's model and effort before its first message

- Date: 2026-11-10
- Status: Accepted; source and tests verified, the Claude form read on a Mac
  without sending. No daemon-to-app creation run, release, installation or
  device acceptance.
- Related: [ADR 0097](0097-task-scoped-scheduler-grant-for-locked-use.md),
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
- The effort levels are a hover-only submenu. `AXPress` and `AXShowMenu`
  leave it closed. `AXShowMenu` on the entry opened the web view's own
  context menu (Copy, Select All) instead.
- The picker names the effort as the app's own picker does: `High` for
  `--effort=high` and `Extra` for `--effort=xhigh`.

The Codex composer shows one combined popup named by model and Power level
(`GPT-6 Astra High`). Its Power slider has the limits recorded in
DESKTOP_CHAT_MODE.md: Accessibility can keep the current level, or raise a
lower level to the middle.

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
  3. On Codex, it chooses the effort from a fresh menu, because the model
     can change the offered levels.
  4. It reads both back: the model from the picker's label or a fresh menu
     read, the effort from the menu's selection or Claude's `Effort <level>`
     entry.
  5. It drafts the first message and finds the send button.
  6. Just before pressing send, `still_shown` re-reads the picker's label. A
     label that changed since step 4 refuses the creation.

  The same `native_options` code serves existing sessions and the new form
  through a `Scope` trait. `SessionScope` keeps the session checks. The form's
  scope reuses the creation's own guard (unlocked, idle, identity and draft
  checks) and its web area.
- **Claude's effort is checked, not chosen.** Synthetic pointer or keyboard
  input stays out of scope. Before anything is pressed, the daemon reads the
  form's `Effort <level>`. If it differs from the request, the creation is
  refused without opening or pressing anything else.
- **Codex.** Models are chosen as on existing sessions. The effort follows the
  Power rules: the current level is kept, and `Middle` raises a lower level.
  Any other level is refused, and the refusal names the levels that can be
  chosen.
- **DeepSeek Harness.** The plugin path (ADR 0167) has no model picker. A
  non-empty `model` or `effort` is refused before the ledger, the plugin or the
  App host is involved.
- **Refusal.** Nothing is sent when a name cannot be chosen or confirmed. The
  ledgered creation settles as `Rejected` (status 2), and nothing is
  submitted to the provider. Every refusal is an `Error::Invalid`, so it is
  final rather than retried. Each message starts with
  `invalid: The new session's model and effort were not set: ` and ends with
  `; nothing was sent`. The reason between them is one of:
  - `model "X" is not offered (the app offers "A", "B")`
  - `effort "X" cannot be chosen through Accessibility and the app's effort
    is "Y"; choose it once in the app` (Claude)
  - `effort "X" cannot be chosen through Accessibility (it can choose "Y");
    choose it once in the app` (Codex)
  - `effort "X" cannot be chosen: the app shows no effort for this model`
  - `the app did not confirm model "X" (it shows "Y")`
  - `the app did not confirm effort "X"`
  - `the model picker changed from "A" to "B" before the first message`
  - `DeepSeek Harness sessions take no model or effort from Muster`
    (returned before the ledger as an `Invalid` RPC error)

  A malformed name is refused before the ledger with
  `invalid: Native creation model and effort must be names of at most 128
  bytes, without control characters or surrounding spaces`.
- **Model list.** The paid fast-mode toggle is never offered as a model.

The scheduler grant (ADR 0097) and the apps' "New desktop session" form leave
both fields empty and keep the app's choice.

## Alternatives

- **Change the model after the first message.** This is what callers could
  already do. The first answer would still come from the wrong model.
- **Synthetic input for Claude's effort.** A synthetic hover or click would
  open the effort submenu. It would move the real pointer, need the window
  frontmost and visible, and cannot run on a locked Mac or in Locked Use,
  where remote creations usually happen. ADR 0116 allows synthetic input only
  as a later stage with its own idle, stop-on-input and restore rules. The
  read-and-refuse check is needed in any case as the fallback for locked Macs.
  Adding synthetic input is a separate decision.
- **Map effort names to CLI flags (`xhigh`).** The picker names are what the
  person sees and what Accessibility reads back. Mapping would add a second
  vocabulary that the app does not show.
- **Best effort: send on the current model and report it.** This was
  rejected. A creation that asked for a model must never run on another one.
- **Refuse unknown names in an older daemon.** That is impossible, because
  protobuf drops the unknown fields. The service string is the guard.

## Consequences

- A caller can pin a new Claude Code session to a model before the first
  message. It can pin Claude's effort only to the level the app already
  shows. A different level is refused with the app's current effort in the
  message, so the person can choose it once in the app and retry.
- Choosing a model on the new form can also change the model the app proposes
  for later new sessions, as choosing it by hand can.
- Callers must check `agent.native_create.options.v1`. Without the service,
  they must not send the fields.
- Refusals are final (`invalid:`), so callers that retry `busy:` or
  `unavailable:` refusals will not retry these.
- The existing-session path keeps its behaviour. Its Claude confirmation by
  picker label is unchanged.

## Verification

- **Source and tests.**
  - `muster-proto`: the Locked Use digest test now also changes `model`,
    `effort` and clears `effort`, and each change alters the digest.
  - `muster-agent`: fixtures modelled on the Claude 4.23118.1 form cover:
    - reading the shown model and effort, without offering fast mode;
    - checking but not choosing Claude's effort;
    - the exact Power refusal message;
    - name validation.
  - `musterd`: tests cover the request shape check and a DeepSeek Harness
    refusal with no ledger row and no plugin call.
- **Mac read (2026-11-10).** A read-only probe opened and closed the form's
  picker on Claude 4.23118.1. It confirmed the tree above, including that
  `AXPress` and `AXShowMenu` leave the effort levels closed. No message was
  sent, and the model was not changed.
- **Not verified.**
  - A daemon-driven creation with `model` and `effort` on Claude or Codex,
    unlocked or in Locked Use.
  - Codex's new-session form. The combined popup was read on an existing
    session only.
  - Signed build, release and installation.
- **Device acceptance (pending).** This needs an unlocked Mac and the
  person's approval to send one test message from a new session in a scratch
  directory:
  1. Through a development daemon with its own state directory, run
     `muster rpc '{"device_info":{}}'` and check that it lists
     `agent.native_create.options.v1`.
  2. Request a Claude creation with a model other than the current one and
     with the current effort. Expect `Applied`, a new session whose picker
     shows that model, and a transcript with the first message.
  3. Request a Claude creation with a different effort. Expect `Rejected`
     with the Claude effort message, and no new session.
  4. Repeat steps 2 and 3 for Codex with a model and `Middle` or the current
     Power level.
  5. Repeat step 2 on a locked Mac through Locked Use.
  6. Restore each app's previous model.
