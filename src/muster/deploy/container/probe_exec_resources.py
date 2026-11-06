#!/usr/bin/env python3
"""Bounded no-op exec regression probe. Does not stop or modify the workload."""
import argparse
import ctypes
import json
import re
import subprocess
import time


def run(*args):
    return subprocess.check_output(args, text=True, timeout=60)


def snapshot(pid):
    fields = (ctypes.c_uint64 * 64)()
    if ctypes.CDLL('/usr/lib/libproc.dylib').proc_pid_rusage(pid, 0, ctypes.byref(fields)):
        raise RuntimeError('Cannot read runtime process usage')
    # Darwin rusage_info_v0: UUID occupies two UInt64s, physical footprint is
    # index 9, process start time is index 10. Never compare RSS under compression.
    summary = run('heap', '--noContent', '-s', '-H', str(pid))
    counts = {
        match[2]: int(match[1]) for match in re.finditer(
            r'^\s*(\d+)\s+\S+\s+[\d.]+\s+(LinuxProcess|NIOHTTP2Handler|SocketChannel)\s',
            summary, re.MULTILINE)
    }
    if 'LinuxProcess' not in counts:
        raise RuntimeError('Heap type information unavailable; no result claimed')
    return {'started': fields[10], 'footprint': fields[9], 'objects': counts}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('container')
    parser.add_argument('runtime_pid', type=int)
    parser.add_argument('--count', type=int, default=128)
    args = parser.parse_args()
    if not 32 <= args.count <= 256:
        parser.error('count must be between 32 and 256')
    for _ in range(8):
        run('container', 'exec', args.container, '/bin/true')
    before = snapshot(args.runtime_pid)
    for _ in range(args.count):
        run('container', 'exec', args.container, '/bin/true')
    time.sleep(2)
    after = snapshot(args.runtime_pid)
    if before['started'] != after['started']:
        raise RuntimeError('Runtime restarted during measurement; no result claimed')
    deltas = {k: after['objects'].get(k, 0) - v for k, v in before['objects'].items()}
    print(json.dumps({'execs': args.count, 'footprint_delta_bytes':
                      after['footprint'] - before['footprint'], 'object_deltas': deltas}))
    # Some allocator high-water growth is normal. Retained per-exec process and
    # channel objects are not. Run against an idle disposable container to avoid
    # unrelated workload changes skewing this measurement.
    if any(delta > 8 for delta in deltas.values()):
        raise SystemExit('FAIL: completed exec objects accumulated')
    print('PASS: completed exec object counts stayed bounded in this sample')


if __name__ == '__main__':
    main()
