#!/usr/bin/env python3
"""Check tracked working-tree text for selected privacy/credential patterns.

No network, history rewrite or credential validation. Never print matched values.
See docs/PUBLIC_CONTENT.md for coverage and intentionally public product identity.
"""
import re
import subprocess
import sys
from pathlib import Path

HOME = re.compile(r"/(?:Users|home)/([^/\s\"'<>`\\]+)")
EMAIL = re.compile(r"(?<![\w/])[\w.+%-]+@([\w.-]+\.[A-Za-z]{2,})")
SAFE_USERS = {"developer", "demo", "me", "test", "user", "alice", "bob", "example"}
RULES = {
    "private-key": re.compile(r"-----BEGIN (?:RSA |EC |OPENSSH |DSA |ENCRYPTED )?PRIVATE KEY-----"),
    "access-token": re.compile(r"\b(?:gh[pousr]_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{50,}|mul_[A-Za-z0-9_-]{20,}|(?:AKIA|ASIA)[A-Z0-9]{16}|sk-(?:ant-api\d+-|proj-)?[A-Za-z0-9_-]{32,}|xox[baprs]-[A-Za-z0-9-]{20,})\b"),
    "personal-ssh-destination": re.compile(r"\b[\w.-]+@(?:[\w.-]+\.)?linsheng\.dev\b"),
    "internal-document-reference": re.compile(r"\bcollabpage/\d+"),
    "corporate-domain": re.compile(r"\b(?:[\w-]+\.)*(?:sankuai|meituan)\.com\b", re.I),
}


def findings(text):
    for name, pattern in RULES.items():
        for match in pattern.finditer(text):
            yield text.count("\n", 0, match.start()) + 1, name
    for match in HOME.finditer(text):
        if match[1] not in SAFE_USERS:
            yield text.count("\n", 0, match.start()) + 1, "personal-home-path"
    for match in EMAIL.finditer(text):
        domain = match[1].lower()
        if (domain in {"example.com", "example.org", "example.net", "example.invalid"}
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
        for line, rule in sorted(set(findings(text))):
            print(f"{name}:{line}: {rule}")
            failures += 1
    print(f"public-content: {scanned} tracked text files; {failures} findings")
    return bool(failures)


if __name__ == "__main__":
    sys.exit(main())
