import unittest

from scripts.check_maintenance_dependency_boundary import forbidden_packages


class MaintenanceDependencyBoundaryTests(unittest.TestCase):
    def test_accepts_maintenance_contracts_and_verifier_dependencies(self):
        tree = """\
omegon-maintain v0.29.0-dev
omegon-maintenance-contracts v0.29.0-dev
reqwest v0.13.4
tokio v1.52.3
"""
        self.assertEqual(forbidden_packages(tree), [])

    def test_rejects_normal_runtime_domains(self):
        tree = """\
omegon-maintain v0.29.0-dev
omegon v0.29.0-dev
omegon-memory v0.29.0-dev
ratatui v0.29.0
rmcp v1.7.0
"""
        self.assertEqual(
            forbidden_packages(tree),
            ["omegon", "omegon-memory", "ratatui", "rmcp"],
        )


if __name__ == "__main__":
    unittest.main()
