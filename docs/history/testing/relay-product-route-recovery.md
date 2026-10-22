# Relay Product Route Recovery (2026-09-11)

## Causes and Fixes

1. The production user-level `musterd.service` was still running the old September 7 binary, lacking
   `/v2/product/register/agenthalo`. Both direct access to `127.0.0.1:7443` and the nginx
   HTTPS entry returned JSON 404, while legacy `/v2/register` correctly reached 401 authentication.
   This was therefore not an nginx forwarding error and did not require resetting pairing.
2. The existing product route implementation from source `24aec78d` was built as a Linux amd64 binary.
   After passing `-validate` and `-validate-pairing` with the original configuration, it atomically replaced the binary and restarted the user-level Relay.
   The configuration, token ledger, Owner directory, and pairing mailbox were not cleared or reconfigured.
3. Once the phone verified the service advertisement, another Shared connector compatibility issue emerged:
   production `muster local-device` publishes a Relay root address and a separate signed `relay_peer`,
   but the connector accepted only a complete device path. Shared now constructs the product path from the verified root address and signed peer;
   explicit paths must still match the signed peer, and query parameters and credentials remain rejected.
   The platform does not construct paths or infer trust state itself.
4. After the HarmonyOS route recovered, certificate exchange still encountered EOF. The signed Mac diagnostic backend recorded
   `certificate exchange rejected: certificate transaction busy`: the observer held the certificate-request
   serialization lock throughout the OS read, making normal requests likely to collide with the observer and be closed.
   The observer now holds only the target CAS baseline lock, leaving the certificate-request channel free; it retains double readback,
   lock-poison checks, the failure-retirement latch, and mutual exclusion between certificate requests. Failure retirement occurs before releasing the baseline lock;
   waiting certificate writes must still check retirement state and exact expected bytes.

## Deployment and Validation

- Relay binary SHA-256:
  `1080599638f02cbc64b494842773f96ff176ad34a8b965057ca6f98415bd1383`.
  `/opt/muster/bin/musterd` and the running `/proc/748102/exe` have matching hashes.
- After the 19:02 restart, both loopback and HTTPS routes returned anonymous 401 instead of 404.
  The ordinary tunnels of three existing devices reconnected automatically, followed by successful real Mac product registration.
- nginx, ntfy, and private-tunnel-relay retained PIDs 1627, 1618, and 1640 respectively,
  without restart or configuration changes. The only service changed was the user-level Muster Relay.
- Race regressions and vet passed for 7 relevant Relay Go packages.
- All 45 Shared pairing-related tests, Clippy, and all 94 FFI tests passed.
  The first FFI run hit sandbox Unix-socket restrictions; a full rerun with elevated execution permissions passed.
- The physical phone retained the original Owner pairing and confirmed the AgentHalo service advertisement after retrying the directory. It then reported
  `remote target Relay route differs from signed peer`, addressed by the Shared compatibility fix above.
- The HarmonyOS Shared fix was signed and installed with `install -r`; cold startup retained original data and the Debug local-authentication bypass.
  The physical-device error advanced to `certificate frame truncated`, no longer a Relay route mismatch.
- A Developer ID diagnostic Mac package updating only backend components was installed over the existing package; the original Debug UI was not rebuilt.
  The package is marked dirty and diagnostic, not represented as a complete new version. The main process still did not exit after TERM; after user authorization,
  the exact PID was force-quit before replacement, without deleting pairing data. The local health endpoint passed.
- Backend library regression: 397 passed, 14 ignored under the original test configuration; targeted listener-observation and certificate-entry regressions passed.
  New cases cover accepting a certificate request during observation while rejecting a second concurrent certificate request.
- The certificate-lock fix backend source is `91d9202`, signed again with Developer ID and installed over the existing package.
  The package's `AgentHaloBackendSourceCommit` separately identifies the backend version, without labeling the old UI as built from new source.
  Original pairing, HUKS, and Mac hardware identity were all retained.

## Pending Validation

- After the Mac lock fix was installed, the observer still hit the independent 2-second validity-window limit:
  logs showed `observation rejected; elapsed_ms=4713` (the TLS generation had already been released), and
  `observation expired; age_ms=2034 elapsed_ms=510`. These durations cannot all be attributed to
  a particular OS read; observation intervals, scheduling, and readback paths still need finer breakdowns.
- The protection window was not relaxed, and state, certificate, and Grant checks were not bypassed. The phone has confirmed pairing and live service advertisement,
  but final authenticated workspace reads and stable end-to-end sessions have not passed, so full availability cannot be claimed.
- At 19:35 and 19:36, both physical-device page observations showed `Loading remote device directory and workspace…` /
  `Refreshing…`, without returning devices or a workspace. Further investigation must also locate the wait/cancellation path for this read;
  backend health, directory advertisement, or 5/5 pairing status must not be treated as evidence that this gate has passed.
