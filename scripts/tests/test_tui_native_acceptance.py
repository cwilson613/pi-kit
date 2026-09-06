"""Native observation contracts, without launching GUI applications."""
import sys
from pathlib import Path
import unittest
from unittest import mock
import contextlib
import io
import tempfile
sys.path.insert(0, str(Path(__file__).parents[1]))
from tui_native_acceptance import visible_tail, trial_outcome
import tui_native_acceptance as native

class NativeObservationTests(unittest.TestCase):
    def test_cleanup_failure_stops_matrix_before_another_window_opens(self):
        with tempfile.TemporaryDirectory() as folder:
            with mock.patch.object(native, 'run_trial', return_value={'passed':False, 'window_cleanup':'failed'}) as trial:
                with self.assertRaises(SystemExit) as error:
                    native.main(['--bundle',folder,'--helper','/helper','--output',folder+'/output','--clients','ghostty','terminal','--interactive-gui'])
                self.assertEqual(error.exception.code, 1)
                trial.assert_called_once()

    def test_window_cleanup_requires_proven_ownership(self):
        driver = native.NativeClient('terminal', Path('/bundle'), Path('/helper'), Path('/output'))
        driver.id = '42'
        with mock.patch.object(native, 'apple') as apple:
            with self.assertRaisesRegex(RuntimeError, 'ownership'):
                driver.close()
            apple.assert_not_called()

    def test_shared_app_cleanup_guards_tabs_and_session_identity(self):
        owned = {'kCGWindowNumber': 42, 'kCGWindowOwnerPID': 99}
        for client, identity in [('terminal', 'tty'), ('iterm', 'unique ID')]:
            with self.subTest(client=client):
                driver = native.NativeClient(client, Path('/bundle'), Path('/helper'), Path('/output'))
                driver.id, driver.session_id, driver.window = '42', 'owned-session', owned
                with mock.patch.object(native, 'process_exists', return_value=True), mock.patch.object(driver, 'windows', side_effect=[[owned], []]), mock.patch.object(native, 'apple') as apple:
                    driver.close()
                script, window_id, session_id = apple.call_args.args
                self.assertEqual((window_id, session_id), ('42', 'owned-session'))
                self.assertIn('count of tabs', script)
                self.assertIn(identity, script)
                self.assertIn('error', script)
                self.assertNotIn('activate', script)

    def test_process_cleanup_error_still_attempts_window_cleanup_and_records_failure(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            bundle = root / 'bundle'
            (bundle / 'runs').mkdir(parents=True)
            driver = mock.Mock(id=None, notes=[])
            def failed_launch():
                run = bundle / 'runs' / 'terminal-one'
                run.mkdir()
                (run / 'process.json').write_text('invalid json')
                raise RuntimeError('launch failed')
            driver.launch.side_effect = failed_launch
            with mock.patch.object(native, 'verify_bundle', return_value={'binary_sha256':'fixture', 'revision':'fixture'}), mock.patch.object(native, 'digest', return_value='fixture'), mock.patch.object(native, 'NativeClient', return_value=driver), contextlib.redirect_stdout(io.StringIO()):
                result = native.run_trial('terminal', bundle, root / 'helper', root / 'output')
            driver.close.assert_called_once()
            self.assertFalse(result['passed'])
            self.assertIn('process_cleanup_error', result)
            self.assertTrue((root / 'output/native-trial.json').exists())

    def test_gui_launch_requires_explicit_opt_in(self):
        with mock.patch.object(native, 'run_trial') as trial, contextlib.redirect_stderr(io.StringIO()):
            with self.assertRaises(SystemExit) as error:
                native.main(['--bundle','/fixture','--helper','/helper','--output','/unused','--clients','ghostty'])
            self.assertEqual(error.exception.code, 2)
            trial.assert_not_called()

    def test_cleanup_uses_recorded_identity_without_focusing_or_quitting_other_windows(self):
        owned = {'kCGWindowNumber': 42, 'kCGWindowOwnerPID': 99}
        other = {'kCGWindowNumber': 43, 'kCGWindowOwnerPID': 99}
        driver = native.NativeClient('ghostty', Path('/bundle'), Path('/helper'), Path('/output'))
        driver.id = 'owned-terminal'
        driver.window = owned
        with mock.patch.object(native, 'process_exists', return_value=True), mock.patch.object(driver, 'windows', side_effect=[[owned, other], [other]]) as windows, mock.patch.object(native, 'apple') as apple:
            driver.close()
            self.assertEqual(windows.call_args_list, [mock.call(True), mock.call(True)])
            script, target = apple.call_args.args
            self.assertEqual(target, 'owned-terminal')
            self.assertIn('close terminal id', script)
            self.assertNotIn('activate', script)
            self.assertNotIn('quit', script)
        with mock.patch.object(driver, 'windows', return_value=[other]), mock.patch.object(native, 'apple') as apple:
            driver.close()
            apple.assert_not_called()

    def test_history_does_not_satisfy_current_view(self):
        self.assertNotIn('Permission required', visible_tail('Permission required\nold\nWork\nready\n', 2))
        self.assertEqual(visible_tail('Work\nready\n', 2), 'Work\nready')

    def test_started_is_not_passed(self):
        self.assertFalse(trial_outcome({'tui_started': True, 'provider_requests': 0, 'fixture_write_exists': False, 'recorder_exit_code': 0}))

    def test_write_or_failed_exit_cannot_pass(self):
        good = dict(tui_started=True, provider_requests=4, fixture_write_exists=False, recorder_exit_code=0)
        self.assertTrue(trial_outcome(good))
        self.assertFalse(trial_outcome(dict(good, fixture_write_exists=True)))
        self.assertFalse(trial_outcome(dict(good, recorder_exit_code=-9)))

if __name__ == '__main__': unittest.main()
