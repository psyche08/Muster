# ADR 0074: Drive DeepSeek Harness through a Muster plugin inside the app

- Date: 2026-10-31
- Status: Accepted; installed-plugin and device acceptance pending
- Related: [ADR 0073](0073-deepseek-harness-session-viewing.md), whose
  "do not drive the app yet" part this supersedes; ARCHITECTURE §3.6.2 and
  §3.6.4; [APP_PROFILES.md](../APP_PROFILES.md);
  [LOCKED_USE.md](../LOCKED_USE.md)

## Context

ADR 0073 made DeepSeek Harness sessions viewable. It left them undriven
because the app's window exposes no web content to Accessibility. The owner
asked to drive them through the app's own plugin system and chose:

- the first version covers prompts, interrupts, live state, approvals and
  question cards;
- a locked Mac meets the same requirements as Claude and Codex.

The installed DeepSeek Harness 0.2.1-rc.5 bundles its packages and their
TypeScript declarations. They show a supported plugin contract:

- **Bundles.** A plugin is an npm package that declares
  `"dsh": {"bundle": {"patch": "./cordis.patch.yml"}}`. It is added to a
  profile with `dsh plugin --profile <name> add <spec>`, where `file:` and
  `link:` specs are accepted. Its patch inserts Cordis plugin entries, which
  the desktop host loads in-process with the user's permissions.
- **Host services a plugin can inject.** These include `agents` (live agents,
  `status` `idle`/`running`, and `agent/status` events) and
  `sessionController`. The window's own Remote API uses
  `sessionController.prompt({requestId, sessionId, mode: queue|steer,
  content}, signal)` and `.cancel({sessionId})`. A prompt is idempotent by
  `requestId`, and the app records that id as the message's `rpcId`.
- **Approvals and question cards.** They are Cordis waterfalls:
  `approval/request` (`{agent, toolName, callId, reason, signal}`, answered
  `allowed-once` or `rejected`) and `user-questions/request` (`{questions,
  agent, signal, wait}`, answered `{answers: [{id, selected, custom}]}`).
  The window answers through the API gateway, which withdraws its prompt
  when the request's `signal` aborts. Both requests are fresh, unfrozen
  object literals.

## Decision

Ship a plugin, `muster-dsh-bridge` (`src/muster/deploy/deepseek-harness`).
DeepSeek Harness becomes the first app driven through a plugin rather than
Accessibility.

- **The plugin is an adapter.** It decides nothing. It serves one Unix
  socket, `$DSH_HOME/muster/bridge.sock`, in a 0700 directory with the
  socket at 0600. The protocol is one JSON request per line: `hello`,
  `state`, `prompt`, `cancel`, `approve` and `answer`. Prompts and cancels go
  through `sessionController`. For approvals and question cards it registers
  the first, global listener of each waterfall. That listener offers the
  request to Muster and passes it to the window at once, and the first answer
  wins.
- **Withdrawing the window's prompt.** The request's `signal` is replaced by
  one that also follows Muster's answer, so an answer from the phone
  withdraws the window's prompt. A request that cannot carry it stays with
  the window alone.
- **When the window cannot answer.** If no window is open, the plugin waits
  for Muster while Muster has polled within the last 10 seconds. Otherwise
  the request fails closed exactly as without the plugin. An answer is
  checked against the questions and options asked.
- **Profile.** `[plugin]` (`socket`, `protocol`, `questions`) is the second
  way a profile can drive an app. `build.rs` refuses an app that has both
  `[controls]` and `[plugin]`, and it accepts send, respond and interrupt
  only with one of them. `deepseek.toml` declares `send`, `respond`,
  `interrupt`, `files` and plugin questions. Queued-message edits and model
  choices stay with the app. The app is still never scanned for windows
  (`App::driven()` remains Accessibility-only).
- **Muster's client, `muster_agent::deepseek_app`.**
  - Every exchange is bound to the serving process. The socket's peer pid
    (`LOCAL_PEERPID`, or `SO_PEERCRED` on Linux) must be the pid that musterd
    verified, and a write is refused before it is sent when another process
    serves.
  - musterd verifies the pid against DeepSeek Harness's code signing
    requirement (`com.deepseek.dsh`, team `NAN929V4UM`). It reads the
    process's launch time on both sides of that check.
  - A request line only takes effect when complete, so a failed write means
    "not sent". A lost reply leaves the outcome unknown and is never
    replayed.
  - A plugin speaking another protocol is not used.
- **Controller and service.** `PluginController` is a new controller kind,
  `plugin`.
  - musterd's 1-second loop (`agent_bridge::deepseek_plugin`, macOS) refreshes
    the link. It attaches controllers to the app's listed sessions for the
    verified launch, and announces and resolves their approvals and question
    cards.
  - It detaches the controllers when that launch exits or another launch
    replaces it. A session reports `running`, `waiting_approval` or `idle`
    from the app's live state.
  - Approval options are `Allow once` and `Reject`. Question options carry
    the app's labels, and free text is allowed.
- **Daemon rules are unchanged.** DeepSeek plugin sessions are not
  "independent" the way cloud or Orca sessions are.
  - Remote operations meet the same gates as Claude and Codex: on a locked
    Mac, the Mac's own `allow_locked_control` policy first, then a signed
    Locked Use operation (presence and `locked.control`, and the app's bundle
    in that device's allowlist). Locked Use accepts the `plugin` controller.
  - `locked_operation` skips only the Accessibility steps for these
    sessions: the Accessibility grant, the input-quiet wait and session
    navigation. The provider queue, the desktop lock, capability checks and
    the idempotency ledger still apply.
  - When no plugin controller is attached, the request is refused with an
    installation hint, and nothing is sent.

This adds a third kind of session control beside Accessibility (desktop)
and the provider API (cloud): an in-app plugin the owner installs. It does
not reopen §3.6.2 for Codex or Claude, whose apps offer no such interface.

## Alternatives

- **Accessibility.** The window publishes no web content, and an
  Accessibility path would read and press UI that the app already exposes
  as typed services.
- **The app's authenticated local web server or Remote API.** Port 64336
  answers 401 without the owner's credentials. Using the API would mean
  holding the owner's credentials, or registering a peer principal and
  policy, which also needs a plugin. Typert framing is more than the six
  operations need.
- **The `dsh` CLI or ACP mode.** It would start its own host and sessions
  rather than drive the desktop's. Provider CLI control was removed in
  §3.6.2.
- **Treat plugin sessions as independent, like cloud sessions.** Sends
  would then work on a locked Mac without presence. The owner chose parity
  with Claude and Codex. The Locked Use transaction still unlocks the
  console, as it does for them, although the plugin does not need it.
- **Answer approvals only from Muster, or only when no window is open.**
  The first would hide them from the person at the Mac. The second would
  leave a phone unable to answer while the window is open.

## Consequences

Once the plugin is installed, a phone can prompt, interrupt, approve and
answer question cards in DeepSeek Harness sessions. It also sees whether a
turn runs or waits. The person at the Mac keeps the same prompts, and
whichever side answers first wins.

The plugin is code that the owner installs into the app's profile, and it
runs with the user's permissions inside the app process. A plugin that a
different DeepSeek Harness release breaks fails closed: requests time out
or are refused, and nothing is replayed.

Manual prerequisites:

1. Install the plugin:
   `dsh plugin --profile desktop add file:<checkout>/src/muster/deploy/deepseek-harness`.
   Then restart DeepSeek Harness.
2. For locked-Mac use, run
   `muster policy --allow-locked-control true --ax-allow com.deepseek.dsh`
   on that Mac.

Not offered: queued-message edits, model or effort choices, continued
questions (a question the tool already gave up waiting for), images and
attachments.

## Verification

- **Plugin.** `node --test test/*.test.js` runs 12 tests. They cover state,
  the prompt and cancel shapes, both answers winning the race, waiting for a
  present Muster when no window is open, failing closed when neither can
  answer, a cancelled tool, a frozen request, answer validation, a
  socket that cannot be served (no throw), registration as a fiber effect,
  and a socket round trip with 0600/0700 permissions. All pass on Node 26 and on
  the app's own Electron Node 24.21.
- **Rust.**
  - Unit tests cover snapshot parsing, the interaction mapping, peer-pid
    binding (a different serving pid gets nothing written), a lost reply
    (unknown) and a refusal (named).
  - `tests/deepseek.rs` drives a session end to end against a fake plugin on
    a real socket: attach, `waiting_approval`, approval, question card,
    prompt, interrupt, resolution, and app exit to `ended`.
  - `musterd/tests/deepseek.rs` covers the locked Mac: local policy refusal,
    then `LockedUseRequired`, with nothing ledgered. On an unlocked Mac
    without the plugin the send is refused with the hint, nothing is
    ledgered, and model choices are refused.
  - The profile and daemon capability tests are updated.
- **Real app host, isolated.** The plugin ran inside the installed DeepSeek
  Harness 0.2.1-rc.5. It was installed with
  `dsh plugin --profile <test> add file:…` into a throwaway profile whose
  `DSH_HOME` was a scratch directory, and booted as `dsh <test> --no-open`.
  The owner's `~/.dsh` was left unchanged.
  - DeepSeek Harness added the bundle to the profile itself and loaded it.
    The plugin served its socket at 0600 in a 0700 directory.
  - `hello` and `state` answered, with `agents` injected.
  - A prompt to an unknown session came back as the app's own refusal.
  - A prompt to a session that `dsh headless` had created was accepted. The
    app resumed the stored session and recorded the prompt as
    `user/message` with `source: {kind: user, rpcId: <request id>}`, then
    started a turn. That turn ended in error only because the scratch home
    held no API key.
  - Re-sending the same request id added no second message. `cancel` was
    accepted, and an unknown approval was refused by name.
- **Bug found and fixed.** That run first exceeded macOS's 104-byte socket
  path limit. The plugin's failed `listen` crashed the whole host. The
  plugin now refuses an over-long path, handles every socket error by
  logging it and leaving the bridge off, and registers the socket through
  `ctx.effect`. The effect is needed because Cordis runs a plugin's `apply`
  as a constructor and drops its return value. The default
  `~/.dsh/muster/bridge.sock` is far below the limit.
- **Not done:** installing the plugin into the owner's own DeepSeek Harness
  profile. Approvals and question cards were not raised by a real model,
  because that needs model credentials. A Locked Use transaction executing a
  plugin send on a device, an installed daemon built from this branch,
  signing and publication were also not done.
