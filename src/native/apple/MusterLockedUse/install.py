#!/usr/bin/env python3
"""Install a signed authorization adapter; preserve the existing password branch.
Run explicitly as root with --install after reviewing the default plan output.
Never unlocks Keychain.
"""
import argparse
import os
from pathlib import Path
import plistlib
import shutil
import stat
import subprocess
import tempfile

RIGHT = "system.login.screensaver"
RULE = "dev.linsheng.muster.locked-use"
BASE = Path("/Library/Application Support/Muster/LockedUse")
DEST = Path("/Library/Security/SecurityAgentPlugins/MusterLockedUse.bundle")

def screensaver(original):
    value = dict(original)
    rules = value.get("rule")
    if value.get("class") != "rule" or not isinstance(rules, list) or "use-login-window-ui" not in rules:
        raise ValueError("unsupported screensaver policy; leave the authorization database unchanged")
    if "k-of-n" in value and (type(value["k-of-n"]) is not int or value["k-of-n"] != 1):
        raise ValueError("refusing to weaken the existing screensaver policy")
    if "k-of-n" not in value and rules != ["use-login-window-ui"]:
        raise ValueError("unknown screensaver rule composition")
    value["rule"] = [RULE] + [r for r in rules if r != RULE]
    value["k-of-n"] = 1
    return value

def read_right(name):
    return plistlib.loads(subprocess.check_output(["/usr/bin/security", "authorizationdb", "read", name]))

def write_right(name, value):
    subprocess.run(["/usr/bin/security", "authorizationdb", "write", name], input=plistlib.dumps(value), check=True)

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("bundle", type=Path)
    parser.add_argument("socket", type=Path)
    parser.add_argument("uid", type=int)
    parser.add_argument("--install", action="store_true")
    args = parser.parse_args()
    if args.uid <= 0 or not args.socket.is_absolute() or len(os.fsencode(args.socket)) >= 104:
        parser.error("a non-root console UID and bounded absolute socket path are required")
    original = read_right(RIGHT)
    modified = screensaver(original)
    requirement = 'identifier "dev.linsheng.muster.locked-use" and anchor apple generic and certificate leaf[subject.OU] = "89LGY6BD53"'
    subprocess.run(["/usr/bin/codesign", "--verify", "--strict", "-R", "=" + requirement, str(args.bundle)], check=True)
    print(plistlib.dumps(modified).decode())
    if not args.install:
        print("Plan only: no system changes")
        return
    if os.geteuid() != 0:
        parser.error("installation requires root")
    for directory in [BASE.parent.parent, BASE.parent, BASE, DEST.parent]:
        if not directory.exists():
            directory.mkdir(mode=0o700)
        meta = directory.lstat()
        if not stat.S_ISDIR(meta.st_mode) or meta.st_uid != 0 or meta.st_mode & 0o022:
            raise RuntimeError(f"refusing insecure system directory: {directory}")
    os.chmod(BASE, 0o700)
    if DEST.is_symlink():
        raise RuntimeError("refusing a symlink at the plugin installation path")
    backup = BASE / "original-screensaver.plist"
    if not backup.exists():
        with backup.open("xb") as f:
            f.write(plistlib.dumps(original))
        backup.chmod(0o600)
    # The installer is explicit; keep no old program copy. The original OS
    # policy is retained separately so it can be restored without guessing.
    if DEST.exists():
        shutil.rmtree(DEST)
    shutil.copytree(args.bundle, DEST)
    for root, dirs, files in os.walk(DEST):
        os.chown(root, 0, 0)
        os.chmod(root, 0o755)
        for name in files:
            path = Path(root) / name
            os.chown(path, 0, 0)
            os.chmod(path, 0o755 if name == "MusterLockedUse" else 0o644)
    subprocess.run(["/usr/bin/codesign", "--verify", "--strict", "-R", "=" + requirement, str(DEST)], check=True)
    config = BASE / f"{args.uid}.plist"
    with tempfile.NamedTemporaryFile(dir=BASE, delete=False) as f:
        f.write(plistlib.dumps({"socket": str(args.socket)}))
        os.fsync(f.fileno())
        temp = f.name
    os.replace(temp, config)
    try:
        write_right(RULE, {"class": "evaluate-mechanisms", "mechanisms": ["MusterLockedUse:invoke,privileged"], "shared": False, "tries": 1})
        write_right(RIGHT, modified)
        actual = read_right(RIGHT)
        if actual.get("rule") != modified["rule"] or actual.get("k-of-n") != 1:
            raise RuntimeError("authorization policy readback did not match")
    except BaseException:
        write_right(RIGHT, original)
        raise
    print("Installed; signature and authorization policy read back successfully")

if __name__ == "__main__":
    main()
