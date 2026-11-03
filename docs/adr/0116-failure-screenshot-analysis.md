# ADR 0116: Look at a failed native send with the user's own LLM

- Date: 2026-11-04
- Status: Accepted; source and tests verified, device acceptance pending.
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
  the failure text only.
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
- Signing, publication and installation: not part of this change.
- Runtime and device acceptance: pending.
  - Configure each protocol in Settings and test it.
  - Grant Screen Recording to musterd.
  - Make a send fail behind a dialog, and check the diagnosis on the phone.
  - Confirm that the stored key is not readable from the app, the store or
    logs.
