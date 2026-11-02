# Service Layer: muster-rpc and Terminals

Part of the [Muster architecture](../ARCHITECTURE.md). Section numbers are shared across all parts, so a reference such as §3.6.2 means the same section wherever it appears.

## 3.6 Service Layer

### 3.6.1 muster-rpc: Identical Local and Remote Interfaces

App connections to local `musterd` (UDS) and remote `musterd` (PeerLink streams) use **the same schema and client code**. The local principal is the local owner, verified through the UDS peer uid; the remote principal is a roster DeviceId.

Desktop Apps also ask local `musterd` to open PeerLink streams to remote devices. Each desktop thus has one network endpoint and shared links for vnet, forwarding, and services.

```proto
service Agent {
  rpc ListSessions(ListReq) returns (SessionPage);
  rpc Watch(WatchReq) returns (stream SessionEvent);          // Push replaces preview polling
  rpc Send(SendReq) returns (OpResult);                       // op_id idempotency
  rpc Respond(RespondReq) returns (OpResult);                 // Approvals / structured questions
  rpc Interrupt(InterruptReq) returns (OpResult);
  rpc Files(FilesReq) returns (FilesPage); rpc Git(GitReq) returns (GitPage);
}
service Terminal {
  rpc Sessions(Empty) returns (stream TermSessionList);       // Push iTerm2 window/tab/session changes
  rpc Attach(AttachReq{session, since_rev}) returns (stream TermFrame); // Snapshot followed by row deltas
  rpc History(HistoryReq{session, line_range}) returns (TermLines);     // Fetch scrollback on demand
  rpc Input(InputReq{op_id, session, text}) returns (Ack);
  rpc Create(CreateReq{profile?, window?}) returns (TermSession);
  rpc Resize(ResizeReq) returns (Ack); rpc Close(CloseReq) returns (Ack);
}
service Device  { rpc Info(Empty) returns (DeviceInfo); rpc WatchState(Empty) returns (stream DeviceState); } // Lock state, online, capabilities, vnet state, exposed ports
service Network { /* Local or network.admin only: roster, invitations, approval, removal, scope changes, vnet switch, locally exposed ports */ }

// forward is a stream-level protocol, not RPC: raw bytes follow ServiceHello{forward, tcp, port} (see 3.5)
```

**One idempotency implementation**: `OperationLedger` (retaining existing JSONL ledger semantics).

- The client generates `op_id`; the server persists pending before any side effect;
- The same `op_id` returns the persisted result; the same `op_id` with a different body is rejected;
- After a crash, the state is Unknown, with no automatic replay.

The five journals become this one in `musterd`; clients store only unconfirmed op_ids.

### 3.6.3 Terminal Service: Independent iTerm2 / Ghostty Adapters (R5)

The two adapters integrate as peers with `Terminal`, independently declaring discovery, creation, input, close, resize, output subscription, and history capabilities. Rust validates operations and projects UI using actual adapter capabilities. Unsupported actions cannot appear available, and successful input receipts cannot fabricate output. Each session binds an adapter and native session ID; do not guess or switch sessions across adapters by title. Both share foundational authorization, OperationLedger, and remote subscriptions without another App-local loopback service.

**Ghostty adapter** (2026-10-04, `muster_terminal::ghostty` over `muster_platform::macos::ghostty`): Ghostty 1.3 has no API like iTerm2's (no socket, no screen notifications). Each of its parts uses a different channel:

- **Discovery, create and close**: its AppleScript dictionary (`osascript`).
- **Input**: the `text:` binding action through `perform action`. This writes bytes to the program as typed, not as a paste.
- **Output**: the terminal view's accessibility text (`AXNumberOfCharacters`, `AXStringForRange`), polled every 300 ms into line deltas. Ghostty refreshes that text at most every 500 ms.
- **Resize**: none; the window keeps its Mac size.
- **Identifying a terminal's view**: accessibility shows the view only as the focused one. Attaching brings the terminal to the front and checks Ghostty's focused-terminal id before and after taking the view.
- **Permission checks**: Apple Events need the Automation permission for `musterd` to control Ghostty. It is checked without a prompt before every use; only the Mac's setup action may ask.

Accessibility shows no windows while the console is locked, so on a locked Mac a phone reads and types only inside a Locked Use terminal session (see [LOCKED_USE.md](../LOCKED_USE.md)). `--terminal auto` picks iTerm2 when it is installed, else Ghostty when it is, else iTerm2.


**Move the iTerm2 client into `musterd`, implemented in Rust**:

- Connect over UDS to `~/Library/Application Support/iTerm2/private/socket`, upgrade to WebSocket, and exchange protobuf. Generate protobuf with prost from the vendored `api.proto` (iTerm2 3.7.0beta8).
- Match the existing Swift version (SwiftNIO, about 3.9K handwritten + 16.7K generated lines), with an estimated 2K lines.
- Result: remote terminals no longer depend on the App process running; delete the entire backend → App loopback terminal-service path.

**Event-driven, no polling**:

- Subscribe to iTerm2 notifications: `NOTIFY_ON_SCREEN_UPDATE`, `NOTIFY_ON_NEW_SESSION`, `NOTIFY_ON_TERMINATE_SESSION`, `NOTIFY_ON_LAYOUT_CHANGE`; optionally `NOTIFY_ON_PROMPT`, which can push command completion to phones with shell integration.
- On screen updates, use `GetBuffer` for the visible region with styles; `musterd` compares rows against the previous contents and pushes only changed rows and cursor.
- Multiple clients attached to one session share each fetch and maintain individual revisions.
- Fetch scrollback on demand through `History`, not with pushed frames.
- This replaces full-screen base64 + `if_revision` + backoff + eight echo polls and resolves the earlier document's remaining full-frame transfer during screen changes.

**Client rendering**: app-core maintains styled row grids, applies deltas, and emits ViewModels; native code only draws the grid and collects keys. Rust maps special keys (arrows, Ctrl-C, etc.) to byte sequences; platforms only report the pressed key. iTerm2 sessions provide styled rows, not a byte stream, so a full terminal emulator such as SwiftTerm is unsuitable; use a custom grid control, shared with SSH sessions below.

**Input, create, close, resize**: Map to `SendText` (explicit `suppress_broadcast`, retaining existing rules), `CreateTab`, `Close`, and `SetProperty(grid_size)` respectively. Input carries `op_id` for `OperationLedger` deduplication.

**Authentication (decided: Automation permission)**: iTerm2 API requires a cookie by default, requested by `musterd` through Apple Events (`request cookie and key for app named …`). Grant `musterd` Automation permission once; do not change iTerm2's global allow-all-Apps setting. Apple Events calls live in the Apple shim static library alongside SEP.

**Lock screen**: iTerm2 API is local socket IPC and works normally while locked.

**Phone SSH (in-App over forwarding streams; decided: Rust, hardware keys)**:

- `app-core` embeds an SSH client (`russh`), using forwarding streams directly as transport without local ports.
- The client key is the phone's Device Key (P-256, `ecdsa-sha2-nistp256`); private keys remain in SEP / StrongBox / HUKS, with signing through `hw_sign`. Depending on local policy, `musterd` can write roster public keys to an `authorized_keys` include file (optional, P6); otherwise users add them manually once.
- `muster-terminal`'s VT state machine parses SSH bytes into the same row grid, reusing iTerm2 terminal ViewModels and the custom grid. To users, iTerm2 and SSH sessions are simply two sources in the terminal list.
- This replaces unused swift-nio-ssh and SwiftTerm in `ios/Deps/MusterDeps`; Rust implements SSH once for all four platforms.

**Managed conversation terminals** (2026-10-23, [ADR 0033](../adr/0033-managed-terminal-compatibility.md)):

The existing conversation workspace's dedicated tmux backend complements both
native adapters. Rust persists a generation-specific handle in the tmux session
and lazily reconstructs its PTY bridge after musterd restarts. Stale handles fail
instead of recreating or retargeting a shell. A view detaches; explicit End or
Restart destroys the session. Full frames carry shell-quoted commands for a
Ghostty/iTerm2 terminal or iTerm2 control mode on the host. Native windows are not
automatically merged by title/cwd. Desktop commands use `ignore-size`, retaining
Muster's viewport sizing in this phase; desktop-first ownership and native pane
mapping are not implemented by this slice. PTY output coalesces over 8 ms and
subscriber snapshots preserve other subscribers' pending updates.

Native host selection ([ADR 0035](../adr/0035-native-terminal-dev.md)) exposes both
installed adapters in automatic mode. Rust namespaces native IDs, reports partial
inventory failures and chooses the exact requested source for new windows.
Managed frames advertise native hosts; opening one starts the generation-checked
attach command as a new window's initial process. Existing native sessions are
not converted or merged heuristically. A live iTerm2 connection is reused without
an extra list request before each input. Local testing uses the separate
`MusterDev.app` bundle and a fresh private daemon directory; see
[terminal development](../TERMINAL_DEV.md).


**Other terminals**:

- **PTY**: Later-phase fallback only for Linux / Windows (ConPTY) and environments without iTerm2.
- **Desktop SSH**: Available directly through vnet (`ssh <device-name>`), requiring no dedicated development.
