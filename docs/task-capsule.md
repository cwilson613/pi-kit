# Omegon task capsule

`task-capsule-v0` is the first explicit smaller Omegon runtime artifact profile.
It preserves the existing bounded `omegon run` execution path while establishing
a measured dependency base for later runtime layers.

V0 is a source-built artifact, not a published release archive, package-manager
package, update-channel artifact, or container image. The normal five-profile
release matrix continues to describe the full product; capsule CI separately
builds and exercises this exact feature graph.

## Build

```bash
just build-task-capsule
```

The recipe builds with:

```bash
CARGO_TARGET_DIR=target/task-capsule \
  cargo build -p omegon --release --locked \
  --no-default-features --features task-capsule
```

The executable is `target/task-capsule/release/omegon`. The separate target
directory prevents a capsule build from replacing the full product artifact.
`task-capsule` is an exclusive artifact selector: combining it with `tui`,
`self-update`, or `local-embeddings` is a compile error.

## Run

Set an absolute state directory, an explicit model route, and the corresponding
credential. Do not put credentials in task files.

```bash
OMEGON_HOME=/absolute/state/.omegon \
  target/task-capsule/release/omegon \
  --cwd /absolute/workspace \
  --model <provider>:<model> \
  run --prompt "Perform one bounded task" --max-turns 1 --timeout 120
```

The capsule retains the normal structured result and exit codes: `0` completed,
`1` error, `2` exhausted, and `3` timeout.

`omegon run` is the canonical capsule entrypoint, not yet an exclusive command
surface. Other non-TUI commands remain linked in V0; command-surface fencing is
deferred to a later subtraction.

## V0 boundary

The capsule excludes:

- native TUI and presentation dependencies;
- the codescan engine, while retaining typed unavailable host contracts;
- Sigstore and X.509 self-update verification.

The capsule still retains the embedded control plane, ACP, memory, lifecycle,
Git, MCP, dynamic extensions, and archive/install support. V0 is an honest first
subtraction, not the final minimal kernel. Each later layer must move dependencies
from retained to excluded only after adding a tested absence contract.

The ratchet checks exact package absence where possible and separately verifies
that direct presentation dependencies remain optional and owned by `tui`.
`unicode-width` remains transitively present through retained web parsing and
non-TUI truncation code even though Omegon's direct dependency is TUI-only.

Run the compile and dependency boundary checks with:

```bash
just check-task-capsule
```
