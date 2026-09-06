"""Operator bundle contracts; no native windows or inference are launched."""
import importlib.util
import os
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).parents[1]))
spec = importlib.util.spec_from_file_location("operator_test", Path(__file__).parents[1] / "tui_operator_test.py")
operator = importlib.util.module_from_spec(spec)
spec.loader.exec_module(operator)


class OperatorTests(unittest.TestCase):
    def test_terminal_environment_survives_without_real_credentials(self):
        env = operator.clean_environment(Path('/tmp/isolated'), {"PATH": "/bin", "TERM": "xterm-kitty", "TERM_PROGRAM": "kitty", "OPENAI_API_KEY": "real-secret", "AWS_SECRET_ACCESS_KEY": "real-secret", "HOME": "/real"})
        self.assertEqual(env['TERM'], 'xterm-kitty')
        self.assertEqual(env['TERM_PROGRAM'], 'kitty')
        self.assertEqual(env['OPENAI_API_KEY'], 'local-only')
        self.assertNotIn('AWS_SECRET_ACCESS_KEY', env)
        self.assertEqual(env['HOME'], '/tmp/isolated')

    def test_native_launches_preserve_literal_command_paths(self):
        command = Path("/tmp/operator's $(echo nope)/Run.command")
        for client, app in operator.APPS.items():
            args = operator.launch_command(client, app, command)
            if client == "iterm":
                self.assertEqual(shlex.split(args[-1]), [str(command)])
            else:
                self.assertEqual(args[-1], str(command))
            self.assertNotIn('sh', args)
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / 'Launch.command'
            args = ['/bin/echo', str(command)]
            operator.write_command(path, args)
            self.assertEqual(shlex.split(path.read_text().splitlines()[1]), ['exec', *args])

    def test_frozen_bundle_rejects_changed_executable(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binary = root / 'binary'
            binary.write_bytes(b'fixture executable')
            bundle = root / 'bundle'
            with mock.patch.object(operator, 'inventory', return_value={'terminal': {'app': operator.APPS['terminal'], 'version': 'fixture'}}), mock.patch.object(operator.shutil, 'which', return_value='/usr/bin/true'):
                operator.prepare(binary, bundle)
            operator.verify_bundle(bundle)
            (bundle / 'omegon').write_bytes(b'changed')
            with self.assertRaisesRegex(RuntimeError, 'executable changed'):
                operator.verify_bundle(bundle)

    def test_cleanup_reaches_child_in_separate_session(self):
        with tempfile.TemporaryDirectory() as temporary:
            pidfile = Path(temporary) / 'child.pid'
            code = "import subprocess,sys,time; from pathlib import Path; p=subprocess.Popen([sys.executable,'-c','import time; time.sleep(60)'],start_new_session=True); Path(sys.argv[1]).write_text(str(p.pid)); time.sleep(60)"
            parent = subprocess.Popen([sys.executable, '-c', code, str(pidfile)])
            try:
                import time
                deadline = time.monotonic() + 5
                while not pidfile.exists() and time.monotonic() < deadline:
                    time.sleep(.02)
                self.assertTrue(pidfile.exists())
                child = int(pidfile.read_text())
                operator.cleanup_tree(parent.pid)
                parent.wait(timeout=5)
                statuses = subprocess.check_output(['ps', '-axo', 'pid=,stat='], text=True)
                alive = {int(row.split()[0]) for row in statuses.splitlines() if 'Z' not in row.split()[1]}
                self.assertNotIn(child, alive)
            finally:
                if parent.poll() is None:
                    operator.cleanup_tree(parent.pid)
                    parent.wait(timeout=5)


if __name__ == '__main__':
    unittest.main()
