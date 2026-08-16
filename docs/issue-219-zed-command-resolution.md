---
id: issue-219-zed-command-resolution
title: "Issue #219 — cross-platform Zed command resolution"
status: seed
tags: [bug, editor, zed, nixos, cross-platform, tdd]
open_questions:
  - "[assumption] A successful `zed`/`zeditor --version` is sufficient evidence that the CLI can dispatch to a GUI in the current session; remote/headless environments may have the CLI installed without a reachable GUI."
  - "[assumption] WSL Windows-host Zed invocation should remain outside issue #219 even though current Zed documentation says its Windows CLI can translate WSL paths automatically."
  - "Should `/editor zed` use `--foreground` only as an explicit manual diagnostic path rather than in normal launch, since foreground mode changes process lifetime and can expose logs in or interfere with the TUI?"
dependencies: []
related: []
---

# Issue #219 — cross-platform Zed command resolution

## Overview

Fix `/editor status` and `/editor zed` so editor executable resolution is configurable rather than tied to names Omegon happens to know. Profiles may define `editorCommands` keyed by stable integration ID (`zed`, `vscode`, and future editors); each value is passed directly to `Command::new` and may be an absolute path or a command resolved through `PATH`. Resolution precedence is explicit profile override, known PATH candidates, then platform-native fallback. The initial Zed candidates are `zed` and NixOS's `zeditor`; macOS retains its app-bundle fallback. `/editor status` and launch must consume the same resolution result.

Executable overrides are machine-local in practice. Operators should put absolute paths in the user profile rather than a repository profile unless the path is intentionally portable across that project's environments. No shell command strings or embedded arguments are accepted; editor arguments remain owned by the integration. WSL is supported only as a Linux process environment, and Windows-host dispatch remains out of scope. Remote/headless environments may report discovery and configure ACP without promising a visible GUI. TDD uses injected command probes so tests do not depend on host PATH or installed GUI applications.

## Research

### Observed NixOS CLI contract

On the issue reporter's NixOS environment, `command -v zeditor` resolves to `/etc/profiles/per-user/wilson/bin/zeditor`; `zed` is absent. `zeditor --version` succeeds and reports Zed 1.14.2. `zeditor --help` identifies itself as the Zed CLI and supports `--version`, `--foreground`, `--wait`, `--new`, `--existing`, `--user-data-dir`, `--zed`, `--dev-server-token`, and `--dev-container`. Therefore `--version` is a safe non-GUI discovery probe; `--foreground` is useful for manual GUI/debug validation but inappropriate for normal detached TUI launch.

### Platform and remote-environment effects

Repository policy supports macOS, Linux, and Linux processes under WSL2; native Windows semantics and reliable lifecycle control after dispatch to Windows-host executables are out of scope (`CONTRIBUTING.md`, `docs/windows-compatibility.md`). Zed's current CLI documentation says a Windows Zed CLI can handle WSL paths automatically, but adopting `zed.exe` discovery would cross the explicitly deferred WSL-to-Windows process boundary. SSH/Coder/devcontainer sessions can expose a Zed CLI without a locally visible GUI. ACP's primary integration direction is editor -> `omegon acp`; `/editor zed` configures and opportunistically launches the editor, so configuration success must remain independent from GUI launch success.

## Open Questions

- [assumption] A successful `zed`/`zeditor --version` is sufficient evidence that the CLI can dispatch to a GUI in the current session; remote/headless environments may have the CLI installed without a reachable GUI.
- [assumption] WSL Windows-host Zed invocation should remain outside issue #219 even though current Zed documentation says its Windows CLI can translate WSL paths automatically.
- Should `/editor zed` use `--foreground` only as an explicit manual diagnostic path rather than in normal launch, since foreground mode changes process lifetime and can expose logs in or interfere with the TUI?
