# ADR 0240: Bundle remote desktop engines behind Muster authorization

- Date: 2026-11-20
- Status: Proposed; not enabled in the product
- Related: architecture §3.4, §3.5 and §3.6; ADR 0105, ADR 0107, ADR 0232

## Context

Muster needs pixel-level viewing and keyboard/mouse control for opted-in desktop
hosts. Sunshine provides capture, encoding, and the GameStream host; Moonlight
provides the matching client. Sunshine's default administration model is a web
server and its streaming data plane requires both TCP and UDP. Muster's
existing phone forwarding protocol is TCP-only, and desktop vnet membership
does not itself authorize remote screen access or input.

## Decision

- Keep identified Sunshine and Moonlight source snapshots in the repository
  for downstream changes, with upstream licenses and attribution intact.
  Treat them as separate media engines, not as an alternative Muster trust
  stack. Keep GPL code out of the Apache-licensed Rust and native UI targets;
  review how the actual combined distribution and source offer work before
  shipping binaries or linking any new viewer into an app.
- The Muster Sunshine build must not start the browser UI, UPnP, or mDNS, or
  accept non-loopback binds. Keep GameStream's control endpoints distinct from
  its web administration listener. Pair and manage the child over private,
  narrowly scoped IPC rather than re-exposing the web API. Do not publish
  Sunshine's loopback ports through the existing generic port-forwarding or
  l4forwarder configuration.
- Define remote desktop as a dedicated service over an authenticated PeerLink.
  Shared Rust owns host opt-in, session admission, separate viewing and input
  grants, fixed endpoints, session teardown, and the native view model. A
  bounded stream carries reliable control; session-bound PeerLink datagrams
  carry media with explicit demultiplexing from vnet frames. Do not accept an
  arbitrary host or port from the client. Stop capture and apply a host-side
  input gate on disconnect, revocation, host disablement, or screen lock;
  do not implement automatic unlock or attempt to reuse Agent Halo Locked Use.
- Expose host setup, session controls, and viewing in Muster's native UI. Feed
  just the required OS capabilities and user intents across the ABI. Start
  device acceptance with macOS as the desktop host and client. Windows host
  support and an iOS viewer are separate acceptance milestones; do not add a
  phone VPN or a LAN fallback.

## Alternatives

- Binding stock Sunshine to the vnet address would still use the generic port
  policy, would not replace the admin web UI, and would not work for phones.
- Exposing Sunshine through a TCP tunnel would not carry GameStream's media
  and UDP control. A separate VPN, direct LAN, or WAN discovery path would
  create a second route outside Muster's authorization boundary.
- Embedding Moonlight's Qt UI or opening Sunshine's web UI would keep separate
  user-facing configuration and pairing surfaces instead of the native flow.

## Consequences

Keep the feature disabled until the native control and authorized streaming
paths are both wired. Shipping the engines also requires checking the pinned
transitive dependencies and license notices, platform capture/input grants,
and signing and update behavior. No upstream admin port is part of the public
Muster service contract.

## Verification

- Source: the engine revisions and preserved license files are listed in the
  [addon README](../../addons/remote-desktop/README.md). The Moonlight C core
  compiled on macOS and its loopback admission test passed. Sunshine's source
  disables web administration and discovery and includes a config override
  regression. Its macOS configure attempt found missing `build-deps` and
  Boost; the host binary and regression test remain unverified.
- Runtime: verify authorized view and input independently; reject unenrolled
  devices, enrolled devices without a grant, and physical LAN/WAN clients.
  Verify wrong-session datagrams, revocation during streaming, host lock and
  process exit; exercise direct and relay paths between separate devices.
- Build, signing, installation, publication, and physical acceptance are
  pending. A source import or a standalone component build is not acceptance.
