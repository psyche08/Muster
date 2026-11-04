# ADR 0116: Look at a failed native send with the user's own LLM

- Date: 2026-11-04
- Status: Accepted; source, tests and device acceptance verified.
- Related: [ADR 0115](0115-target-scoped-local-input.md),
  [ADR 0046](0046-send-recovery-and-quiet-admission.md),
  [SECURITY.md](../SECURITY.md), [AGENT_CONTROL_DESIGN.md](../AGENT_CONTROL_DESIGN.md)

## Context

A native send that fails tells the phone what Muster's Accessibility rules
could not find: "The native prompt is not visible", "desktop changed during
session selection". It does not say what the app showed instead. The cause
is usually obvious to anyone looking at the Mac: a sign-in sheet, an update
dialog, a settings page, a conversation still loading. The person holding
the phone cannot look.

The owner asked for three things, in this order of trust:

1. capture the target app's window when a send fails, and have a
   vision-capable LLM of their choosing (their URL, key and model) say what
   is on screen;
2. let that LLM find the composer and the send button when the
   Accessibility rules cannot, and send through Accessibility;
3. as a last resort, click and type with synthetic input.

This decision covers the first. The second and third change what Muster
submits and get their own decisions; the rules they must keep are recorded
here so the configuration does not have to change again.

Constraints:

- musterd, not Muster.app, holds the Accessibility grant. Window capture
  belongs there too, with its own Screen Recording grant.
- musterd is a bare signed executable without a keychain access group, so it
  cannot read the app's data-protection keychain items.
- The phone sees a failure only as `OpResult.message` or `Error.message`.
- A screenshot and the app's interface text go to a third party only after
  the owner configured that party on this Mac.

## Decision

- **Configuration, local owner only.**
  - A new local-only request `VisionRequest` gets, saves, tests and clears the
    configuration. It has an API format (`openai` for OpenAI-compatible chat
    completions, `anthropic` for the Messages API), a base URL, a model and
    an on/off switch. The answer is `VisionStatus`.
  - The base URL follows each SDK's convention. An OpenAI-compatible base
    includes its version path (`https://api.openai.com/v1`), and requests go
    to `{base}/chat/completions`. An Anthropic base does not
    (`https://api.anthropic.com`), and requests go to `{base}/v1/messages`.
  - Shared Rust validates the base URL. It must be HTTPS, or HTTP to a
    loopback host for a local model server. It must have no credentials,
    query or fragment. Redirects are never followed.
  - The non-secret fields live in musterd's store under `vision.config`.
    The feature is off until the owner turns it on with a URL, a model and a
    key.
- **The key is write-only.**
  - Muster.app sends the key once over the 0600 local socket. musterd stores
    it in a login-keychain generic password it created itself (service
    `dev.linsheng.musterd.vision`), so only musterd's signature reads it
    back without a prompt.
  - `VisionStatus` reports only whether a key is stored. The key never
    appears in the store, logs, diagnostics or any reply. An error text that
    contains the key has it removed before it is used.
  - A read in the background (after a failed send) never shows a keychain
    prompt: it fails instead. Muster never unlocks a keychain or asks for a
    keychain password.
- **When Muster looks.**
  - Only after an ordinary (not Locked Use) native agent send fails with an
    unavailable, not-found, I/O or unknown outcome, and only while the Mac is
    unlocked.
  - A conflict (local input, another operation, an unconfirmed earlier
    send), a busy app or an invalid request already says why. Nothing is
    captured for those.
  - Locked Use never waits for an LLM. Its operations never reach the
    look, and the look starts only after the ordinary request has released
    its console reservation, so a Locked Use transaction is not refused as
    busy while an LLM answers.
- **What Muster sends.**
  - A PNG of the session's app window: the largest on-screen, normal-level
    window of that app, at most 1568 pixels on its longest side, captured by
    ScreenCaptureKit, which needs musterd's Screen Recording permission. The
    picture shows whatever the window shows, including the conversation.
    Muster asks for the permission when the owner turns the feature on.
  - The app's name and the failure text, bounded, with the message being
    sent removed from it.
  - No text field carries the message, the transcript or the session id.
    Without Screen Recording the request carries the text only and the
    reply says so.
- **What comes back.**
  - The model is asked for at most two sentences. The reply is flattened
    to one line, stripped of control characters and bounded to 300
    characters. It is appended to the failure message the phone shows,
    after `Muster looked:`.
  - The diagnosis is advice. It changes neither the ledger's recorded result
    nor the outcome class: a rejected send stays rejected and an unknown one
    stays unknown.
  - The LLM call has a 20 s budget. On any error the phone gets the original
    failure with a short reason why the look failed (`Muster could not
    look:`). A caller that has gone stops the look.
- **Rules the later stages must keep.** They are not implemented here.
  - Assisted send runs only when the outcome is provably not started and the
    session's own web area is identified. The LLM chooses among enumerated
    Accessibility candidates inside that web area and never supplies the
    text.
  - Synthetic input runs only after the Accessibility path is exhausted,
    while the Mac is unlocked, not in Locked Use and idle Mac-wide. It stops
    on any local input and restores the pointer and the previous frontmost
    app.
  - Neither stage ever repeats a send whose outcome is unknown.

## Alternatives

- **Muster.app captures and calls the LLM.** The app does not run when
  musterd sends for the phone, and it holds neither the Accessibility nor the
  session state. Rejected.
- **Keep the key in the app's data-protection keychain and hand it to
  musterd at every launch.** musterd runs under launchd without the app, so
  it would lose the key at every restart. Rejected.
- **Plain text in the store.** The store is unencrypted SQLite. Rejected.
- **Send the screenshot to the phone instead of to an LLM.** It leaks the
  whole window to every paired client and still leaves the person guessing.
  It may come later as an explicit, separate choice.
- **One protocol.** The owner asked for both. Most local and hosted servers
  speak the OpenAI-compatible format; Anthropic's differs in headers, image
  blocks and reply shape.

## Consequences

- A failed send can explain itself: "a sign-in sheet covers the window",
  "the app shows Settings, not a conversation".
- The owner's LLM provider receives a picture of the app window. The
  Settings section says so before the switch can be turned on. Turning the
  switch off, or clearing the key, stops it.
- musterd needs Screen Recording for screenshots. Without it the look uses
  the failure text only. macOS shows no dialog for a launchd agent, so
  turning the feature on registers musterd in System Settings and the owner
  switches it on there.
- A failed send now takes up to 20 s longer to report while the feature is
  on.

## Verification

- Source and tests:
  - `muster-agent` unit tests cover URL validation (HTTPS, loopback HTTP,
    credentials, query, fragment, unknown format).
  - They cover request bodies and headers for both formats, with and
    without an image.
  - They cover reply parsing (string and part arrays, Anthropic text blocks,
    HTTP errors), key redaction, and the diagnosis bounds.
  - `musterd` tests cover which failures are looked at and that the message
    being sent is removed from the failure text.
- Builds: workspace build, strict clippy, rustfmt; Swift bindings
  regenerated; the `MusterUI` Swift target builds.
- Signing and publication: not part of this change.
- Device acceptance, 2026-11-05, on the development Mac (macOS 27.0) against
  an installed bundle built from `406296a5`. That bundle also carried an
  unmerged change that prompts for Screen Recording at startup; it edits none
  of the sources this decision names, which are identical to `main` in it.
  A loopback endpoint stood in for the owner's provider: it recorded exactly
  what musterd sent and answered in either format, so the run needed no API
  key and reached no third party. Requests went in over the 0600 local socket
  with `muster rpc`, the same local-owner path Settings uses.
  - Both protocols. `http://127.0.0.1:18116/v1` as `openai` reached
    `POST /v1/chat/completions` with `authorization: Bearer`;
    `http://127.0.0.1:18116` as `anthropic` reached `POST /v1/messages` with
    `x-api-key` and `anthropic-version: 2023-06-01`. The test action answered
    through both, and saving again without a key kept the stored one.
  - The key. `vision.config` in the store holds the switch, the format, the
    base URL and the model and nothing else; the key is in neither
    `musterd.db` nor `musterd.log`; the item is musterd's own
    `dev.linsheng.musterd.vision` / `api-key`, and `VisionStatus` says only
    that one is stored. The background read after a failed send needed no
    prompt and unlocked nothing.
  - Screen Recording. Turning the feature on calls
    `CGRequestScreenCaptureAccess`, which for a launchd agent shows no
    dialog; it registered musterd in System Settings, where the owner
    switched it on, and `VisionStatus.screen_recording` followed without a
    restart.
  - Without the grant. A native send that failed with `unavailable: ChatGPT
    needs one supported local catalog database …` was looked at from the
    failure text alone: the request carried the app name and that text and
    said no screenshot was available, and the answer came back after
    `Muster looked:`. The missing permission failed nothing.
  - With the grant. The same send carried a 1568 x 902, 406,946-byte PNG of
    the app's largest on-screen window, its longest side exactly at the cap,
    and the answer described what that picture showed. The window held an
    open conversation, as this decision says it would.
  - Bounds and budget. A 900-character answer over several lines reached the
    caller as one line of exactly 300 characters ending in an ellipsis. An
    endpoint that answered after 25 s gave `Muster could not look: it took
    too long` after 20 s, with the failure text unchanged. A caller that gave
    up at 5 s was answered at 5 s, not at 20.
  - What left the Mac. The text being sent never appeared in the request, and
    the outcome did not move: the reply stayed `unavailable: ...` with the
    diagnosis on a second line.
  - Not covered. The annotated failure was read over the local socket, not on
    the phone, which was offline; it is the same `Error.message` the phone
    renders. No hosted provider was called, and the Settings screen was not
    driven.

## Linking the vision helpers into a Swift build

`muster-platform`'s `build.rs` compiles the shim's vision sources into the
static library and asks cargo to link `ScreenCaptureKit`, `ImageIO` and
`CoreGraphics`, so musterd and the Rust workspace build. The Swift package
links that same library through its own manifest, whose framework list is
separate, so `swift test` and `swift run MusterDev` failed to link the capture
classes once the vision objects were in the library. The manifest now asks for
the same three frameworks on macOS. No source of this decision changes.
