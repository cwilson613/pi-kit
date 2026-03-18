# Rust lifecycle crates — design-tree + openspec as native Rust modules

## Intent

Per the `lifecycle-native-loop` decision, design-tree and openspec are not feature crates — they're core lifecycle engine components. They live in the `omegon` crate's `lifecycle/` module (stubs already exist at `lifecycle/mod.rs`).

See [design doc](../../../docs/rust-lifecycle-crates.md).
