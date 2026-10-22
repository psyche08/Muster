#!/usr/bin/env python3
"""Host side of the ACP stdio bridge.

The Multica daemon runs in a container and spawns its runtime command there.
Muster runs on the host. This listens on the container bridge, and for each
connection spawns the host's `muster acp` and pumps bytes both ways, so the
ACP stream crosses the boundary and musterd's own socket never leaves the
host.

The first line a client sends must be the shared token, so that merely
reaching the port is not enough to drive Muster. Everything after that line
is the ACP stream.

After the token line the client sends the task's own `MULTICA_*` variables,
one `NAME=value` per line, terminated by a blank line. They are put on the
spawned process's environment, because the turn's brief tells the agent to
use the scheduler's CLI with that credential — without it the agent can
neither read its issue nor post its result. `extra-path` is prepended to that
process's PATH so the CLI is findable on the host.

Usage: acp-bridge.py <bind-ip> <port> <token> <muster-binary> [agent] [extra-path]
"""
import os
import selectors
import socket
import subprocess
import sys
import threading

BIND, PORT, TOKEN, MUSTER = sys.argv[1], int(sys.argv[2]), sys.argv[3], sys.argv[4]
AGENT = sys.argv[5] if len(sys.argv) > 5 else "codex"
EXTRA_PATH = sys.argv[6] if len(sys.argv) > 6 else ""
# `muster acp` passes a variable into a session only when told to, so say which
# ones here. Both flags are its own; see deploy/multica/README.md.
FORWARD_ARGS = ["--forward-env", "MULTICA_", "--forward-path"]


def pump(name, src_read, dst_write, on_close):
    try:
        while True:
            chunk = src_read()
            if not chunk:
                break
            dst_write(chunk)
    except (OSError, ValueError):
        pass
    finally:
        on_close()


def serve(conn, peer):
    print(f"[bridge] {peer} connected", flush=True)
    conn_file = conn.makefile("rb")
    first = conn_file.readline()
    if first.strip() != TOKEN.encode():
        print(f"[bridge] {peer} rejected: bad token", flush=True)
        try:
            conn.sendall(b'{"error":"bridge: bad token"}\n')
        finally:
            conn.close()
        return

    # The task environment block: NAME=value lines until a blank line. Only
    # the scheduler's own names are accepted, so a client cannot set anything
    # else in the spawned process.
    task_env = {}
    while True:
        line = conn_file.readline()
        if not line or line.strip() == b"":
            break
        name, sep, value = line.decode("utf-8", "replace").rstrip("\r\n").partition("=")
        if sep and name.startswith("MULTICA_"):
            task_env[name] = value

    env = dict(os.environ)
    env.update(task_env)
    if EXTRA_PATH:
        env["PATH"] = EXTRA_PATH + os.pathsep + env.get("PATH", "")

    proc = subprocess.Popen(
        [MUSTER, "acp", "--agent", AGENT] + FORWARD_ARGS,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=sys.stderr,
        env=env,
    )
    names = ",".join(sorted(task_env)) or "none"
    print(
        f"[bridge] {peer} -> {MUSTER} acp --agent {AGENT} (pid {proc.pid}), task env: {names}",
        flush=True,
    )

    def close_all():
        try:
            conn.shutdown(socket.SHUT_RDWR)
        except OSError:
            pass
        if proc.poll() is None:
            proc.terminate()

    threads = [
        threading.Thread(
            target=pump,
            args=("net->proc", lambda: conn_file.read1(65536),
                  lambda b: (proc.stdin.write(b), proc.stdin.flush()), close_all),
            daemon=True,
        ),
        threading.Thread(
            target=pump,
            args=("proc->net", lambda: proc.stdout.read1(65536),
                  conn.sendall, close_all),
            daemon=True,
        ),
    ]
    for thread in threads:
        thread.start()
    proc.wait()
    for thread in threads:
        thread.join(timeout=2)
    conn.close()
    print(f"[bridge] {peer} closed (exit {proc.returncode})", flush=True)


def main():
    listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    listener.bind((BIND, PORT))
    listener.listen(8)
    print(f"[bridge] listening on {BIND}:{PORT}, agent={AGENT}", flush=True)
    while True:
        conn, peer = listener.accept()
        threading.Thread(target=serve, args=(conn, peer), daemon=True).start()


if __name__ == "__main__":
    main()
