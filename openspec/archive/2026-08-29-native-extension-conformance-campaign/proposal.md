# Native extension conformance campaign

## Intent

Define one host-backed conformance campaign that every first-party native
extension can run before it becomes part of an additive product composition.

## Scope

Build a reusable fixture and test harness for production discovery, admission,
handshake, invocation, cancellation, replacement, crash isolation, and cleanup.
Run the first complete campaign against the real codescan extension. Packaging
and multi-profile growth remain separate changes.

## Success criteria

- Native extensions share one protocol and lifecycle conformance suite.
- The real host discovers and invokes the real codescan process.
- In-flight cancellation, replacement, crash isolation, and stale-generation behavior are tested end to end.
- Successful and failed runs settle the complete owned process tree.
