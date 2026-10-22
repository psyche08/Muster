#!/bin/sh
# A fake desktop for testing the phone shells against invented data: a second
# musterd on this Mac with its own state, synthetic Claude and Codex sessions
# (make-fixtures.py) and a PTY terminal running fake-shell.sh.
#
#   fakemac.sh run [relay]       in the foreground until Ctrl-C (default relay
#                                muster.linsheng.dev:9443)
#   fakemac.sh muster <args>     the muster CLI against it, e.g. create test-net,
#                                invite, pending, approve <device> --scope agents
#   fakemac.sh clean             forget its identity, network and fixtures
#
# As the admin of its own network it invites the phone (paste the invite in
# the phone's welcome screen) and approves it with the scopes under test.
#
# It refuses to start with Accessibility: a process that has it (or is started
# by an app that has it) could see and drive the real Claude and ChatGPT apps.
# It needs musterd and muster built for the host:
#   cd src/muster && cargo build -p musterd -p muster-cli
set -eu
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../../../../../.." && pwd)
bin="$repo/src/muster/target/debug"
dir="${FAKEMAC_DIR:-$HOME/.muster-fakemac}"
case "${1:-}" in
  run)
    test -x "$bin/musterd" -a -x "$bin/muster" || { echo "build musterd and muster first (see the header)" >&2; exit 1; }
    trusted=$(python3 -c "import ctypes; ax = ctypes.cdll.LoadLibrary('/System/Library/Frameworks/ApplicationServices.framework/ApplicationServices'); ax.AXIsProcessTrusted.restype = ctypes.c_bool; print(ax.AXIsProcessTrusted())")
    if [ "$trusted" != False ]; then
      echo "This process has Accessibility; run it from a terminal without it." >&2
      exit 1
    fi
    mkdir -p "$dir/state" "$dir/home"
    python3 "$here/make-fixtures.py" "$dir/fixtures"
    exec /usr/bin/env -i HOME="$dir/home" PATH=/usr/bin:/bin MUSTER_DIR="$dir/state" RUST_LOG=info \
      CLAUDE_CONFIG_DIR="$dir/fixtures/claude" CODEX_HOME="$dir/fixtures/codex" SHELL="$here/fake-shell.sh" \
      "$bin/musterd" --relay "${2:-muster.linsheng.dev:9443}" --name FakeMac --kind desktop --no-vnet --terminal pty
    ;;
  muster) shift; MUSTER_DIR="$dir/state" exec "$bin/muster" "$@" ;;
  clean) rm -rf "$dir" && echo "removed $dir" ;;
  *) sed -n '2,18p' "$0" | sed 's/^# \{0,1\}//'; exit 2 ;;
esac
