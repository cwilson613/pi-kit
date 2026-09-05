"""Native observation contracts, without launching GUI applications."""
import sys
from pathlib import Path
import unittest
sys.path.insert(0, str(Path(__file__).parents[1]))
from tui_native_acceptance import visible_tail, trial_outcome

class NativeObservationTests(unittest.TestCase):
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
