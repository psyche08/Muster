# ADR 0154: Wait for slow Codex windows and install 1.1.27

- Date: 2026-11-06
- Status: Accepted; installation and acceptance pending
- Related: [ADR 0150](0150-remote-desktop-session-creation.md) (desktop
  session creation), [ADR 0152](0152-install-1-1-26-for-desktop-session-acceptance.md)
  (1.1.26 acceptance), [ADR 0153](0153-large-session-history-costs.md),
  [ADR 0144](0144-local-release-and-independent-installation.md),
  [ADR 0120](0120-locked-control-without-a-per-operation-signature.md)

## Context

ADR 0152 installed 1.1.26 on the operator's Mac and asked its daemon to create
one desktop session in each provider's app. Neither creation was submitted.

- **Codex.** The daemon rejected the request with "Native renderer is not ready
  for navigation". Codex creation opens a new ChatGPT window, then waits for
  that window's composer before it dispatches the directory route. It waited
  25 s.

  Three new windows on that Mac were measured from the menu press:
  - Each window's web area was loaded within 0.5 s.
  - One window, sampled every 5 s, stayed a skeleton of 11 to 18
    accessibility nodes until it rendered.
  - The full page, composer included, appeared after 60.6 s and 63.3 s. The
    third window still had no composer after 40 s.

  The ChatGPT app logged "Reached the post-login Statsig bootstrap deadline"
  63 s after a new window was shown, and rendered that window 5 s later. The
  feature-flag hosts answered a command-line probe within 3 s, so the stall is
  inside that app instance.

  The rejection submitted nothing, but it left the new blank window open.
- **Claude.** The daemon rejected the request with "Native composer is not
  empty". Claude's new-session form already held a short unsent draft, which
  the app keeps across navigation. This is the intended refusal (ADR 0150).
  The draft was left untouched, so this creation stays pending until the
  operator sends or clears it.

## Decision

- **Window startup.** Native creation waits up to 90 s for windows to show
  their renderer, instead of 25 s. This covers two waits:
  - activation, where every window must expose its composer;
  - a new Codex window before its route is dispatched.

  Both waits come before any navigation or typing. The guard still stops the
  attempt on physical input, a console change or a changed app process, so a
  longer wait adds no side effect.

  Nothing else changes:
  - the 25 s wait for the routed form;
  - the 15 s receipt wait;
  - draft preservation;
  - identity proofs;
  - the ledger.
- **Locked Macs.** No change. A Locked Use lease lasts at most 15 s (ADR
  0120). On a Mac whose new ChatGPT windows need 60 s, a locked Codex creation
  ends before navigation and is rejected. It is never replayed.
- **Release.** Set `MARKETING_VERSION: 1.1.27` and `CURRENT_PROJECT_VERSION:
  117`, and regenerate the Xcode project with xcodegen 2.46.0.

  Build, sign, notarize and install with the ADR 0152 process, from a clean
  detached checkout of the merge that lands this change. The build also carries
  ADR 0153, which is already on main.

  Install on the operator's Mac first. Do not publish.

## Alternatives

- **Restart the ChatGPT app.** That might clear this instance's stall, but it
  interrupts the operator's running Codex work. It also does not protect Muster
  from the next slow bootstrap.
- **Navigate an existing window instead of opening one.** That avoids the
  bootstrap, but the route goes to whichever window has focus, so the target
  window would be less certain than a window Muster opened itself. It needs its
  own identity proof and device evidence.
- **Use the app's new tab.** This version of the app has tabs. Proving that a
  new tab belongs to the request needs a different proof from a window, and
  that proof has not been recorded on a device.
- **Close the blank window after a rejection.** Unnecessary once the wait
  covers the measured startup. It also adds a write action to a path that
  submitted nothing.

## Consequences

- On a Mac with this stall, an unlocked Codex creation takes about a minute
  before the session exists. Clients have no RPC deadline, so they wait for
  the daemon's result instead of reporting an unknown outcome.
- A Codex creation that fails on a slower Mac now takes up to 90 s to be
  rejected.
- On such a Mac, Codex creation on a locked Mac still cannot finish. Claude
  creation and Codex creation on unaffected Macs are unchanged.
- An installed Mac runs a build newer than the published feed.

## Verification

Before landing:

- run `cargo clippy` with `-D warnings` and the `muster-platform` tests;
- run the ADR check and the public-content check;
- confirm the generated version fields.

After landing, record each of the following separately:

- source commit and build;
- signing, notarization and Gatekeeper;
- installation, identity and runtime health;
- local Codex creation, with its reply, ledger row, unique new session and
  first message;
- local Claude creation once the existing draft is gone;
- creation from another Mac and on a locked Mac.

No installation or acceptance result is claimed yet.
