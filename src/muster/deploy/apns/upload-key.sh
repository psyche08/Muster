#!/bin/sh
# On the Mac that exported the certificate: turn the Keychain Access export
# into the PEM pair muster-apns uses, prove it against APNs, and copy it to
# the relay host for install.sh. openssl asks for the .p12 password once.
# Here the plaintext key lives only in a private temporary directory.
#   deploy/apns/upload-key.sh <export.p12> <ssh host>
set -eu
p12="${1:?usage: upload-key.sh <export.p12> <ssh host>}"
host="${2:?usage: upload-key.sh <export.p12> <ssh host>}"
here=$(cd "$(dirname "$0")" && pwd)
src=$(cd "$here/../.." && pwd)
umask 077
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# Keychain Access encrypts the certificate with RC2-40, which OpenSSL 3
# decrypts only with its legacy provider (LibreSSL needs no flag).
legacy=""
openssl version | grep -q '^OpenSSL 3' && legacy="-legacy"
# shellcheck disable=SC2086
openssl pkcs12 $legacy -in "$p12" -clcerts -nodes -out "$work/both.pem"
openssl x509 -in "$work/both.pem" -out "$work/cert.pem"
openssl pkey -in "$work/both.pem" -out "$work/key.pem"
openssl x509 -in "$work/cert.pem" -noout -subject -enddate
openssl x509 -in "$work/cert.pem" -noout -subject | grep -q "Apple Push Services" \
  || { echo "not an Apple Push Services certificate" >&2; exit 1; }
[ "$(openssl x509 -in "$work/cert.pem" -noout -pubkey)" = "$(openssl pkey -in "$work/key.pem" -pubout)" ] \
  || { echo "the key does not match the certificate" >&2; exit 1; }

# APNs answers BadDeviceToken to the all-zero token only once it accepts the
# certificate; a 403 here stops before anything leaves this Mac.
probe=$(cd "$src" && cargo run -q -p muster-apns -- probe --cert "$work/cert.pem" --key "$work/key.pem")
echo "$probe"
echo "$probe" | grep -q "^production: BadDeviceToken$" \
  || { echo "APNs did not accept the certificate; nothing uploaded" >&2; exit 1; }

ssh "$host" 'umask 077 && mkdir -p muster-apns'
scp -q "$work/cert.pem" "$work/key.pem" "$host:muster-apns/"
ssh "$host" 'chmod 600 muster-apns/cert.pem muster-apns/key.pem && ls -l muster-apns/*.pem'
echo "uploaded to $host:~/muster-apns; on the host run deploy/apns/install.sh as root with these two files"
