#!/usr/bin/env python3
"""Check tracked working-tree text for selected privacy/credential patterns.

No network, history rewrite or credential validation. Never print matched values.
See docs/PUBLIC_CONTENT.md for coverage and intentionally public product identity.
"""
import hashlib
import re
import subprocess
import sys
from pathlib import Path

HOME = re.compile(r"/(?:Users|home)/([^/\s\"'<>`\\]+)")
EMAIL = re.compile(r"(?<![\w/])[\w.+%-]+@([\w.-]+\.[A-Za-z]{2,})")
SAFE_USERS = {"developer", "demo", "me", "test", "user", "alice", "bob", "example"}
RESERVED = ("example.com", "example.org", "example.net", "example.invalid")
RULES = {
    "private-key": re.compile(r"-----BEGIN (?:RSA |EC |OPENSSH |DSA |ENCRYPTED )?PRIVATE KEY-----"),
    "access-token": re.compile(r"\b(?:gh[pousr]_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{50,}|mul_[A-Za-z0-9_-]{20,}|(?:AKIA|ASIA)[A-Z0-9]{16}|sk-(?:ant-api\d+-|proj-)?[A-Za-z0-9_-]{32,}|xox[baprs]-[A-Za-z0-9-]{20,})\b"),
    "personal-ssh-destination": re.compile(r"\b[\w.-]+@(?:[\w.-]+\.)?linsheng\.dev\b"),
    "internal-document-reference": re.compile(r"\bcollabpage/\d+"),
    "corporate-domain": re.compile(r"\b(?:[\w-]+\.)*(?:sankuai|meituan)\.com\b", re.I),
}
VENDOR_PREFIX = "addons/remote-desktop/vendor/"
VENDOR_PUBLIC_LINES = {
    ("moonlight-pc/app/SDL_GameControllerDB/LICENSE", "contact-email"):
        ("c5229ca3a72a0b53c115950de7db21d93ed73b2d76c3f6866d9e8a1262537cfa",),
    ("moonlight-pc/app/deploy/linux/com.moonlight_stream.Moonlight.appdata.xml", "contact-email"):
        ("7414e85d6e527311695e0baf80a32d8b5b431b447c6b5006ec1b9464aeea2585",),
    ("sunshine/.github/workflows/ci-alpine.yml", "personal-home-path"):
        ("7768eaffa40856fe9bd2b1aaa58968aa69728abd47ff27cec8e5d220a3c073c7",),
    ("sunshine/DOCKER_README.md", "personal-home-path"):
        ("f686e69e45b7b39bff27c5d1c110dc2445aae27ad76a3c73a43652395471113b",),
    ("sunshine/cmake/FindWayland.cmake", "contact-email"):
        ("8d93e6c22002de6f1179eaa976c4fc7ab98fe4a1a2b2c2aa2a209c2533f013fb",
         "d5a93e0cf4648c6c11b24ad0a6c86ae1066ea762ab33a8bc246b30838312e99f"),
    ("sunshine/cmake/dependencies/FindOpus.cmake", "contact-email"):
        ("479f8cf31788c81a658a3ab5ce98ec0bbe6d12c432c7269df11a14a0a2fec47f",),
    ("sunshine/cmake/dependencies/glad.cmake", "personal-home-path"):
        ("0464f48bc6e3b6c36fccb051efe8c3442e0777b3cc5b98f43525df4a0b50ea0d",),
    ("sunshine/docker/debian.dockerfile", "personal-home-path"):
        ("c3befcffaee2036c934de188396badbf4a673919d716031cf990ff2a97330847",),
    ("sunshine/packaging/linux/dev.lizardbyte.app.Sunshine.desktop", "contact-email"):
        ("8a46be36edca634f5d4d859654235b2912ee5e4502f844dc167cdfd1bd1f2bbd",),
    ("sunshine/pyproject.toml", "contact-email"):
        ("2fedf38dfa898b276582584768f49e41faaac22590cdb69280763e1edc0c754d",),
    ("sunshine/src/confighttp.cpp", "personal-home-path"):
        ("9a7b30227fb743c9433caf0e841c6399d424f8df53e5e8845f503eca0590df1a",),
}


def recognized_vendor_line(name, rule, line):
    if not name.startswith(VENDOR_PREFIX) or rule not in {"contact-email", "personal-home-path"}:
        return False
    allowed = VENDOR_PUBLIC_LINES.get((name[len(VENDOR_PREFIX):], rule), ())
    return bool(allowed) and hashlib.sha256(line.encode("utf-8")).hexdigest() in allowed


def findings(text):
    for name, pattern in RULES.items():
        for match in pattern.finditer(text):
            yield text.count("\n", 0, match.start()) + 1, name
    for match in HOME.finditer(text):
        if match[1] not in SAFE_USERS:
            yield text.count("\n", 0, match.start()) + 1, "personal-home-path"
    for match in EMAIL.finditer(text):
        domain = match[1].lower()
        # RFC 2606 reserves these names and everything under them, so a
        # fixture host such as api.example.com is as synthetic as the name
        # itself; URL userinfo (user:pass@api.example.com) reads as an address.
        if (domain in RESERVED
                or domain.endswith(tuple(f".{name}" for name in RESERVED))
                or domain.endswith((".example", ".invalid"))
                or match[0] == "git@github.com"
                or re.fullmatch(r"icon_\d+x\d+@[12]x\.png", match[0])):
            continue
        yield text.count("\n", 0, match.start()) + 1, "contact-email"


def main():
    root = Path(subprocess.check_output(["git", "rev-parse", "--show-toplevel"], text=True).strip())
    tracked = subprocess.check_output(["git", "ls-files", "-z"], cwd=root).decode().split("\0")
    failures = 0
    scanned = 0
    for name in tracked:
        if not name:
            continue
        path = root / name
        # Never follow a tracked symlink into local credentials or other state.
        if path.is_symlink() or not path.is_file():
            continue
        try:
            text = path.read_bytes().decode("utf-8")
        except UnicodeError:
            continue
        if "\0" in text:
            continue
        scanned += 1
        matches = sorted(set(findings(text)))
        if not matches:
            continue
        # Git may check out the same reviewed line with CRLF on Windows.
        lines = text.replace("\r\n", "\n").split("\n")
        for line, rule in matches:
            if recognized_vendor_line(name, rule, lines[line - 1]):
                continue
            print(f"{name}:{line}: {rule}")
            failures += 1
    print(f"public-content: {scanned} tracked text files; {failures} findings")
    return bool(failures)


if __name__ == "__main__":
    sys.exit(main())
