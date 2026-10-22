# AgentHalo remote product transport V2

## Status and boundary

The components described here are transport foundations, not an enabled remote
product. They do not publish a service, make HarmonyOS sessions usable, or alter
the installed applications. Pairing success, a Relay routing slot and a local
admin health response are not remote readiness.

Shared Rust owns credentials, TLS, MST2 operations, HTTP parsing, authorization,
deadlines and the decision whether to retry. Native adapters supply bounded raw
OS I/O and hardware signatures. No native authentication boolean, legacy MST1
forward or local-admin Unix socket may substitute for the product listener.

## Byte path

The intended client path is:

1. Shared selects a target from the current verified Owner directory and its
   durable consumer Grant; this production credential factory is still missing.
2. `RemoteWebSocketConnectorV2` runs bounded RFC 6455 framing over WebPKI TLS 1.3
   using compiled Mozilla roots and an untrusted raw TCP adapter.
3. `RemoteMst2TransportV2` establishes a separate mutually authenticated TLS 1.3
   connection inside that byte stream, using exact Shared-verified short
   certificates and a non-exportable hardware signer.
4. One allow-listed MST2 operation and one HTTP request run per connection.
   A mutation whose response is lost is not automatically replayed.

The Relay does not terminate inner TLS or interpret product authorization:

- `/v2/product/register/agenthalo` claims an independent routing slot using the
  existing registration token.
- `/v2/product/connect/<user>/<device>/agenthalo` splices raw bytes into an outer
  yamux child stream without prepending the legacy MST1 pipe header.
- The existing `/v2/register` and `/v2/connect/<user>/<device>` remain separate;
  neither namespace falls back to the other. Deploying the new routes does not
  make the client or backend ready.

The backend-side OS carrier is `muster product-pipe --agent-config <path>`.
It performs one registration attempt and forwards only to the dedicated
`127.0.0.1:8765` Rust product listener. It has no arbitrary target argument,
Unix-socket forwarding, identity key or reconnect policy. A backend-owned stdin
pipe controls its lifetime; EOF, unexpected input or termination closes all
streams. The `untrusted_relay_pipe_open` event is routing availability only.
The carrier is not launched by production bootstrap yet.

HarmonyOS's new native bounded TCP primitive uses numeric addresses, nonblocking
I/O, a cancellation pipe and fixed-size transfers. DNS and the asynchronous NAPI
binding remain separate work. It performs neither TLS nor trust decisions and
does not replace the still-unavailable production transport in the UI.

## Required production gates

These gates remain mandatory before enabling or advertising the service:

- Construct the backend's seven opaque verified protected-state objects from
  an authenticated source, including the target Grant ledger and interaction
  ledger. A paired consumer Grant is not a target admission ledger.
- Complete the mutually authenticated bootstrap and renewal exchange for both
  short certificate authorizations. A directory SPKI is not authorization of an
  arbitrary certificate hash.
- Open the original paired Mac Secure Enclave identity rather than generating
  an independent TLS key, and verify its actual SPKI in Shared. The OS blob
  locator is not trust evidence. Real signed-process access remains an OS gate.
- Resolve the requested scope of removing independent anti-rollback witnesses.
  The existing client/backend requirement has not been weakened. A local flag,
  serialized readiness or an HTTP success cannot satisfy it.
- Create phone credentials only from the current durable acknowledged pairing
  and verified directory. Directory, Grant, authority and certificate changes
  must invalidate the credential and cancel any blocked OS streams. The current
  transport checks invalidation around I/O; its bounded request deadline alone
  is not proof of immediate revocation during a blocked OS call.
- Bind the real product listener before starting the raw carrier. Publish only
  from its current verified publication lease. Carrier failure, lease loss or
  revocation must withdraw publication and close all active streams.
- Add the opaque client ABI, bounded async DNS/TCP/HUKS effects and HarmonyOS UI
  binding. Native DTOs may render state but may not infer readiness.

## Verification scope

Automated coverage includes service-slot isolation and raw Relay routing,
carrier parent cancellation and idle lifetime, real backend TLS/MST2 requests,
wrong certificate/signature rejection and ambiguous mutation handling. These
are separate from the native adapter checks and parser attack cases.

No automated transport result is evidence of a signed application launch,
production Relay deployment, hardware-key access, or physical Mac-to-HarmonyOS
session creation/prompt/approval completion. Those acceptance gates must be
recorded separately after the production integration exists.
