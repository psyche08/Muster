# Deploying muster-apns

`muster-apns` wakes an offline iPhone (or Mac) through APNs when a member's
traffic reaches the relay for it. The relay already keeps each device's push
token and runs its `--push-command` as `<device id hex> <kind> <token hex>`;
`muster-apns-push` is that command. It passes the request over
`/run/muster-apns/push.sock` to the `muster-apns` service, which sends a
silent background push (`content-available`, priority 5, collapse id
`muster-wake`, 10 minute expiry) to topic `dev.linsheng.muster`.

- Outbound only: HTTP/2 to `api.push.apple.com:2197` and
  `api.sandbox.push.apple.com:2197`, one kept-alive connection each. No port
  is opened on the host.
- Only `muster-apns` reads the provider key (`/etc/muster/apns/key.pem`, owner
  `muster-apns`, 0400). The relay joins group `muster-push`, which opens the
  socket and nothing else.
- Production is tried first; a `BadDeviceToken` there is retried on the
  sandbox (development builds), and the environment that took a token is
  remembered until the service restarts.

## The provider certificate

Authentication uses the "Apple Push Services: dev.linsheng.muster"
certificate (sandbox and production, valid one year). Its private key was
created with the CSR in the login keychain of the Mac that requested it. On
that Mac, export both yourself into the ignored `keys/`: Keychain Access
(`/System/Library/CoreServices/Applications`) › login › My Certificates ›
"Apple Push Services: dev.linsheng.muster" › Export… as a `.p12` with a
password. "Not trusted" there only means the WWDR G4 intermediate is not
installed on that Mac; APNs does not need it.

Then, from this workspace on the Mac:

```sh
deploy/apns/upload-key.sh ../../keys/apns_20260929.p12 linsheng.dev
```

It asks for the `.p12` password once, splits the export into the PEM pair
(with `-legacy` on OpenSSL 3: Keychain Access encrypts the certificate with
RC2-40), checks it, and runs `probe` with it: the all-zero token must come
back `BadDeviceToken`, or nothing is uploaded (403 means APNs refused the
certificate). The pair goes to `~/muster-apns/` on the host, mode 0600; the
plaintext key on the Mac exists only in a temporary directory.

Without `--cert`, `probe` reports `MissingProviderToken`: APNs is reachable
but the request is anonymous.

## Installation

On the relay host, from this workspace:

```sh
cargo build --release -p muster-apns
sudo deploy/apns/install.sh ~/muster-apns/cert.pem ~/muster-apns/key.pem
shred -u ~/muster-apns/key.pem
```

The installer checks that the pair matches and has not expired, installs
both programs under `/opt/muster/bin`, the pair under `/etc/muster/apns`,
and runs `probe` as `muster-apns` before replacing the service: a
certificate APNs refuses stops the installation. It then installs
`muster-apns.service`, the relay drop-in `relay-push.conf`, fills in an empty
`RELAY_PUSH_ARGS` in `/etc/muster/relay.env` (an operator's own value is
kept), and restarts both services; relay clients reconnect.

`journalctl -u muster-apns` logs each push as `push <device>: delivered
(production)` or with APNs's reason. `Unregistered` means the app is gone
from that device or its notifications are off.

The certificate expires a year after issue (`openssl x509 -in
/etc/muster/apns/cert.pem -noout -enddate`); renew it in the developer
portal and run the installer again with the new pair.
