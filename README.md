# Omegon

Native AI agent harness — for agents, by agents.

Single binary. Zero dependencies. Full autonomy.

## Install

```sh
curl -fsSL https://omegon.styrene.dev/install.sh | sh
```

## Structure

```
core/           Rust workspace (omegon-core submodule)
  crates/
    omegon/           Main binary — TUI, tools, agent loop
    omegon-memory/    Memory system (sqlite, vectors, decay)
    omegon-secrets/   Secret resolution, redaction, tool guards
    omegon-traits/    Shared trait definitions
  site/               omegon.styrene.dev landing page
design/         Design exploration tree (markdown nodes)
docs/           Architecture docs and design decisions
graphics/       Logo and icon assets
openspec/       Spec-driven development artifacts
prompts/        Prompt templates
skills/         Markdown skill definitions
themes/         Alpharius theme
```

## Build

```sh
cd core
cargo build --release
```

## Platforms

| Platform | Architecture |
|----------|-------------|
| macOS    | arm64 (Apple Silicon) |
| macOS    | x86_64 (Intel) |
| Linux    | x86_64 |
| Linux    | arm64 / aarch64 |

## Legacy

The TypeScript/pi-based harness is archived at [omegon-pi](https://github.com/styrene-lab/omegon-pi).

## License

[BSL 1.1](LICENSE) — © 2024–2026 Black Meridian, LLC
