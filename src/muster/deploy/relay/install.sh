#!/bin/sh
# Install the native TLS+QUIC relay. Only TCP/UDP 9443 is public; no proxy.
# First obtain a Let's Encrypt DNS-01 certificate covering the relay host.
set -eu
host="${1:?usage: install.sh <relay hostname>}"
case "$host" in *[!A-Za-z0-9.-]*|'') echo "invalid hostname" >&2; exit 1;; esac
here=$(cd "$(dirname "$0")" && pwd)
src=$(cd "$here/../.." && pwd)
bin="$src/target/release/muster-relay"
[ -x "$bin" ] || { echo "build first: cargo build --release -p muster-relay" >&2; exit 1; }
[ "$(id -u)" = 0 ] || { echo "run as root" >&2; exit 1; }
cert_dir="${CERT_DIR:-/etc/letsencrypt/live/$host}"
"$here/install-certbot-hook.sh" "$host" "$cert_dir" --print >/dev/null
# A specific ingress IP can coexist with an unrelated loopback :9443 listener.
listen="${RELAY_LISTEN:-0.0.0.0:9443}"
# Fail before replacing the executable if the certificate/key is invalid.
"$bin" --listen "$listen" --cert "$cert_dir/fullchain.pem" --key "$cert_dir/privkey.pem" --check-config
openssl x509 -in "$cert_dir/fullchain.pem" -noout -checkhost "$host"
openssl x509 -in "$cert_dir/fullchain.pem" -noout -checkend 0
install -d /opt/muster/bin /etc/muster
id muster-relay >/dev/null 2>&1 || useradd --system --home /nonexistent --shell /usr/sbin/nologin muster-relay
# Replace atomically; never overwrite an executing ELF file in place.
install -m 0755 "$bin" /opt/muster/bin/muster-relay.new
mv -f /opt/muster/bin/muster-relay.new /opt/muster/bin/muster-relay
install -m 0755 "$here/renew-cert.sh" /opt/muster/bin/muster-relay-renew-cert
/opt/muster/bin/muster-relay-renew-cert "$host" "$cert_dir" --no-restart
"$here/install-certbot-hook.sh" "$host" "$cert_dir"
if [ ! -f /etc/muster/relay.env ]; then
  install -m 0640 -o root -g muster-relay "$here/muster-relay.env" /etc/muster/relay.env
else
  # Preserve operator limits/push settings; obsolete UDP/WS vars are ignored.
  for setting in RELAY_LISTEN=0.0.0.0:9443 RELAY_CERT=/etc/muster/relay-tls/current/fullchain.pem RELAY_KEY=/etc/muster/relay-tls/current/privkey.pem; do
    name=${setting%%=*}
    if ! grep -q "^$name=" /etc/muster/relay.env; then printf '%s\n' "$setting" >> /etc/muster/relay.env; fi
  done
fi
if [ -n "${RELAY_LISTEN:-}" ]; then
  # Rust SocketAddr parsing above validated the explicit bind address.
  sed -i "s|^RELAY_LISTEN=.*|RELAY_LISTEN=$listen|" /etc/muster/relay.env
fi
install -m 0644 "$here/muster-relay.service" /etc/systemd/system/muster-relay.service
systemctl daemon-reload
systemctl enable muster-relay
systemctl restart muster-relay
if command -v firewall-cmd >/dev/null 2>&1 && firewall-cmd --state >/dev/null 2>&1; then
  firewall-cmd --permanent --add-port=9443/tcp >/dev/null
  firewall-cmd --permanent --add-port=9443/udp >/dev/null
  firewall-cmd --reload >/dev/null
fi
systemctl --no-pager --lines=5 status muster-relay
echo "relay: $host:9443 (TCP/TLS and UDP/QUIC)"
