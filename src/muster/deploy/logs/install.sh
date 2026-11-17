#!/bin/sh
# Install the log collector beside the relay (ADR 0228). It serves HTTPS on
# TCP 9444 with the relay's certificate, so deploy/relay/install.sh comes first.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
src=$(cd "$here/../.." && pwd)
# cargo's output: $CARGO_TARGET_DIR when set (sudo drops it unless kept),
# else the workspace's own target/.
bin="${CARGO_TARGET_DIR:-$src/target}/release/muster-logs"
[ -x "$bin" ] || { echo "build first: cargo build --release -p muster-logs" >&2; exit 1; }
[ "$(id -u)" = 0 ] || { echo "run as root" >&2; exit 1; }
tls=/etc/muster/relay-tls/current
if ! id muster-relay >/dev/null 2>&1 || [ ! -r "$tls/fullchain.pem" ]; then
  echo "install the relay and its certificate first (deploy/relay/install.sh)" >&2
  exit 1
fi
# Fail before replacing the executable if the certificate/key is invalid.
"$bin" check-config --cert "$tls/fullchain.pem" --key "$tls/privkey.pem"
install -d /opt/muster/bin /etc/muster
id muster-logs >/dev/null 2>&1 || useradd --system --home /nonexistent --shell /usr/sbin/nologin muster-logs
# Replace atomically; never overwrite an executing ELF file in place.
install -m 0755 "$bin" /opt/muster/bin/muster-logs.new
mv -f /opt/muster/bin/muster-logs.new /opt/muster/bin/muster-logs
if [ ! -f /etc/muster/logs.env ]; then
  install -m 0640 -o root -g muster-logs "$here/muster-logs.env" /etc/muster/logs.env
fi
install -m 0644 "$here/muster-logs.service" /etc/systemd/system/muster-logs.service
systemctl daemon-reload
systemctl enable muster-logs
systemctl restart muster-logs
if command -v firewall-cmd >/dev/null 2>&1 && firewall-cmd --state >/dev/null 2>&1; then
  firewall-cmd --permanent --add-port=9444/tcp >/dev/null
  firewall-cmd --reload >/dev/null
fi
systemctl --no-pager --lines=5 status muster-logs
echo "log collector: TCP 9444 (HTTPS, POST /v1/logs, GET /healthz)"
