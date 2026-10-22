#!/bin/sh
# Install a validated certificate generation, atomically select it, restart.
# Called by Certbot only for this lineage, or by install.sh with --no-restart.
set -eu
host="${1:?usage: renew-cert.sh <relay hostname> <certificate directory> [--no-restart]}"
cert_dir="${2:?certificate directory required}"
mode="${3:-}"
[ "$(id -u)" = 0 ] || { echo "run as root" >&2; exit 1; }
case "$host" in *[!A-Za-z0-9.-]*|'') echo "invalid hostname" >&2; exit 1;; esac
[ -r "$cert_dir/fullchain.pem" ] && [ -r "$cert_dir/privkey.pem" ] || { echo "certificate files unreadable" >&2; exit 1; }
# These print only validity/name checks, never key material.
openssl x509 -in "$cert_dir/fullchain.pem" -noout -checkhost "$host"
openssl x509 -in "$cert_dir/fullchain.pem" -noout -checkend 0
/opt/muster/bin/muster-relay --cert "$cert_dir/fullchain.pem" --key "$cert_dir/privkey.pem" --check-config
base=/etc/muster/relay-tls
install -d -m 0750 -o root -g muster-relay "$base"
stage=$(mktemp -d "$base/generation.XXXXXXXX")
trap 'rm -f "$base/current.next"; [ -z "${stage:-}" ] || rm -rf "$stage"' EXIT HUP INT TERM
chown root:muster-relay "$stage"
chmod 0750 "$stage"
install -m 0644 -o root -g muster-relay "$cert_dir/fullchain.pem" "$stage/fullchain.pem"
install -m 0640 -o root -g muster-relay "$cert_dir/privkey.pem" "$stage/privkey.pem"
previous=$(readlink "$base/current" 2>/dev/null || true)
ln -s "$(basename "$stage")" "$base/current.next"
mv -Tf "$base/current.next" "$base/current"
new_generation=$stage
stage=
if [ "$mode" != --no-restart ]; then
  if ! systemctl restart muster-relay; then
    echo "relay restart failed; restoring previous certificate selection" >&2
    if [ -n "$previous" ]; then
      ln -s "$previous" "$base/current.next"
      mv -Tf "$base/current.next" "$base/current"
      systemctl restart muster-relay || true
    fi
    exit 1
  fi
fi
# No old key copies retained after successful certificate rotation.
case "$previous" in generation.*) [ "$base/$previous" = "$new_generation" ] || rm -rf "$base/$previous";; esac
echo "relay certificate installed for $host (TCP/TLS + UDP/QUIC 9443)"
