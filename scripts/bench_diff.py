#!/usr/bin/env python3
"""Compare two benchmark usage JSON files side by side."""
import json, sys

with open(sys.argv[1]) as f:
    b = json.load(f)
with open(sys.argv[2]) as f:
    c = json.load(f)

def delta(label, bv, cv):
    diff = cv - bv
    pct = (diff / bv * 100) if bv else 0
    arrow = "+" if diff > 0 else "-" if diff < 0 else "="
    print(f"  {label:20s}  {bv:>8}  ->  {cv:>8}  {arrow} {abs(diff):>6} ({pct:+.1f}%)")

print("Metric                  Baseline    Current   Delta")
print("-" * 65)
delta("turns", b["turn_count"], c["turn_count"])
delta("input_tokens", b["input_tokens"], c["input_tokens"])
delta("output_tokens", b["output_tokens"], c["output_tokens"])
delta("cache_tokens", b["cache_tokens"], c["cache_tokens"])
delta("avg_input/turn", b["per_turn"]["avg_input_tokens"], c["per_turn"]["avg_input_tokens"])
delta("avg_output/turn", b["per_turn"]["avg_output_tokens"], c["per_turn"]["avg_output_tokens"])
bc = b.get("context_composition", {})
cc = c.get("context_composition", {})
print()
print("Context Composition     Baseline    Current   Delta")
print("-" * 65)
for k in ["system_tokens", "tool_schema_tokens", "conversation_tokens", "memory_tokens", "tool_history_tokens", "thinking_tokens"]:
    delta(k, bc.get(k, 0), cc.get(k, 0))
