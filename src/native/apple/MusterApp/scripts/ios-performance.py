#!/usr/bin/env python3
"""Extract and summarize Muster's built-in, content-free iOS performance logs."""
import argparse
import datetime as dt
import json
import math
from pathlib import Path
import statistics
import subprocess
import sys


def records(directory):
    rows = {}
    truncated = 0
    paths = [directory] if directory.is_file() else sorted(directory.rglob('*performance*.jsonl'))
    for path in paths:
        data = path.read_bytes()
        lines = data.splitlines(keepends=True)
        for index, line in enumerate(lines):
            if not line.endswith(b'\n') and index == len(lines) - 1:
                truncated += 1
                continue
            value = json.loads(line)
            stamp = value.get('timestamp')
            cpu = value.get('cpu_percent')
            if not isinstance(stamp, (float, int)) or not math.isfinite(stamp):
                raise ValueError(f'Invalid timestamp in {path.name}')
            if not isinstance(cpu, (float, int)) or not math.isfinite(cpu):
                raise ValueError(f'Invalid CPU value in {path.name}')
            rows[(value.get('run_id', ''), stamp)] = value
    return sorted(rows.values(), key=lambda row: row['timestamp']), truncated


def summarize(directory, last_minutes=None):
    rows, truncated = records(directory)
    if last_minutes is not None and rows:
        cutoff = rows[-1]['timestamp'] - last_minutes * 60
        rows = [row for row in rows if row['timestamp'] >= cutoff]
    result = {'samples': len(rows), 'incomplete_tail_records_ignored': truncated,
              'cpu_units': 'percent of one CPU core', 'thermal_units': 'iOS pressure category, not temperature'}
    if not rows:
        return result
    result.update({
        'first_utc': dt.datetime.fromtimestamp(rows[0]['timestamp'], dt.timezone.utc).isoformat(),
        'last_utc': dt.datetime.fromtimestamp(rows[-1]['timestamp'], dt.timezone.utc).isoformat(),
        'builds': sorted({str(row.get('build', 'legacy')) for row in rows}),
        'screens': {},
        'stack_captures': sum(bool(row.get('high_cpu_stacks')) for row in rows),
        'stack_samples': sum(len(row.get('high_cpu_stacks', {}).get('samples', [])) for row in rows),
    })
    for screen in sorted({row['screen'] for row in rows}):
        group = [row for row in rows if row['screen'] == screen]
        cpu = sorted(row['cpu_percent'] for row in group)
        result['screens'][str(screen)] = {
            'samples': len(group), 'cpu_median': round(statistics.median(cpu), 2),
            'cpu_p95': round(cpu[max(0, math.ceil(len(cpu) * .95) - 1)], 2),
            'cpu_max': round(max(cpu), 2),
            'thermal_max': max(row['thermal_state'] for row in group),
            'memory_max_bytes': max((row.get('memory_footprint_bytes') or 0) for row in group),
            'snapshots': sum(row.get('snapshots', 0) for row in group),
            'snapshot_bytes': sum(row.get('snapshot_bytes', 0) for row in group),
        }
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    extract = commands.add_parser('extract')
    extract.add_argument('--device', required=True, help='Paired iPhone UDID or CoreDevice name')
    extract.add_argument('--output', type=Path, required=True, help='New local output directory')
    analyze = commands.add_parser('summarize')
    analyze.add_argument('directory', type=Path)
    analyze.add_argument('--last-minutes', type=float)
    args = parser.parse_args()
    if args.command == 'extract':
        if args.output.exists():
            parser.error('--output must be a new directory so earlier evidence is preserved')
        args.output.mkdir(parents=True, mode=0o700)
        destination = args.output / 'MusterDiagnostics'
        result = subprocess.run([
            'xcrun', 'devicectl', 'device', 'copy', 'from', '--device', args.device,
            '--domain-type', 'appDataContainer', '--domain-identifier', 'dev.linsheng.muster',
            '--source', 'Library/Caches/MusterDiagnostics', '--destination', str(destination),
            '--timeout', '45',
        ], stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
        (args.output / 'extraction.log').write_text(result.stdout)
        if result.returncode:
            print(f'Extraction failed; see {args.output / "extraction.log"}. iOS may require an interactive unlock.', file=sys.stderr)
            return result.returncode
        report = summarize(destination)
        (args.output / 'summary.json').write_text(json.dumps(report, indent=2) + '\n')
    else:
        if args.last_minutes is not None and args.last_minutes <= 0:
            parser.error('--last-minutes must be positive')
        report = summarize(args.directory, args.last_minutes)
    print(json.dumps(report, indent=2))
    return 0


if __name__ == '__main__':
    sys.exit(main())
