# MusterMacShim

The pieces musterd needs from macOS inside its own process, as a static
library the Rust binary links (ARCHITECTURE §3.9, §3.4). Nothing here
decides anything; it signs when asked.

- `SepSign`: `sepsign_create / sepsign_public_key_raw / sepsign_sign /
  sepsign_free / sepsign_free_str`, the P-256 Secure Enclave key of
  `muster-platform/src/macos/sep.rs`. The key is created with
  `kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly` and no user-presence
  flag, so the daemon signs while the screen is locked; the presence key is
  the app's, not the daemon's.
- `LockedUse/Console.m`: the Locked Use console adapters that
  `muster-platform/src/macos/locked_use.rs` calls: wake and display
  assertions, the console session sample, the lock-screen Return key, the
  cover panels, locking, and the plugin socket's peer check. `muster-platform/build.rs`
  always compiles it on macOS; Rust owns every lease, authorization and
  watchdog decision. `Tests/DisplayAwakeTests.m` checks that its display
  assertions are released (build it with Console.m, see its header).

`build.sh` (or `cargo build -p musterd --features sep` on a Mac) builds musterd with the signer: muster-platform's build.rs compiles
`Sources/SepSign/SepSign.swift` with swiftc into a static archive and
links the Swift runtime from the SDK. Without the feature musterd keeps a
software key (the CI build); the packaged app must ship the SEP build.

The iTerm2 cookie needs no shim: muster-terminal asks iTerm2 itself through
`osascript` (Automation permission, decision D2).
