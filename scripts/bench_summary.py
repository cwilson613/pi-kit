#!/usr/bin/env python3
"""Print a concise summary of a benchmark usage JSON file."""
import json, sys

path = sys.argv[1]
with open(path) as f:
    d = json.load(f)

turns = d.get("turns", [])
print(f"Turns: {d['turn_count']}  Input: {d['input_tokens']}  Output: {d['output_tokens']}  Cache: {d['cache_tokens']}")
cc = d.get("context_composition", {})
print(f"Context: sys={cc.get('system_tokens',0)} tools={cc.get('tool_schema_tokens',0)} conv={cc.get('conversation_tokens',0)} mem={cc.get('memory_tokens',0)}")
if turns:
    print(f"Per-turn input: {' -> '.join(str(t['input_tokens']) for t in turns)}")
    print(f"Per-turn tools: {' -> '.join(str(t['context_composition'].get('tool_schema_tokens',0)) for t in turns)}")
