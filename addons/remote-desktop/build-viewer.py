"""Build and test the x64 helper and collect its corresponding sources for portable ZIPs."""
import argparse
from hashlib import sha256
from pathlib import Path
import shutil
import subprocess
from tempfile import TemporaryDirectory
from zipfile import ZipFile, ZIP_DEFLATED, ZIP_STORED

BASELINE = "b94b6bdc326c0faea9070b4862e0e0d4e4ac5396"
ARCHIVES = {
    "ffmpeg-ffmpeg-n9.0.2.tar.gz": "6e374ed621e48faa40639307dff48ba6fe574a509977956d2cce9669b7cc27e9",
    "openssl-openssl-openssl-3.6.5.tar.gz": "f3091b7ccb41a11d877b7406b7afc9dd9c5003d299c6bb33cff882127bc06c91",
}
CORE = Path("vendor/moonlight-pc/moonlight-common-c/moonlight-common-c")


def run(*args):
    subprocess.run(args, check=True)


def build(vcpkg, build_dir, output):
    source = Path(__file__).resolve().parent
    revision = subprocess.check_output(["git", "-C", vcpkg, "rev-parse", "HEAD"], text=True).strip()
    if revision != BASELINE:
        raise ValueError("Use vcpkg checkout " + BASELINE)
    run("git", "-C", vcpkg, "diff", "--quiet", "HEAD", "--", "ports", "scripts", "triplets")
    run(vcpkg / "vcpkg.exe", "install", "ffmpeg[core,avcodec,swscale]:x64-windows-static",
        "openssl:x64-windows-static", "--classic")
    # Additional FFmpeg features could link libraries outside the source set.
    status = (vcpkg / "installed/vcpkg/status").read_text()
    features = set()
    for stanza in status.split("\n\n"):
        fields = dict(row.split(": ", 1) for row in stanza.splitlines() if ": " in row)
        if fields.get("Package") == "ffmpeg" and fields.get("Architecture") == "x64-windows-static":
            features.add(fields.get("Feature", "core"))
    if features != {"core", "avcodec", "swscale"}:
        raise ValueError("Use a vcpkg tree with just the required FFmpeg features")
    run("cmake", "-S", source, "-B", build_dir, "-G", "Visual Studio 17 2022", "-A", "x64",
        "-DCMAKE_TOOLCHAIN_FILE=" + (vcpkg / "scripts/buildsystems/vcpkg.cmake").as_posix(),
        "-DVCPKG_TARGET_TRIPLET=x64-windows-static", "-DBUILD_SHARED_LIBS=OFF",
        "-DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded", "-DMUSTER_BUILD_WINDOWS_VIEWER=ON")
    run("cmake", "--build", build_dir, "--config", "Release", "--parallel", "6")
    run("ctest", "--test-dir", build_dir, "-C", "Release", "--output-on-failure")
    output.mkdir(parents=True, exist_ok=True)
    share = vcpkg / "installed/x64-windows-static/share"
    with TemporaryDirectory(dir=output) as stage:
        stage = Path(stage)
        notices = ["Muster Moonlight viewer · GPL-3.0-or-later\n"
                   "Corresponding sources and build scripts accompany the portable archive in Viewer-Sources.zip.\n"]
        for license_file in (source/CORE/"LICENSE.txt", source/CORE/"enet/LICENSE", source/CORE/"nanors/LICENSE",
                             share/"ffmpeg/copyright", share/"openssl/copyright"):
            notices.append(license_file.parent.name + "\n" + license_file.read_text(encoding="utf-8"))
        (stage / "Viewer-Notices.txt").write_text("\n\n".join(notices), encoding="utf-8")
        run("git", "-C", vcpkg, "archive", "--format=tar.gz", "-o", stage/"vcpkg.tar.gz", BASELINE)
        with ZipFile(stage/"Viewer-Sources.zip", "w", compression=ZIP_DEFLATED) as bundle:
            bundle.write(stage/"vcpkg.tar.gz", "vcpkg-" + BASELINE + ".tar.gz", compress_type=ZIP_STORED)
            for name, digest in ARCHIVES.items():
                archive = vcpkg / "downloads" / name
                if sha256(archive.read_bytes()).hexdigest() != digest:
                    raise ValueError("Unexpected source archive: " + name)
                bundle.write(archive, "downloads/" + name, compress_type=ZIP_STORED)
            files = subprocess.check_output(["git","-C",source,"ls-files","--cached","--others",
                "--exclude-standard","-z","--",str(CORE.as_posix()),"viewer","tests","CMakeLists.txt",
                "build-viewer.py","README.md","vendor/sunshine/src/muster_session.h"])
            for name in sorted(filter(None, files.decode("utf-8").split("\0"))):
                bundle.write(source/name,"remote-desktop/" + name)
        shutil.copy2(build_dir/"Release/muster-moonlight.exe",stage)
        for name in ("muster-moonlight.exe","Viewer-Notices.txt","Viewer-Sources.zip"):
            (stage/name).replace(output/name)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("vcpkg", "build", "output"):
        parser.add_argument(name, type=Path)
    args = parser.parse_args()
    build(args.vcpkg.resolve(), args.build.resolve(), args.output.resolve())
