"""Build the private Sunshine host with the upstream x64 MSYS2 UCRT64 toolchain."""
import argparse
import os
from pathlib import Path
import runpy
import shutil
from zipfile import ZIP_DEFLATED, ZipFile

root = Path(__file__).resolve().parent
host = runpy.run_path(str(root / "build-host.py"))


def build(cache, output, msys):
    cache.mkdir(parents=True, exist_ok=True)
    ucrt = msys / "ucrt64/bin"
    if not (ucrt / "g++.exe").is_file():
        raise ValueError("Install the upstream Sunshine UCRT64 build dependencies in MSYS2")
    modules = host["MODULES"] + ("nvapi", "ViGEmClient")
    src, deps = host["host_sources"](cache, modules)
    ffmpeg = host["download"](cache, "Windows-AMD64-ffmpeg.tar.gz",
        "https://github.com/LizardByte/build-deps/releases/download/" +
        host["FFMPEG_TAG"] + "/Windows-AMD64-ffmpeg.tar.gz",
        "ee79a7a295e8fbd24305a3f4ec94a06f550e0b52f49cb1c4535f649e2dd8b234")
    prefix = host["extract"](ffmpeg, cache, "ffmpeg")
    obj = cache / (src.name + "-ucrt64")
    env = dict(os.environ, PATH=os.pathsep.join([str(ucrt), str(msys / "usr/bin"), os.environ["PATH"]]))
    run = host["run"]
    run(ucrt / "cmake.exe", "-G", "Ninja", "-S", src, "-B", obj,
        "-DCMAKE_BUILD_TYPE=Release", "-DBUILD_TESTS=OFF", "-DBUILD_DOCS=OFF",
        "-DSUNSHINE_ENABLE_TRAY=OFF", "-DSUNSHINE_ASSETS_DIR=assets",
        "-DFFMPEG_PREPARED_BINARIES=" + prefix.as_posix(), env=env)
    run(ucrt / "cmake.exe", "--build", obj, "--target", "sunshine", "--parallel", "6", env=env)
    payload = output / "Sunshine"
    payload.mkdir(parents=True, exist_ok=True)
    shutil.copy2(obj / "sunshine.exe", payload / "muster-desktop-host.exe")
    shutil.copy2(ucrt / "zlib1.dll", payload)
    shaders = src / "src_assets/windows/assets/shaders"
    shutil.copytree(shaders, payload / "assets/shaders", dirs_exist_ok=True)
    for name in ("LICENSE", "NOTICE"):
        shutil.copy2(src / name, payload / (name + ".txt"))
    # Keep the encoder source revisions corresponding to the pinned FFmpeg binary.
    host["checkout"](cache, host["output"]("git", "-C", deps["build-deps"], "remote", "get-url", "origin"),
                     deps["build-deps"].name, host["CODECS"])
    with ZipFile(output / "Host-Sources.zip", "w", compression=ZIP_DEFLATED, compresslevel=1) as archive:
        for path in sorted(src.rglob("*")):
            if path.is_file() and ".git" not in path.parts and "third-party" not in path.relative_to(src).parts[:1]:
                archive.write(path, "remote-desktop/vendor/sunshine/" + path.relative_to(src).as_posix())
        for name, tree in deps.items():
            for path in sorted(tree.rglob("*")):
                if path.is_file() and ".git" not in path.parts:
                    archive.write(path, "dependencies/" + name + "/" + path.relative_to(tree).as_posix())
        for name in ("build-host.py", "build-windows-host.py", "sunshine-submodules.txt"):
            archive.write(root / name, "remote-desktop/" + name)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("cache", "output", "msys"):
        parser.add_argument(name, type=Path)
    args = parser.parse_args()
    build(args.cache.resolve(), args.output.resolve(), args.msys.resolve())
