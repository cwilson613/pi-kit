+++
id = "07b23b16-bbcb-41d9-aa6a-1f143b31f495"
tags = []
aliases = []
imported_reference = false

[publication]
enabled = false
visibility = "private"
+++

# Getting Started

## Installation

### Direct installer

The direct installer requires an independently trusted `omegon-maintain`
executable through `OMEGON_BOOTSTRAP_VERIFIER`. Homebrew, Nix, and OCI publication
remain deferred while stable release channels are prepared. See
`docs/omegon-install.md`.

### Manual download

Download a release from [GitHub Releases](https://github.com/styrene-lab/omegon/releases). Preserve the complete archive layout; do not install only the host binary.

### From source

```bash
git clone https://github.com/styrene-lab/omegon.git
cd omegon
just link
```

## Authentication

Omegon needs an API key from at least one LLM provider.

### Anthropic (default)

```bash
# OAuth login (recommended — no API key needed)
omegon login

# Or set an API key directly
export ANTHROPIC_API_KEY=sk-ant-...
```

### OpenAI

```bash
omegon login openai

# Or set an API key
export OPENAI_API_KEY=sk-...
```

## First session

```bash
cd your-project
omegon
```

This launches the interactive TUI. Type a prompt and press Enter.

### Headless mode

```bash
omegon --prompt "add error handling to src/main.rs"
```

### Key commands

| Command | Description |
|---------|-------------|
| `/model` | Switch LLM provider/model |
| `/think` | Adjust reasoning level (off/low/medium/high) |
| `/context` | Toggle 200k ↔ 1M context window |
| `/sessions` | List saved sessions |
| `/help` | Show all commands |
| `Ctrl+C` | Cancel current operation / quit |
| `Ctrl+R` | Search command history |

## Configuration

Omegon auto-detects project conventions from config files (Cargo.toml, tsconfig.json, pyproject.toml, go.mod) and adjusts its behavior accordingly.

### Project profile

Settings persist per-project in `<repo-root>/.omegon/profile.json` (not the current nested working directory). If no project profile exists, Omegon falls back to `~/.omegon/profile.json`:

```bash
# These are saved automatically when you use /model or /think
omegon --model anthropic:claude-opus-4-6
```

### Global directives

Create `~/.config/omegon/AGENTS.md` with directives that apply to all sessions across all projects.

### Project directives

Create `AGENTS.md` in your project root for project-specific instructions.
