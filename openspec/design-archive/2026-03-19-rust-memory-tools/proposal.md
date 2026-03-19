# Register memory_* agent-callable tools in Rust

## Intent

Bridge the 7 memory tools (memory_query, memory_recall, memory_store, memory_supersede, memory_archive, memory_focus, memory_release, memory_episodes, memory_connect, memory_compact, memory_search_archive) to the omegon-memory crate. Storage layer exists — need tool registration and JSON schema definitions.

See [design doc](../../../docs/rust-memory-tools.md).
