# Implementation Status and Open Risks

Part of the [Muster architecture](../ARCHITECTURE.md). Section numbers are shared across all parts, so a reference such as §3.6.2 means the same section wherever it appears.

## 8. Implementation Status

### 8.1 Historical Snapshot (2026-09-25)

The following preserves that day's progress, including its not-yet-implemented/verified conclusions. Later updates are in §8.2; this is not the current overall status.

`src/muster/` contains every Rust crate listed in 3.9 and passes Linux tests:

- Networking: roster, pairing, gossip, service/forwarding streams, direct/Relay paths, revocation ≤ 2 seconds, vnet filtering (TUN requires root; test marked ignored).
- Services: Codex/Claude session viewing and change push; Owned controllers (codex app-server, Claude CLI stream-json); iTerm2 controller (editor readback, bracketed paste, screen approval prompts); iTerm2 API client (row deltas, screenshots, protected sends); VT grid; hardware-key SSH client using russh.
- `musterd`: UDS and `muster` service streams share one handler; remote calls use roster scopes and local policy, with Presence signatures for high-risk scopes; mutations with `op_id` use OperationLedger; `Envelope.target` lets local Apps reach other members through musterd.
- `muster-app-core` / `muster-ffi`: Intent → Effect / ViewModel runtime (embedded mobile links, desktop through musterd), nine C functions; session-worktree files, individual-file, git log/diff views; `SetPushToken` supplies platform push tokens to Relay; network changes/foreground restoration re-register Relay, reprobe paths, and restore session watching. Independent C ABI tests: `muster-ffi/tests/abi.rs`.
- `muster-agent`: Filesystem watching (notify) wakes transcript-directory polling; polling is fallback only, with subsequent rescans covering files in new directories.
- `muster-platform`: macOS lock state, input counter, code signatures, AX controller, SEP signer, and NetHelper XPC client are written but compiled only on macOS, **not yet device-verified**, corresponding to P0 items in section 9.

- `muster-relay`: Registration caps, per-device/source rate limits, mailbox limits, offline-device push hooks (`PUSH_REG` + `--push-command`), systemd/Caddy deployment files (`deploy/relay/`). Mailbox roster cold start: store on `Node` publication, reconcile after registration and every 10 minutes (`tests/mailbox.rs`).
- Documents: [PROTOCOL.md](../PROTOCOL.md) (wire formats), [SECURITY.md](../SECURITY.md) (threat model/invariants); CI in `.github/workflows/muster.yml`.
- Native shells (`src/native/`, see README): One Apple SwiftUI package built and run with Xcode 27 on macOS and iOS Simulator. Mac App creates networks, invites, and approves over musterd socket; iPhone Simulator pastes invitations, joins, directly connects to Mac through local Relay, lists/follows Claude/Codex sessions live, and views active iTerm2 sessions (draws/scales/scrolls the peer grid without resizing its window; core determines `TerminalView.resizable`). Simulator SEP device keys work; Presence keys, scanning, and push await real devices.
- macOS `musterd`: `cargo build -p musterd --features sep` uses `muster-platform/build.rs` and swiftc to build/static-link SepSign and Swift runtime; started locally and connected to iTerm2 API (cookies via osascript, handshake headers via tungstenite), listing/pushing real sessions. Files follow macOS conventions: daemon bundled in `Muster.app` (`Contents/MacOS/musterd` + `Contents/Library/LaunchAgents/dev.linsheng.musterd.plist`, registered through SMAppService on first launch), state in `~/Library/Application Support/Muster`, logs in `~/Library/Logs/Muster` (`musterd::paths`); Relay is configured/persisted in App Settings or `muster relay` (`DeviceSetRelay`).
- Relay deployment: `muster.linsheng.dev:9443` (UDP) and `wss://muster.linsheng.dev` (nginx to loopback WebSocket), `deploy/relay/install.sh`; verified registration by Mac and physical iPhone, followed by LAN direct connectivity. Rescanning enrolled devices has `pair::serve` resend the roster without approval. `MusterMacShim` SepSign static library; Android JNI shim + Kotlin wrapper + Keystore + Compose (not SDK-built); HarmonyOS NAPI shim + ArkTS wrapper + HUKS + ArkUI (not DevEco-built). Windows remains P4 per §5.

Not yet done: local storage encryption (D3), Ghostty / PTY fallback (Linux PTY exists), Windows shell, Codex desktop AX adapter, Android FCM registration (iOS APNs registration exists in the shell; HarmonyOS push unwired).

---

### 8.2 Three-Layer Architecture Source Review (2026-09-28)

| Target component | Current evidence | Remaining work |
|---|---|---|
| Agent Halo / Codex, Claude Desktop | New Rust tree has viewers, NativeApp profiles, and Locked Use orchestration; `muster-platform/src/macos/native_app.rs` defines two profiles | Accept Claude scenarios independently; Codex passing does not establish every operation passes |
| Agent Halo / Catdesk Desktop | 2026-10-02: `muster-agent::catdesk` reads the app's SQLite store read-only (ui-sdk and legacy conversations, stable `message_id`s, receipts from new user rows); `catdesk_app` reads its owner-only command socket for the GUI pid, live status, project paths and current session; `catdesk_ui` holds labels/routes; `muster-platform` has the `com.catpaw.cowork` / `BHWTW6L8X6` profile with selection-replacement drafting. Unit tests and a read-only parse of a real store copy pass; see [history record](../history/2026-10-02-catdesk-adaptation.md) | Install on Mac-B, allowlist `com.catpaw.cowork`, collect covered AX evidence (sidebar rows, composer settability), then iPhone view → send → observe → relock |
| Agent Halo / Locked Use | [LOCKED_USE.md](../LOCKED_USE.md) records mac-b + iPhone passing two consecutive user-authenticated sends, relock, uncover, and live message display | Independently verify cancellation, local-input interruption, disconnection, crashes, multiple-display failures |
| iTerm2 adapter | Rust API client exists in `muster-terminal`; daemon connects directly to local terminal | P3 device acceptance of output latency, locked behavior, and complete operation capabilities |
| Ghostty adapter | Control-only implementation in frozen `src/native/apple/AgentHaloTerminalAdapters` (removed 2026-10-02, §8.5); not integrated in new Rust tree | Migrate business logic/OS shim per §4 and complete P3; old implementation existence does not make it usable in the new architecture |
| Muster clients | [AGENT_INTERFACE.md](../AGENT_INTERFACE.md) records device tabs, full Agent list, collapsed tool activity, Markdown, turn usage, file previews | New-session file preview still needs iPhone tap acceptance; mobile devices appear only in network-settings roster |
| Networking + Hardware Security | New Rust tree includes identity, roster, networking, authorization, SEP implementation, shared by all integrations | Judge platform/link acceptance separately from existing records; this documentation update adds no network/hardware device verification |

This reviews source and recorded acceptance only; this update changes architecture documentation without implementing Catdesk, migrating Ghostty, or deploying a new version.

---

### 8.3 Dual Secure Transport Source Update (2026-09-28, Pending Deployment)

`muster-net::relay_tls`, `secure_relay`, and `muster-relay` implement TCP/TLS and UDP/QUIC endpoints, WebPKI, 3-second connect/register/first full-frame probe, health/RTT routing, ACTIVATE return-path assertions, and nonce-matched PROBE/PROBE_ACK. Source status does not establish installation; signed artifacts, public TCP/UDP handshakes, production-device switching, and renewal acceptance require separate records. See [NETWORK_DIAGNOSTICS.md](../NETWORK_DIAGNOSTICS.md).

### 8.4 Session Controller Consolidation (2026-09-28)

The active Rust tree removed Codex app-server, Claude CLI stream-json, iTerm2 Agent bridging, and related process configuration/test fixtures. Section 8.1's Owned description is historical and no longer applies. CLI session-creation requests and platform entries are removed; missing desktop AX controllers explicitly reject writes. Read-only transcripts, independent terminal/browser services, and the Locked Use authorization chain remain.


### 8.5 Legacy Tree Removal (2026-10-02)

The frozen implementation left the tree in one change, because nothing in the active build, CI, packaging or runtime read it:

- Go: `src/cmd`, `src/internal`, `src/test`, `src/go.mod`, `src/Makefile`, and a committed 10 MB `musterd` Go binary.
- Rust: `src/rust` (`agenthalo-*`, `muster-transport`, `terminal-core`, `terminal-adapter-fake`, the 499-function header).
- Apps: `src/native/apple/AgentHalo{MacApp,CoreClient,TerminalAdapters}`, `src/ios`, the AgentHalo Android project (`android/app`, `android/agenthalo-core`), the AgentHalo HarmonyOS project (`harmonyos/entry`, `tests`, `native-smoke`), and the Windows preview.
- Tooling and records: `src/deploy`, `src/spike`, `src/poc` (the P0 tools; see [P0-validation.md](../P0-validation.md)), and the AgentHalo UI specifications, now under `docs/history/design/`.

The one live dependency was moved, not deleted: the bundled root helper (`dev.linsheng.muster.helper`) was built by `MusterApp/project.yml` from `src/cmd/muster-net-helper`. It now lives in `src/native/apple/NetHelper`, keeping only its bundled configuration (musterd caller, `192.168.13.0/24`).

Deleting source stops nothing that is already installed. The Go relay `musterd.service` on the relay host and the manually installed `dev.linsheng.muster.nethelper` on the Macs keep running until retired separately ([relay deployment](../../src/muster/deploy/relay/README.md), `src/muster/deploy/desktop/retire-agenthalo.py`). Their sources, installers and configurations are recoverable with `git show acf23be7:<path>`; the paths in §2 and the appendix refer to that commit.

### 8.6 Shell Decisions Moved to Rust (2026-10-02)

An audit of the Apple, HarmonyOS and Android shells found decisions §3.7 assigns to Rust. The ViewModel now carries them; the Apple shell renders them all and the HarmonyOS shell those it has UI for (it has no cover or Mac setup): `DeviceActions` (cover, in-app SSH, access summary, edit access, remove), `ChatView.can_interrupt` / `can_choose_model` / `can_browse_files` / `provider_label`, `TerminalsView.can_create`, `DesktopSetupView.ready`, `DesktopChatsView.choices`, and the display strings `JoinView.message`, `MemberView.link` and `DeviceInfo.key_label`. Queued messages are listed only in `ChatView.queue`. Shells no longer pair `can_go_back` with their own root-screen lists, and pass any Ctrl-letter key to `muster-terminal`.

Schema leftovers left the protocol with reserved numbers: the never-emitted `Log` / `KeepAwake` / `Haptic` effects, the never-sent `Init`, `PresenceGranted` and `ChatDraft` intents (a presence answer is the `Authenticate` effect's result), `ViewModel.presence`, and three `FileView` fields copied from `DesktopSetupView`. `ChatView` also stopped repeating the transcript as a flat `messages` list beside `timeline`, which doubled every chat view's size; `message_count` carries the count. The Android shell, already stale (src/native/README.md), was brought up to this contract on 2026-10-04 (§8.9).

### 8.7 Code Organization and App Profiles (2026-10-02 – 2026-10-03)

Behavior-preserving restructuring after §8.5 and §8.6:

- `muster-app-core`'s 4,400-line `update.rs` became `update/`, one module per feature (network, operations, agents, desktop chats, files, terminals, shell). The dispatchers in `update/mod.rs` name every intent and message variant, so a new variant cannot fall through a wildcard.
- `musterd`'s `locked_use.rs` became `locked_use/`: the owner API (`mod.rs`), the actor's state machine (`actor.rs`) and its tests. The code moved without logic changes; [LOCKED_USE.md](../LOCKED_USE.md) stays the acceptance record, and a later change to the actor is accepted on devices again.
- CatDesk landed (PR #52; §8.2 row): store viewer, socket runtime, `muster-platform` profile.
- App profiles (§3.6.4, pending decision 6). Step 1: `muster_core::apps` is the one registry of each app's provider, labels, Chat mode, bundle id and signing team; signing requirements, Locked Use provider checks, the default AX allowlist and access labels read it. Step 2 started: `muster_agent::profile` holds controls as data, matched by one `Selector` over the live macOS element and over recorded trees: each app's agent-conversation composer, send, stop and approval options, and Chat mode's send, stop, new chat, dialog close and settings exit (ChatGPT only, which removed a provider branch). Chat mode matches exactly as before. In agent conversations, Stop and approvals are now named the way Chat mode names controls (title, else description; an empty title counts as none), and an interrupt presses whatever the running-state check reports as Stop. Before, a present but empty AXTitle hid the description, and an interrupt pressed only a control named exactly "Stop". CatDesk's Stop is its own two labels ("停止", "Stop"). The remaining provider branches then became profile data without changing behavior: the agent conversation's queue, question cards (labels and Claude's paged, renderer-refresh and button-choice flags), session list and drafting; Chat mode's page, conversation URL, mode switch and history rows. A probe over the recorded trees printed every Chat-mode reading for both apps before and after: identical.
- AX fixtures: idle agent conversations of the three apps were recorded on Mac-B with `muster debug ax-dump`, redacted by `scripts/redact-ax-dump.py` (it keeps only labels Muster's product code names) and stored in `muster-agent/tests/fixtures/ax`. Raw recordings hold conversation text and never enter the repository. Running Claude Code and Codex turns, a Claude Code question card and both apps' Chat mode followed on Mac-C ([AGENT_INTERFACE.md](../AGENT_INTERFACE.md#ax-recordings-on-mac-c-2026-10-03)). They showed that Claude's Stop button has an empty AXTitle, so through 1.0.32 a running Claude turn was never seen; the profiles' name rule (title, else description) fixes that. The redactor now also keeps vocabulary prefixes and replaces links outside the apps' own pages.

Deployment: 1.0.33 (`5614785c`, PRs #50–#57) is installed on Mac-B only and not published ([DESKTOP_UPDATES.md](../DESKTOP_UPDATES.md)); Mac-A and Mac-C stay on 1.0.31. CatDesk is not allowlisted on Mac-B, so its device acceptance (§8.2) has not started. 1.0.33 carries the step-2 selectors including the Stop change; a locked send and an interrupt through each app are their device acceptance, not yet done.

### 8.8 Module Map, Layer Check and the Profile DSL (2026-10-04)

Behavior-preserving restructuring of the upper layer, on Linux with the macOS code type-checked (`scripts/check-apple.sh`); no device acceptance changed:

- [MODULES.md](../MODULES.md) maps the six crate layers, the dependency rules, where each product state machine lives and which product knowledge is data. `scripts/check-layers.py` enforces the dependency map in CI, so the direction of a new edge is a decision recorded in both places.
- App profiles (§3.6.4) are now the TOML DSL of [APP_PROFILES.md](../APP_PROFILES.md): `muster-agent/profiles/{vocabulary,claude,codex,catdesk}.toml`, compiled by the crate's `build.rs` into the same `profile` statics the engine already read, plus the `App` aggregate and `APPS`. The generated data was compared against the previous hand-written statics; the fixture tests over the recorded AX trees pass unchanged, and new tests check the profiles against `muster_core::apps`, the capability tables and the lock names.
- The daemon no longer enumerates apps: `agent_bridge` builds provider queues, desktop locks (keyed by the profile's `lock`), the background scan and each app's resolver (the profile's `session_key` strategy) from `APPS`; muster-platform's `AppProfile` is the shared `App`. Adding an app is a registry entry, a TOML file and fixtures (MODULES.md §5).
- Capabilities (step 4): each profile declares what the app offers; the daemon rejects an undeclared agent operation before any queue, lock, unlock or ledger entry, and `ChatView` projects the declared set. All three apps declare what they did before, so no operation changed hands.
- `muster-agent`'s 2,200-line `desktop_chat.rs` became `desktop_chat/`, one submodule per interpreter (`tree`, `app`, `mode`, `history`, `pickers`, `conversation`, `transcript`) with `mod.rs` re-exporting them and holding the tests; every item kept its name and path.
- Step 3 of §3.6.4, same day: the Codex and Claude transcripts are read by one engine (`muster_agent::jsonl`) over the record grammar each profile declares (`[source]`: message envelope and roles, text and media blocks, block numbering, tool calls and results, turn start and end, bootstrap, skipped records) plus four named hooks per source (text cleaning, title records, usage, Claude's prompt queue). `receipt.rs` and `recency.rs` classify records through the same grammar. Message ids, kinds, texts, phases and states are the readers' previous ones; the one visible difference is that a Codex bootstrap prompt no longer names a session whose title is still a placeholder, which matches the head reader. The viewer, service, receipt, recency and question tests pass unchanged.

### 8.9 Mobile Shells Aligned with iOS (2026-10-04)

The HarmonyOS and Android shells were brought level with the iOS shell against the current `app.proto`; neither was compiled or run (the environment had no DevEco Studio or Android SDK), and each README's verification status says so.

- HarmonyOS (`src/native/harmonyos/muster`) gained the Chrome extension screen (`Screen.BROWSER`, from a device's details and from Settings on a device with services), Cover / Uncover on a device's details while `DeviceActions.can_cover` holds, the composer draft riding along with every intent (`ComposerAction`, restored from `ChatView.draft`, cleared only for the revision a send receipt names), the unconfirmed-send card (`ChatView.unconfirmed`), a terminal Paste control (`TerminalPaste` while `TerminalView.can_paste`), and answers to `ActivateNetwork` (one TCP connect to the core's endpoint), `LocalAuth` (the system user-authentication prompt) and `Cover` (refused on a phone), which it had refused as unknown. Names were checked against the generated `muster.d.ts`; the host tests pass.
- Android (`src/native/android/muster`) was rewritten screen by screen after the iOS shell: the workspace switcher and device tabs, the chat timeline (Markdown, Working groups, tool groups with paired output, turn changes, usage footers), the composer with attachments, drafts and the unconfirmed-send card, interaction cards with questions, queued messages, the Locked Use status, desktop chats and their options sheet, the browser screen, device actions, the resizable / fixed terminal grid with paste, diff previews, the relay editor, and the three effect answers above; the removed `Log` / `KeepAwake` / `Haptic` effects and `PresenceGranted` intent are gone. Every getter and DSL builder it uses was checked against the Kotlin and Java `protoc` generates for the schema.
- Neither shell ports the iOS performance monitor (the only `UploadDiagnostics` sender).

### 8.10 Half-Fold Layouts and the Muster Keyboard on iOS and Android (2026-10-04)

The two half-fold workspaces and the in-app keyboard the HarmonyOS shell already had (`src/native/harmonyos/muster/README.md`, "Keyboard" and "Half-fold workspace") were ported to the iOS and Android shells, so every mobile shell offers Top and bottom, Side by side, Automatic and Single pane, and can type without the system keyboard. Both ports keep the HarmonyOS structure file for file, and all of it is display state: no intent, effect or ViewModel field changed, and `app.proto` is untouched.

- Shared logic, pure on each platform and covered by ports of the HarmonyOS host tests (32 each): the workspace geometry (`FlexPreference`, `WorkspaceArea`, `FoldGeometry`, `workspaceLayout`: stacked above a horizontal crease or an equal split, side by side across a vertical crease plus 8 units on each side or at the middle, 280 units per pane, 56 / 220 for the controls), the pinyin engine (syllable segmentation, frequency-ranked whole words, sentence composition, partial words, learned picks, 80 candidates), the MacBook layout data (14.5 units per row, the function row, Chinese punctuation) and the UTF-16 caret arithmetic (character, word, line, line ends). Swift: `MusterCore/Display/WorkspaceLayout.swift`, `MusterCore/Input/{PinyinEngine,MacLayout,TextEdits}.swift`, `swift test` on Linux. Kotlin: `app/.../display/WorkspaceLayout.kt`, `app/.../input/{PinyinEngine,MacLayout,TextEdits}.kt`, JUnit 4. The 55,320-word AOSP PinyinIME dictionary ships as a package resource on Apple and an asset on Android.
- iOS (`MusterUI/Display/FoldDisplay.swift`, `MusterUI/Keyboard/{KeyboardPrefs,MusterKeyboard,KeyboardFields}.swift`): the preference lives in `UserDefaults`, `MusterRootView` resolves the split for the window and hands it down as an environment value; the chat and terminal screens stack above the hinge with a separately scrolling lower pane, and Side by side keeps the workspace list beside the open row. The keyboard is a SwiftUI view inside a `UIInputView`, attached as the input view of UIKit-backed fields (`MusterTextField`, `MusterTextEditor`, the composer's editor) and of the terminal's key catcher; arrows and modifiers edit through `UITextInput`, terminal keys go to the core by name. iOS exposes no fold geometry, so Automatic is one pane. In text fields it is off by default (Settings → Keyboard) until verified on a device; in a terminal it is on by default, with its own switch (`terminalSystemKeyboard`, the same on all three shells), since the system keyboard has no Esc, F-keys, Ctrl or Alt. The UI parts are not yet compiled (no Xcode here).
- Android (`app/.../display/FoldDisplay.kt`, `app/.../input/{Keyboard,KeyboardFields}.kt`): the preference lives in `muster-display` SharedPreferences; Automatic follows Jetpack WindowManager's half-opened `FoldingFeature` (`androidx.window:window:1.3.0`); `MusterRoot` resolves the layout and provides it as `LocalWorkspaceLayout`, draws the keyboard as the scaffold's bottom bar for whichever field or terminal registered a `KeyboardTarget`, and the IME is suppressed with `InterceptPlatformTextInput` so a field and the keyboard never both show. The whole `app` and `core` Kotlin was compiled on Linux with kotlinc 2.1.0 and the Compose compiler plugin against the declared BOM's jars (no errors or warnings); that check also caught nine `ListItem` calls from §8.9 that passed the modifier positionally, now fixed. Not built with Gradle and never run.
- HarmonyOS gained only the terminal's own keyboard switch, on by default as its one switch already was; not compiled or run.

### 8.11 Agents Cloud Mode (2026-10-04)

The agent service gained the providers' cloud sessions (§3.6.2 revision, decision D14): Claude Code on the web and Codex cloud tasks are listed under the Mac beside its local sessions, followed, and (Claude) written to, through the APIs the `claude` and `codex` CLIs speak, with the credentials those CLIs keep on the Mac. No protobuf field changed: a cloud session is `mode = "cloud"` with `controller = "cloud"`, shows its repository or environment label where a local session shows its directory, and carries the profile's label (`Claude Cloud`, `Codex Cloud`) as its provider label, so the three shells show it without a change.

- The profile DSL gained `[cloud]` (APP_PROFILES.md): the strategy (`claude-code`, `codex-tasks`), origin, label, poll cadence, where the CLI keeps its credentials (file paths, the token's expiry, the account id, the macOS Keychain item) and what a cloud session offers (`send`). `build.rs` validates it into `profile::Cloud`; `claude.toml` and `codex.toml` declare theirs.
- `muster_agent::cloud` is the engine's two strategies over an `Http` trait the daemon implements (`musterd::cloud_http`: reqwest, WebPKI roots, no redirects) and tests fake. Claude: the session list (sessions bridged from a machine, archived and pooled ones left out), its events read incrementally by sequence number through the one JSONL engine, whose record reader was separated from the file tail for that (`jsonl::Reader`), and a prompt posted as a user record. Codex: the task list and the task details (prompt, replies, the latest diff as a `diff` tool pair, a failed turn's error), re-read whole and replaced when changed. Credentials are read per request and never refreshed: an expired or refused token reads as signed out, logged once, and the listing stays empty until the CLI signs in again.
- `AgentService` keeps cloud sessions in their own catalog, lists each source at the profile's cadence while watched (a quarter as often otherwise), reads the sessions someone opened, sends through the source and refuses interrupts, answers, question cards and files for them; `musterd::handler` routes a cloud session's operations past the desktop locks, Accessibility and Locked Use to the source, still under the roster's `agents:<provider>` scope, the profile's cloud capabilities and the idempotency ledger; `muster-app-core` projects the cloud label and withholds `can_interrupt` and `can_browse_files`. `musterd --no-cloud` (`DaemonConfig::cloud`) keeps a device to local sessions; phones, which embed the daemon without the agent service, never speak to the clouds.
- Verified on Linux: the profile, cloud, handler and render unit tests and a service test over a fake transport (list, read, send, refusals), with `cargo fmt`, `check-layers.py`, clippy and the workspace tests. The request and reply shapes were taken from the open-source Codex CLI (`codex-rs/cloud-tasks-client`, `backend-client`) and from the calls the Claude Code CLI makes; neither API was called from here (no credentials, no device), so the first run against the live services is the next acceptance step.

### 8.12 Multica Management and Runtime Bridge (2026-11-01)

Documentation and read-only-projection update; no device or end-to-end
acceptance was performed in this change.

- Shared Rust owns the whole Multica management state machine: `musterd::multica`
  (install/adopt, config validation, keep-alive supervision, offline data-root
  acceptance, stopped/installed/unauthenticated classification),
  `musterd::multica::installation` (guided stages, bounded/redacted output,
  single-install exclusivity), `musterd::multica::bridge` (private gateway
  listener, `MUSTER_NATIVE_V1` token handshake, task-scoped session key),
  `muster-acp::native_host` / `scheduler` (provider protocol and task-credential
  lifetime, destination choice in `muster-agent::native_runtime`), and
  `muster-app-core::update::multica` (install progress polling, reconnect
  recovery, no replay). Apple shells render the status projection and keep only
  the launchd lifecycle.
- Additive ABI: `MulticaStatus.version` (guest `multica --version`, empty when
  unknown) and `MulticaStatus.migration_required` (installed container's bind
  mounts no longer match the saved root; saving a changed root stays refused
  until the attended offline migration is done). No administration action was
  added, so the local-owner ABI still has CHECK/SAVE/INSTALL/PROGRESS only.
- Bridge fixes in this change: a runtime-bridge session key requires
  `MULTICA_TASK_ID` (a connection without one is rejected before launch);
  daemon startup revokes stale task launchers; a running container's bridge and
  provider wrappers are reconciled with keep-alive off, without starting
  stopped services.
- Boundaries: there is no in-app Multica upgrade, image pull, stop or rollback.
  The manual upgrade, login and data-root migration runbooks are in
  [DESKTOP_UPDATES.md](../DESKTOP_UPDATES.md) and
  `src/muster/deploy/multica/README.md`. ADR 0009 remains Proposed/historical;
  ADR 0053's execution status statement is superseded by ADR 0064/0077; ADR
  0060, 0064 and 0072 keep their own pending acceptance.
- Verification: the change's tests cover the bridge handshake, task-scoped
  session key and wrapper provisioning, guided-install stages/output bounds and
  timeout, app-core busy/reconnect behavior, and Claude scheduler terminal
  frames; they use fake executables, temporary directories, duplex streams and
  a fake scheduler host. A real container, installer, terminal login,
  reboot/app-upgrade recovery, real provider turn and real scheduler task remain
  unverified. The device records in ADR 0052/0054/0060/0064/0072 are historical
  and were not reproduced.

## 9. Risks and Open Validation Questions (P0 Must Answer)

**Locked Use update (2026-09-28)**: Acceptance records are in [LOCKED_USE.md](../LOCKED_USE.md).
Codex passed two normal-path consecutive sends and message display; all-display failures, local input, cancellation,
disconnection, and crashes still require independent verification. One successful path does not establish all P0 gates passed.

1. **NativeApp temporary-unlock chain**: Validate system-plugin authorization, actual unlock, all-display cover, trusted terminal state, and relock per D11, plus local input/crash/disconnect/late receipts. Missing links reject this path without CLI fallback; accept independent terminal channels separately.
2. **LaunchAgent TCC ownership**: `musterd` needs Accessibility and Automation (iTerm2 cookie, Ghostty). Confirm permissions bind to its code identity and survive upgrades after one approval through stable signing identity.
3. **iTerm2 API in `musterd`**: Can LaunchAgent cookie requests display authorization? Do screen-update notifications arrive while locked? What coalescing/fetch throttling handles high-frequency output?
4. **SEP access in LaunchAgent**: Reuse CryptoKit blobs in the in-process static library; verify locked signing (Device Key must not require unlock ACL; Presence Key should).
5. **vnet performance and MTU**: QUIC DATAGRAM carries 1150-byte inner packets at any working path MTU; confirm acceptable throughput and Relay VNC bandwidth/latency with small outer MTUs (PPPoE, proxy tunnels).
6. **Coexistence with local proxies**: macOS Surge/Clash utun versus vnet route priority; reuse `physdial` physical-interface binding and `doh` Fake-IP bypass experience. Phones no longer have this issue because there is no VPN.
7. **`russh` interoperability with macOS sshd**: `ecdsa-sha2-nistp256` public-key authentication with hardware signing callbacks; first-trust host-fingerprint UI; long-session keepalives and reconnection after forwarding-stream loss.
8. **iOS backgrounding**: Service links/forwarding streams disconnect in background, interrupting SSH; approval notifications require APNs. Relay may send content-free notifications by DeviceId, as post-P2 work.
9. **Catdesk and Ghostty integration**: Catdesk reading, unique location, and AX profile exist in source but have no device acceptance (its window is not exposed to AX while locked, and its Lexical composer needs selection replacement rather than `AXValue`); Ghostty awaits migration, with no promise of output reading or locked operation. Verify each application's signature identity, post-upgrade control/interface changes, and unsupported-capability rejection independently.
10. **Rust sockets and tokio on HarmonyOS**: Validate with the OpenHarmony NDK. If sockets are unavailable, HarmonyOS is the only platform that must retain raw socket callbacks.
