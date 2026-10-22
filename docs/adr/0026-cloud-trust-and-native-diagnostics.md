# ADR 0026: Use platform Cloud trust and expose bounded native diagnostics

- Date: 2026-10-22
- Status: Accepted
- Related: ADR 0002, ADR 0010, ADR 0020; ARCHITECTURE.md §3.6.2

## Context

One test Mac's Cloud request fails with Rustls `UnknownIssuer`, while the
system HTTPS client verifies the same destination's locally trusted issuer.
The daemon currently uses only bundled WebPKI roots. The other two test Macs
complete TLS with the same unauthenticated Rust probe. This identifies a
reproducible trust-source mismatch on one device, not every intermittent error.
The transport currently discards the underlying cause by formatting only the
outer reqwest error, which also includes its URL.

Separate native logs report two matching Codex composers and Claude question
lookup deadlines. Generic toolbar diagnostics exhaust their sample allowance
before reporting the matching composers. Current live Codex inspection has one
chat composer and a distinct question editor; it does not establish the old
ambiguity's cause. Claude's old pending forms have already resolved. Retain
fail-closed behavior until new evidence identifies the actual controls.

## Decision

For macOS Cloud HTTPS only, use Rustls with the operating system's certificate
verifier, including hostname, validity and trust/distrust policy. Do not import
an intercepted leaf or a private root, disable verification, modify network
software, or change device/relay/DoH/update trust. Keep redirects disabled and
existing timeouts. Other platforms keep their existing trust source.

Report bounded, content-free transport error categories and native phase timing.
Always retain a bounded sample of matching composer metadata independently of
unrelated toolbar controls. Never log draft values, bearer headers, response
bodies or arbitrary request URLs. Do not select the first of two composers,
extend authorization, retry a possibly delivered operation, or answer an old
business question for diagnosis.

## Alternatives

Disabling certificate validation or pinning a captured issuer would bypass
normal trust policy. Flattening system roots into a static root list can lose
platform distrust/hostname policy. Changing all reqwest clients would also
change unrelated security boundaries. Blind input selection or a longer AX
deadline cannot explain which view/control failed.

## Consequences

Cloud HTTPS follows the macOS operator's existing trust policy. Runtime errors
distinguish certificate, connection, timeout and body failures without disclosing
credentials. Native failures retain safety checks and become attributable.
Evidence may require a later targeted fix; diagnostics alone are not acceptance.

## Verification

Reproduce the trust mismatch before the change and compare the new client on
all test Macs. Test untrusted certificates, wrong hostnames, redirects,
timeouts and content-free error reporting. Test matching-composer diagnostics
after unrelated controls exhaust the generic budget. Run relevant Rust tests,
Clippy, architecture, public-content and ADR gates. Use only owner-authorized
idle acceptance sessions for a fresh native send and require the provider's
receipt/reply. Keep source, build, signing, publication, installation and actual
provider acceptance separate. GitHub Actions remain disabled.

### Source and read-only device evidence

The old Rust client failed immediately with `InvalidCertificate(UnknownIssuer)`
on the affected Mac, while its system client accepted the current local issuer.
The new platform-verifier probe completed TLS there without changing system
trust or network software. The actual new Cloud client and existing credential
reader then completed authenticated task listing on all three Macs, each with
an empty task list (not an authentication error). No credential or response
content was written to the evidence output. This is read-only client validation,
not installed-daemon or native-send acceptance.

The platform verifier delegates hostname and certificate policy to
[the OS verifier](https://docs.rs/rustls-platform-verifier/0.7.1/rustls_platform_verifier/).
Four local fault tests cover an untrusted TLS server, HTTPS-only rejection,
redirect refusal and timeout classification; error messages exclude the test
URL's synthetic secret. There was no system-trust modification for tests.
Explicit platform tests of a trusted chain with the wrong hostname or expiry
were not performed. Agent tests passed 224 (four existing ignored), platform
tests passed 34, and daemon tests passed 55. Strict all-target Clippy, the 17-crate architecture gate and public-content
checks passed. Installed acceptance will be recorded after completion.

Prepare paired Apple 1.0.84 (75) artifacts to install the Cloud repair and
native diagnostics. Existing native ambiguity and question timeout observations
remain unresolved until the new diagnostic evidence explains them; no global
timeout increase or ambiguous-control fallback is included.
