# ADR 0227: musterd runs from a system LaunchAgent, the app starts it before its main page, and a restart restarts every Muster process

- Date: 2026-11-17
- Status: Accepted; installed and verified on the owner's Mac, except the next login, an update, and the restart button itself
- Related: [ADR 0021](0021-desktop-startup-retry.md) (Shared Rust owns desktop startup; extended), [ADR 0124](0124-prompt-for-screen-recording-and-move-terminal-authorization.md) (its startup prompt for Screen Recording is superseded here), [ADR 0125](0125-screen-recording-is-the-bundles-and-only-a-restart-takes-it.md), [ADR 0128](0128-the-panel-helper-keeps-its-own-identity.md), [ADR 0164](0164-the-daemons-main-thread-serves-the-window-server.md), `docs/architecture/runtime.md` §3.3

## Context

The owner asked for four changes to how the Mac app starts and restarts:

1. musterd should be a LaunchAgent in `/Library/LaunchAgents`.
2. When musterd is not running as the app opens, the app should say so, and
   start it on the owner's word. The main page should open only once it has
   started.
3. With Accessibility already granted, the permission page should not
   appear again.
4. The page's "重启 Muster" should restart every Muster process, not just
   the app and musterd.

What was there before:

- **Registration.** The app registered musterd as an `SMAppService.agent`.
  The plist sat in `Contents/Library/LaunchAgents` and pointed at
  `BundleProgram Contents/MacOS/musterd`. launchd listed the job as
  `path = (submitted by smd…)`, `managed_by = com.apple.xpc.ServiceManagement`.
  On every launch the app silently re-registered the job and waited for the
  socket (ADR 0021). A DerivedData build did the same from its own bundle.
  Nothing asked the owner: when registration failed, the app showed "Muster
  could not start".
- **The permission page** opened by itself whenever any of these held:
  - Accessibility was missing;
  - Screen Recording was missing (ADR 0124);
  - the Locked Use plugin needed installing;
  - a restart was pending.

  So a Mac with Accessibility granted kept seeing it. Its
  `RequestAccessibility` also opened System Settings unconditionally.
- **The relauncher** waited for the app's PID. It then sent musterd SIGTERM
  through launchd, kickstarted it, and opened the app. Any Muster process
  outside musterd's launchd job kept running the old program. That includes
  `muster-browser-host`, which Chrome starts, and a helper musterd left
  behind.

## Decision

### 1. The system LaunchAgent

- **The file.** launchd runs musterd from
  `/Library/LaunchAgents/dev.linsheng.musterd.plist`, root:wheel 0644. It is
  a byte-for-byte copy of `Contents/Resources/SystemLaunchAgent/dev.linsheng.musterd.plist`
  (source `Apps/macOS/dev.linsheng.musterd.system.plist`). The keys:
  - `Label dev.linsheng.musterd`.
  - `Program` and `ProgramArguments`, with the absolute path
    `/Applications/Muster.app/Contents/MacOS/musterd --log`.
  - `RunAtLoad`, `KeepAlive`, `ProcessType Interactive`.
  - `LimitLoadToSessionType Aqua`.
  - `AssociatedBundleIdentifiers dev.linsheng.muster`.

  The executable path is the one SMAppService used, so the daemon keeps the
  bundle's TCC identity (ADR 0125, ADR 0128) and ADR 0164's main-thread
  event queue.
- **The one root step is writing that file.** The app runs it through
  macOS's own administrator prompt:
  `/usr/bin/osascript … do shell script "/bin/mkdir -p /Library/LaunchAgents && /usr/bin/install -o root -g wheel -m 0644 " & quoted form of item 1 of argv & … with administrator privileges`.
  - Both paths travel as `argv`, never inside the script source.
  - A cancelled prompt (`-128`) is reported as cancelled, and nothing else
    happens.
  - The app writes nothing as root besides that file. Bootstrapping,
    kickstarting and booting out the job are the console user's own
    operations in `gui/<uid>`.
- **Who decides.** Shared Rust (`desktop_startup`) decides when each step
  runs. `DaemonAgent` in MusterUI is the OS adapter behind five callback
  actions:

  | Action | What the adapter does |
  | --- | --- |
  | Inspect | Reports facts and changes nothing. The agent file is missing, current (same bytes, owner root, no group or world write), or outdated. The job is absent, stopped or running. Its origin comes from the `path =` line of `launchctl print`: the system plist, or anything else. |
  | Install | The password step above. |
  | Register | No job: unregister an older installation's SMAppService agent if it is still registered (`install-mac.sh` boots its job out, so the registration can outlive the job, and both would claim the label at the next login), then `bootstrap`. Stopped system job: `kickstart` without `-k`. Running system job: left alone. Job from another origin: unregister the old SMAppService agent, `bootout` whatever remains, wait for it to unload, then `bootstrap`. |
  | Reload | `bootout`, wait, `bootstrap`. Runs only after an Install changed the definition of a loaded job. |
  | RecoverStopped | Starts a loaded job that is not running. |

  If `bootstrap` fails while `SMAppService.statusForLegacyPlist` reports
  `requiresApproval`, the adapter reports that the owner must allow Muster
  under Login Items.
- **Upgrades.** An app update with an unchanged template needs no password:
  the file is still current and the job is reloaded as the user.
  - `DesktopUpdateLifecycle.restoreService` bootstraps the system plist when
    it exists. Otherwise it falls back to the old `launchctl submit`.
  - `install-mac.sh` never writes `/Library/LaunchAgents`. When the file
    matches the new bundle, it bootstraps it as the user. Otherwise it
    prints the `sudo install …` command and says the app will ask.
  - **Changing the template makes every installed Mac ask for the password
    once more.**
- **The old registration.** The SMAppService plist stays in the bundle for
  one reason: so the app can unregister an older installation's job. The app
  never registers it again.
- **Which bundle is managed.** Only the app at `/Applications/Muster.app`,
  without `MUSTER_DIR`, is managed (`MacInstall.isInstalled`). A DerivedData
  build no longer touches launchd. It waits for a daemon started by hand.

Which steps need the administrator's password:

| Step | Password |
| --- | --- |
| First install on a Mac (file missing) | yes, once |
| Moving an SMAppService installation to the system LaunchAgent | yes, once (the file is new) |
| Starting a stopped musterd, restarting, an update with an unchanged template | no |
| An update whose template changed | yes, once |
| Removing Muster (`sudo rm /Library/LaunchAgents/dev.linsheng.musterd.plist`) | yes |

### 2. The app starts musterd before its main page

- **The decision is in Rust.** A new C ABI call,
  `muster_desktop_check(socket, managed, resumed, action)`, runs Inspect
  only and returns what to show:
  - `Ready`: musterd answers, the agent file is current, and the job is the
    system one or cannot be attributed (a daemon run by hand). Opens the
    main page.
  - `Start{needs_admin, automatic}`: musterd does not answer. If launchd
    lists the job as running, it is given five seconds to come up first.
  - `Migrate{needs_admin}`: musterd answers, but from another origin, or
    with an outdated file.
- **The shared coordinator.** `muster_desktop_prepare` runs the start:
  - It installs the file if needed.
  - It runs Register, or Reload when the definition changed under a loaded
    system job.
  - It waits for readiness, then makes one RecoverStopped attempt.

  The check and prepare share one coordinator. A second call while one is
  in flight is refused rather than run twice.
- **`automatic`.** This holds when Muster itself relaunched the app
  (`--muster-restarted` from the relauncher, `--muster-updated` from
  Velopack) and no password is needed. The daemon it stopped a moment ago
  then starts without a question.
- **The page.** MusterModel holds the answer as `gate`, and
  `DesktopGateView` words it:
  - **Start:** "启动 musterd". When the password is needed, the button reads
    "安装并启动…" and the page says that `/Library/LaunchAgents` needs it.
  - **Migrate:** "迁移…", plus "暂不迁移", which opens the main page with
    the old job still running. The next launch asks again.
  - **Development build:** "重试".

  A failed start stays on the page with its reason. "Muster could not
  start" remains for errors before any answer.

### 3. The permission page

- **When it opens by itself.** `render::desktop_setup` opens the page only
  when one of these holds:
  - `device.accessibility == "missing"`;
  - this session saw Accessibility go from missing to granted
    (`desktop_accessibility_granted`). The page then stays, or comes back,
    once to offer the restart.

  It never opens by itself on any later launch once Accessibility is
  granted.
- **Screen Recording and the Locked Use plugin** are still listed, and
  their restart and install offered, in the same page. That page opens from
  Settings ("完成 Mac 授权与安装…"). They no longer open it by themselves.
  This supersedes ADR 0124's "a Mac missing only Screen Recording now
  prompts at launch". Its restart rule (ADR 0125) stands.
- **The request.** `RequestAccessibility` does nothing when the daemon
  already reports Accessibility as granted. It neither opens System
  Settings nor sets the busy flag.

### 4. A restart restarts every Muster process

- **The snapshot.** Before the app quits, `DesktopRestart` lists every
  process of this uid whose kernel path (`proc_pidpath`) is under
  `/Applications/Muster.app/Contents/`. That covers musterd, `muster-panel`
  with the Locked Use watchdog it hosts, `muster-browser-host`, and
  `l4forwarder`. Three are left out:
  - the app itself;
  - the command line `Helpers/muster`, which runs the owner's own commands;
  - the root `muster-helper`, which belongs to root, has no TCC grant a
    restart could renew, and would need the password.
- **The relauncher.** The list goes to a detached `/bin/sh` relauncher as
  separate `pid:name` arguments. It:
  1. waits up to 60 s for the app to exit;
  2. sends `launchctl kill SIGTERM` to musterd's job;
  3. sends SIGTERM to each listed PID that still runs a program of that
     name, so a reused PID is spared;
  4. waits up to 10 s, then sends SIGKILL to whatever is left;
  5. runs `kickstart` without `-k`;
  6. opens the app with `--args --muster-restarted`.

  Chrome reconnects to the browser host with backoff. Rust's
  `can_restart_desktop` still refuses a restart during Locked Use, a locked
  screen, a send, or an open terminal.

## Alternatives

- **Keep SMAppService.** It needs no password, but it was not what was asked
  for. Its job is bound to the bundle's Login Items entry, and the owner
  wants a plain LaunchAgent they can see and manage.
- **Let the root helper write the file.** The helper is approved once, so
  there would be no password. It was rejected because it adds a "write a
  launchd plist" method to a root XPC service: any caller that passes its
  code-signing check could install a LaunchAgent for every user. The request
  was also explicit that the root step is the owner's password.
- **A LaunchAgent in `~/Library/LaunchAgents`.** It needs no root, but it is
  not what was asked for, and it applies to one user only.
- **Have the installer script write it with `sudo`.** That works only for
  installs through the script, not for Velopack updates or a first launch
  from a copied app. The script prints the command instead, and the app asks
  for the password itself.
- **Decide the gate in Swift.** That was rejected by the repository rules:
  startup policy stays in Shared Rust, and the app only reports facts and
  runs OS operations.
- **Kill helpers by name (`pkill -f Muster.app`).** musterd's argv is
  relative under SMAppService, so name matching misses it, and a name match
  can hit another user's processes. A PID snapshot with a name re-check
  touches only processes running this bundle's programs.

## Consequences

- **Every console user** on the Mac gets a musterd at login. Each runs from
  its own `~/Library/Application Support/Muster`. Before this, only users
  who had opened the app had one.
- **Background Task Management** attributes the agent to Muster through
  `AssociatedBundleIdentifiers`. The owner may see "Muster added items that
  can run in the background" once. If the item is turned off under Login
  Items, launchd will not load it, and the start page says where to allow
  it.
- **The password dialog** belongs to `osascript`. Its custom prompt text
  says what is written.
- **Development builds** no longer self-register. Run musterd by hand, or
  use the installed copy's daemon on the shared socket.
- **Grants after the move.** Accessibility and Screen Recording carried
  over to the system job on the owner's Mac. The executable path and
  signature are unchanged; only the job that runs it changed.
- **One Login Items record.** Background Task Management keys the record by
  label, so the system agent reuses the record that SMAppService made for
  `dev.linsheng.musterd`. Whether that lets smd load the bundle's old
  definition at the next login, ahead of the system agent, is not yet
  observed. If it does, the app sees a job of another origin and offers the
  move again.
- **A forced kill.** A helper that ignores SIGTERM for 10 s is killed. For
  the Locked Use watchdog this can leave displays dimmed (ADR 0222 §6), but
  a restart is refused while Locked Use runs.

## Verification

Source and tests, on this Mac (2026-11-17):

- **Rust** (`src/muster`):
  - `cargo test -p muster-app-core`: 195 lib tests plus the integration
    suites passed. That includes 11 `desktop_startup` tests (facts
    round-trip, stable codes, healthy job, stopped or resumed job, daemon
    still starting, migration and reload ordering, cancelled password,
    recovery, admission). It also includes the page rules
    (`a_granted_accessibility_never_brings_the_page_back`,
    `only_an_accessibility_grant_reopens_the_setup_page`) and
    `a_granted_accessibility_is_never_asked_for_again`.
  - `cargo test -p muster-ffi`: 4 ABI tests passed, including
    `desktop_check_reports_what_to_show_and_never_acts`.
  - `cargo clippy --workspace --all-targets -- -D warnings`: clean.
  - `cargo fmt --check`: clean for the changed crates. `muster-agent`'s
    `questions.rs` already differs on `origin/main` and was not touched.
- **Swift** (`MUSTER_FFI_LIB=…/debug/libmuster_ffi.a swift test`): 122 tests
  passed. That includes:
  - `DesktopStartupTests`: the gate asks before starting, a failed start
    stays on the page, a resumed start runs without asking, a move can be
    put off.
  - `DesktopLaunchAgentTests`: C ABI codes, `launchctl print` origin
    parsing against the real SMAppService output, agent-file ownership
    rules, and the process filter.
  - The relauncher against stand-in `launchctl` and `open`, with live
    processes. A helper is stopped with SIGTERM, one that ignores SIGTERM
    is killed, a PID now running another program is spared, and the app
    reopens with `--muster-restarted`.
- `xcodegen generate` regenerated `Muster.xcodeproj` from `project.yml`.

On the owner's Mac (2026-11-17), installed from this branch:

- **Build.** The release Rust binaries, then `xcodebuild` Release signed
  with the Developer ID certificate that the provisioning profile names,
  then the steps of `install-mac.sh` after its build, unchanged. The
  certificate was passed by hash: two certificates in the keychain share
  the name, and with the name `install-mac.sh` looks up, Xcode picked the
  one the profile does not include. That lookup predates this change. The
  designated requirement matches the previous installation.
- **Before.** musterd ran from SMAppService (`path = (submitted by
  smd.114)`), nothing was in `/Library/LaunchAgents`, and `device_info`
  reported Accessibility and Screen Recording granted and Locked Use ready.
  `install-mac.sh` booted the job out and printed the `sudo install` line.
- **First launch.** The app log shows
  `facts=Facts { agent: Missing, job: Absent, origin: None }
  startup=Start { needs_admin: true, automatic: false }`. The start page
  appeared; the owner chose 安装并启动… and entered the password in the
  macOS administrator prompt. `install`, `Register` and `ready` followed
  within four seconds.
- **The file and the job.** `/Library/LaunchAgents/dev.linsheng.musterd.plist`
  is root:wheel 0644 and identical to the bundled template.
  `launchctl print` shows `path = /Library/LaunchAgents/dev.linsheng.musterd.plist`,
  `type = LaunchAgent`, the absolute program path, running.
- **The old registration.** The unified log shows the app's SMAppService
  `Unregister … error: 0` and smd's bootout completing, then launchd
  spawning musterd from the system plist. Background Task Management then
  re-enabled the same Login Items record for the system agent, and smd
  tried to load the bundle's definition, skipping it because the job was
  already running. See Consequences.
- **Grants.** After the move, `device_info` reports Accessibility and Screen
  Recording granted and Locked Use ready. The app opened on its main page;
  the permission page did not appear.
- **Restart.** The button appears only after a grant in the current
  session. Every grant was already in place, and Muster changes no privacy
  setting, so the button was not pressed. The relauncher text of
  `DesktopRestart.script` was run instead, with the targets the adapter's
  filter yields (musterd and `muster-browser-host`), and the app was quit
  from its menu.
  - The relauncher exited 0 after 11 s.
  - musterd, `muster-browser-host` and the app all have new PIDs. Chrome
    started the browser host again.
  - The app reopened with `--muster-restarted`. Its check reported
    `agent: Current, job: Running, origin: System`, so it opened the main
    page directly.
  - `muster-panel` and `l4forwarder` were not running, so their restart was
    not observed.
  - musterd logged "musterd stopping" on SIGTERM but did not exit within
    10 s. launchd reports `last terminating signal = Killed: 9`. Every stop
    in `musterd.log` since 2026-11-16 took about 10 s, so this predates the
    change; the SIGKILL step is what lets the restart finish.
  - Run by hand, the restart skipped the app's own check for work in
    flight. A Muster dispatch that was waiting for the Claude app to leave
    the foreground was interrupted and recorded as rejected before anything
    was submitted. The button runs that check first.

Not observed yet:

- The next login: whether the system agent or the bundle's old definition
  loads first (see Consequences).
- An update with an unchanged template, completing without a password.
- The restart button itself, and the restart of `muster-panel` and
  `l4forwarder`.
