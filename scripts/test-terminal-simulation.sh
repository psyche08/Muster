#!/usr/bin/env bash
# Native protocol/RPC fixtures and private PTY/tmux sessions; no native GUI,
# permission changes, installation or release. tmux must already be available.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
command -v tmux >/dev/null || { echo 'tmux is required for managed-terminal acceptance' >&2; exit 1; }
cd "$root/src/muster"
cargo test -p muster-terminal -p musterd -p muster-app-core
cargo test -p muster-terminal tmux::tests -- --ignored
cargo test -p musterd native_managed_window_uses_exact_handle_and_replay_opens_only_once -- --ignored
cargo clippy -p muster-terminal -p musterd -p muster-app-core --tests -- -D warnings
cargo fmt --all -- --check
cd "$root"
python3 src/muster/scripts/check-layers.py
python3 scripts/check-public-content.py
