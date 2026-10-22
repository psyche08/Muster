#!/bin/sh
# Install muster-apns beside the relay and turn on the relay's push hook.
#   cargo build --release -p muster-apns
#   sudo deploy/apns/install.sh <cert.pem> <key.pem>
# The certificate is "Apple Push Services: dev.linsheng.muster" (sandbox and
# production); README.md explains exporting it and its key as PEM.
set -eu
cert="${1:?usage: install.sh <cert.pem> <key.pem>}"
key="${2:?usage: install.sh <cert.pem> <key.pem>}"
here=$(cd "$(dirname "$0")" && pwd)
src=$(cd "$here/../.." && pwd)
for b in muster-apns muster-apns-push; do
  [ -x "$src/target/release/$b" ] || { echo "build first: cargo build --release -p muster-apns" >&2; exit 1; }
done
[ "$(id -u)" = 0 ] || { echo "run as root" >&2; exit 1; }
[ -f /etc/muster/relay.env ] || { echo "install the relay first (deploy/relay/install.sh)" >&2; exit 1; }
# Fail before touching anything if the pair is wrong or expired.
openssl x509 -in "$cert" -noout -subject | grep -q "Apple Push Services" \
  || { echo "$cert is not an Apple Push Services certificate" >&2; exit 1; }
openssl x509 -in "$cert" -noout -checkend 0 >/dev/null || { echo "$cert has expired" >&2; exit 1; }
[ "$(openssl x509 -in "$cert" -noout -pubkey)" = "$(openssl pkey -in "$key" -pubout)" ] \
  || { echo "$key does not match $cert" >&2; exit 1; }

getent group muster-push >/dev/null || groupadd --system muster-push
id muster-apns >/dev/null 2>&1 || useradd --system --home /nonexistent --shell /usr/sbin/nologin muster-apns
install -d -m 0755 /opt/muster/bin /etc/muster/apns
install -m 0755 "$src/target/release/muster-apns" /opt/muster/bin/muster-apns.new
install -m 0755 "$src/target/release/muster-apns-push" /opt/muster/bin/muster-apns-push
install -m 0444 -o root -g root "$cert" /etc/muster/apns/cert.pem
install -m 0400 -o muster-apns -g root "$key" /etc/muster/apns/key.pem

# APNs answers BadDeviceToken to the all-zero token only once it accepts the
# certificate; anything else (403, network) stops here with the old service.
probe=$(runuser -u muster-apns -- /opt/muster/bin/muster-apns.new probe \
  --cert /etc/muster/apns/cert.pem --key /etc/muster/apns/key.pem)
echo "$probe"
echo "$probe" | grep -q "^production: BadDeviceToken$" \
  || { rm -f /opt/muster/bin/muster-apns.new; echo "APNs did not accept the certificate" >&2; exit 1; }
mv -f /opt/muster/bin/muster-apns.new /opt/muster/bin/muster-apns

install -m 0644 "$here/muster-apns.service" /etc/systemd/system/muster-apns.service
install -d /etc/systemd/system/muster-relay.service.d
install -m 0644 "$here/relay-push.conf" /etc/systemd/system/muster-relay.service.d/50-muster-apns.conf
# Only an unset hook is filled in; an operator's own push settings stay.
sed -i 's|^RELAY_PUSH_ARGS=$|RELAY_PUSH_ARGS=--push-command /opt/muster/bin/muster-apns-push --push-interval-secs 600|' /etc/muster/relay.env
grep '^RELAY_PUSH_ARGS=' /etc/muster/relay.env
systemctl daemon-reload
systemctl enable muster-apns
systemctl restart muster-apns
# The relay takes the group and the hook only on restart; clients reconnect.
systemctl restart muster-relay
systemctl --no-pager --lines=5 status muster-apns muster-relay
