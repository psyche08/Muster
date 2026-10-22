#!/usr/bin/env python3
"""Symbolicate native Rust/Swift high-CPU samples using UUID-matched Apple dSYMs.

No connection to the phone is needed. Missing symbols stay explicitly unresolved.
"""
import argparse
import json
from pathlib import Path
import re
import subprocess


def symbol_index(root):
    files = [root] if root.is_file() else sorted(root.rglob('Contents/Resources/DWARF/*'))
    result = {}
    for path in files:
        if not path.is_file():
            continue
        output = subprocess.run(['xcrun', 'dwarfdump', '--uuid', str(path)],
                                capture_output=True, text=True, check=True).stdout
        for uuid, arch in re.findall(r'UUID: ([0-9A-Fa-f-]+) \(([^)]+)\)', output):
            result[(uuid.lower(), arch)] = path
    return result


def symbolicate(row, index, cache=None):
    if cache is None:
        cache = {}
    report = row.get('high_cpu_stacks', {})
    images = report.get('images', {})
    for sample in report.get('samples', []):
        for frame in sample.get('frames', []):
            image = images.get(frame.get('image'), {})
            match = index.get((image.get('uuid', '').lower(), image.get('arch')))
            if match is None:
                frame['symbolication'] = 'missing_matching_dsym'
                continue
            pc = int(frame['pc'], 16)
            if frame.get('kind') == 'return':
                pc -= 1  # Attribute the call instruction, not the next function.
            base = int(image['base'], 16)
            size = int(image['size'])
            if not base <= pc < base + size:
                frame['symbolication'] = 'invalid_image_range'
                continue
            key = (str(match), image['arch'], base, pc)
            if key not in cache:
                output = subprocess.run(['xcrun', 'atos', '-o', str(match), '-arch', image['arch'],
                                         '-l', hex(base), hex(pc)], capture_output=True, text=True)
                symbol = output.stdout.strip()
                if output.returncode or not symbol or re.fullmatch(r'0x[0-9a-fA-F]+', symbol):
                    cache[key] = ('unresolved', None)
                else:
                    # atos demangles Swift but can leave Rust's v0/legacy names.
                    token = symbol.split(' ', 1)[0]
                    if token.startswith(('_R', '_ZN')):
                        demangled = subprocess.run(['xcrun', 'llvm-cxxfilt', '--no-strip-underscore', token],
                                                   capture_output=True, text=True)
                        if demangled.returncode == 0 and demangled.stdout.strip():
                            symbol = demangled.stdout.strip() + symbol[len(token):]
                    cache[key] = ('resolved', symbol)
            status, symbol = cache[key]
            frame['symbolication'] = status
            if symbol:
                frame['symbol'] = symbol
    return row


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('report', type=Path)
    parser.add_argument('--symbols', required=True, type=Path,
                        help='xcarchive/dSYM directory, or a DWARF file; UUID and architecture must match')
    args = parser.parse_args()
    index = symbol_index(args.symbols)
    if not index:
        parser.error('No DWARF UUIDs found at --symbols')
    cache = {}
    for line in args.report.read_text().splitlines():
        print(json.dumps(symbolicate(json.loads(line), index, cache), ensure_ascii=False))


if __name__ == '__main__':
    main()
