# Directory Synchronization 304 Fix After Pairing Completion (2026-09-11)

## On-site Evidence

Mac displayed Owner-side pairing completion recorded; HarmonyOS displayed ACK saved,
and had saved pairing with the new Owner, but directory synchronization stopped at `https_execute / https_redirect_rejected / HTTP 304`.
This error does not imply that the user needs to scan again or clear pairing.

## Scope of the Fix

- The HarmonyOS HTTPS adapter passes 304 and its empty response unchanged to Shared instead of treating it as a redirect.
  Other 3xx responses remain rejected, and `maxRedirects: 0` is unchanged.
- The Shared pairing directory path previously accepted only 200. It now permits 304 for the exact pending request,
  but only if a verified snapshot saved by the current opaque session already exists. Directory state is restored only after checking
  the snapshot and Grant validity periods, Owner rotation chain, and authorized targets.
- Signed snapshot validity is not extended. Client JSON/service counts are not trusted. First-request 304, nonempty 304,
  expired/future snapshots, expired Grants, foreign requests, and duplicate requests remain rejected. Old connection handles remain invalid; subsequent selection uses new handles.
- Pairing and operation journals are retained; no new Owner is created, and TLS, signature, and permission validation are not relaxed.

## Validated

- All 44 Shared pairing-related tests passed, including new positive and negative 304 cases; Clippy passed.
- All 94 Shared FFI tests passed, including the actual C ABI's null-pointer/zero-length 304 response.
- The complete HarmonyOS build-script regression passed; real `.ets` HTTP/pairing binding tests cover continued polling after 304 and rejection of other 3xx responses.
- The signed Debug HAP build succeeded and was installed with `install -r` on `5NC0225C10020352`,
  then cold-started with `agenthalo.debug.skipLocalAuth=true`; pairing data was not cleared.
- After the user unlocked the device, the physical device's Devices page displayed `5 of 5 · Pairing verified · AgentHalo service not advertised`,
  retained the original Owner, showed `owner-signed directory r1 verified`, and had state `paired service_unavailable`.
  Page reads at 18:54 and 18:55 both retained this state without the original 304 redirect error.
  This log read did not return HTTP status codes for successful requests, so the page observation cannot be described as a physical-device 304 packet capture;
  the 304 branch itself is verified by Shared, FFI, and real HarmonyOS adapter regressions.

## Remaining Gates

- Pairing and directory verification have recovered, but physical-device remote access and session reads/writes have not passed; Debug local-authentication bypass does not count as security acceptance.
- The Mac backend was restarted. Actual bundled `muster product-pipe` registration returned
  `product relay registration failed (HTTP 404)`, so the remote transport process exited and the service could not be advertised.
  An unauthenticated public-path probe timed out; it is not yet established whether the remote version lacks the endpoint or reverse-proxy routing is at fault.
- No Relay deployment or Mac package replacement occurred, and remote sessions/connections were not claimed to be available.
