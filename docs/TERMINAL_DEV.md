# Isolated terminal development

Run `scripts/dev-terminal.sh` from a session worktree. `--build-only` builds
without launching. The output is `build/dev/MusterDev.app`, with a distinct
bundle ID. It uses ad-hoc signing; no certificate, notarization, installation,
launch-agent registration or release publication is performed.

Every app launch creates a private `/tmp/muster-dev-<UUID>` directory, printed
in the window. Its software identity, database, socket and logs stay there.
The app starts its bundled daemon with loopback bind, no virtual interface,
no agent/cloud discovery and no relay configuration. Quitting stops only that
child daemon. Shared Rust checks daemon readiness through the existing ABI.
Logs and state remain for diagnosis. The installed Muster app remains separate.

## Native host acceptance

1. Start iTerm2 and/or Ghostty. Automatic mode registers both installed apps.
2. For iTerm2, enable Python API under Settings → General → Magic. Use the local
   setup action to authorize Automation and the iTerm2 API connection.
3. For Ghostty, authorize this dev app's Automation and Accessibility access.
   Output reading still requires the unlocked console or an authorized Locked
   Use lease. Development does not install or replace the system helper/plugin.
4. Select this device in Terminals. Each existing native session has its own
   source-qualified identity. A failed source appears as an error without
   hiding sessions from the other source.
5. Choose New window → Open in iTerm2/Ghostty. Confirm a real new native window
   appears and its input/output corresponds to the selected Muster view. Use
   newly created test windows for input tests, never another live program.
6. In a normal conversation workspace, Connect starts/resumes its managed tmux
   shell. The terminal menu's Open in action opens a native view of that same
   shell. Disconnect preserves it; End/Restart are explicit shell lifecycle
   actions. The minimal isolated dev app disables agent discovery, so this
   conversation path is covered by shared-core/tmux fixtures separately.

The native view uses the existing managed viewport policy (`ignore-size`).
Ordinary native sessions keep their native adapters and are not converted to
tmux. Ghostty output remains accessibility text with its refresh/cursor/style
limitations; it is not a full VT stream. Multipane mapping, desktop ownership
and physical-device latency are separate acceptance work.

## Automated checks

From `src/muster`:

```
cargo test -p muster-terminal -p musterd -p muster-app-core
cargo test -p muster-terminal tmux::tests -- --ignored
cargo clippy -p muster-terminal -p musterd -p muster-app-core --tests -- -D warnings
```

The native fixtures simulate existing windows, initial-command creation, IDs,
connection loss and RPC replay. Real tmux tests use dedicated temporary sockets.
These do not establish successful native GUI permission grants or iOS acceptance.
Do not publish an update until the remaining device acceptance is complete.
