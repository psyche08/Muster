#!/bin/sh
# Install the log collector (ADR 0228, 0229): plain HTTP on 127.0.0.1:9180,
# behind nginx, which serves muster-logs.linsheng.dev on the public 8443.
# nginx itself is configured by hand from nginx-muster-logs.conf (README).
set -eu
here=$(cd "$(dirname "$0")" && pwd)
src=$(cd "$here/../.." && pwd)
# cargo's output: $CARGO_TARGET_DIR when set (sudo drops it unless kept),
# else the workspace's own target/.
bin="${CARGO_TARGET_DIR:-$src/target}/release/muster-logs"
[ -x "$bin" ] || { echo "build first: cargo build --release -p muster-logs" >&2; exit 1; }
[ "$(id -u)" = 0 ] || { echo "run as root" >&2; exit 1; }
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
listen=$(sed -n 's/^LOGS_LISTEN=//p' /etc/muster/logs.env)
if command -v curl >/dev/null 2>&1; then
  tries=0
  until curl -fsS "http://$listen/healthz" >/dev/null 2>&1; do
    tries=$((tries + 1))
    [ "$tries" -lt 10 ] || { echo "collector not answering on $listen" >&2; exit 1; }
    sleep 1
  done
fi
systemctl --no-pager --lines=5 status muster-logs
echo "log collector: http://$listen (loopback). Next: nginx-muster-logs.conf, see README."
