---
name: rust
description: Rust development guidance including Zellij WASM plugin development. Covers project setup (Cargo.toml), testing, clippy, rustfmt, CI/CD patterns, and the zellij-tile plugin API. Use when creating, modifying, or debugging Rust code or Zellij plugins.
guardrails:
  - name: clippy
    cmd: cargo clippy -- -D warnings
    timeout: 120
    condition: file_exists(Cargo.toml)
  - name: test
    cmd: cargo test
    timeout: 120
    condition: file_exists(Cargo.toml)
---

# Rust Development Skill

Conventions for Rust development, with a dedicated section for Zellij WASM plugin development.

## Core Conventions

- **Rust stable** toolchain (`rustup default stable`)
- **Cargo** for build, test, lint, format — no external build tools needed
- **clippy** for linting, **rustfmt** for formatting
- **Edition 2021** minimum
- Workspace layout for multi-crate projects, single `Cargo.toml` otherwise

## Project Scaffold

```
<project>/
├── Cargo.toml              # Package metadata, deps, lint config
├── rustfmt.toml            # max_width = 100
├── src/
│   ├── lib.rs              # Library root (or main.rs for binary)
│   └── ...
├── tests/
│   └── integration_test.rs
└── .github/workflows/ci.yml
```

## Tooling Quick Reference

### Clippy (Linting)

```bash
cargo clippy                        # Lint
cargo clippy -- -D warnings         # Warnings as errors (CI)
cargo clippy --all-targets           # Include tests/benches
cargo clippy --fix                   # Auto-fix
```

Project config in `Cargo.toml`:
```toml
[lints.clippy]
pedantic = { level = "warn", priority = -1 }
unwrap_used = "warn"
```

### Rustfmt (Formatting)

```bash
cargo fmt                           # Format
cargo fmt -- --check                # Check only (CI)
```

### Build & Test

```bash
cargo build                         # Debug build
cargo build --release               # Release build
cargo test                          # All tests
cargo test -- --nocapture            # Show println output
cargo test test_name                 # Specific test
cargo test --lib                     # Unit tests only
cargo test --test integration_test   # Specific integration test
```

### Other Useful Commands

```bash
cargo doc --open                    # Generate and browse docs
cargo audit                         # Security vulnerability check
cargo tree                          # Dependency tree
cargo expand                        # Macro expansion
```

## Testing Patterns

### Unit Tests (in-module)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid() {
        let result = parse("input");
        assert_eq!(result, expected);
    }
}
```

### Async Tests (tokio)

```rust
#[tokio::test]
async fn test_async_op() {
    let result = fetch_data().await;
    assert!(result.is_ok());
}
```

## Error Handling

| Context | Pattern |
|---------|---------|
| Libraries | `thiserror::Error` derive for custom error types |
| Applications | `anyhow::Result` for ergonomic error propagation |
| Unwrap | Never in library code; `expect("reason")` in main/tests only |

## Common Dependencies

| Crate | Purpose |
|-------|---------|
| `serde` + `serde_json` | Serialization |
| `tokio` | Async runtime |
| `anyhow` / `thiserror` | Error handling |
| `clap` | CLI parsing |
| `tracing` | Structured logging |

## CI/CD

```yaml
name: CI
on: [push, pull_request]
jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo fmt -- --check
      - run: cargo clippy -- -D warnings
      - run: cargo test
```

---

## Zellij WASM Plugin Development

Zellij plugins are Rust crates compiled to `wasm32-wasip1`, loaded by the Zellij multiplexer. The `zellij-tile` crate provides the plugin API.

### Setup

```bash
rustup target add wasm32-wasip1
cargo build --release --target wasm32-wasip1
# Output: target/wasm32-wasip1/release/<name>.wasm
```

### Plugin Cargo.toml

```toml
[dependencies]
zellij-tile = "0.41"        # Match your Zellij version
serde = { version = "1", features = ["derive"] }
serde_json = "1"

[profile.release]
opt-level = "s"             # Optimize for WASM size
lto = true
strip = true
```

### Plugin Lifecycle

```rust
use zellij_tile::prelude::*;

#[derive(Default)]
struct MyPlugin { /* state */ }

impl ZellijPlugin for MyPlugin {
    fn load(&mut self, config: BTreeMap<String, String>) {
        subscribe(&[EventType::Timer, EventType::Key]);
        request_permission(&[PermissionType::ReadApplicationState]);
        set_timeout(10.0);
    }

    fn update(&mut self, event: Event) -> bool {
        match event {
            Event::Timer(_) => { set_timeout(10.0); true }
            _ => false,
        }
    }

    fn render(&mut self, rows: usize, cols: usize) {
        print!("status: ok");
    }

    fn pipe(&mut self, pipe_message: PipeMessage) -> bool {
        false
    }
}

register_plugin!(MyPlugin);
```

### Permissions

| Permission | Allows |
|-----------|--------|
| `ReadApplicationState` | Query panes, tabs, session info |
| `ChangeApplicationState` | Create/close panes/tabs, switch focus |
| `RunCommands` | Execute commands in panes |
| `OpenFiles` | Open files in editor panes |
| `WriteToStdin` | Write to pane stdin |
| `ReadCliPipes` | Receive `zellij pipe` messages |

### External Communication via Pipes

```bash
echo '{"status":"ok"}' | zellij pipe --plugin "file:plugin.wasm" --name "update"
```

Plugin handles in `fn pipe()`. Primary pattern for bridging external events into WASM plugins (WASM can't open sockets directly).

### Loading in KDL Layouts

```kdl
pane size=1 borderless=true {
    plugin location="file:/path/to/plugin.wasm"
}
```

### WASM Constraints

| Limitation | Workaround |
|-----------|------------|
| No sockets | `zellij pipe` bridge from external script |
| No threads | `ZellijWorker` for background work |
| No system clock | `set_timeout()` for time-based logic |
| Binary size | `opt-level = "s"`, LTO, strip |

## Debugging

```bash
cargo test -- --nocapture           # Show test output
RUST_BACKTRACE=1 cargo test         # Full backtraces
RUST_LOG=debug cargo run            # With tracing/env_logger

# WASM plugins
tail -f /tmp/zellij-*/zellij-log/zellij.log
```

## Common Gotchas

| Issue | Fix |
|-------|-----|
| `wasm32-wasip1` not found | `rustup target add wasm32-wasip1` |
| Plugin won't load | Check Zellij version matches `zellij-tile` crate version |
| Clippy too noisy | Tune via `[lints.clippy]` in Cargo.toml |
| WASM binary too large | `opt-level = "s"`, `lto = true`, `strip = true` |
| Can't open sockets in WASM | Use `zellij pipe` bridge pattern |
| Plugin doesn't re-render | Return `true` from `update()` or `pipe()` |
</content>
</invoke>