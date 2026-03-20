# Functional plugins — code-backed skills with tools, endpoints, and runtime logic

## Intent

Markdown-only plugins (persona/tone/skill) are passive — they inject context. Functional plugins have executable code: tools backed by HTTP endpoints, WASM modules, or subprocess scripts. The question: how does someone write a plugin that *does* something, not just *says* something? This bridges the existing HTTP plugin system (plugin.toml with tools/endpoints) and the new armory manifest format.

See [design doc](../../../docs/functional-plugins.md).
