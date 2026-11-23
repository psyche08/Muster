# 0287. Authorize remote desktop as a persistent service

## Context

ADR 0286 required each receiving desktop to maintain a separate pairing permission. Administrators already assign member access and owners manage incoming service rules under ADR 0284.

## Decision

Use the signed member desktop permissions by default. Viewing and mouse control are separate choices. Mouse access includes viewing. An explicitly configured receiver desktop rule set replaces those member permissions on that host. An empty rule set denies everyone. Both paths require current paired desktop membership, an available unlocked console and the operating system permissions granted to Muster. Recheck authorization while streaming and on each connection.

Keep all decisions in Shared Rust. Preserve explicit compatible receiver grants on upgrade. Retire the per-device sharing actions. Only issue new roster encodings after confirming decoder support.

## Alternatives

A per-session prompt interrupts reconnects. A second persistent list in Devices gives administrators and owners conflicting controls. Pairing alone would authorize access without a desktop grant.

## Consequences

Existing networks without desktop grants start with access denied. A receiver can assign a device locally while other network members are being upgraded. Platform screen capture and accessibility consent remain under OS control.

## Verification

Exercise signed access, receiver precedence, restart, revocation, locked consoles, and mixed-version roster handling. Record real frame and mouse acceptance separately from policy tests.
