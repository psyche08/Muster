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

Run `scripts/test-terminal-simulation.sh` from the repository root for the full
acceptance set below. It requires an existing tmux installation and uses only
synthetic hosts, private sockets, temporary directories and PTYs. It also runs
strict Clippy, formatting, architecture-layer and public-content checks.
Before landing a development branch, also run `scripts/check-adr.sh origin/main`
after committing its numbered decision record.
The dev bundle is built separately with `scripts/dev-terminal.sh --build-only`.

| Capability / failure boundary | Executable coverage |
| --- | --- |
| iTerm2 existing inventory, exact opaque IDs, ordinary/new managed window request | `iterm2::native_protocol_tests`, `creation_tests` |
| Input connection reuse with no inventory preflight; reconnect without replay | `existing_sessions_and_new_windows_share_connection_without_input_rescans`, `a_new_operation_reconnects_after_socket_loss_without_replaying_input` |
| Output burst coalescing, cursor-only update, changed rows, shrinking screen, unchanged output | `screen_bursts_cursor_only_changes_and_shrinking_screens_are_delivered` |
| Termination within a burst; unrelated sessions; notification overflow resync | `termination_during_coalescing_is_not_swallowed_or_refetched`, `lost_screen_notifications_resync_even_when_only_unrelated_events_remain` |
| Inventory subscription, changes, overflow and cancellation during a fetch | `inventory_notifications_and_lag_refresh_and_cancel_during_fetch` |
| Backpressure, timeouts, late/out-of-order replies, cancellation cleanup, disconnect versus receipt | `iterm2::simulation_tests` request lifecycle cases |
| Exact input/resize/history target, broadcast suppression, protocol failure and close refusal | `input_resize_and_history_use_exact_target_and_surface_protocol_failures`, `native_close_refusal_missing_and_malformed_receipts_never_report_success` |
| Ghostty text escaping, snapshot/delta/end, scrollback trimming | `ghostty::tests` |
| Both sources, partial inventory errors, opaque IDs, legacy primary IDs, bounded slow inventory | `musterd/tests/native_terminals.rs` |
| Native create concurrency/replay, conflicting operation bodies, missing receipt and stale managed handle | `ambiguous_creation_and_conflicting_replay_never_open_a_second_window`, `native_managed_window_uses_exact_handle_and_replay_opens_only_once` |
| Input deduplication, invalid IDs, unsupported resize, attach cancellation without closing window | `input_replay_id_conflicts_and_invalid_routes_have_no_extra_side_effects`, `native_attach_qualifies_frames_and_cancellation_does_not_close_the_window` |
| Simulated OS denial/recovery, remote permission rejection, secondary Ghostty console lease routing | `simulated_permission_denials_are_visible_and_never_synthesize_success`, `musterd/tests/access.rs`, `secondary_ghostty_ids_still_require_the_console_lease` |
| UI frame projection, cursor deltas, shrinking rows, ended state, wrong-device/session rejection, advertised-host creation guards | `muster-app-core::update::tests` native frame/creation cases |
| Managed shell reconnect/service recreation, detach, restart/end, stale generation, cwd and code runs | Three opt-in `tmux::tests::real_*` tests and the native managed-window RPC test |
| PTY output scheduling, lag recovery, resize, VT alternate screen, styles, terminal query replies | `pty::tests`, `grid::tests`; SSH transport fixture in `muster-terminal/tests/ssh.rs` |

This matrix covers the implemented simulated behavior, not every possible OS
behavior. Native GUI opening/focus, macOS grants, actual iTerm2/Ghostty rendering,
physical iOS input and end-to-end latency remain manual acceptance. Ghostty's
existing accessibility limits, native pane mapping and desktop viewport ownership
remain as described above. No simulated pass authorizes a production release.

From `src/muster`:

```
cargo test -p muster-terminal -p musterd -p muster-app-core
cargo test -p muster-terminal tmux::tests -- --ignored
cargo test -p musterd native_managed_window_uses_exact_handle_and_replay_opens_only_once -- --ignored
cargo clippy -p muster-terminal -p musterd -p muster-app-core --tests -- -D warnings
```

The native fixtures simulate existing windows, initial-command creation, IDs,
connection loss and RPC replay. Real tmux tests use dedicated temporary sockets.
These do not establish successful native GUI permission grants or iOS acceptance.
Do not publish an update until the remaining device acceptance is complete.
