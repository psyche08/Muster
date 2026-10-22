#!/usr/bin/env python3
"""Verify release ancestry, including an explicitly bound root-only import."""
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys

BASELINE = 'src/muster/deploy/desktop/source-baseline.json'
LEDGER = 'src/muster/deploy/desktop/released-commits.txt'


def git(repo, *args):
    return subprocess.check_output(['git', '-C', str(repo), *args], stderr=subprocess.DEVNULL)


def tree_digest(repo, tree):
    entries = git(repo, 'ls-tree', '-rz', '--full-tree', tree).split(b'\0')
    # Exclude only the self-referential manifest; bind names, modes and blob IDs.
    data = b'\0'.join(e for e in entries if e and e.split(b'\t', 1)[1] != BASELINE.encode())
    return hashlib.sha256(data + b'\0').hexdigest()


def releases(text):
    rows = []
    versions = set()
    for line in text.splitlines():
        if not line.strip() or line.startswith('#'):
            continue
        fields = line.split()
        if len(fields) < 2 or not re.fullmatch(r'[0-9a-f]{7,40}', fields[1]):
            raise ValueError('invalid release ledger row')
        version, ref = fields[:2]
        if version in versions:
            raise ValueError('duplicate release version')
        versions.add(version)
        rows.append((version, ref))
    if not rows:
        raise ValueError('empty release ledger')
    return rows


def historical_releases(repo, commit):
    roots = git(repo, 'rev-list', '--max-parents=0', commit).decode().splitlines()
    baseline_roots = []
    for root in roots:
        try:
            raw = git(repo, 'show', f'{root}:{BASELINE}')
        except subprocess.CalledProcessError:
            continue
        baseline_roots.append((root, raw))
    if not baseline_roots:
        # Original repository: every release must pass normal Git ancestry.
        return set()
    if len(roots) != 1 or len(baseline_roots) != 1:
        raise ValueError('snapshot history must have exactly one root')
    root, raw = baseline_roots[0]
    baseline = json.loads(raw)
    if (not isinstance(baseline, dict) or baseline.get('schema') != 1
            or not isinstance(baseline.get('archived_source'), str)
            or not re.fullmatch(r'[0-9a-f]{40}', baseline['archived_source'])
            or baseline.get('root_tree_sha256') != tree_digest(repo, root)):
        raise ValueError('invalid or mismatched source baseline')
    return set(releases(git(repo, 'show', f'{root}:{LEDGER}').decode()))


def verify(repo, commit, ledger_text):
    historical = historical_releases(repo, commit)
    for version, ref in releases(ledger_text):
        if (version, ref) in historical:
            continue
        result = subprocess.run(['git', '-C', str(repo), 'merge-base', '--is-ancestor', ref, commit],
                                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        if result.returncode != 0:
            raise ValueError(f'source does not contain desktop {version} ({ref})')


def main():
    if len(sys.argv) != 3:
        print('usage: verify-release-ancestry.py COMMIT RELEASE_LEDGER', file=sys.stderr)
        return 2
    repo = Path(__file__).resolve().parent
    try:
        verify(repo, sys.argv[1], Path(sys.argv[2]).read_text())
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        print(f'release ancestry: {error}', file=sys.stderr)
        return 1
    print('release ancestry verified')
    return 0


if __name__ == '__main__':
    sys.exit(main())
