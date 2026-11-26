"""Verify that the native viewer can terminate while its daemon still owns stdin."""
import ctypes
from ctypes import wintypes
import os
import socket
import struct
import subprocess
import sys
import threading
import time


def reject_rtsp(listener):
    with listener.accept()[0] as client:
        client.settimeout(5)
        client.recv(8192)
        client.sendall(b"RTSP/1.0 503 Service Unavailable\r\nCSeq: 1\r\nContent-Length: 0\r\n\r\n")


def test_exit(executable, legacy):
    # A real parent window receives cross-thread messages from the media child.
    ui = ctypes.WinDLL("user32", use_last_error=True)
    ui.CreateWindowExW.restype = wintypes.HWND
    ui.CreateWindowExW.argtypes = [
        wintypes.DWORD, wintypes.LPCWSTR, wintypes.LPCWSTR, wintypes.DWORD,
        ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_int,
        wintypes.HWND, wintypes.HMENU, wintypes.HINSTANCE, wintypes.LPVOID,
    ]
    parent = ui.CreateWindowExW(0, "STATIC", "Viewer test", 0, 0, 0, 640, 360,
                                 None, None, None, None)
    if not parent:
        raise ctypes.WinError(ctypes.get_last_error())
    try:
        with socket.socket() as listener:
            if legacy:
                for base in range(53200, 54200):
                    try:
                        listener.bind(("127.0.0.1", base + 21))
                        break
                    except OSError:
                        continue
                else:
                    raise RuntimeError("no local RTSP test port available")
                port_field = base
            else:
                listener.bind(("127.0.0.1", 0))
                port_field = listener.getsockname()[1]
            listener.listen(1)
            listener.settimeout(10)
            threading.Thread(target=reject_rtsp, args=(listener,), daemon=True).start()
            with subprocess.Popen([executable], stdin=subprocess.PIPE, stdout=subprocess.PIPE) as child:
                try:
                    # Split the fixed bootstrap record across writes.
                    child.stdin.write(b"MVW1" if legacy else b"MVW2")
                    child.stdin.flush()
                    child.stdin.write(bytes([0, 0]) + struct.pack(">HQ", port_field, parent) + os.urandom(20))
                    child.stdin.flush()
                    deadline = time.monotonic() + 15
                    msg = wintypes.MSG()
                    while child.poll() is None:
                        if time.monotonic() >= deadline:
                            raise TimeoutError("native viewer remained alive with its control pipe open")
                        while ui.PeekMessageW(ctypes.byref(msg), None, 0, 0, 1):
                            ui.TranslateMessage(ctypes.byref(msg))
                            ui.DispatchMessageW(ctypes.byref(msg))
                        time.sleep(0.01)
                    assert child.returncode != 0
                    assert child.stdout.read().splitlines() == [b"ERROR RTSP handshake (503)"]
                finally:
                    if child.poll() is None:
                        child.kill()
    finally:
        ui.DestroyWindow(wintypes.HWND(parent))


if __name__ == "__main__":
    for legacy in (True, False):
        test_exit(sys.argv[1], legacy)
