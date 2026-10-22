# Remote terminal

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](../../PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

Status: 2026-09-21. Slices one, three, and four are implemented (authorization in slice two was deferred at the user's request).

> **End-to-end operation still fails, and the cause is confirmed.** `remote_route_scope` has no `POST /terminal` entry,
> so it falls through to `_ => None` → `remote_forbidden()`, and the peer returns 403. This is a structural default deny, not an omission:
> the network-member scope set is hardcoded to three entries, excluding `TerminalConnect`.
> See [agenthalo-network-authority.md](agenthalo-network-authority.md). The goal is to give AgentHalo terminal mode a device dimension, like sessions: operate Ghostty / iTerm2 on mac-b from mac-a.

## Why the session path cannot simply be copied

Remote **sessions** (Claude/Codex) follow: peer App → peer backend (Rust) → Provider executor. Providers live in Rust, so the backend can execute them itself.

**Terminals do not live in Rust.** Both adapters are Swift/macOS-specific:

- `GhosttyAdapter` — ScriptingBridge
- `Iterm2Adapter` — iTerm2's local API socket

They run in the **App process**, while the **backend process** receives remote requests. The backend has no terminal concept and no reverse channel to the App (`desktop.sock` is a leftover file from August 28; no code references it and no process holds it).

Remote terminals therefore need one extra hop compared with remote sessions:

```
mac-a App → muster → mac-b backend → mac-b App → terminal adapter → Ghostty/iTerm2
                                   ↑
                              This hop is missing
```

## Slices

**Slice one: reverse channel (implemented).** The App exposes a local terminal service callable by the backend; the backend adds terminal routes and forwards requests. **This can be verified without pairing**: a working backend → App → adapter path on one machine is sufficient. This matters because pairing is currently blocked on user-presence confirmation (see [muster-single-network.md](muster-single-network.md)).

**Slice two: remote authorization (not implemented, explicitly deferred by the user).** Integrate terminal operations with the existing remote authorization and ownership checks used by provider operations. Terminals are a **write** capability: they can type arbitrary characters into a session and must not receive the same permissions as session reads.

**Slice three: client and UI (implemented).** Add a device dimension to terminal mode; snapshots and input travel through the muster channel.

**Slice four: snapshot stream (implemented).** Terminal snapshots use high-frequency polling, unlike session-list updates, so bandwidth and throttling need separate consideration.

## Slice one boundaries

- The App-side service **listens only on loopback**, with file permissions 0600. It can drive terminal input, equivalent to a keyboard, and must not be exposed to other local users.
- The backend infers no trust when calling it: it only forwards requests. Until slice two integrates authorization, **remote routes remain closed**.
- Adapter authorization is unchanged: Ghostty requires Automation consent, and iTerm2 requires explicit connection authorization. Remote access does not bypass either.

## Slice three: implementing the device dimension

The key decision is to **represent the peer's terminals as an ordinary adapter** (`AgentHaloRemoteTerminalAdapter`).

Everything above it—session lists, snapshot polling, input, and refusal to fall back for Claude CLI binding—already talks only to
`NativeTerminalAdapter`. The device dimension thus becomes a lookup instead of a second code path:

```swift
selectedTerminalDeviceID == "local"
    ? terminalAdapters[providerID]
    : remoteTerminalAdapters[selectedTerminalDeviceID]?[providerID]
```

Neither models nor views need to know which machine hosts the terminal.

### Protocol

The App's loopback service is the protocol definition; the two Rust hops forward it verbatim without interpreting the payload. Operations:

| op | Description |
| --- | --- |
| `providers` | Which adapters the peer has. **Ask; never guess**: a wrong guess reports a running peer terminal as `unknown_provider`, indistinguishable from an absent terminal |
| `activate` | Connect the adapter. Pass `request_permission` unchanged: consent belongs to the person at the peer machine, and background refresh must never display prompts on someone else's screen |
| `list` / `snapshot` / `send` / `resize` | Terminal operations themselves |
| `create` / `close` | Session lifecycle. If a capability bit claims support, it must actually work; otherwise the UI shows an inoperative button |

Three fields were added later because omitting them would misrepresent behavior:

- `content_model` (snapshot): without it, bytes from a control-only adapter would be rendered as a screen.
- `disposition` (send): “typed” and “duplicate, ignored” are different outcomes.
- `continuity_token` (send): continuity of peer deduplication; without it, peer restarts are invisible.

### Boundaries

- **Claude CLI binding is local-only.** It needs the concrete `Iterm2Adapter` type and Shared signature verification; the remote adapter can never be that type. The UI hides this button on remote devices, and the model rejects the operation directly without silent fallback.
- **Peer disconnection returns to local.** Peer-list changes rebuild the peer client, leaving cached adapters holding a nonexistent connection. Discard the entire cache and return terminal mode to `local`. Otherwise every subsequent operation reports a transport error instead of showing local terminals.
- **Authorization is not connected yet.** With slice two unimplemented, any peer passing owner authorization can currently drive the peer terminal—equivalent to typing on its keyboard. This is a known state, not the final design.

## Slice four: the cost of watching

Terminal snapshots use high-frequency polling. Local rendering is in-process and almost free; every remote frame is a muster round trip
carrying the full screen as base64. The old watcher made no distinction: a fixed 250ms interval and a full frame every time.
Even an idle terminal repeatedly sent the same screen through the relay at 4Hz.

Three changes:

**1. Conditional frames (`if_revision`).** The iTerm2 adapter's revision was already the first eight bytes of
`SHA256(content + session identity)`—**it is a content hash**. Thus
identical revision ⇒ identical screen; no extra digest field is needed. The caller sends its current revision;
the peer renders and compares, returning `{"ok":true,"unchanged":true,"revision":N}` when equal, only a few dozen bytes.

The peer still renders the screen (rendering produces the revision). This saves network traffic, not CPU—and the network
is the expensive part here.

**2. Backoff cadence.** `terminalPollInterval(isRemote:unchangedPolls:base:)` is a pure function,
so its behavior can be established without waiting:

| | Interval |
| --- | --- |
| Local | `base` (250ms, unchanged: slowing local polling would only hurt responsiveness without saving anything) |
| Remote, just changed | `base × 2` |
| Remote, unchanged for 4 polls | `base × 6` |
| Remote, unchanged for 12 or more polls | `base × 16` (cap) |

**A cap is mandatory**: unbounded backoff would make the next line take an hour to appear after an hour of inactivity.

Any change resets the counter. Input resets it too (`terminalActivityEpoch`): after typing a command, the user is
waiting for output, making backoff latency particularly noticeable.

**3. Echo polling uses the same path.** After each line sent, `sendTerminalInput` polls eight times for echo.
Remotely, that meant eight full-frame round trips per input line. It now uses conditional frames too.

### Unresolved

**While the screen changes, every update still carries a full frame.** A remote terminal producing output sends
2000 lines of rendered text on every change. A delta protocol is the real solution, but that requires the peer to remember “what was last sent”
for each caller—state that can lose synchronization—and its benefit cannot be measured before pairing works. It is therefore not implemented;
this limitation is recorded here rather than presented as solved.
