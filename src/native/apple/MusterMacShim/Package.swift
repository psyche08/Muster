// swift-tools-version: 5.10
// MusterMacShim: what musterd needs from the OS in-process and cannot get from
// Rust alone (ARCHITECTURE §3.9): the Secure Enclave signer here; the Locked
// Use console adapters (Sources/LockedUse) are compiled by muster-platform's
// build.rs. The library is static so musterd stays one binary.
//
// musterd's build does not use this manifest: muster-platform's build.rs
// compiles Sources/SepSign with swiftc directly (`cargo build -p musterd
// --features sep`, or build.sh). The package exists so the
// Swift source can be edited and built on its own: `swift build`.
import PackageDescription

let package = Package(
    name: "MusterMacShim",
    platforms: [.macOS(.v14)],
    products: [
        .library(name: "SepSign", type: .static, targets: ["SepSign"]),
    ],
    targets: [
        .target(name: "SepSign", path: "Sources/SepSign"),
    ]
)
