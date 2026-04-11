#!/usr/bin/env python3
"""Generate a single-file benchmark dashboard HTML report.

Lean by design:
- Python stdlib only for data loading/normalization
- one HTML output file
- Chart.js loaded from CDN in the generated HTML
- targets Omegon benchmark result JSON artifacts
"""

from __future__ import annotations

import argparse
import json
import subprocess
from collections import Counter, defaultdict
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Generate an HTML dashboard from benchmark JSON artifacts")
    parser.add_argument(
        "input",
        nargs="?",
        default="ai/benchmarks/runs",
        help="Directory containing benchmark JSON artifacts (default: ai/benchmarks/runs)",
    )
    parser.add_argument(
        "--output",
        default="ai/benchmarks/reports/latest.html",
        help="Output HTML path (default: ai/benchmarks/reports/latest.html)",
    )
    parser.add_argument(
        "--title",
        default="Omegon Benchmark Dashboard",
        help="Dashboard title",
    )
    return parser.parse_args()


def repo_root() -> Path:
    return Path(__file__).resolve().parents[1]


def load_results(input_dir: Path) -> list[dict[str, Any]]:
    if not input_dir.exists() or not input_dir.is_dir():
        raise SystemExit(f"input directory does not exist: {input_dir}")
    results: list[dict[str, Any]] = []
    for path in sorted(input_dir.glob("*.json")):
        payload = json.loads(path.read_text())
        if isinstance(payload, dict):
            payload["_file"] = path.name
            results.append(payload)
    return results


def git_sha(root: Path) -> str | None:
    try:
        return (
            subprocess.check_output(
                ["git", "rev-parse", "--short", "HEAD"],
                cwd=root,
                text=True,
                stderr=subprocess.DEVNULL,
            )
            .strip()
        )
    except Exception:
        return None


def normalize_row(result: dict[str, Any]) -> dict[str, Any]:
    task = result.get("task") if isinstance(result.get("task"), dict) else {}
    process = result.get("process") if isinstance(result.get("process"), dict) else {}
    derived = process.get("derived") if isinstance(process.get("derived"), dict) else {}
    tokens = result.get("tokens") if isinstance(result.get("tokens"), dict) else {}
    return {
        "file": result.get("_file"),
        "task_id": result.get("task_id"),
        "task_kind": result.get("task_kind") or task.get("kind"),
        "prompt": task.get("prompt"),
        "base_ref": task.get("base_ref"),
        "repo": task.get("repo"),
        "harness": result.get("harness"),
        "model": result.get("model"),
        "provider": result.get("provider") or result.get("resolved_provider") or result.get("requested_provider"),
        "status": result.get("status"),
        "score": result.get("score"),
        "wall_clock_sec": result.get("wall_clock_sec"),
        "input_tokens": tokens.get("input"),
        "output_tokens": tokens.get("output"),
        "cache_tokens": tokens.get("cache"),
        "cache_write_tokens": tokens.get("cache_write"),
        "total_tokens": tokens.get("total"),
        "turn_count": process.get("turn_count"),
        "orientation_only_turns": derived.get("orientation_only_turns", 0),
        "progress_nudge_count": derived.get("progress_nudge_count", 0),
        "tool_continuation_turns": derived.get("tool_continuation_turns", 0),
        "assistant_completed_turns": derived.get("assistant_completed_turns", 0),
    }


def summarize(results: list[dict[str, Any]]) -> dict[str, Any]:
    rows = [normalize_row(r) for r in results]
    task_ids = sorted({r.get("task_id") for r in rows if r.get("task_id")})
    harnesses = sorted({r.get("harness") for r in rows if r.get("harness")})
    models = sorted({r.get("model") for r in rows if r.get("model")})
    providers = sorted({r.get("provider") for r in rows if r.get("provider")})
    statuses = Counter(r.get("status") for r in rows if r.get("status"))

    prompts: dict[str, str] = {}
    kinds: dict[str, str] = {}
    base_refs: dict[str, str] = {}
    for r in rows:
        task_id = r.get("task_id")
        if task_id and r.get("prompt") and task_id not in prompts:
            prompts[task_id] = str(r["prompt"])
        if task_id and r.get("task_kind") and task_id not in kinds:
            kinds[task_id] = str(r["task_kind"])
        if task_id and r.get("base_ref") and task_id not in base_refs:
            base_refs[task_id] = str(r["base_ref"])

    return {
        "rows": rows,
        "task_ids": task_ids,
        "harnesses": harnesses,
        "models": models,
        "providers": providers,
        "statuses": dict(statuses),
        "prompts": prompts,
        "kinds": kinds,
        "base_refs": base_refs,
    }


def build_html(title: str, root: Path, input_dir: Path, summary: dict[str, Any]) -> str:
    sha = git_sha(root)
    generated_at = datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M:%SZ")
    data_json = json.dumps(summary)
    title_json = json.dumps(title)
    input_json = json.dumps(str(input_dir.relative_to(root) if input_dir.is_relative_to(root) else input_dir))
    sha_json = json.dumps(sha or "unknown")
    generated_json = json.dumps(generated_at)
    return f"""<!doctype html>
<html lang=\"en\">
<head>
  <meta charset=\"utf-8\" />
  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\" />
  <title>{{title}}</title>
  <script src=\"https://cdn.jsdelivr.net/npm/chart.js\"></script>
  <style>
    :root {{
      --bg: #020408;
      --card: #040a12;
      --surface: #020408;
      --primary: #2ab4c8;
      --primary-muted: #1a8898;
      --primary-bright: #6ecad8;
      --fg: #c4d8e4;
      --muted: #6c8898;
      --dim: #48647c;
      --border: #285c74;
      --border-dim: #245068;
      --green: #1ab878;
      --red: #e04848;
      --orange: #c86418;
      --yellow: #78b820;
    }}
    * {{ box-sizing: border-box; }}
    body {{
      margin: 0;
      font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
      background: var(--bg);
      color: var(--fg);
      line-height: 1.45;
    }}
    .wrap {{ max-width: 1600px; margin: 0 auto; padding: 24px; }}
    .hero {{
      background: linear-gradient(180deg, rgba(42,180,200,0.10), rgba(4,10,18,0.95));
      border: 1px solid var(--border);
      border-radius: 14px;
      padding: 20px 24px;
      margin-bottom: 20px;
      box-shadow: 0 0 0 1px rgba(42,180,200,0.06) inset;
    }}
    h1,h2,h3 {{ margin: 0 0 12px 0; }}
    h1 {{ color: var(--primary-bright); font-size: 28px; }}
    h2 {{ color: var(--primary); font-size: 18px; margin-top: 24px; }}
    h3 {{ color: var(--primary-muted); font-size: 15px; }}
    .meta {{ color: var(--muted); font-size: 13px; display: flex; flex-wrap: wrap; gap: 16px; }}
    .grid {{ display: grid; grid-template-columns: repeat(12, 1fr); gap: 16px; }}
    .card {{
      grid-column: span 12;
      background: var(--card);
      border: 1px solid var(--border-dim);
      border-radius: 12px;
      padding: 16px;
    }}
    .metric-grid {{ display: grid; grid-template-columns: repeat(6, minmax(0,1fr)); gap: 12px; }}
    .metric {{ background: rgba(2,6,16,0.85); border: 1px solid var(--border-dim); border-radius: 10px; padding: 12px; }}
    .metric .label {{ color: var(--muted); font-size: 12px; }}
    .metric .value {{ color: var(--fg); font-size: 22px; margin-top: 6px; }}
    .half {{ grid-column: span 6; }}
    .third {{ grid-column: span 4; }}
    .canvas-wrap {{ position: relative; height: 340px; }}
    .task-box {{ background: rgba(3,7,14,0.9); border: 1px solid var(--border-dim); border-radius: 10px; padding: 12px; white-space: pre-wrap; }}
    table {{ width: 100%; border-collapse: collapse; font-size: 13px; }}
    th, td {{ text-align: left; padding: 8px 10px; border-bottom: 1px solid var(--border-dim); vertical-align: top; }}
    th {{ color: var(--primary-bright); font-weight: 600; }}
    td {{ color: var(--fg); }}
    .tag {{ display: inline-block; padding: 2px 8px; border-radius: 999px; border: 1px solid var(--border); color: var(--primary-bright); font-size: 12px; margin-right: 6px; margin-bottom: 6px; }}
    .ok {{ color: var(--green); }}
    .warn {{ color: var(--orange); }}
    .bad {{ color: var(--red); }}
    @media (max-width: 1100px) {{ .half, .third {{ grid-column: span 12; }} .metric-grid {{ grid-template-columns: repeat(2, minmax(0,1fr)); }} }}
  </style>
</head>
<body>
  <div class=\"wrap\">
    <section class=\"hero\">
      <h1 id=\"title\"></h1>
      <div class=\"meta\">
        <div><strong>Git</strong>: <span id=\"gitSha\"></span></div>
        <div><strong>Input</strong>: <span id=\"inputDir\"></span></div>
        <div><strong>Generated</strong>: <span id=\"generatedAt\"></span></div>
      </div>
    </section>

    <section class=\"card\">
      <h2>Run Set Summary</h2>
      <div class=\"metric-grid\" id=\"metrics\"></div>
    </section>

    <section class=\"grid\">
      <div class=\"card half\">
        <h2>Tasks Under Test</h2>
        <div id=\"taskDefs\"></div>
      </div>
      <div class=\"card half\">
        <h2>Included Harnesses / Models</h2>
        <h3>Harnesses</h3>
        <div id=\"harnessTags\"></div>
        <h3>Models</h3>
        <div id=\"modelTags\"></div>
        <h3>Providers</h3>
        <div id=\"providerTags\"></div>
      </div>

      <div class=\"card half\">
        <h2>Tokens vs Wall Clock</h2>
        <div class=\"canvas-wrap\"><canvas id=\"scatterChart\"></canvas></div>
      </div>
      <div class=\"card half\">
        <h2>Turn Count by Harness / Model</h2>
        <div class=\"canvas-wrap\"><canvas id=\"turnChart\"></canvas></div>
      </div>

      <div class=\"card half\">
        <h2>Token Composition</h2>
        <div class=\"canvas-wrap\"><canvas id=\"tokenStackChart\"></canvas></div>
      </div>
      <div class=\"card half\">
        <h2>Process Metrics</h2>
        <div class=\"canvas-wrap\"><canvas id=\"processChart\"></canvas></div>
      </div>

      <div class=\"card\">
        <h2>Run Matrix</h2>
        <table>
          <thead>
            <tr>
              <th>Task</th><th>Kind</th><th>Harness</th><th>Model</th><th>Status</th><th>Score</th><th>Wall</th><th>Total Tokens</th><th>Turns</th><th>Orientation</th><th>Nudges</th><th>File</th>
            </tr>
          </thead>
          <tbody id=\"runTable\"></tbody>
        </table>
      </div>
    </section>
  </div>

<script>
const TITLE = {title_json};
const INPUT_DIR = {input_json};
const GIT_SHA = {sha_json};
const GENERATED_AT = {generated_json};
const DATA = {data_json};

document.getElementById('title').textContent = TITLE;
document.getElementById('gitSha').textContent = GIT_SHA;
document.getElementById('inputDir').textContent = INPUT_DIR;
document.getElementById('generatedAt').textContent = GENERATED_AT;

const rows = DATA.rows || [];
const metrics = [
  ['Runs', rows.length],
  ['Tasks', (DATA.task_ids || []).length],
  ['Harnesses', (DATA.harnesses || []).length],
  ['Models', (DATA.models || []).length],
  ['Providers', (DATA.providers || []).length],
  ['Passes', (DATA.statuses && DATA.statuses.pass) || 0],
];
const metricsEl = document.getElementById('metrics');
for (const [label, value] of metrics) {{
  const div = document.createElement('div');
  div.className = 'metric';
  div.innerHTML = `<div class="label">${{label}}</div><div class="value">${{value}}</div>`;
  metricsEl.appendChild(div);
}}

function addTags(id, items) {{
  const el = document.getElementById(id);
  for (const item of items || []) {{
    const span = document.createElement('span');
    span.className = 'tag';
    span.textContent = item;
    el.appendChild(span);
  }}
}}
addTags('harnessTags', DATA.harnesses || []);
addTags('modelTags', DATA.models || []);
addTags('providerTags', DATA.providers || []);

const taskDefs = document.getElementById('taskDefs');
for (const taskId of DATA.task_ids || []) {{
  const box = document.createElement('div');
  box.className = 'task-box';
  const kind = (DATA.kinds || {{}})[taskId] || 'unknown';
  const baseRef = (DATA.base_refs || {{}})[taskId] || 'unknown';
  const prompt = (DATA.prompts || {{}})[taskId] || '';
  box.innerHTML = `<strong>${{taskId}}</strong> <span class="tag">${{kind}}</span> <span class="tag">base:${{baseRef}}</span>\n\n${{prompt}}`;
  taskDefs.appendChild(box);
}}

const runTable = document.getElementById('runTable');
for (const row of rows) {{
  const tr = document.createElement('tr');
  const statusClass = row.status === 'pass' ? 'ok' : (row.status === 'fail' ? 'bad' : 'warn');
  tr.innerHTML = `
    <td>${{row.task_id || ''}}</td>
    <td>${{row.task_kind || ''}}</td>
    <td>${{row.harness || ''}}</td>
    <td>${{row.model || ''}}</td>
    <td class="${{statusClass}}">${{row.status || ''}}</td>
    <td>${{row.score ?? ''}}</td>
    <td>${{row.wall_clock_sec ?? ''}}</td>
    <td>${{row.total_tokens ?? ''}}</td>
    <td>${{row.turn_count ?? ''}}</td>
    <td>${{row.orientation_only_turns ?? 0}}</td>
    <td>${{row.progress_nudge_count ?? 0}}</td>
    <td>${{row.file || ''}}</td>`;
  runTable.appendChild(tr);
}}

const alpharius = {{
  fg: '#c4d8e4',
  muted: '#6c8898',
  border: '#285c74',
  primary: '#2ab4c8',
  primaryBright: '#6ecad8',
  primaryMuted: '#1a8898',
  green: '#1ab878',
  red: '#e04848',
  orange: '#c86418',
  yellow: '#78b820'
}};

Chart.defaults.color = alpharius.fg;
Chart.defaults.borderColor = alpharius.border;
Chart.defaults.font.family = 'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace';

const pointColors = rows.map(r => r.harness === 'omegon' ? alpharius.primary : r.harness === 'om' ? alpharius.primaryMuted : r.harness === 'claude-code' ? alpharius.orange : alpharius.yellow);

new Chart(document.getElementById('scatterChart'), {{
  type: 'scatter',
  data: {{
    datasets: [{{
      label: 'Runs',
      data: rows.filter(r => typeof r.total_tokens === 'number' && typeof r.wall_clock_sec === 'number').map(r => ({{ x: r.total_tokens, y: r.wall_clock_sec, label: `${{r.harness}} / ${{r.model}} / ${{r.task_id}}` }})),
      backgroundColor: pointColors,
      borderColor: pointColors,
      pointRadius: 6,
    }}]
  }},
  options: {{
    plugins: {{ tooltip: {{ callbacks: {{ label: (ctx) => ctx.raw.label + ` — tokens:${{ctx.raw.x}} wall:${{ctx.raw.y}}s` }} }} }},
    scales: {{ x: {{ title: {{ display: true, text: 'Total Tokens' }} }}, y: {{ title: {{ display: true, text: 'Wall Clock (s)' }} }} }}
  }}
}});

const labels = rows.map(r => `${{r.harness}}\n${{(r.model || '').replace('anthropic:','').replace('openai-codex:','')}}`);
new Chart(document.getElementById('turnChart'), {{
  type: 'bar',
  data: {{ labels, datasets: [{{ label: 'Turns', data: rows.map(r => r.turn_count || 0), backgroundColor: pointColors }}] }},
  options: {{ responsive: true, plugins: {{ legend: {{ display: false }} }} }}
}});

new Chart(document.getElementById('tokenStackChart'), {{
  type: 'bar',
  data: {{
    labels,
    datasets: [
      {{ label: 'Input', data: rows.map(r => r.input_tokens || 0), backgroundColor: alpharius.primary }},
      {{ label: 'Output', data: rows.map(r => r.output_tokens || 0), backgroundColor: alpharius.green }},
      {{ label: 'Cache', data: rows.map(r => r.cache_tokens || 0), backgroundColor: alpharius.yellow }},
      {{ label: 'Cache Write', data: rows.map(r => r.cache_write_tokens || 0), backgroundColor: alpharius.orange }},
    ]
  }},
  options: {{ responsive: true, scales: {{ x: {{ stacked: true }}, y: {{ stacked: true }} }} }}
}});

new Chart(document.getElementById('processChart'), {{
  type: 'bar',
  data: {{
    labels,
    datasets: [
      {{ label: 'Orientation Turns', data: rows.map(r => r.orientation_only_turns || 0), backgroundColor: alpharius.red }},
      {{ label: 'Progress Nudges', data: rows.map(r => r.progress_nudge_count || 0), backgroundColor: alpharius.orange }},
      {{ label: 'Tool Continuation Turns', data: rows.map(r => r.tool_continuation_turns || 0), backgroundColor: alpharius.primaryMuted }},
      {{ label: 'Assistant Completed Turns', data: rows.map(r => r.assistant_completed_turns || 0), backgroundColor: alpharius.green }},
    ]
  }},
  options: {{ responsive: true, scales: {{ x: {{ stacked: false }}, y: {{ beginAtZero: true }} }} }}
}});
</script>
</body>
</html>
""".replace("{title}", title)


def main() -> int:
    args = parse_args()
    root = repo_root()
    input_dir = Path(args.input)
    if not input_dir.is_absolute():
        input_dir = (root / input_dir).resolve()
    output = Path(args.output)
    if not output.is_absolute():
        output = (root / output).resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    results = load_results(input_dir)
    summary = summarize(results)
    html = build_html(args.title, root, input_dir, summary)
    output.write_text(html)
    print(output)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
