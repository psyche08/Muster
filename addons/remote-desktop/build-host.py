"""Compile the downstream macOS capture engine for the enclosing Muster bundle."""
import argparse
import configparser
import hashlib
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parent
VENDOR = ROOT / "vendor/sunshine"
FFMPEG_TAG = "v2026.910.121303"
FFMPEG_SHA = {
    "arm64": "dac277b307498011e747019941e794514b2824ef73a574fec33f0cf85fbcd291",
    "x86_64": "cb63a85c4599d94b547d68b240d96c6cc146eba33e6bf22b4d591a1bf52ecfc7",
}
# Only these top-level projects participate in the macOS capture build.
MODULES = ("Simple-Web-Server", "TPCircularBuffer", "build-deps", "libdisplaydevice",
           "libvirtualhid", "lizardbyte-common", "moonlight-common-c")
NESTED = {"libdisplaydevice": ["third-party/lizardbyte-common"],
          "libvirtualhid": ["third-party/lizardbyte-common"],
          "moonlight-common-c": ["enet", "nanors"]}
CODECS = ["third-party/FFmpeg/" + name for name in ("FFmpeg", "SVT-AV1", "x264", "x265_git",
                                                     "AMF", "Vulkan-Headers", "Vulkan-Loader")]


def run(*args, **kwargs):
    subprocess.run(args, check=True, **kwargs)


def output(*args):
    return subprocess.check_output(args, text=True).strip()


def digest(path):
    h = hashlib.sha256()
    with path.open("rb") as file:
        for chunk in iter(lambda: file.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def download(cache, name, url, checksum):
    archive = cache / name
    if not archive.exists():
        with tempfile.TemporaryDirectory(dir=cache) as stage:
            file = Path(stage) / name
            run("curl", "--fail", "--location", "--retry", "3", "--output", file, url)
            if digest(file) != checksum:
                raise ValueError("Unexpected download: " + name)
            file.replace(archive)
    elif digest(archive) != checksum:
        raise ValueError("Unexpected cached archive: " + name)
    return archive


def checkout(cache, url, revision, nested):
    dest = cache / revision
    if not dest.exists():
        with tempfile.TemporaryDirectory(dir=cache) as stage:
            repo = Path(stage) / "repo"
            run("git", "init", "--quiet", repo)
            run("git", "-C", repo, "remote", "add", "origin", url)
            run("git", "-C", repo, "fetch", "--quiet", "--depth=1", "origin", revision)
            run("git", "-C", repo, "checkout", "--quiet", "--detach", revision)
            repo.rename(dest)
    if output("git", "-C", dest, "rev-parse", "HEAD") != revision:
        raise ValueError("Incorrect dependency revision in " + str(dest))
    if nested:
        run("git", "-C", dest, "submodule", "update", "--init", "--depth=1", "--", *nested)
    if output("git", "-C", dest, "status", "--porcelain", "--untracked-files=all"):
        raise ValueError("Modified dependency in " + str(dest))
    return dest


def host_sources(cache, modules=MODULES, *, windows_user_input=False):
    # Separate source revisions keep stale build objects and removed files out
    # of later builds. Include local edits so the normal dev build tests them.
    if (ROOT.parents[1] / ".git").exists():
        names = subprocess.check_output(["git", "-C", VENDOR, "ls-files", "--cached", "-z"])
        files = [VENDOR / os.fsdecode(name) for name in names.split(b"\0") if name]
    else:
        # An exported source tree from the companion archive has no git index.
        files = sorted(p for p in VENDOR.rglob("*") if p.is_file() and ".git" not in p.parts)
    h = hashlib.sha256()
    for p in files:
        h.update(p.relative_to(VENDOR).as_posix().encode() + b"\0")
        h.update(bytes.fromhex(digest(p)))
    pins = (ROOT / "sunshine-submodules.txt").read_bytes()
    h.update(pins)
    if windows_user_input:
        h.update(b"windows-user-input-v1")
    if modules != MODULES:
        h.update(" ".join(modules).encode())
    src = cache / ("sunshine-" + h.hexdigest()[:20])
    config = configparser.ConfigParser(interpolation=None)
    config.read(VENDOR / ".gitmodules")
    revisions = dict((path, sha) for sha, path in (line.split() for line in pins.decode().splitlines()))
    deps = {}
    for name in modules:
        path = "third-party/" + name
        deps[name] = checkout(cache, config['submodule "' + path + '"']["url"],
                              revisions[path], NESTED.get(name, []))
    if not src.exists():
        with tempfile.TemporaryDirectory(dir=cache) as stage:
            tree = Path(stage) / "src"
            for path in files:
                target = tree / path.relative_to(VENDOR)
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(path, target)
            for name, dep in deps.items():
                target = tree / "third-party" / name
                if os.name == "nt" or (windows_user_input and name == "libvirtualhid"):
                    shutil.copytree(dep, target, ignore=shutil.ignore_patterns(".git"))
                else:
                    target.symlink_to(dep, target_is_directory=True)
                if windows_user_input and name == "libvirtualhid":
                    # The packaged backend never opens the privileged HID broker.
                    backend = target / "src/platform/windows/windows_backend.cpp"
                    backend.write_bytes(backend.read_bytes().replace(b"\r\n", b"\n"))
                    run("git", "apply", str((tree / "muster-windows-user-input.patch").resolve()),
                        cwd=target, env=dict(os.environ, GIT_CEILING_DIRECTORIES=str(target.parent.resolve())))
                    if digest(backend) != "8d82350ddaf9df44a83ff2abb30f7cff6f544034941b32d28de1b5231de51f90":
                        raise ValueError("Windows user-mode input backend was not prepared")
                    shutil.copy2(tree / "src/platform/windows/muster_ordinary_desktop.h",
                                 target / "src/include/libvirtualhid")
            tree.rename(src)
    return src, deps


def extract(archive, destination, top):
    if not (destination / top).is_dir():
        with tempfile.TemporaryDirectory(dir=destination) as stage:
            if os.name == "nt" and top in ("boost-1.89.0", "json"):
                # Windows bsdtar can reject UTF-8 filenames under legacy code pages.
                with tarfile.open(archive, "r|xz") as source:
                    for member in source:
                        path = (Path(stage) / member.name).resolve()
                        if not path.is_relative_to(stage) or not (member.isdir() or member.isfile()):
                            raise ValueError("Unexpected source entry: " + member.name)
                        source.extract(member, stage)
            else:
                run("tar", "-xf", archive, "-C", stage)
            (Path(stage) / top).rename(destination / top)
    return destination / top


def boost_source(cache, archive):
    tree = extract(archive, cache, "boost-1.89.0")
    # The umbrella header is one directory above the Locale header group.
    cmake = tree / "libs/locale/CMakeLists.txt"
    source = cmake.read_text()
    fixed = source.replace('source_group(TREE "${CMAKE_CURRENT_SOURCE_DIR}/include/boost/locale"',
                           'source_group(TREE "${CMAKE_CURRENT_SOURCE_DIR}/include/boost"')
    if fixed != source:
        cmake.write_text(fixed)
    return tree


def cmake_sources(cache):
    boost = download(cache, "boost-1.89.0.tar.xz",
                     "https://github.com/boostorg/boost/releases/download/boost-1.89.0/boost-1.89.0-cmake.tar.xz",
                     "67acec02d0d118b5de9eb441f5fb707b3a1cdd884be00ca24b9a73c995511f74")
    json = download(cache, "json-3.11.3.tar.xz",
                    "https://github.com/nlohmann/json/releases/download/v3.11.3/json.tar.xz",
                    "d6c65aca6b1ed68e7a182f4757257b107ae403032760ed6ef121c9d55e81757d")
    return boost, json


def build(cache, executable, sources, identity):
    arch = platform.machine()
    if platform.system() != "Darwin" or arch not in FFMPEG_SHA:
        raise ValueError("Build the native macOS capture engine on an Intel or Apple Silicon Mac")
    cache.mkdir(parents=True, exist_ok=True)
    src, deps = host_sources(cache)
    name = "Darwin-" + arch + "-ffmpeg.tar.gz"
    ffmpeg = download(cache, name, "https://github.com/LizardByte/build-deps/releases/download/" +
                      FFMPEG_TAG + "/" + name, FFMPEG_SHA[arch])
    libdir = cache / ("media-" + FFMPEG_SHA[arch][:16])
    libdir.mkdir(exist_ok=True)
    prepared = extract(ffmpeg, libdir, "ffmpeg")
    openssl = download(cache, "openssl-3.6.5.tar.gz",
                       "https://codeload.github.com/openssl/openssl/tar.gz/refs/tags/openssl-3.6.5",
                       "f3091b7ccb41a11d877b7406b7afc9dd9c5003d299c6bb33cff882127bc06c91")
    opus = download(cache, "opus-1.6.1.tar.gz", "https://downloads.xiph.org/releases/opus/opus-1.6.1.tar.gz",
                    "6ffcb593207be92584df15b32466ed64bbec99109f007c82205f0194572411a1")
    boost, json = cmake_sources(cache)
    # Keep minimum deployment settings in the cache identity too.
    minimum = os.environ.get("MACOSX_DEPLOYMENT_TARGET") or "14.0"
    config = cache / (arch + "-" + minimum)
    config.mkdir(exist_ok=True)
    prefix = config / "prefix"
    ssl_src = extract(openssl, cache, "openssl-openssl-3.6.5")
    opus_src = extract(opus, cache, "opus-1.6.1")
    jobs = os.environ.get("CMAKE_BUILD_PARALLEL_LEVEL", "6")
    if not (prefix / "lib/libssl.a").exists() or not (prefix / "lib/libcrypto.a").exists():
        obj = config / "openssl"
        obj.mkdir(exist_ok=True)
        target = "darwin64-" + ("arm64" if arch == "arm64" else "x86_64") + "-cc"
        run(ssl_src / "Configure", target, "no-shared", "no-tests", "no-module",
            "--prefix=" + str(prefix), "--libdir=lib", "-mmacosx-version-min=" + minimum, cwd=obj)
        run("make", "-j" + jobs, "build_sw", cwd=obj)
        run("make", "install_sw", cwd=obj)
    run("cmake", "-S", opus_src, "-B", config / "opus", "-DCMAKE_BUILD_TYPE=Release",
        "-DCMAKE_INSTALL_PREFIX=" + str(prefix), "-DBUILD_SHARED_LIBS=OFF",
        "-DCMAKE_OSX_DEPLOYMENT_TARGET=" + minimum)
    run("cmake", "--build", config / "opus", "--target", "install", "--parallel", jobs)
    obj = config / src.name
    run("cmake", "-S", src, "-B", obj, "-DCMAKE_BUILD_TYPE=Release", "-DBUILD_TESTS=OFF",
        "-DMUSTER_MAC_CAPTURE=ON", "-DBUILD_DOCS=OFF", "-DSUNSHINE_ENABLE_TRAY=OFF", "-DBOOST_USE_STATIC=ON",
        "-DOPENSSL_USE_STATIC_LIBS=TRUE", "-DOPENSSL_ROOT_DIR=" + str(prefix),
        "-DOPENSSL_SSL_LIBRARY=" + str(prefix / "lib/libssl.a"),
        "-DOPENSSL_CRYPTO_LIBRARY=" + str(prefix / "lib/libcrypto.a"),
        "-DCMAKE_PREFIX_PATH=" + str(prefix), "-DOpus_LIBRARY=" + str(prefix / "lib/libopus.a"),
        "-DFFMPEG_PREPARED_BINARIES=" + str(prepared), "-DCMAKE_OSX_DEPLOYMENT_TARGET=" + minimum,
        "-DFETCHCONTENT_SOURCE_DIR_BOOST=" + str(boost_source(cache, boost)),
        "-DFETCHCONTENT_SOURCE_DIR_JSON=" + str(extract(json, cache, "json")))
    run("cmake", "--build", obj, "--target", "sunshine", "--parallel", jobs)
    exe = obj / "muster-stream-host"
    for line in output("otool", "-L", exe).splitlines()[1:]:
        library = line.strip().split(" (compatibility", 1)[0]
        if not library.startswith(("/usr/lib/", "/System/Library/")):
            raise ValueError("Capture host requires an external runtime library: " + library)
    cache_key = src.name + "-" + digest(Path(__file__))[:20] + "-" + arch
    viewer_sources = [ROOT / "build-apple-viewer.py", ROOT / "CMakeLists.txt", ROOT / "viewer/apple.c"]
    viewer_core = ROOT / "vendor/moonlight-pc/moonlight-common-c/moonlight-common-c"
    viewer_sources.extend(sorted(p for p in viewer_core.rglob("*") if p.is_file()))
    viewer_digest = hashlib.sha256()
    for path in viewer_sources:
        viewer_digest.update(path.relative_to(ROOT).as_posix().encode())
        viewer_digest.update(bytes.fromhex(digest(path)))
    offer = cache / (cache_key + "-" + viewer_digest.hexdigest()[:20] + "-sources.tar.gz")
    if not offer.is_file():
        # The FFmpeg build recipes reference gitlinks. Include those exact
        # sources with the downstream code, compiler inputs and dependency pins.
        checkout(cache, output("git", "-C", deps["build-deps"], "remote", "get-url", "origin"),
                 deps["build-deps"].name, CODECS)
        def keep(entry):
            return None if ".git" in Path(entry.name).parts else entry
        with tempfile.TemporaryDirectory(dir=cache) as temp:
            archive = Path(temp) / offer.name
            with tarfile.open(archive, "w:gz", compresslevel=1) as tar:
                # Dependency symlinks in the build tree refer to a private
                # cache. Store their real contents separately below.
                def downstream(entry):
                    return None if entry.issym() else entry
                tar.add(src, "remote-desktop/vendor/sunshine", filter=downstream)
                for path in viewer_sources:
                    tar.add(path, "remote-desktop/" + path.relative_to(ROOT).as_posix())
                for name in ("moonlight_loopback.c", "sunshine_bootstrap.cpp"):
                    tar.add(ROOT / "tests" / name, "remote-desktop/tests/" + name)
                tar.add(ROOT / "build-host.py", "remote-desktop/build-host.py")
                tar.add(ROOT / "sunshine-submodules.txt", "remote-desktop/sunshine-submodules.txt")
                for name, tree in deps.items():
                    tar.add(tree, "dependencies/" + name, filter=keep)
                for path in (openssl, opus, boost, json, ffmpeg):
                    tar.add(path, "downloads/" + path.name)
            archive.replace(offer)
    contents = executable.parent.parent
    if executable != contents / "MacOS/muster-stream-host" or contents.name != "Contents":
        raise ValueError("Expected a capture tool in the enclosing app's Contents/MacOS")
    executable.parent.mkdir(parents=True, exist_ok=True)
    sources.parent.mkdir(parents=True, exist_ok=True)
    resources = contents / "Resources/MusterDesktop"
    resources.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=executable.parent) as temp:
        candidate = Path(temp) / executable.name
        shutil.copy2(exe, candidate)
        run("codesign", "--force", "--options", "runtime", "--identifier",
            "dev.linsheng.muster.desktop-host", "--sign", identity, candidate)
        run("codesign", "--verify", "--strict", candidate)
        run("ditto", obj / "assets", resources / "assets")
        for name in ("LICENSE", "NOTICE"):
            shutil.copy2(src / name, resources / ("Sunshine-" + name + ".txt"))
        with tempfile.TemporaryDirectory(dir=sources.parent) as work:
            staged = Path(work) / sources.name
            shutil.copy2(offer, staged)
            staged.replace(sources)
        candidate.replace(executable)
        # Keep the GPL controller in its own executable on macOS.
        import runpy
        viewer = runpy.run_path(str(Path(__file__).with_name("build-apple-viewer.py")))
        viewer_dir = viewer["build"](cache, "macosx", arch)
        shutil.copy2(viewer_core / "LICENSE.txt", resources / "Moonlight-LICENSE.txt")
        controller = executable.with_name("muster-moonlight")
        shutil.copy2(viewer_dir / "muster-moonlight", controller)
        run("codesign", "--force", "--options", "runtime", "--sign", identity, controller)
        # An incremental Xcode or developer build may still have the former bundle.
        legacy = contents / "Helpers/Sunshine.app"
        if legacy.is_dir():
            legacy.rename(Path(temp) / "obsolete.app")


if __name__ == "__main__":
    import fcntl
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("cache", type=Path)
    p.add_argument("executable", type=Path)
    p.add_argument("sources", type=Path)
    p.add_argument("--identity", default="-")
    args = p.parse_args()
    cache = args.cache.resolve()
    cache.mkdir(parents=True, exist_ok=True)
    with (cache / "build.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        build(cache, args.executable.resolve(), args.sources.resolve(), args.identity)
