# ADR 0051: Manage Multica containers and subscribe to session catalogs

- Date: 2026-10-26
- Status: Accepted
- Related: ADR 0009, ADR 0015, ADR 0049, ARCHITECTURE §3.6–3.7

## Context

The Chat workspace is being hidden. Multica has only a manually provisioned
Apple container and no settings entry or supervision. Its shared work directory
was placed directly in the home directory. Catalog RPC supports events, but the
UI repeatedly fetches snapshots; ordinary file writes and title-only changes
can fail to produce prompt catalog notifications.

## Decision

Keep all configuration, validation, container lifecycle and catalog subscriptions
in Shared Rust. Apple views render the projection and send intents. Hide Chat
from the workspace picker without deleting its conversation infrastructure.
Offer Multica installation/configuration in desktop Settings and show its tab
after Agents and Terminals only when Rust observes an installed container.
Default new data roots to ~/Documents/Multica and allow a user-selected absolute
path. Persist the CLI's configuration and workspaces in that root. Existing
containers retain their observed mount until explicitly changed; never silently
replace an existing container or erase its files. Management is local-owner only.
For an explicit directory change, require a stopped container, rename the data
root on the same filesystem, and retain a compatibility symlink at the original
mount path. Refuse occupied/nested targets and roll back a failed symlink. This
keeps task cwd paths valid on both host and guest without rebuilding a legacy
container or copying its credentials. Cross-volume relocation remains a manual
operation. The macOS tab embeds the configured site in WebKit with its own login;
this change does not add a Multica phone proxy or a remote installation UI.

The daemon supervises opted-in containers independently of the app. Restart only
the container/daemon, never a task or an unknown provider send. Installation uses
the operator's HTTPS installer URL, with no private endpoints or credentials
compiled into the program. Login remains an explicit terminal operation. Managed installs use the existing
ACP runtime command and the Rust ACP server on the private container gateway;
the bridge has a random token, bounded handshake and connection count, and the
host's private Unix socket is never exposed. Legacy adoption leaves its tools
intact and retains its bridge environment inside that container. A stopped
or unauthenticated service is reported separately from an installed container.
The web tab opens the configured HTTPS application URL; it does not imply phone
reachability or implement a new authenticated browser proxy.

Use bounded catalog-only watches over existing authorized RPC streams. Subscribe
before the initial snapshot; replace that device's catalog on every event.
Reconnect with backoff and a fresh snapshot. Cancel old device/filter generations
and suspended phone watches. Coalesce filesystem writes and compare display
metadata as well as file identity before broadcasting.

## Alternatives

Keeping three-second client polling misses the requested push behavior. Putting
container state or retry logic in Swift duplicates business semantics. Moving a
legacy container's files or recreating it automatically could lose its unmounted
credentials and tools, so legacy migration needs an explicit controlled path.
Desktop CLI creation is not treated as desktop-app creation: ADR 0009's
`orca.create` launches CLI providers and does not establish a native GUI session.

## Consequences

Apple container and operator installation information are prerequisites. The app
must display setup errors and login instructions. Catalog traffic remains subject
to existing provider scope filtering. Existing Chat data and controllers remain
available internally. Device/provider acceptance is recorded separately below.

## Verification

- 429 tests passed across agent/app-core/daemon unit suites, transcript service
  integration and the real socket app-core chat/catalog tests; seven existing
  manual diagnostics were ignored. The catalog test covers pushed rename/removal,
  zero snapshot polling on a live stream, background cancellation and foreground
  resubscription. The index-only rename test uses a 60-second fallback interval
  and observes the event within five seconds.
- Four Multica tests passed, including a fake executable exercising opt-in
  supervision, one restart of a stopped container, daemon recovery and no restart
  of an already running daemon; real temporary-directory migration preserves
  both the moved data and the old shared path. This is not a real container test.
- The separate real-link access test passed, including refusal of all Multica
  administration requests from a remote member. Strict Clippy passed for the
  affected Rust crates and tests. Swift protobuf bindings were regenerated and
  the macOS MusterUI target compiled.
- Built the isolated MusterDev.app locally with ad-hoc signing. The build emitted
  existing SDK/minimum-macOS linker warnings; only this host build is established.
  Developer ID signing, notarization, release publication and installation were
  not performed. No installed application was replaced.
- Read-only inspection of the existing deployment confirmed an Apple `container`
  instance with a same-path home-directory work mount and a running Multica daemon.
  No real container, credentials or data directory was changed. Actual installer,
  login, container restart, Documents access and migration acceptance remain manual.
- Historical native session creation is reviewed in
  [NATIVE_SESSION_CREATION_REVIEW.md](../NATIVE_SESSION_CREATION_REVIEW.md).
  No native provider session was created by this turn.

CLI syntax follows Apple's [container command reference](https://github.com/apple/container/blob/main/docs/command-reference.md).
Android/HarmonyOS only hide the Chat picker item here; they were not built.
iOS device build, physical-device push timing and WebKit site login remain unverified.

The desktop WebKit page is superseded by the native read-only workbench in
[ADR 0053](0053-multica-native-workbench-and-runtime-routing.md). Runtime execution
and container management are not changed by that follow-up.

### Data relocation supersession

ADR [0054](0054-native-multica-targeted-installation.md) supersedes the
compatibility-symlink decision. Settings adopts a verified offline migration
with no old path; it no longer moves data while retaining the original mount.
