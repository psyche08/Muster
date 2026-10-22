//! Builds the macOS-only console OS shim, and (with `sep`) the Secure
//! Enclave Swift adapter. Other hosts and targets never link these adapters.
//! Module caches live under Cargo OUT_DIR, keeping builds sandbox-compatible.

use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=SDKROOT");
    let target_macos = env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos");
    let host_macos = env::var("HOST").is_ok_and(|h| h.contains("apple-darwin"));
    if !(target_macos && host_macos) {
        // `cargo check` from Linux never links; the extern block still type-checks.
        return;
    }
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let shim = manifest.join("../../../native/apple/MusterMacShim/Sources/LockedUse/Console.m");
    println!("cargo:rerun-if-changed={}", shim.display());
    assert!(
        Command::new("xcrun")
            .args([
                "clang",
                "-mmacosx-version-min=14.0",
                "-fobjc-arc",
                "-O2",
                "-c"
            ])
            .arg(&shim)
            .arg("-o")
            .arg(out.join("console.o"))
            .status()
            .unwrap()
            .success()
    );
    assert!(
        Command::new("ar")
            .arg("rcs")
            .arg(out.join("libmuster_console.a"))
            .arg(out.join("console.o"))
            .status()
            .unwrap()
            .success()
    );
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=muster_console");
    for framework in ["AppKit", "ApplicationServices", "Security", "IOKit"] {
        println!("cargo:rustc-link-lib=framework={framework}");
    }
    println!("cargo:rustc-link-lib=bsm");
    if env::var_os("CARGO_FEATURE_SEP").is_none() {
        return;
    }
    let source = manifest.join("../../../native/apple/MusterMacShim/Sources/SepSign/SepSign.swift");
    println!("cargo:rerun-if-changed={}", source.display());
    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let status = Command::new("swiftc")
        .args([
            "-parse-as-library",
            "-module-name",
            "SepSign",
            "-emit-library",
            "-static",
            "-O",
        ])
        .arg("-module-cache-path")
        .arg(out.join("swift-module-cache"))
        .arg("-o")
        .arg(out.join("libsepsign.a"))
        .arg(&source)
        .status()
        .expect("run swiftc (Xcode or the command line tools)");
    assert!(status.success(), "swiftc failed building SepSign");

    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=sepsign");
    // The Swift runtime: stubs come from the SDK, the libraries from the
    // dyld shared cache at run time (rpath /usr/lib/swift).
    let sdk = match env::var("SDKROOT") {
        Ok(path) if !path.is_empty() => path,
        _ => {
            let output = Command::new("xcrun")
                .args(["--show-sdk-path"])
                .output()
                .expect("run xcrun");
            String::from_utf8(output.stdout)
                .expect("utf-8")
                .trim()
                .to_string()
        }
    };
    // musterd's own build.rs adds `-rpath /usr/lib/swift`: link arguments
    // from a library's build script do not reach the binary.
    println!("cargo:rustc-link-search=native={sdk}/usr/lib/swift");
    for lib in [
        "swiftCore",
        "swiftFoundation",
        "swiftCoreFoundation",
        "swiftDispatch",
        "swiftObjectiveC",
        "swiftDarwin",
        "swiftXPC",
        "swiftIOKit",
        "swiftos",
        "swift_Concurrency",
        "swift_StringProcessing",
    ] {
        println!("cargo:rustc-link-lib=dylib={lib}");
    }
    for framework in ["Foundation", "Security", "CryptoKit"] {
        println!("cargo:rustc-link-lib=framework={framework}");
    }
}
