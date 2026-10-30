"""The scheduler keeps stdin open while waiting for initialize; EOF must escape."""
import os
import signal
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

COMMAND = Path(__file__).resolve().parents[1] / 'runtime-command'

class RuntimeTransportTests(unittest.TestCase):
    def run_transport(self, body, expected):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            for name, source in {
                'socat': body,
                'base64': 'import sys,base64;sys.stdout.write(base64.b64encode(sys.stdin.buffer.read()).decode())',
            }.items():
                p = root / name
                p.write_text('#!' + sys.executable + '\n' + source + '\n')
                p.chmod(0o700)
            env = dict(os.environ, PATH=str(root)+os.pathsep+os.environ['PATH'],
                TMPDIR=d, MUSTER_RUNTIME_PROVIDER='codex', MUSTER_BRIDGE_TOKEN='fixture-token',
                MUSTER_BRIDGE_HOST='127.0.0.1', MUSTER_BRIDGE_PORT='12345')
            child = subprocess.Popen(['/bin/sh', str(COMMAND), 'app-server'],
                stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env, start_new_session=True)
            try:
                # Intentionally don't close stdin: this reproduced the old hang.
                status = child.wait(timeout=5)
                self.assertEqual(status, expected, child.stderr.read().decode())
                self.assertFalse(any(root.glob('tmp.*')), 'owned FIFO directory leaked')
            finally:
                if child.poll() is None: os.killpg(child.pid, signal.SIGKILL)
                child.communicate(timeout=5)

    def test_refused_connection_exits_with_scheduler_stdin_open(self):
        self.run_transport('import sys; sys.exit(42)', 42)

    def test_peer_eof_exits_with_scheduler_stdin_open(self):
        body = "import sys\nfor line in sys.stdin:\n if line == '\\n': break\nprint('fixture-response', flush=True)"
        self.run_transport(body, 0)

if __name__ == '__main__': unittest.main()
