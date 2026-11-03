#!/bin/sh
# Builds l4forwarder, checks a configuration and (re)loads it as a launchd
# user agent on this Mac. Run it on the machine that forwards; run it again
# to upgrade.
#
#   deploy/install.sh [config.json]
#
# A configuration given here replaces the installed one; without one the
# installed configuration is kept.
set -eu
cd "$(dirname "$0")/.."

LABEL=dev.linsheng.l4forwarder
BIN="$HOME/.local/bin/l4forwarder"
CONF="$HOME/.config/l4forwarder/config.json"
LOG="$HOME/Library/Logs/l4forwarder.log"
PLIST="$HOME/Library/LaunchAgents/$LABEL.plist"
DOMAIN="gui/$(id -u)"

NEW=${1:-$CONF}
[ -f "$NEW" ] || { echo "install: no configuration; pass one (see config.example.json)" >&2; exit 1; }
cargo build --release --locked
target/release/l4forwarder --config "$NEW" --check

launchctl bootout "$DOMAIN/$LABEL" 2>/dev/null || true
while launchctl print "$DOMAIN/$LABEL" >/dev/null 2>&1; do sleep 0.2; done

mkdir -p "$(dirname "$BIN")" "$(dirname "$CONF")" "$(dirname "$LOG")" "$(dirname "$PLIST")"
[ "$NEW" = "$CONF" ] || install -m 0600 "$NEW" "$CONF"
install -m 0755 target/release/l4forwarder "$BIN"
sed -e "s|@LABEL@|$LABEL|" -e "s|@BIN@|$BIN|" -e "s|@CONF@|$CONF|" -e "s|@LOG@|$LOG|" \
  deploy/l4forwarder.plist.in >"$PLIST"
launchctl bootstrap "$DOMAIN" "$PLIST"
echo "installed $LABEL; status: launchctl print $DOMAIN/$LABEL; log: $LOG"
