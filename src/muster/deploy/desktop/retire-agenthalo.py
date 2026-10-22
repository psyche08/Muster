#!/usr/bin/env python3
"""Remove the retired AgentHalo installation, never provider or new Muster data.

Default: inventory only. --apply stops the old user jobs and removes their
application, history, caches, preferences and installer backups. Run separately
as root with --apply --root-only to retire the legacy nethelper. No backups.
"""
import argparse
import os
from pathlib import Path
import shutil
import signal
import subprocess
import time


def remove(path):
    if path.is_symlink() or path.is_file():
        path.unlink()
    elif path.is_dir():
        shutil.rmtree(path)


def user_paths(home):
    paths = [Path('/Applications/AgentHalo.app'), home / 'Applications/AgentHalo.app',
             home / 'Library/Application Support/AgentHalo', home / 'Library/Logs/AgentHalo',
             home / 'Library/LaunchAgents/AgentHaloBackups']
    patterns = {
        'Library/LaunchAgents': ['dev.linsheng.agenthalo*.plist', 'dev.linsheng.muster.agent.plist'],
        'Library/Preferences': ['dev.linsheng.agenthalo*.plist', 'AgentHalo*.plist', 'agenthalo.tests.*.plist'],
        'Library/Preferences/ByHost': ['dev.linsheng.agenthalo*.plist'],
        'Library/Caches': ['dev.linsheng.agenthalo*'],
        'Library/Saved Application State': ['dev.linsheng.agenthalo*.savedState'],
        'Library/HTTPStorages': ['dev.linsheng.agenthalo*'],
        'Library/WebKit': ['dev.linsheng.agenthalo*'],
        'Library/Containers': ['dev.linsheng.agenthalo*'],
    }
    for directory, globs in patterns.items():
        for glob in globs:
            paths.extend((home / directory).glob(glob))
    return sorted(set(p for p in paths if p.exists() or p.is_symlink()))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--apply', action='store_true')
    parser.add_argument('--root-only', action='store_true')
    args = parser.parse_args()
    if args.root_only:
        paths = [Path(p) for p in (
            '/Library/LaunchDaemons/dev.linsheng.muster.nethelper.plist',
            '/Library/PrivilegedHelperTools/dev.linsheng.muster.nethelper',
            '/usr/local/etc/muster/nethelper.conf', '/var/log/muster/nethelper.log')]
        if args.apply:
            if os.geteuid() != 0:
                raise SystemExit('root-only cleanup requires administrator authorization')
            subprocess.run(['launchctl', 'bootout', 'system/dev.linsheng.muster.nethelper'], check=False)
    else:
        if os.geteuid() == 0:
            raise SystemExit('Run user cleanup as the login user; root needs --root-only')
        paths = user_paths(Path.home())
        if args.apply:
            for label in ('dev.linsheng.agenthalo.backend', 'dev.linsheng.agenthalo.tls-broker', 'dev.linsheng.muster.agent'):
                subprocess.run(['launchctl', 'disable', f'gui/{os.getuid()}/{label}'], check=False)
                subprocess.run(['launchctl', 'bootout', f'gui/{os.getuid()}/{label}'], check=False)
            # Only processes whose executable is inside the retired bundle.
            output = subprocess.check_output(['ps', '-axo', 'pid=,comm='], text=True)
            pids = []
            for line in output.splitlines():
                pid, _, executable = line.strip().partition(' ')
                if executable.strip().startswith('/Applications/AgentHalo.app/'):
                    pids.append(int(pid))
            for pid in pids:
                try:
                    os.kill(pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
            if pids:
                time.sleep(2)
                for pid in pids:
                    try:
                        os.kill(pid, 0)
                    except ProcessLookupError:
                        continue
                    raise SystemExit(f'Old process {pid} has not exited; files have not been deleted')
    for path in paths:
        print(('remove ' if args.apply else 'would remove ') + str(path), flush=True)
        if args.apply:
            remove(path)


if __name__ == '__main__':
    main()
