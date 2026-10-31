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
