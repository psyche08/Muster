# DeepSeek Harness bridge plugin

`muster-dsh-bridge` lets the Muster daemon on the same Mac drive DeepSeek
Harness sessions without Accessibility. A phone can then send prompts,
interrupt a turn, answer approvals and question cards, and see whether a
turn is running or waiting. The decision is
[ADR 0074](../../../../docs/adr/0074-deepseek-harness-plugin-control.md).

It is a DeepSeek Harness bundle: a Cordis plugin that the app's desktop host
loads in-process. It decides nothing. It adapts the app's own Host services
on one owner-only socket, `$DSH_HOME/muster/bridge.sock` (by default under
`~/.dsh`):

| Request | What it does in the app |
|---|---|
| `hello`, `state` | Its protocol, its pid, the live sessions (running or idle), and the approvals and question cards waiting for a person |
| `prompt` | `sessionController.prompt`, the path the app's own window uses; the request id becomes the message's `rpcId` |
| `cancel` | `sessionController.cancel`: stops the running turn and keeps queued prompts |
| `approve`, `answer` | Answers a waiting approval or question card |
| `prompt.lookup` | Reads an existing inbox/event receipt by session and native request ID, without resuming or sending |

The desktop plugin also offers `task.create.v1`, `task.inspect.v1` and
`task.resume.v1` to the local Multica workbench. On the unlocked host Mac,
open an issue, choose **在 DSH App 新建任务会话** and select its working
directory. Choose **在 DSH App 恢复原会话** to reopen that same native ID,
including after a restart. A missing App, outdated plugin or missing original
is an error. Neither action sends a prompt or launches a CLI. To enable the
actions, install/update this plugin in the desktop profile as described below.

Muster's native creation (`agent_native_create` with provider `deepseek`)
uses `task.create.v1` too, in a session named from its ledger operation, and
then sends its first message as a `prompt`
([ADR 0167](../../../../docs/adr/0167-deepseek-harness-native-session-creation.md)).

Approvals and question cards still appear in the app's window. Whichever
side answers first wins, and an answer from Muster withdraws the window's
prompt. With no window open, the plugin waits for a Muster daemon that has
been in touch in the last 10 seconds. Otherwise the request fails closed,
exactly as it would without the plugin.

## Install

Muster only uses the plugin once you install it. Run this on the Mac that
runs DeepSeek Harness:

```sh
dsh plugin --profile desktop add file:<checkout>/src/muster/deploy/deepseek-harness
```

`dsh` is the CLI inside the app bundle,
`/Applications/DeepSeek Harness.app/Contents/Resources/runtime/cli/bin/dsh`.
The **Plugins** page in DeepSeek Harness can install the same directory.
Restart DeepSeek Harness afterwards. Muster's daemon finds the socket
within a second, and sessions then show the `plugin` controller.

To drive sessions while the Mac is locked, apply the same settings that
Claude and Codex need. On that Mac, allow locked control and add the app to
its allowlist:

```sh
muster policy --allow-locked-control true --ax-allow com.deepseek.dsh
```

Each such operation then also asks for Face ID or Touch ID on the phone.

## Remove

```sh
dsh plugin --profile desktop remove muster-dsh-bridge
```

You can also turn it off or remove it on the app's **Plugins** page. Restart
DeepSeek Harness. Its sessions stay viewable in Muster but can no longer be
driven.

## Security

- The plugin runs inside DeepSeek Harness with your user's permissions, like
  every DeepSeek Harness plugin.
- The socket's directory is mode 0700 and the socket 0600.
- Muster talks to the socket only when the process serving it is DeepSeek
  Harness, checked by code signature (`com.deepseek.dsh`, team `NAN929V4UM`)
  and launch time. It refuses to write when another process serves it.
- Who may send, approve or interrupt is decided by Muster's daemon, by the
  same rules as for any agent session.

## Test

```sh
node --test test/*.test.js
```

The tests run on Node 22 or newer, including the app's own runtime.

### Local contract and receipt recovery (0.2)

Protocol 1 remains available to existing daemons. `hello` and `state` now also
advertise `control`: version 1, domain `app`, provider `deepseek`, a random
per-instance epoch and the supported operation names. A new daemon binds each
request to that epoch, session and ledger operation; replies echo that binding.
Malformed or unsupported contracts are refused rather than silently downgraded.
An old plugin without `control` retains its previous behavior.

Shared Rust derives the native prompt request ID from the session and ledger
operation. After a lost reply it can read matching host `source.rpcId` evidence
from the inbox or session events. Empty evidence stays unknown; a status query
never resends. Explicit host acceptance means the prompt was accepted, not that
the model completed its work. Host exceptions after invocation stay unknown.
Only errors before invocation declare `delivery: not_started`.

This is an App adapter. Launching its host with the bundled launcher does not
turn its sessions into Muster CLI sessions. Device RPCs, protobufs, scopes and
the Apple ABI are unchanged. Model selection, queue editing and native
attachments remain unavailable unless separately implemented and verified.
See [ADR 0094](../../../../docs/adr/0094-local-plugin-contract-and-receipt-recovery.md).

### Installed App acceptance on macOS

Use the installed App's host with an isolated temporary profile. Do not copy
production credentials or restart an active desktop session. From this plugin
directory, in a dedicated terminal:

```sh
plugin_dir="$PWD"
acceptance_home=$(mktemp -d /private/tmp/muster-dsh-XXXXXX)
cat > "$acceptance_home/acceptance.yml" <<EOF
- insert:
    - id: muster-bridge-acceptance
      name: "$plugin_dir/index.js"
    - id: muster-local-acceptance
      name: "$plugin_dir/test/host-acceptance.mjs"
EOF
DSH_HOME="$acceptance_home" "/Applications/DeepSeek Harness.app/Contents/Resources/runtime/cli/bin/dsh" \
  --profile web --patch "$acceptance_home/acceptance.yml" \
  --no-open --host 127.0.0.1 --port 0 > "$acceptance_home/host.log" 2>&1 &
acceptance_pid=$!
```

Wait for `acceptance-session.json` in that directory, then from `src/muster`
in the same terminal run:

```sh
MUSTER_DSH_ACCEPTANCE_HOME="$acceptance_home" cargo test -p musterd --test deepseek \
  installed_app_plugin -- --ignored --nocapture
kill -TERM "$acceptance_pid"
```

This explicit test verifies the host's code signature and launch identity,
sends one synthetic prompt through authenticated existing device RPCs, waits
for its native receipt, checks duplicate/status responses and interrupts the
session. It requires no model credentials and does not verify a model answer.
The host log can contain a local UI token; keep it private. Separate regular
integration tests simulate a lost reply and reconstruct the plugin link to
verify recovery with exactly one host send. This is not a physical phone test.

### Multica P0 contract probe

`test/multica-contract-probe.mjs` is an explicit isolated installed-host probe
(ADR 0149). It refuses any home outside `/private/tmp/muster-dsh-p0-*`.
Use a fresh `DSH_HOME`, a patch that inserts this fixture, and the installed App
host with `--profile web --no-open --host 127.0.0.1 --port 0`, as above.
Set `MUSTER_DSH_PROBE_MODE=create` on the first launch. The fixture records a
native ID before creating it, submits one synthetic no-tools prompt and writes
`p0-create.json`. Stop that host, then start the same profile with mode `lookup`.
It writes `p0-lookup.json` by observing persisted state only; it never creates,
resumes or sends in lookup mode. Always stop the isolated host after collecting
the report; host logs stay private because they can include a local UI token.

The report distinguishes accepted prompt evidence from the terminal reason.
`modelSelectionMethodPresent` is discovery only, not verified model selection;
`taskCredentialCallbackVerified` remains false. A successful probe does not
register a Multica executor or authorize a real scheduler task. Copy only
sanitized outcomes into durable validation records; scratch profiles and build
directories are not permanent acceptance evidence.
