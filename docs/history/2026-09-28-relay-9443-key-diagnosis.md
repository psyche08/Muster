# Mac-A Key Display and Public Port 9443 Inspection (2026-09-28)

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](../PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

The user confirmed that `muster.linsheng.dev` **only exposes public port 9443**. The earlier assessment that an unreachable public port 443 was
a deployment failure is withdrawn. This inspection did not require opening port 443 and did not modify the server, router, proxy, or keys.
All times below are Asia/Shanghai.

## Incorrect Source for the Key Display

The fingerprint in the screenshot, `GUEP-X6MF-6FUR-UXBK`, matches the local daemon's `3508fbf9…`.
The startup log at 15:15:08 explicitly says `musterd 3508fbf9 (sep) as Mac-A`.
`sep` comes from the actual `HwSigner::backend()`, not an inference from build features.

The old UI checked whether `MusterModel.keys.devicePublicKey` existed, but the macOS client uses
the daemon's identity over UDS. The App's own key provider cannot represent the daemon's device key.
The software label in the screenshot therefore does not prove that this network identity uses a software key.

Fix: DeviceInfo now includes `key_backend`, reported by the daemon's actual signer and passed unchanged through the Rust
ViewModel to the UI. Older services that do not report it display `not reported`, rather than defaulting to software.
This field is for display only; it is not remote hardware attestation and does not participate in authorization. No identities or pairing data were replaced.

## UDP 9443

- DoH returned `203.0.113.10` and one AAAA record. The current client uses an IPv4 UDP socket and selects the A
  record; this inspection does not attribute failure of this IPv4 session to the unverified AAAA record.
- The server's `muster-relay` listens on `0.0.0.0:9443/udp`; it has run continuously since September 26, with a restart count of 0.
- Mac-A registered successfully at 15:15:08; its keepalive exceeded the 3-second deadline at 15:15:21; the server expired
  the identity at 15:17:15. Server authentication-failure and rate-limit counters remained 0 during this period, followed by dropped forwarding traffic.
- Registration recovered at 15:30:46; another keepalive timeout occurred at 15:31:30. The reflected address was
  `198.51.100.7:13344` at 15:33:13, then changed to `198.51.100.37:6585` at 15:34:06, with
  `not registered` received in between. The protocol requires keepalives to come from the currently registered source address; an address change triggers re-registration.
- An independent temporary-identity probe registered in 34 ms; three keepalives at 10-second intervals took 22/19/16 ms.
  This shows that UDP 9443 was reachable at that time, not that the production client was stable over the long term.

Conclusion: failed in the screenshot represents an actual UDP registration/keepalive failure, not a domain TLS certificate validation failure.
Egress-address changes and intermittent loss were observed, but these alone cannot identify a particular proxy, NAT, or ISP as the cause.
Why the production client received no replies on the same mapping for an extended period still requires comparing packet headers from both ends. Server packet capture
requires sudo, and current passwordless privileges are insufficient. No capture was performed, and no password was requested or handled.

## Certificate and Protocol Boundaries

UDP relay registration uses a device-signed challenge and does not terminate domain TLS at the relay; forwarded QUIC
connections use end-to-end device identity authentication. Changing nginx's domain certificate cannot repair this UDP keepalive path.

The server's internal nginx WSS configuration uses `/etc/letsencrypt/live/linsheng.dev/fullchain.pem`,
forwarding to `127.0.0.1:9444`. Local SNI/domain validation passed and WSS Upgrade returned 101:

- Issuer: Let's Encrypt YR2; SANs: `*.linsheng.dev`, `linsheng.dev`.
- Valid from 2026-08-27 05:17:02 UTC to 2026-11-25 05:17:01 UTC.
- SHA-256 `61:AC:CA:BC:E9:AE:1D:6B:3A:BC:AE:0C:2A:FA:1B:5B:1B:38:6E:B3:FB:65:51:1C:7F:20:A3:9C:6C:43:79:4B`.

This internal WSS result does not establish that WSS is publicly exposed. After connecting to public TCP 9443, the TLS handshake was closed,
and a plaintext WebSocket Upgrade also received an empty reply. No certificate was obtained from that port, so public
`wss://muster.linsheng.dev:9443` cannot be claimed to be configured or available. TCP and UDP are separate entry points, and the current
client configuration `muster.linsheng.dev:9443` explicitly uses UDP.

## Fix Validation and Delivery Boundaries

- Added a real local RPC regression: when the stored identity differs from the injected signer, DeviceInfo must return
  the current signer's backend and corresponding device ID; the test passed.
- Clippy for musterd tests (warnings denied) and cargo check for app-core / FFI consumers passed.
- macOS / iOS Release UI compilation passed using the existing Rust FFI archive; this is a UI compilation check,
  not runtime validation of an entirely new artifact set. This round did not sign, notarize, or install, and did not change the running version.
- Server inspection and the UDP probe are independent evidence; the Relay's long-term stability has not been accepted, and no fix is claimed.
