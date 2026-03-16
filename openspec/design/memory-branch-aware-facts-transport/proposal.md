# Memory branch-aware facts transport — isolate tracked facts.jsonl intent from branch-local runtime drift

## Intent

Explore how Omegon should manage tracked .pi/memory/facts.jsonl when operators move between branches in the Omegon repo. The goal is to preserve cross-machine portability and mergeable durable knowledge without letting branch-local session activity create unrelated dirty diffs or release blockers.

See [Memory branch-aware facts transport — isolate tracked facts.jsonl intent from branch-local runtime drift design doc](../../../docs/memory-branch-aware-facts-transport.md) for full context.
