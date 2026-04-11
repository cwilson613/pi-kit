import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "benchmark_dashboard.py"
SPEC = importlib.util.spec_from_file_location("benchmark_dashboard_module", SCRIPT)
assert SPEC and SPEC.loader
BENCHMARK_DASHBOARD = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = BENCHMARK_DASHBOARD
SPEC.loader.exec_module(BENCHMARK_DASHBOARD)


class BenchmarkDashboardTests(unittest.TestCase):
    def test_summarize_extracts_matrix_and_process_fields(self) -> None:
        result = {
            "task_id": "task-a",
            "task_kind": "implementation",
            "harness": "omegon",
            "model": "anthropic:claude-sonnet-4-6",
            "provider": "anthropic",
            "status": "pass",
            "score": 1.0,
            "wall_clock_sec": 12.5,
            "tokens": {"input": 100, "output": 20, "cache": 5, "cache_write": 1, "total": 126},
            "task": {"prompt": "Do the thing", "base_ref": "main", "repo": ".", "kind": "implementation"},
            "process": {
                "turn_count": 4,
                "derived": {"orientation_only_turns": 1, "progress_nudge_count": 2, "tool_continuation_turns": 3, "assistant_completed_turns": 1}
            },
        }
        summary = BENCHMARK_DASHBOARD.summarize([result])
        self.assertEqual(summary["task_ids"], ["task-a"])
        self.assertEqual(summary["harnesses"], ["omegon"])
        self.assertEqual(summary["models"], ["anthropic:claude-sonnet-4-6"])
        self.assertEqual(summary["providers"], ["anthropic"])
        row = summary["rows"][0]
        self.assertEqual(row["turn_count"], 4)
        self.assertEqual(row["orientation_only_turns"], 1)
        self.assertEqual(row["progress_nudge_count"], 2)

    def test_build_html_contains_alpharius_and_task_metadata(self) -> None:
        summary = {
            "rows": [{"task_id": "task-a", "task_kind": "implementation", "harness": "omegon", "model": "anthropic:claude-sonnet-4-6", "provider": "anthropic", "status": "pass", "score": 1.0, "wall_clock_sec": 10, "total_tokens": 100, "turn_count": 2, "orientation_only_turns": 0, "progress_nudge_count": 0, "tool_continuation_turns": 1, "assistant_completed_turns": 1, "input_tokens": 80, "output_tokens": 10, "cache_tokens": 5, "cache_write_tokens": 5, "file": "x.json"}],
            "task_ids": ["task-a"],
            "harnesses": ["omegon"],
            "models": ["anthropic:claude-sonnet-4-6"],
            "providers": ["anthropic"],
            "statuses": {"pass": 1},
            "prompts": {"task-a": "Do the thing"},
            "kinds": {"task-a": "implementation"},
            "base_refs": {"task-a": "main"},
        }
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            input_dir = root / "ai" / "benchmarks" / "runs"
            input_dir.mkdir(parents=True)
            html = BENCHMARK_DASHBOARD.build_html("Bench", root, input_dir, summary)
            self.assertIn("#2ab4c8", html)
            self.assertIn("task-a", html)
            self.assertIn("Do the thing", html)
            self.assertIn("cdn.jsdelivr.net/npm/chart.js", html)


if __name__ == "__main__":
    unittest.main()
