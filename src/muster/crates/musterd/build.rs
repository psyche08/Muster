//! Compiles in the source commit, in two forms:
//! - `MUSTER_GIT_COMMIT`: `git rev-parse --short HEAD`, which every device
//!   reports in its settings and heartbeat (display only);
//! - `MUSTER_BUILD_COMMIT`: the full commit, with `-dirty` for uncommitted
//!   tracked changes, which the release check reads from an installed daemon.
//!
//! Either environment variable overrides its form. "unknown" outside a
//! checkout.
//!
//! With the `sep` feature on macOS, the binary links the Swift runtime
//! (muster-platform's build.rs adds the libraries); the rpath that finds
//! them at run time has to come from the binary's own build script.

use std::env;
use std::path::Path;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("ios") {
        apple_video();
    }
    let head = head();
    println!(
        "cargo:rustc-env=MUSTER_GIT_COMMIT={}",
        overridden("MUSTER_GIT_COMMIT")
            .or_else(|| head
                .as_ref()
                .and_then(|_| git(&["rev-parse", "--short", "HEAD"])))
            .unwrap_or_else(|| "unknown".into())
    );
    println!(
        "cargo:rustc-env=MUSTER_BUILD_COMMIT={}",
        overridden("MUSTER_BUILD_COMMIT")
            .or_else(|| head.map(|head| {
                // Only re-evaluated when HEAD moves; the release check
                // (deploy/desktop/verify-release.sh) also requires a clean tree.
                let dirty = git(&["status", "--porcelain", "--untracked-files=no"])
                    .is_none_or(|status| !status.is_empty());
                if dirty { format!("{head}-dirty") } else { head }
            }))
            .unwrap_or_else(|| "unknown".into())
    );
    if env::var_os("CARGO_FEATURE_SEP").is_none() {
        return;
    }
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-link-arg-bins=-Wl,-rpath,/usr/lib/swift");
    }
}

fn overridden(name: &str) -> Option<String> {
    println!("cargo:rerun-if-env-changed={name}");
    env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// The full HEAD commit. Rebuilds when HEAD moves: the HEAD file itself, the
/// branch it names and packed refs (worktrees keep refs in the common dir).
fn head() -> Option<String> {
    let git_dir = git(&["rev-parse", "--absolute-git-dir"])?;
    let common = git(&["rev-parse", "--path-format=absolute", "--git-common-dir"])?;
    let mut watched = vec![format!("{git_dir}/HEAD"), format!("{common}/packed-refs")];
    if let Some(branch) = git(&["symbolic-ref", "-q", "HEAD"]) {
        watched.push(format!("{common}/{branch}"));
    }
    for path in watched {
        if Path::new(&path).exists() {
            println!("cargo:rerun-if-changed={path}");
        }
    }
    git(&["rev-parse", "HEAD"])
}

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn apple_video() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../addons/remote-desktop");
    let sdk = if env::var("TARGET").unwrap().ends_with("-sim")
        || env::var("TARGET").unwrap().starts_with("x86_64")
    {
        "iphonesimulator"
    } else {
        "iphoneos"
    };
    let arch = if env::var("CARGO_CFG_TARGET_ARCH").unwrap() == "aarch64" {
        "arm64"
    } else {
        "x86_64"
    };
    let out = std::path::PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let cache = env::var_os("CARGO_TARGET_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or(out)
        .join("desktop-host");
    for path in [
        "build-apple-viewer.py",
        "build-host.py",
        "CMakeLists.txt",
        "viewer/apple.c",
        "vendor/moonlight-pc/moonlight-common-c",
    ] {
        println!("cargo:rerun-if-changed={}", root.join(path).display());
    }
    assert!(
        Command::new("python3")
            .arg(root.join("build-apple-viewer.py"))
            .arg(&cache)
            .args([sdk, arch])
            .status()
            .unwrap()
            .success()
    );
    println!(
        "cargo:rustc-link-search=native={}",
        cache
            .join("moonlight")
            .join(format!("{sdk}-{arch}"))
            .display()
    );
    println!("cargo:rustc-link-lib=static=muster_apple_video");
}
