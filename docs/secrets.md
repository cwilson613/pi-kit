---
subsystem: secrets
design_docs: []
openspec_baselines: []
last_updated: 2026-03-10
---

# Secrets

> Secure API key and credential management with clipboard-based input, 1Password integration, and shell command evaluation.

## What It Does

The secrets extension manages provider API keys and credentials needed by Omegon's model routing layer. It supports three input modes:

1. **Clipboard-based**: Copy the secret, run `/secrets configure`, confirm by length — value never displayed on screen
2. **1Password references**: Store `op://vault/item/field` references that resolve at runtime via 1Password CLI
3. **Shell command evaluation**: Store `$(command)` patterns that evaluate at runtime (e.g., `$(aws secretsmanager get-secret-value ...)`)

Secrets are stored in `~/.config/omegon/auth.json` and the configured Omegon secrets backend, with mode-appropriate handling. The extension probes for clipboard commands (`pbpaste`, `xclip`, `xsel`, `wl-paste`) at runtime.

## Key Files

| File | Role |
|------|------|
| `extensions/00-secrets/index.ts` | Extension entry — `/secrets` command, `promptForSecretValue()`, `detectClipboardCommand()`, `readClipboard()` |

## Design Decisions

- **Clipboard-based input over masked text field**: pi-tui `Input` component lacks password mode. Clipboard flow avoids showing secrets on screen with length-only confirmation.
- **Fallback to direct input with warning**: If no clipboard command is available, falls back to `ctx.ui.input()` with a security warning.
- **Non-secret inputs use standard input**: 1Password references and shell commands (not actual secrets) still use `ctx.ui.input()`.

## Constraints & Known Limitations

- Clipboard-based input requires `pbpaste` (macOS), `xclip`/`xsel` (Linux X11), or `wl-paste` (Wayland)
- `ExtensionUIDialogOptions` supports only `signal` and `timeout` — no `secret`/`password` field
- Secrets state is Omegon-owned under `~/.config/omegon/` and the configured backend, not per-project

## Related Subsystems

- [Operator Profile](operator-profile.md) — provider authentication status
- [Model Routing](model-routing.md) — consumes API keys for provider access
