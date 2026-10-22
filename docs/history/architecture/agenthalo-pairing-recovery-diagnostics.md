# Pairing successor recovery and diagnostics

## Scope

Shared ABI 1.48 adds non-authorizing issuance diagnostics to the existing
`AgentHaloSecurityV2Result.error_kind`. The result layout and protected runtime
schema are unchanged. ABI 1.47's consumer deadline contract remains supported;
rebuilding Shared does not upgrade already-installed mobile or Relay binaries.

| Error kind | Meaning | Platform behavior |
| --- | --- | --- |
| 32 | Initial issuance already exists for this subject | Explain that a fresh QR cannot replace the existing trust history |
| 33 | Expiry successor does not match the original subject | Explain that recovery is bound to the original device identity |
| 34 | Consumer confirmation revalidation failed | Show a fixed confirmation-validation error |
| 35 | Owner acceptance validation failed | Show a fixed owner-acceptance error |

These codes are display diagnostics, not permission to retry, clear state,
replace a key, or issue a grant. Control flow still uses Shared's opaque
runtime and typed next step. Do not infer a reason from an error message or
serialized projection. Unknown codes retain a generic fail-closed display.
Diagnostics must not log QR payloads, SAS, keys, signatures, requests, bearer
capabilities, or protected journal bytes.

## Historical-state regression

The reproduced failure sequence is: an initial issuance commits locally but
its Relay delivery does not complete; expiry authorizes a constrained
successor; that successor persists a signature checkpoint and expires before
its authority commit. The former implementation stored `Idle` while retaining
the predecessor's issuance ledger, so another QR for the same subject reached
the deliberately forbidden duplicate-initial-issuance path.

An unfinished, verified successor must instead preserve constrained recovery
for its exact current predecessor. Shared validates the process base,
predecessor, revision, subject, scopes, targets, and expiry. Journal and witness
exact readback remain prerequisites before another transition is executable.
Initial uncommitted issuance expiry still returns to `Idle`; committed expiry
still uses the existing delivery retry path.

An old journal that has already become `Idle` and lost the successor evidence
cannot be reconstructed from a ledger or platform projection. It remains
fail-closed; this patch is not an automatic repair of every pre-existing
protected state and does not authorize deleting such state.

Required tests include every durable successor signature stage, cold restore,
repeat expiry, one-generation-ahead crash recovery, rollback rejection,
substituted predecessor/subject/scope/target rejection, and legacy `Idle`
refusal. A clean first-pairing test alone does not cover this recovery path.

## Verification boundaries

Rust/ABI and native binding regressions, signed App installation, live startup,
and real phone/Relay pairing are separate gates. Stage-specific diagnostics
identify the actual failing boundary; a broad owner-approval error or healthy
backend is not pairing completion. Production Relay compatibility remains a
separate deployment requirement. As explicitly requested on 2026-09-07, Relay
deployment does not require an independent anti-rollback witness; that protection
is NOT PROVIDED. This does not relax Shared/platform recovery validation.
