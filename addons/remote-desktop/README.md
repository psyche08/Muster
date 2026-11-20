# Muster remote desktop engines

This directory holds the downstream Sunshine host and Moonlight PC source.
Nothing here is built into a Muster release, installed, or started by Muster yet.
See [ADR 0240](../../docs/adr/0240-muster-remote-desktop-engines.md) for the
authorization boundary and device acceptance plan.

## Downstream changes

- Sunshine forces IPv4 loopback even if the configuration supplies a different
  bind address. It does not start the admin web server, system tray, mDNS, or
  UPnP. GameStream HTTP(S), RTSP, and UDP remain separate engine endpoints.
- The Moonlight C core only accepts literal `127.0.0.1` as a streaming host.
  The upstream Qt shell is included for source development, not as the Muster
  UI or an authorized network client.
- Neither loopback restriction authenticates a peer. Before enabling either
  app, Muster still needs the Shared Rust service, peer-scoped TCP/UDP broker,
  and native control and viewer integration. Do not publish the engine ports
  through a generic forwarder as a substitute.

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
