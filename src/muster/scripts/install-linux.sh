#!/bin/sh
# Installs musterd, muster and (optionally) the relay on a Linux server.
#   sudo MUSTER_RELAY=relay.example.com:9443 MUSTER_NAME=box scripts/install-linux.sh
# Then: muster --socket /var/lib/muster/musterd.sock join "<invite>"   (or create)
# sshd: add "AuthorizedKeysFile .ssh/authorized_keys /etc/muster/authorized_keys"
set -eu
cd "$(dirname "$0")/.."
[ "$(id -u)" = 0 ] || { echo "run as root"; exit 2; }
: "${MUSTER_RELAY:?set MUSTER_RELAY=host:port}"
: "${MUSTER_NAME:=$(hostname -s)}"
cargo build --release -p musterd -p muster-cli -p muster-relay
install -d /opt/muster/bin /var/lib/muster /etc/muster
install -m 0755 target/release/musterd target/release/muster target/release/muster-relay /opt/muster/bin/
ln -sf /opt/muster/bin/muster /usr/local/bin/muster
printf 'MUSTER_RELAY=%s\nMUSTER_NAME=%s\n' "$MUSTER_RELAY" "$MUSTER_NAME" > /etc/muster/musterd.env
install -m 0644 deploy/linux/musterd.service /etc/systemd/system/musterd.service
systemctl daemon-reload
systemctl enable --now musterd
echo "musterd running; socket /var/lib/muster/musterd.sock"
