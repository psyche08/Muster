# Server TLS pre-expiry renewal and overlapping generations

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](../../PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

2026-09-13. Scope: the existing AgentHalo V2 server and client; preserve Owner,
pairing, seven-day certificate limits and hard expiry. No QR or management-page
authentication is involved in routine renewal.

## Implementation

- Shared selects renewal inside a 60-minute overlap (updated from 30 seconds at
  the user's request). The verified
  observation supplies the scheduling effect; macOS performs hardware and exact
  storage effects only. The one-second supervisor executes a due effect without
  stopping the listener. Certificate ingress also checks immediately, so a client
  reaching the overlap before the worker receives a renewed server certificate.
- Server renewal and client certificate exchange share the existing transaction
  lane and exact compare-and-swap/readback mechanism. Before-write failure leaves
  the current generation running; uncertain storage writes still fail closed.
- Shared accepts only a contiguous, signed, same-hardware server renewal while
  both certificates are live. Changed Owner/interaction, rollback, omitted
  history, revocation and missed overlap remain terminal. Peer restrictions keep
  shrinking; server renewal does not grant new access to old connections.
- The signed observer builds a complete TLS server and MST2 resolver generation,
  checks exact storage again, then swaps the generation atomically. OS preparation
  failure retries with the still-live old generation without extending its expiry.
- Clients select their already verified certificate with a public hash in the
  synthetic TLS server name. Shared parses the selector; exact DER/SPKI and TLS
  signature checks remain mandatory. The server retains at most current and
  immediately previous certificate generations for new handshakes. Unknown pins
  fail; legacy clients without the selector use the default current certificate.
- ClientHello reads are bounded by five seconds and 64 KiB, with lifecycle checks
  between reads. TLS and MST2 always use the same captured generation. Existing
  connections are not retargeted. Old credentials/connections retain their original
  hard expiry; this is not permission to keep a seven-day-old session alive forever.
- Client certificate planning also notices the server overlap and reconciles its
  exact stored request to obtain the renewed public server certificate. Grant
  lifetime is unchanged; a renewal that cannot extend a live certificate is refused.

## Software verification

- The 60-minute update reran all 324 Shared tests and five actual TLS generation
  tests successfully. Signed renewal at exactly 3600 seconds remaining succeeds;
  3601 seconds is outside the window. Three old short-fixture assertions were
  updated to the new window while preserving the signed excessive-overlap rejection.

- Shared transport: 324 passed. Includes exact overlap boundary, signed server
  renewal continuity across the old expiry, missed overlap, rollback and skipped
  history rejection, and bounded selector parsing. First sandbox execution had
  nine socket-denied failures; the network-enabled full rerun passed.
- Backend: 453 passed, 14 existing ignored. A new real TLS test uses one listener
  for established old-generation traffic, new-pin connections and old-pin new
  connections after a swap, and checks actual MST2 resolution. These generations
  are software-selected; signed same-key succession is verified separately by
  Shared tests, not falsely claimed as one physical-device test.
- FFI: 104 passed. macOS production composition compiled after adding the cached
  Shared scheduling effect. No ABI layout change and no Keychain unlocking.
- Signing, installation and physical two-Mac renewal are separate gates. A current
  seven-day certificate does not enter its renewal window during this short run;
  do not alter system time, saved certificates or pairing merely to force it.

## Signed deployment at 19:41 Beijing time

Superseded by the 60-minute build below; this section retains the original deployment evidence.

- Both Macs installed clean source `f8b0ac20369c2a9283e7d1b0e282b781c673107a`.
  Developer ID signing, strict deep signature validation, App/Shared runtime
  comparison (ABI 1.87), AppKit SIGTERM regression and broker software checks
  passed. Not notarized.
- Package `/private/tmp/AgentHalo-hotrenew-f8b0ac2.zip`, SHA-256
  `ea5d4c145efbc8ddca375c17f1aa6ad601f6c4249e20421ec1e23d80e32893ad`.
  Installed GUI SHA-256 on both hosts
  `2248d3f512a3eb0e6f329ed5799d197628b7fdb84a99dda976365f14668815a4`;
  backend `75b31f7ac1276d7b744c304ae658db8bb4fd5e8d43fac94033a9ba15b7f0e924`.
- Backups: local `/Applications/AgentHalo.app.backup-20260913-194102`, mac-c
  `/Applications/AgentHalo.app.backup-20260913-194141`. Normal termination only;
  pairing preserved. mac-c management processes 50699/50724 still run from
  September 12 15:04:08. mac-b untouched.
- Both installed signed Apps returned `read_completed` from the real broker;
  the previously reviewed ordinary same-user probe was rejected with
  `backend identifier mismatch`. Both backend health endpoints returned `ok=true`.
- Local App started with mac-c Offline during initialization, then automatically
  showed its 22 sessions. The management authentication sheet was closed before
  upgrade, and no new authentication or pairing was requested for this recovery.
- Actual public mac-c certificate remains September 13 10:08:38 through
  September 20 10:08:38 UTC (604800 seconds), fingerprint
  `example-certificate-fingerprint`.
  It is not yet due. Physical pre-expiry rotation is therefore not claimed.
- Automatic approval review blocked opening the previously used private history
  session `Clone sample git repositories locally`; explicit read-only approval was
  requested and remains pending at this record. History content was not opened.
  A combined log/certificate probe was also blocked for its log payload; the
  permitted retry retrieved only the public certificate and known process status.

## 60-minute build deployment at 19:52 Beijing time

- Both Macs installed `b591452acd2b220a38a2a4cb13c115650f3b5e28`, Developer ID
  Release / Shared ABI 1.87. Strict signatures, App/Shared binary consistency,
  AppKit termination and broker software checks passed. Not notarized.
- Package `/private/tmp/AgentHalo-renew60-b591452.zip`, SHA-256
  `bdd405fe16cc7db81db55df5d34213a59f8cc1bb597da841da24233ad3a96b13`.
  Both installed GUI hashes are
  `b2c75bc98ac47f167ec5ed6d2539f7a58f722951907394d28aea7863bd30f9c1`;
  backend hashes `7b9e657ba72aa7c8ccd42df0b170c6f8acb0d9b2d84d6724961c825febd95dbb`.
- Normal install with backups: local
  `/Applications/AgentHalo.app.backup-20260913-195230`, mac-c
  `/Applications/AgentHalo.app.backup-20260913-195246`. No pairing reset or force kill.
- Both actual signed Apps returned `read_completed`; ordinary same-user probes
  were rejected with `backend identifier mismatch`. Both backends returned
  `ok=true`. Private session histories and backend logs were not read.
- The window is 3600 seconds on both client and server, while individual
  certificate lifetime remains at most seven days and is still Grant-bounded.
  Physical renewal at the actual future expiry has not been observed.
- After local startup, mac-c automatically recovered from Offline to 22 sessions
  without management authentication or re-pairing. No history session was opened.
