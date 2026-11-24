"""Build the separate macOS Moonlight executable or the iOS static viewer."""
import argparse
import fcntl
import os
from pathlib import Path
import runpy
import subprocess

root = Path(__file__).resolve().parent
host = runpy.run_path(str(root / "build-host.py"))
run = host["run"]

def build(cache, sdk, arch):
    obj = cache / "moonlight" / (sdk + "-" + arch)
    obj.mkdir(parents=True, exist_ok=True)
    mac = sdk == "macosx"
    prefix = cache / "arm64-14.0" / "prefix" if mac and arch == "arm64" else obj / "prefix"
    if not (prefix / "lib/libcrypto.a").exists():
        archive = host["download"](cache, "openssl-3.6.5.tar.gz",
            "https://codeload.github.com/openssl/openssl/tar.gz/refs/tags/openssl-3.6.5",
            "f3091b7ccb41a11d877b7406b7afc9dd9c5003d299c6bb33cff882127bc06c91")
        src = host["extract"](archive, cache, "openssl-openssl-3.6.5")
        build_ssl = obj / "openssl"
        build_ssl.mkdir(exist_ok=True)
        sdkpath = subprocess.check_output(["xcrun", "--sdk", sdk, "--show-sdk-path"], text=True).strip()
        triple = arch + ("-apple-macos14" if mac else "-apple-ios17" + ("-simulator" if sdk == "iphonesimulator" else ""))
        env = dict(os.environ, SDKROOT=sdkpath, CC="clang", CFLAGS="-target " + triple + " -isysroot " + sdkpath)
        target = "darwin64-" + arch + "-cc" if mac else ("ios64-xcrun" if sdk == "iphoneos" else "iossimulator-xcrun")
        run(src / "Configure", target, "no-shared", "no-tests", "no-module", "--prefix=" + str(prefix),
            "--libdir=lib", cwd=build_ssl, env=env)
        run("make", "-j8", "build_sw", cwd=build_ssl, env=env)
        run("make", "install_dev", cwd=build_ssl, env=env)
    options = [] if mac else ["-DCMAKE_SYSTEM_NAME=iOS", "-DCMAKE_TRY_COMPILE_TARGET_TYPE=STATIC_LIBRARY"]
    run("cmake", "-S", root, "-B", obj, *options, "-DCMAKE_BUILD_TYPE=Release",
        "-DCMAKE_OSX_SYSROOT=" + sdk, "-DCMAKE_OSX_ARCHITECTURES=" + arch,
        "-DCMAKE_OSX_DEPLOYMENT_TARGET=" + ("14.0" if mac else "17.0"),
        "-DBUILD_SHARED_LIBS=OFF", "-DOPENSSL_USE_STATIC_LIBS=TRUE",
        "-DOPENSSL_INCLUDE_DIR=" + str(prefix / "include"),
        "-DOPENSSL_CRYPTO_LIBRARY=" + str(prefix / "lib/libcrypto.a"),
        "-DOPENSSL_SSL_LIBRARY=" + str(prefix / "lib/libssl.a"))
    run("cmake", "--build", obj, "--target", "muster-moonlight", "--parallel", "8")
    if not mac:
        run("xcrun", "libtool", "-static", "-o", obj / "libmuster_apple_video.a",
            obj / "libmuster-moonlight.a", obj / "moonlight-common-c/libmoonlight-common-c.a",
            obj / "moonlight-common-c/enet/libenet.a", prefix / "lib/libcrypto.a")
    return obj

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("cache", type=Path)
    parser.add_argument("sdk", choices=["macosx", "iphoneos", "iphonesimulator"])
    parser.add_argument("arch", choices=["arm64", "x86_64"])
    args = parser.parse_args()
    cache = args.cache.resolve()
    cache.mkdir(exist_ok=True, parents=True)
    with (cache / "moonlight-build.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        build(cache, args.sdk, args.arch)
