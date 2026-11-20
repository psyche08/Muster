# ADR 0033: Recover managed terminals and attach existing desktop clients

- Date: 2026-10-23
- Status: Accepted
- Related: [ADR 0003](0003-confirmed-conversation-workbench.md), ARCHITECTURE §3.6.3

## Context

Managed conversation terminals already run on a dedicated tmux socket, but their
public IDs identify a transient PTY bridge. Recreating musterd loses that bridge.
PTY output publishes a frame for each read, and taking a subscriber snapshot can
consume dirty rows that another subscriber still needs. Desktop users also lack
an explicit way to attach iTerm2 or Ghostty to the same managed shell.

## Decision

Keep ordinary iTerm2 and Ghostty adapters and their authorization unchanged.
Shared Rust assigns a generation-specific managed handle stored as a tmux session
option. Reattach resolves that exact handle on the dedicated socket; a missing
handle fails rather than creating or switching shells. Explicit restart replaces
the handle. Existing managed shells receive a handle on their next Start.

Full managed frames carry Rust-generated, shell-quoted desktop attach commands.
The Apple shell offers copy actions for a regular terminal (Ghostty or iTerm2)
and iTerm2 control mode. Commands check the handle inside tmux before attaching;
they are meant to run on the session's host, never automatically typed into an
existing terminal. Native clients remain independent and no title/cwd heuristic
merges their identities. Closing a view detaches; only End/Restart kills a shell.

Bound PTY frame publication to an 8 ms coalescing window while continuing to
parse bytes and answer terminal queries. Snapshots do not consume pending deltas;
lagged subscribers resume from a fresh snapshot, ignoring older queued frames.
Report cursor-only changes and monotonic end revisions. Serialize PTY writes.

This slice supplies recovery, desktop attachment, and output scheduling. Full
tmux pane topology, automatic deduplication of native windows, and exclusive
multi-client resize/input ownership remain separate work. Desktop attach commands
use ignore-size so viewing from a desktop does not change the managed viewport;
the existing Muster viewport continues to set its size. This explicit interim
policy avoids claiming a desktop-follow/take-control UI that is not implemented.

## Alternatives

Replacing existing Ghostty windows with tmux would lose compatibility with live
programs that cannot be migrated. Inferring bindings from titles could target the
wrong shell. Restarting on a stale handle could silently lose the user's work.
Running a separate ttyd HTTP service would duplicate transport and authorization.

## Consequences

No extra daemon port, automatic command execution, or GUI permission bypass is
introduced. tmux must be installed on the host. Existing native sessions retain
their capabilities and limitations. Reconnection starts with a full snapshot;
terminal input is never automatically replayed. Stable handles do not imply
persistence through a host reboot or a stopped tmux server.

## Verification

Source and local verification on macOS:

- 25 terminal tests passed, including three explicitly enabled real-tmux tests
  using isolated sockets (tmux 3.7c). They cover service recreation, old-handle
  rejection, detach persistence, removed-cwd recovery, separate command runs,
  quoted socket paths, regular and control-mode desktop attachment, and refusal
  of a copied command after restart. No existing user terminal is driven.
- 180 app-core/daemon tests passed; one existing manual diagnostic remains
  ignored. Existing Ghostty stream and SSH integration tests passed.
- The burst test emitted 2,000 output lines as 2 frame updates in the recorded
  run; final output precedes the end frame. This is synthetic output scheduling
  evidence, not an end-to-end device latency result.
- Strict Clippy for the three affected crates/tests passed. Rust formatting,
  layer checks and public-content checks passed. Swift bindings were regenerated
  and the macOS MusterUI target built successfully.
- Hosted Actions remains disabled under ADR 0016; no hosted CI pass is claimed.

Signing/notarization: not run. Publication and installation: not performed.
Runtime: isolated PTY/tmux tests passed. Actual iTerm2/Ghostty GUI acceptance,
iOS device build/installation, and physical-device latency remain unverified.
Desktop-first sizing, controller ownership and pane management remain follow-up
scope, as stated above.

## Exact session lookup on tmux 3.8

A Windows compatibility check using MSYS2 tmux 3.8 found that a bare exact
session name passed to `display-message -t` produced empty session fields even
though `has-session` succeeded. The former accepts a pane target. Shared Rust
will specify the session component explicitly, require the returned name to
match, and require a native numeric session ID before using it for handle
lookup, attachment or removal. An empty target cannot identify a terminal.

Keep the existing generation handles and validate the session on the dedicated
socket. Verify lookup with two live synthetic sessions, resume and end them
independently, and check missing or malformed identity fields. Separately test
ConPTY input, the returned grid and host-observed resize on Windows.

On Windows, the MSYS2 tmux 3.8 fixture returned distinct native IDs for the
two exact session selectors. An isolated build of the shared service using
ConPTY recovered the same shell variable in a second client process. Restart
invalidated the old handle and ending that session left its sibling usable.
A separate ConPTY round trip returned the computed result and resized the
viewport to 94 by 27, including the tmux status line. These probes ran outside
musterd. Directly passing a Windows drive path to tmux selected the home
directory, while the equivalent MSYS2 path selected the requested directory.
Windows host integration still needs directory conversion and daemon-level
acceptance before it can offer managed terminals.
