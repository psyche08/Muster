# private-tunnel Hardware-Rooted Identity Migration · Phased Implementation Plan

> Privacy note: operational identities in this record are anonymized examples; see [public content policy](../../../PUBLIC_CONTENT.md). Technical outcomes and acceptance limits are unchanged.

**Date:** 2026-08-13
**Design specification:** [../specs/2026-08-13-private-tunnel-hardware-identity.md](../specs/2026-08-13-private-tunnel-hardware-identity.md)
**Target machines:** relay (Linux), mac-a, mac-b, mac-c, jetson
**Principles:** Each phase can ship and roll back independently; establish the foundation before adding hardware; prioritize server-side enforcement over client-side rejection.

---

## Phase Overview

| Phase | Scope | Blocks the next phase? | Hardware dependencies |
|---|---|---|---|
| **P0** | DER identities + device registry + capability gate + **effective revocation** + CSWSH/CSRF fixes + Unix socket | Yes | None |
| **P1** | Two-tier PKI + off-host `pt-ca` + 30-day leaf certificates + CRL | Yes | None |
| **P2** | `internal/hwkey` abstraction + hard compile-time/runtime rejection + signer TLS | Yes | None (`pt_testkey`) |
| **P3** | Enrollment service + approval ceremony + audit chain + automatic renewal (fake attestor) | Yes | None |
| **P4** | Split nginx into two vhosts + WebAuthn session layer | No (can run alongside P5) | Browser with a passkey |
| **P5** | Real attestation: Apple SEP signer + pure-Go PIV verifier | No | Apple silicon Mac |
| **P6** | Platform migration: move macOS agents to the host / downgrade jetson / retire `.p12` / `enforce: deny` | — | Entire fleet |
| **P7** | Windows TPM 2.0（PCP + tpm2 attestation） | — | Win11 + TPM |

**Prerequisite before P0** (no code changes):

- **Test the prerequisites for two nginx vhosts**: whether the relay host’s LE certificate can add the `mtls.<domain>` SAN, and whether the current nginx setup accepts a second server block reusing the same certificate.

> Both previously open decisions are now final (2026-08-13):
> **Move agents back to the host**; rule 4 in `private-tunnel/CLAUDE.md` has been revised (host agents must be registered under `tools/private-services/`; ad hoc launchd plists are prohibited);
> **Decommission jetson**; see specification §3.2 and P6 of this plan for the checklist.

> The decision to **decommission jetson** is final (2026-08-13), so no hardware solution needs to be selected for it. See specification §3.2 and P6 of this plan for the checklist.

```bash
ssh relay 'sudo nginx -T | grep -c "server_name"; openssl x509 -in /etc/letsencrypt/live/<domain>/fullchain.pem -noout -text | grep -A2 "Subject Alternative Name"'
```

---

## P0 — Foundation: DER Identities + Registry + Capability Gate + Effective Revocation

This phase delivers security improvements without hardware. **P0 should ship independently even if none of the later phases happen**: it fixes vulnerabilities that are exploitable today.

### Work

- **See the relay direct mTLS decision memo, `2026-08-13-relay-direct-tls-ingress.md` (historical reference; not included in this repository) for the ingress topology**. Principle: **add, do not replace**. Machine traffic uses a direct mTLS listener on a new port; browser traffic stays on nginx:8443. Keep both paths until end-to-end validation passes.
- **`internal/tlsutil`**: add `IdentityFromCert(*x509.Certificate)`; direct connections provide `*x509.Certificate` without header parsing. `IdentityFromDN` and `ParseEscapedCertHeader` are only for browser traffic still routed through nginx.
- **Per-listener trust mode** (a strict prerequisite for coexistence): `trust_proxy_headers` is currently **process-wide** and controls `ListenAndServe` versus `ListenAndServeTLS`, whether `TLSConfig` is built, and how `identityFromRequest` derives identity. Pass the trust mode of the **listener that accepted the connection** through to the Dispatcher.
  - ⚠ **Do not implement coexistence with two processes**: `Registry` (`registry.go:36-40`) and `NetworkRouter` both live in process memory. Two processes create split-brain behavior (browsers on the new listener cannot reach agents still connected to the old one), and both contend for the same `/var/lib/private-tunnel`.
- **Fix the empty-host vulnerability in `isLoopbackAddr`** (already present in production, independent of the ingress design): it returns true when the host from `net.SplitHostPort(":8443")` is empty, allowing `listen.addr: ":8443"` with `trust_proxy_headers: true` to pass `Validate()`. That guard is intended to prevent forged `X-Client-S-DN` values on non-loopback listeners.
- **`internal/devstore`** (new): records + denylist + 2-second mtime cache + serial LRU + **fail closed on read errors**.
- **`internal/relay/devicegate.go`** (new): integrate into `dispatch.go` / `handleTunnel` / `handleNetwork` / `fanout.go` / `connector.go`.
- **Make revocation effective**: `pt-relay revoke` writes the denylist (effective within ≤2s), synchronously closes the device’s yamux session, and removes its virtual network address.
- **Delete** `config.LocalForwardConfig.TargetUser` and the cross-tenant path at `connector.go:50-58`; **delete** the user-cert preference and glob fallback at `remote-agent/internal/relaycert/relaycert.go:36-48`.
- **CSWSH/CSRF fixes**: add an Origin allowlist at `proxy.go:105-123`; implement real validation in both `CheckOrigin` functions at `server.go:224-237`; require CSRF tokens for all state-changing POST requests (preparing for cookies in P4).
- `pt-relay devices import`: seed the registry from the six production certificates and `certs/.inventory.tsv` (**required for a rollout without downtime**).
- Configuration: `enroll.enforce: off|warn|deny`; **ship `warn` first**.

### Validation

```bash
cd private-tunnel && go vet ./... && go test ./... && make integration
go test ./test/integration/ -run 'DeviceGate|ReverseProxy|Origin' -v
```

Critical regression checks that must all pass:

- Valid CA certificate with an SPKI absent from the registry → `403 unknown_spki`
- Same certificate after `devices import` → 200
- Active yamux session closes **within 1 second** of revocation
- Browser ingress (still through nginx) has `X-Client-Verify`/`S-DN` but **no** `X-Client-Cert` → **503** (not 200)
- Direct machine ingress: unregistered SPKI → **TLS handshake failure** (not 403), with SPKI/serial/SNI/reason in server logs
- `listen.addr: ":8443"` + `trust_proxy_headers: true` → `-validate` **must fail**
- Agent certificate accessing `/_pt/enroll/pending` → 403
- `target_user` in `agent.yaml` → `agent -validate` fails
- WebSocket upgrade with a cross-site Origin → 403

```bash
ssh relay 'sudo nginx -t && ls -l /run/private-tunnel/relay.sock'   # Expect srw-rw---- private-tunnel:nginx
bash scripts/smoke-relay-services.sh                                # On the relay host: expected to **fail** now
ssh relay "journalctl -u private-tunnel-relay --since -48h | grep -c 'devicegate: would-deny'"   # Observe warn for 48h; must be 0
```

### Completion Criteria

The `would-deny` count stays at 0 for 48 hours in `warn` mode; all six devices are in the registry; the revocation drill succeeds.

### Rollback

Set `enroll.enforce: off` and restore the nginx upstream to `127.0.0.1:9443`. Read the registry without enforcing it.

---

## P1 — Two-Tier PKI + Off-Host `pt-ca` + 30-Day Leaf Certificates + CRL

### Work

- `internal/pki`: issuer / oid (generate `2.25.<uuid>` once and hard-code it) / crl / profile enforcement.
- `cmd/pt-ca`: `init-root`, `issue-ica --profile hw`, `bundle`, `sign` (independently revalidate attestation and approver assertions), `crl`, `emergency-issue`, `issue-local`.
- Keep `scripts/certs.sh` / `cert-admin.sh`, but add deprecation banners.
- Add `ssl_crl` to nginx and one sudoers rule for `pt-relay crl-reload`.

### Validation

```bash
cd private-tunnel && go test ./internal/pki/... -v
go run ./cmd/pt-ca init-root --out /tmp/ptca
go run ./cmd/pt-ca issue-ica --profile hw --out /tmp/ptca
go run ./cmd/pt-ca bundle --out /tmp/ptca/ca_chain.crt
openssl verify -CAfile /tmp/ptca/ca_chain.crt /tmp/ptca/leaf.crt
openssl x509 -in /tmp/ptca/leaf.crt -noout -text | grep -E 'Policy: 2\.25|CA:FALSE|Digital Signature|TLS Web Client Auth|Not After'
ssh relay 'sudo nginx -t'
```

### Completion Criteria

The new chain can sign and verify certificates; CRL generation works and `nginx -t` passes; complete one `emergency-issue` drill.

---

## P2 — `internal/hwkey` Abstraction + Hard Rejection + Signer TLS

### Work

- `hwkey.go` / `require/` / `fake.go` (`pt_testkey`). **D1 explicitly forbids a `pt_softkey` escape hatch.**
- `tlsutil.ClientTLSFromSigner`: `GetClientCertificate` + **pin** `SupportedSignatureAlgorithms = {ECDSAWithP256AndSHA256}`.
- Add `key_source` (`apple-sep|windows-pcp|piv|remote-signer`) and `key_label` to `config.TLS`; `Validate()` no longer requires `key_file` for non-file sources.
- `cmd/agent` prints the reason and calls `os.Exit(3)` if no provider is available.

### Validation

```bash
cd private-tunnel
go test -tags pt_testkey ./internal/hwkey/... ./internal/tlsutil/... -v
# Required: complete mutual TLS1.2 and TLS1.3 handshakes using only crypto.Signer
# Required: client still selects P256/SHA256 when the server prefers ECDSAWithP384AndSHA384
make verify-deny
```

Implementation of `verify-deny`:

```bash
! CGO_ENABLED=0 GOOS=linux GOARCH=arm64 go build ./cmd/agent 2>&1 \
    | grep -q 'undefined: pt_hardware_key_backend_unavailable_for_this_platform' && exit 1
CGO_ENABLED=0 GOOS=windows GOARCH=amd64 go build ./cmd/agent
CGO_ENABLED=0 GOOS=darwin  GOARCH=arm64 go build ./cmd/agent
go list -deps ./cmd/relay | grep -q internal/hwkey && exit 1
CGO_ENABLED=0 GOOS=linux GOARCH=amd64 go build ./cmd/relay
```

### Completion Criteria

An agent build on Linux without tags **must fail** with the exact identifier in the error message; the relay dependency graph must not contain `internal/hwkey`.

---

## P3 — Enrollment Service + Approval Ceremony + Audit Chain + Automatic Renewal

### Work

- Implement all of `internal/enroll`; relay runs a second `http.Server` on `:8444` (self-managed self-signed certificate, client SPKI pinning).
- Add `/_pt/enroll/*` to the allowlist in `route.go` (`rootStaticPaths` is a closed set; explicitly add the approval page).
- `cmd/pt-enroll`, `cmd/pt-approve`.
- Add `push_proxy` to relay push (**FCM is blocked; without this, iOS cannot receive approval notifications**; see `remote-agent/internal/api/push.go:396`).
- Add an `enroll` event to `/_pt/devices/stream`.
- Agent renewal goroutine (`GetClientCertificate` swaps certificates atomically).
- `pt-relay bootstrap` (blocking + manually enter the first eight fingerprint characters).

### Validation

```bash
cd private-tunnel && go test ./internal/enroll/... -v
go test -tags='integration pt_testkey' ./test/integration/ -run TestEnroll -v
```

Required negative cases (**one test per case**):

- Bootstrap token works for the first device; replay fails; expiry fails
- Wrong fingerprint selected → record becomes permanently `denied` (no retry)
- Approver assertion replayed against another `enroll_id` → failure
- Self-approval (`approver.device == pending.device`) → 403
- Approver record created <5 min ago → 403
- Approval with an agent certificate → 403; cross-tenant approval → 403
- `finish` without PoP → 403; second `finish` → 409
- Pending for more than 10 min → 410
- `renew` with a changed public key → `409 key_changed`
- Enrollment ingress SPKI pin mismatch → client aborts

Drill on real hardware:

```bash
ssh relay "sudo pt-relay bootstrap --user example --device testdev --ttl 5m"
pt-enroll --uri 'pt-enroll://relay.<domain>:8444/e_...#<spki-pin>'
pt-approve --list && pt-approve --id <enroll_id> --fingerprint <value>
```

### Completion Criteria

One test device completes start→attest→decide→finish→renew→revoke; the audit chain traces back to the approving device and SPKI.

---

## P4 — Two nginx Vhosts + WebAuthn Session Layer

⚠ **Prerequisite**: P0’s CSWSH/CSRF fixes must already be deployed. Once cookies ship, these two vulnerabilities immediately change from “masked by mTLS” to “exploitable.”

### Work

- **Machine ingress moved in P0** (direct listener); this phase handles only browser ingress: change nginx’s `private-tunnel-relay.conf` to stop requiring client certificates, pin `X-Client-*` to constants, and clear all `X-Pt-*`. **Do not split the stream block or change `deploy.sh:514-537` and `:539-554`.**
- Migrate agents **one machine at a time** during P0’s coexistence period (mac-c → mac-b → mac-a, with mac-a last). In addition to `agent.yaml.template`, update `/opt/private-tunnel/etc/services.d/remote-agent.yaml` and `agenthalo.yaml` on each machine (`update-relay-host.sh --ext-port` **does not cover them**).
- **Change the agent’s `ca_file` to a concatenated bundle of system CAs and the private CA before changing `relay_url`**: `RootCAs` at `internal/tlsutil/load.go:25-37` **replaces**, rather than extends, the system trust store. Reversing the order prevents the agent from connecting.
- `internal/webauthn` (new): registration/assertion endpoints, credentials stored in `devstore`, `__Host-pt_session` cookie, session → `VerifiedPeer{Kind:user, Class:webauthn-synced}`.
- **Device admission is independent of passkeys**: create a pending record for a device fingerprint seen for the first time; issue a long-lived session only after confirmation from an approved device.
- PWA approval page: the WebAuthn assertion covers **the same transcript** as native clients; `sw.js` provides only **Review / Deny** actions, **never one-click Approve**.

### Validation

```bash
cd private-tunnel && go test ./internal/webauthn/... -v
go test -tags integration ./test/integration/ -run 'WebAuthn|VhostSplit' -v
ssh relay 'sudo nginx -t'
```

Required coverage:

- Browser ingress without a cookie → 401 and redirect to login, **not** 403
- Machine ingress receives a request without a client certificate → reject at TLS (never reaches Go)
- Forged `X-Client-Verify: SUCCESS` header on browser ingress → overwritten by nginx’s constant; still 401
- Synced passkey assertion succeeds on an **unapproved** device → pending status only, **no long-lived session**
- After an approved device confirms this device → long-lived session issued
- State-changing POST without a CSRF token → 403

Manual acceptance (real devices):

```bash
# macOS Safari / Chrome: register passkey → log out → sign in with assertion → /s/iterm/ and WSS both work
# iPhone Safari: same as above; then “Add to Home Screen” and verify the PWA needs a new assertion (known cookie partitioning behavior)
# Sign in with the same Apple ID on a new Mac: verify the passkey syncs but the device remains pending approval
```

### Completion Criteria

Browsers no longer need client certificates at any point; new devices require approval; agents on all three Macs are stable on `mtls.<domain>`.

### Rollback

Restore the single-vhost nginx template (`ssl_verify_client on`); retain WebAuthn endpoints, but do not make them the only entry point.

---

## P5 — Real Attestation: Apple SEP + PIV

### Work

- `cmd/pt-sep-helper` (Swift) + `internal/hwkey/applesep_darwin.go` (framed subprocess protocol, no cgo).
- `internal/hwkey/piv.go` (`pt_piv`, cgo) + **handwritten pure-Go** `internal/attest/piv.go`.
- Implement I2 (public-key binding), class tiers, and the `min_assurance` policy in `internal/attest/verdict.go`.
- Integrate revalidation of real verdicts into `pt-ca`.

### Validation

```bash
# Apple (Apple silicon Mac with a GUI session)
swift build --package-path cmd/pt-sep-helper -c release
CGO_ENABLED=0 go test ./internal/hwkey/ -run AppleSEP -v
```

Assertions: create → persist blob → restore in a new process → identical public key → successful mutual TLS1.2/1.3 handshakes; `SecKeyCopyExternalRepresentation(privateKey)` returns `-4`; **an existing blob can still sign while the screen is locked**; no code uses `SecKeyCreateWithData` + `kSecAttrTokenIDSecureEnclave`.

```bash
grep -rn 'SecKeyCreateWithData' cmd/pt-sep-helper/ && exit 1
go test -tags pt_piv ./internal/hwkey/ -run PIV -v
CGO_ENABLED=0 go test ./internal/attest/ -run PIV -v    # Pure-Go verifier, fixture-driven
CGO_ENABLED=0 GOOS=linux GOARCH=amd64 go build ./cmd/relay   # Verify cgo has not leaked into relay
```

Negative cases: wrong attestation chain root → reject; slot certificate public key != CSR public key → reject (I2).

**Two unverified experiments that P5 must perform first** (failure requires replanning the macOS approach):

1. Restore a SEP blob **after reboot** (only cross-process restoration has been proven so far);
2. Use a SEP blob from a **noninteractive launchd session**.

### Completion Criteria

`pt-enroll --backend apple-sep` returns `class=asserted-apple-sep`, and the approval page displays the literal “未经证明” (“unproven”); `--backend piv` returns `proven-piv`.

---

## P6 — Platform Migration + Enforcement Mode

### Work

- **All three Macs: move the agent from its container to a host LaunchAgent** (registered under `tools/private-services/`), using a SEP signer; private-edge stops copying any key material. See specification §3.1.2 for handling the direction reversal:
  - Change nginx / websockify in containers to listen on TCP at the container IP; change the host agent’s `unix_targets` to `tcp_targets` (two on mac-b: 8080/6080; one on mac-a: 8080; **mac-c has no `unix_targets` and is unaffected**);
  - Add dynamic `__CONTAINER_IP__` resolution to the deployment script (following `__HOST_GATEWAY__`); **test container IP stability first**;
  - Change `tcp_targets` previously pointing to `__HOST_GATEWAY__` (18765 / 22 / 5900 / 18770) to the host loopback directly, eliminating the detour;
  - **Never run container and host agents simultaneously on the same machine**; during cutover, stop the container agent before starting the host agent.
- **Decommission jetson** (six-item checklist in specification §3.2): stop and disable its agent, and clear `/opt/private-tunnel/`; remove the `0.0.0.0:2223` forwards to jetson from the mac-a/mac-b profiles; remove the `.14` device entry from `deploy.sh:159,166,175` and `examples/relay.yaml:17`; delete `deploy/agent-jetson.yaml`, `deploy-agent-jetson.sh`, and `deploy/private-tunnel-agent.jetson.service`; destroy `certs/agent-example-jetson.*` and mark it revoked in the registry; correct the outdated descriptions at `AGENTS.md:83` and `docs/root-runtime-context.md:13`.
  - **Prerequisite**: exclude jetson from P0’s `devices import`; the device gate is the actual revocation point, not deleting configuration and shutting down.
  - **Confirm the fate of two services**: iterm-web running on jetson (8080/8765) stops when it is decommissioned; tunneled SSH to jetson disappears, leaving only LAN access.
- **Retire `.p12`**: delete `generate_user_p12` from `onboard-user.sh`; rewrite the bootstrap download paths in `publish-user-bootstrap.sh` / `publish-user-binary-update.sh` (unauthenticated downloads with detached-signature verification).
- Roll out `enroll.enforce: deny` in steps: one device → one user → global → `min_assurance`.
- Require “≥2 approve-capable devices per user”; prohibit revoking the last one over HTTP.

### Validation

```bash
ssh mac-b 'launchctl print gui/$UID/com.private-services.private-tunnel-agent | head -20'
ssh mac-b 'container inspect private-edge | grep -i address'                      # Record the container IP and verify stability after restart
ssh mac-b 'pgrep -fl private-tunnel-agent | wc -l'                                # Expect 1 (not one each on the host and in the container)
container exec <mac-b-ct> ls -la /etc/private-edge/certs                          # Expect only .crt, no .key
container exec <mac-b-ct> sh -c "grep -rl 'BEGIN.*PRIVATE KEY' / 2>/dev/null"     # Expect empty
curl --cert <hw cert> --key ... https://mtls.<domain>/_pt/whoami                  # 200 throughout
bash ../private-edge/scripts/smoke.sh                                             # All three Macs pass
ssh jetson 'systemctl is-active private-tunnel-agent'                             # Expect inactive (decommissioned)
grep -rn 'jetson' private-tunnel/deploy.sh private-tunnel/examples/relay.yaml \
     private-edge/config/profiles/*/agent.yaml.template                           # Expect empty
curl -b <session> https://relay.<domain>/_pt/devices | jq -r '.[].device'         # Expect no jetson
ls private-tunnel/certs/agent-example-jetson.* 2>/dev/null                          # Expect empty
grep -rn 'p12\|pkcs12' private-tunnel/ scripts/ | grep -v CHANGELOG               # Expect empty
ssh mac-a 'ls /opt/private-tunnel/certs/*.p12 2>/dev/null'                        # Expect empty
```

### Completion Criteria

No `.key` files remain anywhere in the fleet; all services work under `enforce: deny`; jetson services remain uninterrupted.

### Rollback

Validate each step before proceeding. Roll back `enforce` one level at a time; retain the old container configuration for macOS agents for one release cycle.

---

## P7 — Windows TPM 2.0

### Work

- `internal/hwkey/pcp_windows.go` (pure-Go NCrypt, ECDSA P-256, **no usage auth**; set `NCRYPT_UI_POLICY` only for approval keys).
- Full validation in `internal/attest/tpm2.go` + EKPub allowlist.

### Validation

```bash
CGO_ENABLED=0 GOOS=windows GOARCH=amd64 go build ./cmd/pt-enroll ./cmd/agent
CGO_ENABLED=0 go test ./internal/attest/ -run TPM2 -v
```

Reject every negative fixture: cleared `fixedTPM` / cleared `fixedParent` / cleared `sensitiveDataOrigin` / set `restricted` / Name does not match pubArea / wrong magic / **pubArea public key != CSR public key (I2)** / EKPub absent from the allowlist.

Real hardware: Windows 11 + discrete TPM → `pt-enroll.exe` → `class=proven-tpm2`; negative case: Hyper-V local-protector vTPM → expect rejection with `no_ek_pin`.

---

## Risk Register (Tracked Throughout Implementation)

| Risk | Trigger | Mitigation |
|---|---|---|
| SEP blob unavailable after reboot / under launchd | P5 experiments fail | Move macOS to Keychain + Developer ID + provisioning profile (`go build` no longer means deployment) |
| Cannot add `mtls.<domain>` SAN to the LE certificate | P4 prerequisite experiment fails | Fall back to separate ports (`relay.<domain>:8443` for machine traffic) |
| Decommissioning jetson removes iterm-web and tunneled SSH | P6 | Accepted; if needed, recreate the services on another hardware-bound Mac |
| “Delete configuration + shut down” is mistaken for revocation before P0 | Before P6 | jetson’s certificate remains cryptographically valid until P0’s device gate ships; a copy can reconnect and take over registration |
| Production `relay.yaml` has drifted (production: 2 users / 9 services; generator: 1 user / 5 services) | Before running deploy.sh | Deploying directly loses the liucun tenant and bootstrap services; reconcile line by line before deployment |
| No unattended rollback (`sudo -n true` fails on the relay host) | Any server-side change | The on-call operator must have the password; use `ssh "$MUSTER_RELEASE_HOST"` for out-of-band snapshots (external port `${MUSTER_RELEASE_SSH_PORT}`, bypassing the tunnel) |
| Unstable container IP (`192.168.64.4` changes) | P6 | Resolve dynamically at deployment and write into rendered configuration; if still unstable, fall back to `--publish-socket` (requires recreating the container, conflicts with private-edge rules, and needs a new decision) |
| Host and container agents coexist during cutover | P6 | Duplicate registration of the same device name ejects the other agent (`server.go:281-284`); stop before starting, and verify with `pgrep` |
| A user’s only approve-capable device fails | Anytime | Break-glass access via `ssh relay`; require ≥2 devices from P6 onward |
| `pt-ca` (operations Mac) offline for an extended period | Anytime | 20-day offline tolerance + `emergency-issue` drill |
| Lost `devstore` | Anytime | Include it in backups; fail closed on read errors |
