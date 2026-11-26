# Muster remote desktop engines

This directory holds the downstream Sunshine host and Moonlight PC source.
macOS and Windows each use a private Sunshine host for incoming sessions and a
Moonlight controller for outgoing sessions. The iOS development build embeds
the Moonlight C core and renders the incoming H.264 stream in Remote. Review
the combined iOS binary's distribution license before publication.
See [ADR 0301](../../docs/adr/0301-desktop-gamestream-roles.md).

The Windows portable build uses the existing vcpkg viewer dependencies and
the upstream Sunshine x64 UCRT64 toolchain. Set `MSYS2_ROOT` if MSYS2 is
installed outside `C:\msys64`. Install the UCRT64 compiler, Ninja, CMake,
Boost, cppwinrt, curl-winssl, MinHook, miniupnpc, nlohmann-json, oneVPL,
OpenSSL and Opus per the vendored Windows build workflow. The build
packages the private host, shaders, runtime DLL and corresponding source
archive alongside the viewer. On Windows, the root `muster-stream-host.exe`
is the LocalService supervisor. The ordinary account's `muster-stream-worker.exe`
launches the private `Sunshine/muster-stream-host.exe` engine for a live grant.
The supervisor has no network listener. The macOS app embeds the media engine
directly as `Contents/MacOS/muster-stream-host`.

## Downstream changes

- Sunshine forces IPv4 loopback even if the configuration supplies a different
  bind address. It does not start the admin web server, system tray, mDNS, or
  UPnP. GameStream HTTP(S), RTSP, and UDP remain separate engine endpoints.
- The Moonlight C core only accepts literal `127.0.0.1` as a streaming host.
  The upstream Qt shell is included for source development, not as the Muster
  UI or an authorized network client.
- The Sunshine child reads one session certificate and view/mouse mode from an inherited pipe,
  refuses upstream pairing and exits when the owner closes the pipe. The host
  accepts only desktop capture. On macOS the parent supplies a private session
  directory and its pinned TLS key pair. See [ADR 0278](../../docs/adr/0278-sunshine-session-input.md).
- Shared Rust carries the selected session's fixed media channels over PeerLink.
  The host checks the local console and the owner grant for the selected viewer.

## Source pins

| Component | Upstream | Revision |
| --- | --- | --- |
| Sunshine | `https://github.com/SunshineStream/Sunshine` | `v2026.914.233613` (`63d35f702ee9e362e43263742981836ec0710384`) |
| Moonlight PC | `https://github.com/moonlight-stream/moonlight-qt` | `v6.2.0` (`de2467e433821664cdd2224aad8c89a625be1ad9`) |
| Moonlight C | `https://github.com/moonlight-stream/moonlight-common-c` | `f900dd4767759c7b9d0e93bcea666b55c69ea62f` |
| ENet | `https://github.com/cgutman/enet` | `aca87840b57f045a1f7f9299e4b1b9b8e2a5e2f1` |
| nanors | `https://github.com/sleepybishop/nanors` | `b1e3c22ca0cdc0bb83e3cd6ed1a2fc77869ed99a` |
| qmdnsengine | `https://github.com/cgutman/qmdnsengine` | `920c097ffa742e2968290f15d4dde6693aec02e5` |
| SDL_GameControllerDB | `https://github.com/gabomdq/SDL_GameControllerDB` | `8d9fefd7b810f2541f78cc7a8ccbd185bc84c7a5` |

Moonlight's Git submodules above are ordinary files in this checkout. Sunshine's
external build dependencies are not bundled; their paths, URLs, and exact
revisions are recorded in the vendored `.gitmodules` and
[`sunshine-submodules.txt`](sunshine-submodules.txt). The Sunshine snapshot
excludes website/branding art under `gh-pages-template/`, `branding/` and
`docs/images/` and the two upstream tests with embedded private-key fixtures
(`test_confighttp.cpp` and `test_http_pairing.cpp`).

Preserve the original license and attribution files in the vendor trees.
Muster's root Apache-2.0 license does not relicense the vendored engines.

## Local check

With CMake, a C compiler and OpenSSL installed, build and test the Moonlight C
core without launching either engine:

```sh
cmake -S addons/remote-desktop -B /tmp/muster-remote-desktop-check
cmake --build /tmp/muster-remote-desktop-check --parallel
ctest --test-dir /tmp/muster-remote-desktop-check --output-on-failure
```

If CMake cannot locate OpenSSL, pass `-DOPENSSL_ROOT_DIR` for your installation.
The Sunshine regression in `tests/unit/test_config.cpp` requires the full host
build dependencies; this check does not build or run Sunshine or Moonlight Qt.

## Windows viewer

Use x64 MSVC, CMake, Python and a bootstrapped vcpkg checkout at
`b94b6bdc326c0faea9070b4862e0e0d4e4ac5396`. From this directory run
`python build-viewer.py <vcpkg> <build-directory> <viewer-output>`.
The script builds the separate Moonlight process, checks a decoded 720p frame
and mouse coordinates, and collects license texts and corresponding sources.
The native Windows packager always compiles this code using the pinned
vcpkg checkout. A Windows cross-package build can consume the corresponding
output directory via `MUSTER_WINDOWS_VIEWER`. The synthetic fixture is one blue frame from
FFmpeg's color filter encoded with libx264, with SEI metadata removed.

Use Settings to grant the paired controller desktop view or mouse access.
On the controller, select the host in Remote and connect. The first viewer
keeps its session until a second controller confirms takeover. macOS hosts
require Screen Recording and Accessibility for capture and mouse control.

## macOS capture host

Build the normal macOS application or run `scripts/dev-terminal.sh --build-only`.
The package step invokes `build-host.py` with the Cargo build cache, the private
helper destination and a source archive destination. The build machine needs
Apple command line tools, Python 3, CMake 3.24 or newer, Git, curl, Perl, make
and pkg-config. The first build downloads pinned dependencies. The native
engine uses static codec, Opus and OpenSSL libraries and the macOS SDK's curl.
The build checks every runtime library reference and signs the bundled host.
Its matching source archive lives in `Contents/Resources/ThirdParty`.
The private capture executable sits beside musterd in Contents/MacOS. The foreground
Muster app requests the OS permissions; the capture tool only preflights them. Source builds and private handshakes can be tested on a locked Mac.
Test ScreenCaptureKit on the signed-in physical console. Record the unlocked desktop and the locked login surface separately. See [ADR 0309](../../docs/adr/0309-macos-remote-screencapturekit.md).
