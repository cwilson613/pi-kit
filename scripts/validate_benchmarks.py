#!/usr/bin/env python3
"""Validate benchmark task definitions in ai/benchmarks/tasks/."""

import os
import sys

try:
    import yaml
except ImportError:
    # PyYAML not available — skip validation gracefully in environments without it.
    print("pyyaml not installed — skipping benchmark validation")
    sys.exit(0)

TASKS_DIR = "ai/benchmarks/tasks"

if not os.path.isdir(TASKS_DIR):
    print(f"No benchmark tasks directory at {TASKS_DIR}")
    sys.exit(0)

tasks = [f for f in os.listdir(TASKS_DIR) if f.endswith(".yaml")]
if not tasks:
    print("No benchmark tasks found")
    sys.exit(1)

errors = []
for task_file in sorted(tasks):
    path = os.path.join(TASKS_DIR, task_file)
    with open(path) as f:
        task = yaml.safe_load(f)
    if not task.get("id"):
        errors.append(f"{task_file}: missing id")
    if not task.get("acceptance"):
        errors.append(f"{task_file}: missing acceptance")
    if not task.get("budget"):
        errors.append(f"{task_file}: missing budget")
    print(f'  {task_file}: ok ({task.get("id", "?")})')

if errors:
    for e in errors:
        print(f"ERROR: {e}", file=sys.stderr)
    sys.exit(1)

print(f"{len(tasks)} benchmark tasks validated")
