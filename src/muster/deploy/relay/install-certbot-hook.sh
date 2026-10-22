#!/bin/sh
# Install only the renewal hook; do not copy keys or restart the relay.
set -eu
usage() { echo "usage: $0 <relay hostname> <certificate lineage> [--print]" >&2; exit 1; }
[ "$#" -ge 2 ] && [ "$#" -le 3 ] || usage
host=$1
lineage=$2
mode=${3:-}
case "$mode" in ''|--print) ;; *) usage;; esac
# Restrict interpolated values to literal DNS names and absolute paths.
case "$host" in ''|*[!A-Za-z0-9.-]*|.*|*.) echo "invalid hostname" >&2; exit 1;; esac
[ "${#host}" -le 253 ] || { echo "hostname too long" >&2; exit 1; }
printf '%s\n' "$host" | awk -F. '{ for (i = 1; i <= NF; i++) if (length($i) < 1 || length($i) > 63 || $i ~ /^-/ || $i ~ /-$/) exit 1 }' || {
  echo "invalid hostname label" >&2; exit 1;
}
case "$lineage" in
  /*) ;;
  *) echo "certificate lineage must be absolute" >&2; exit 1;;
esac
case "$lineage" in
  *[!A-Za-z0-9._/-]*|*/|*//*|*/./*|*/../*|*/.|*/..)
    echo "certificate lineage must be a normalized literal path" >&2; exit 1;;
esac
render() {
  cat <<EOF
#!/bin/sh
# Managed by Muster. Ignore renewals of every other certificate lineage.
set -eu
[ "\${RENEWED_LINEAGE:-}" = '$lineage' ] || exit 0
exec /opt/muster/bin/muster-relay-renew-cert '$host' '$lineage'
EOF
}
# A reviewable preview needs no root access and reads no certificates.
if [ "$mode" = --print ]; then render; exit 0; fi
[ "$(id -u)" = 0 ] || { echo "run as root" >&2; exit 1; }
[ -x /opt/muster/bin/muster-relay-renew-cert ] || { echo "install the relay renewal helper first" >&2; exit 1; }
[ -r "$lineage/fullchain.pem" ] && [ -r "$lineage/privkey.pem" ] || {
  echo "certificate files unreadable" >&2; exit 1;
}
openssl x509 -in "$lineage/fullchain.pem" -noout -checkhost "$host"
openssl x509 -in "$lineage/fullchain.pem" -noout -checkend 0
hook_dir=/etc/letsencrypt/renewal-hooks/deploy
install -d -m 0755 -o root -g root "$hook_dir"
# Stage outside the deploy directory so Certbot cannot execute a partial hook.
stage=$(mktemp "$hook_dir/../.muster-relay.XXXXXXXX")
trap 'rm -f "$stage"' EXIT HUP INT TERM
render > "$stage"
chown root:root "$stage"
chmod 0755 "$stage"
mv -f "$stage" "$hook_dir/50-muster-relay"
echo "Certbot deploy hook installed for $host, lineage $lineage (no restart)"
