#!/usr/bin/env bash
# Creates a small Rust project with a deliberate bug for the Omegon demo.
set -euo pipefail

DEMO_DIR="/tmp/omegon-demo-rec"

rm -rf "$DEMO_DIR"
mkdir -p "$DEMO_DIR"
cd "$DEMO_DIR"

git init -q

# --- Cargo.toml ---
cat > Cargo.toml << 'TOML'
[package]
name = "fib-cli"
version = "0.1.0"
edition = "2021"
TOML

mkdir -p src

# --- src/lib.rs  (contains the bug) ---
cat > src/lib.rs << 'RUST'
/// Return the nth Fibonacci number (0-indexed).
///
/// fib(0) = 0, fib(1) = 1, fib(2) = 1, fib(3) = 2, ...
pub fn fibonacci(n: u64) -> u64 {
    if n < 2 {
        return n;
    }
    let mut a: u64 = 0;
    let mut b: u64 = 1;
    for _ in 2..n {
        let tmp = b;
        b = a + b;
        a = tmp;
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base_cases() {
        assert_eq!(fibonacci(0), 0);
        assert_eq!(fibonacci(1), 1);
    }

    #[test]
    fn test_sequence() {
        assert_eq!(fibonacci(2), 1);
        assert_eq!(fibonacci(3), 2);
        assert_eq!(fibonacci(5), 5);
        assert_eq!(fibonacci(10), 55);
    }
}
RUST

# --- src/main.rs ---
cat > src/main.rs << 'RUST'
use fib_cli::fibonacci;
use std::env;

fn main() {
    let n: u64 = env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| {
            eprintln!("Usage: fib-cli <n>");
            std::process::exit(1);
        });

    println!("fib({n}) = {}", fibonacci(n));
}
RUST

# Initial commit so Omegon has git context
git add -A
git commit -q -m "init: fibonacci CLI with tests"

echo "Demo project ready at $DEMO_DIR"
echo "Tests will fail — that's the point."
echo ""
echo "Verify:  cd $DEMO_DIR && cargo test"
