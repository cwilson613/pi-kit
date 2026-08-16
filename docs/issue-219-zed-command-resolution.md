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

## Future implementation targets

Issue #219 intentionally stops at Zed launch resolution plus a reusable profile configuration surface. `editorCommands` accepts stable integration IDs, but arbitrary keys do not imply that Omegon knows an editor's launch arguments, status probe, ACP installation mechanism, or configuration format.

A follow-up should replace the remaining editor-specific branching with a typed editor integration registry. Each registered editor should declare:

- a stable ID and display name;
- default executable candidates and platform-native fallbacks;
- a side-effect-free status probe;
- argument-vector launch behavior (never a shell command string);
- ACP setup/configuration behavior;
- remote/headless launch policy and diagnostic provenance.

Initial registry targets:

1. **VS Code** — honor `editorCommands["vscode"]` for both status and launch; consider `code`, `code-insiders`, and `codium` as explicit candidates; retain the current `vscode-acp` setup instructions until a safe settings writer exists.
2. **Other ACP-capable editors** — add only after their executable, launch, and ACP contracts are documented and tested. Unknown configured IDs should be reported as configured-but-unsupported, not executed generically.
3. **Shared status surface** — make `/editor status` iterate registry entries so TUI and future non-interactive surfaces consume one semantic projection rather than adding frontend-specific checks.
4. **Remote environments** — define behavior for SSH, Coder, devcontainers, and WSL host dispatch separately from local executable discovery. Do not infer GUI reachability solely from `--version` success.

Tests should inject command probes and launchers, assert override precedence and argument vectors, and avoid requiring installed GUI applications. Manual GUI verification remains appropriate for confirming that a resolved editor accepts the launch request.

## Open Questions

- [assumption] A successful `zed`/`zeditor --version` is sufficient evidence that the CLI can dispatch to a GUI in the current session; remote/headless environments may have the CLI installed without a reachable GUI.
- [assumption] WSL Windows-host Zed invocation should remain outside issue #219 even though current Zed documentation says its Windows CLI can translate WSL paths automatically.
- Should `/editor zed` use `--foreground` only as an explicit manual diagnostic path rather than in normal launch, since foreground mode changes process lifetime and can expose logs in or interfere with the TUI?
