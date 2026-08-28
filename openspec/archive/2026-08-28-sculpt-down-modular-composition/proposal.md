# Sculpt-down modular composition

## Intent

Convert Omegon's runtime-optional domain boundaries into replaceable artifact
boundaries so the agent binary can shrink without weakening the existing product.

## Scope

Codescan is the first proof. Its portable contracts remain in the host, while its
SQLite, parser, indexing, and BM25 engine moves to a release-coupled native
extension process. Host-owned tools and context policy bind to the extension RPC
when available and report typed unavailability when it is absent.

The current default product remains behaviorally compatible because release
packaging installs the extension alongside Omegon. Packaging a minimal release,
removing additional domains, and extracting the complete kernel runtime are later
slices.

## Success criteria

- The `omegon` package never links the codescan engine in any feature composition.
- Codescan contracts remain available without indexing dependencies.
- A bundled native extension exclusively owns SQLite, indexing, and BM25 work.
- Tool and code-context calls share the extension service and propagate cancellation.
- An extension-free runtime keeps host schemas and returns typed unavailability.
- Repository and CI checks fail if the engine re-enters the Omegon dependency graph.
