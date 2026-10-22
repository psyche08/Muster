// swift-tools-version: 5.10
// The Apple shells of Muster (ARCHITECTURE §3.7): one SwiftUI package for
// macOS and iOS around the Rust core's C ABI. Nothing here decides trust;
// the package renders ViewModels, sends Intents, performs Effects and signs
// with the Secure Enclave when the core asks.
//
// Linking: the Rust core is `libmuster_ffi.a` from `src/muster` (scripts/build-ffi.sh).
// Xcode app targets link it in OTHER_LDFLAGS (project.yml); on the command
// line: MUSTER_FFI_LIB=$PWD/../../../muster/target/debug/libmuster_ffi.a swift test
import Foundation
import PackageDescription

// For `swift build` / `swift test` outside Xcode: the libmuster_ffi.a to link,
// by path so the archive wins over the cdylib cargo builds beside it. Unset
// in Xcode (the app target links it), so the package stays a plain local
// dependency.
let ffiArchive = ProcessInfo.processInfo.environment["MUSTER_FFI_LIB"]
let ffiLink: [LinkerSetting] = ffiArchive.map { [.unsafeFlags([$0])] } ?? []

let package = Package(
    name: "MusterApp",
    platforms: [.macOS(.v14), .iOS(.v17)],
    products: [
        .library(name: "MusterCore", targets: ["MusterCore"]),
        .library(name: "MusterUI", targets: ["MusterUI"]),
        .executable(name: "MusterDev", targets: ["MusterDev"]),
    ],
    dependencies: [
        .package(url: "https://github.com/apple/swift-protobuf.git", from: "1.29.0"),
        .package(url: "https://github.com/gonzalezreal/swift-markdown-ui.git", from: "2.4.1"),
    ],
    targets: [
        // The C ABI (src/muster/crates/muster-ffi/include/muster.h, symlinked).
        .systemLibrary(name: "CMuster", path: "Sources/CMuster"),
        // OS-only, allocation-free remote-thread stack capture (Rust and Swift).
        .target(name: "CMusterDiagnostics"),
        // The core wrapper: bytes over the ABI, typed messages, keys.
        .target(
            name: "MusterCore",
            dependencies: [
                "CMuster",
                .target(name: "CMusterDiagnostics", condition: .when(platforms: [.macOS, .iOS])),
                .product(name: "SwiftProtobuf", package: "swift-protobuf"),
            ],
            // The in-app keyboard's pinyin dictionary (Input/PinyinEngine.swift).
            resources: [.copy("Resources/pinyin.txt")],
            linkerSettings: ffiLink + [
                .linkedFramework("Security", .when(platforms: [.macOS, .iOS])),
                .linkedFramework("CoreFoundation", .when(platforms: [.macOS, .iOS])),
                .linkedFramework("CoreServices", .when(platforms: [.macOS])),
                .linkedFramework("ApplicationServices", .when(platforms: [.macOS])),
                .linkedFramework("IOKit", .when(platforms: [.macOS])),
                .linkedFramework("SystemConfiguration", .when(platforms: [.macOS, .iOS])),
            ]
        ),
        // SwiftUI screens and effects; empty on Linux.
        .target(name: "MusterUI", dependencies: [
            "MusterCore",
            .product(name: "MarkdownUI", package: "swift-markdown-ui", condition: .when(platforms: [.macOS, .iOS])),
        ]),
        // The unbundled macOS app for development (`swift run MusterDev`);
        // XcodeGen builds Muster.app (target MusterMac) from project.yml.
        .executableTarget(name: "MusterDev", dependencies: ["MusterUI"]),
        .testTarget(name: "MusterCoreTests", dependencies: ["MusterCore", "MusterUI"]),
    ],
    swiftLanguageVersions: [.v5]
)
