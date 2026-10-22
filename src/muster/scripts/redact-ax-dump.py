#!/usr/bin/env python3
"""Redact `muster debug ax-dump` output for use as a test fixture.

Keeps the tree, roles, subroles, DOM identifiers and every label string the
Muster source code names (selector vocabulary). Replaces everything else a
person wrote or read (titles, descriptions, values, conversation text) with
stable placeholders: the same original text always becomes the same
placeholder, so uniqueness rules still see the same shape. Only the apps'
own pages keep their URLs (ids inside become fake ids of the same form);
any other link, which conversation text points to, becomes a placeholder
host. A label that
starts with vocabulary keeps it and replaces only the rest, through the same
table, so "More options for <title>" still names its title's placeholder
and "Code, <status>" its mode.

usage: scripts/redact-ax-dump.py crates IN.dump OUT.dump   (from src/muster)
"""
import re
import sys
import pathlib

root, src, dst = map(pathlib.Path, sys.argv[1:4])

# Vocabulary: every string literal in the agent and platform sources.
vocab = set()
for crate in ["muster-agent/src", "muster-platform/src"]:
    for f in (root / crate).rglob("*.rs"):
        # Product code only: test data is not UI vocabulary.
        code = f.read_text().split("#[cfg(test)]")[0]
        for lit in re.findall(r'"((?:[^"\\]|\\.)*)"', code):
            if "{" in lit or len(lit) > 80:
                continue
            vocab.add(lit.replace('\\"', '"'))

PREFIXES = sorted((v for v in vocab if len(v) >= 8), key=len, reverse=True)


def known(text):
    """UI vocabulary, or a vocabulary prefix followed by vocabulary
    ("Switch mode, current mode: " + "Codex")."""
    if text in vocab:
        return True
    return any(text.startswith(p) and text[len(p):].strip() in vocab for p in PREFIXES)


def redact_label(text):
    """Vocabulary as is; a vocabulary prefix, or a vocabulary name before a
    comma, followed by a placeholder for the rest; else a placeholder."""
    if not text or known(text):
        return text
    for p in PREFIXES:
        if text.startswith(p) and len(text) > len(p):
            return p + stable(texts, text[len(p):], "text")
    head, sep, rest = text.partition(", ")
    if sep and head in vocab:
        return head + sep + stable(texts, rest, "text")
    return stable(texts, text, "text")


ATTR = re.compile(r'(\w+)="((?:[^"\\]|\\.)*)"')
UUID = re.compile(r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}")
LONG_ID = re.compile(r"(?<![A-Za-z])(?=[A-Za-z0-9_-]*\d)[A-Za-z0-9_-]{12,}")

texts: dict[str, str] = {}
values: dict[str, str] = {}
ids: dict[str, str] = {}


def stable(table, original, prefix):
    if original not in table:
        table[original] = f"{prefix}-{len(table) + 1}"
    return table[original]


def fake_uuid(m):
    n = len(ids) + 1
    return ids.setdefault(m.group(0), f"00000000-0000-4000-8000-{n:012d}")


def fake_id(m):
    n = len(ids) + 1
    return ids.setdefault(m.group(0), f"id{n:0{max(len(m.group(0)) - 2, 1)}d}"[: len(m.group(0))])


# The pages the apps themselves show; their paths are what selectors read.
APP_HOSTS = {"claude.ai", "chatgpt.com", "chat.openai.com"}
links: dict[str, str] = {}


def url(u):
    head, sep, rest = u.partition("://")
    if not sep:
        return stable(links, u, "link")
    host, slash, path = rest.partition("/")
    own = (
        head == "app"
        or (head == "https" and host in APP_HOSTS)
        or (head == "file" and path.startswith("Applications/"))
    )
    if not own:
        return f"https://{stable(links, u, 'link')}.invalid/"
    u = UUID.sub(fake_uuid, u)
    head, _, rest = u.partition("://")
    host, slash, path = rest.partition("/")
    path = LONG_ID.sub(fake_id, path)
    return f"{head}://{host}{slash}{path}"


out = []
for line in src.read_text().splitlines():
    indent = line[: len(line) - len(line.lstrip())]
    body = line.strip()
    if not body or body.startswith("…"):
        out.append(line)
        continue
    head, _, rest = body.partition(" ")
    attrs = ATTR.findall(rest)
    # A value is kept only when it echoes a label that is itself kept (an empty
    # composer showing its placeholder).
    labels = {v for k, v in attrs if k in ("title", "desc", "label") and known(v)}
    kept = []
    for key, value in attrs:
        if key in ("title", "desc", "label"):
            value = redact_label(value)
        elif key == "value":
            bare = value.rstrip("⏎")
            if bare and bare not in labels and not known(bare):
                value = stable(values, value, "value")
        elif key == "url":
            value = url(value)
        elif key == "id":
            value = LONG_ID.sub(fake_id, value)
        elif key not in ("sel", "enabled", "disabled"):
            raise SystemExit(f"unknown attribute {key!r}: review before redacting")
        kept.append(f'{key}="{value}"')
    out.append(indent + " ".join([head] + kept))
dst.write_text("\n".join(out) + "\n")
print(f"{src.name}: {len(texts)} texts, {len(values)} values, {len(links)} links, {len(ids)} ids replaced")
