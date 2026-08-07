+++
title = "Improvement: TUI shell mode toggle and exit semantics"
tags = ["improvement", "tui", "shell", "pty", "terminal"]
+++

# Improvement: TUI shell mode toggle and exit semantics

**Status:** Proposed; ready for design validation  
**Suggested branch:** `feat/tui-shell-mode`  
**Primary surfaces:** TUI event loop, terminal lifecycle, PTY execution

## Assignment brief

Implement `Ctrl+\`` as a reversible TUI state transition into an interactive shell. Resolve PTY ownership, renderer suspension versus embedding, shell selection, active-turn behavior, key interception, resize propagation, and terminal restoration. Prove repeated entry/exit is leak-free and preserves the Omegon session.

## User intent

- `Ctrl+\`` from the ordinary TUI enters shell mode.
- The shell behaves like a normal interactive terminal.
- Typing `exit` returns to the existing TUI session.
- Pressing `Ctrl+\`` again also returns to the TUI.

## Scope

- One shell owned by the current TUI session.
- Input/output through a real PTY or justified equivalent.
- Cwd, environment policy, resizing, exit paths, redraw, and help text.
- Interaction with an active agent turn.
- Child cleanup and terminal-state restoration.

## Non-goals

- Replacing the existing `terminal` tool.
- Building a full terminal emulator without evidence it is required.
- Changing the operator's global shell configuration.
- Claiming arbitrary foreground programs can always yield `Ctrl+\`` without validating terminal protocol behavior.
- Adding general multi-pane shell management in this slice.

## Investigation targets

Search TUI keymaps/event dispatch, raw mode, alternate screen, cursor handling, suspend/resume, redraw, PTY/session APIs, terminal dimensions and resize events, project cwd, `$SHELL`, active-turn state, and shutdown cleanup. Verify the actual byte/key event produced by `Ctrl+\`` across supported terminals.

## Design decisions to resolve

### Renderer model

Compare:

1. suspending TUI rendering while a child PTY owns the terminal; and
2. embedding terminal output in a TUI surface.

Prefer the simplest mechanism that provides normal shell semantics and deterministic restoration. An embedded model requires explicit terminal emulation and should not be selected casually.

### Shell selection

Define precedence among an Omegon setting, `$SHELL`, and platform login-shell lookup. Spawn through argument arrays, not shell interpolation. Establish login versus non-login invocation and inherited environment rules.

### Toggle interception

Determine whether the parent can reliably intercept `Ctrl+\`` while the child PTY is foregrounded. If not, define a PTY escape protocol or parent input multiplexer and document collisions with child applications.

### Active turns

Choose and surface one policy: continue agent work, pause display only, or block entry during unsafe transitions. Shell mode must not accidentally cancel or detach the active turn.

## State model

At minimum:

`Conversation → EnteringShell → ShellActive → LeavingShell → Conversation`

Exceptional exits include spawn failure, child crash, Omegon shutdown, resize failure, and forced toggle. Every path restores terminal state exactly once.

## Security and process constraints

- Never construct the shell launch through interpolated command text.
- Shell child has an explicit session owner and descendant boundary.
- Toggle/exit sends bounded termination only when the shell has not exited naturally.
- Environment inheritance follows existing secret policy.
- Cwd is validated and falls back deterministically if unavailable.

## Implementation sequence

1. Verify key-event portability and existing PTY primitives.
2. Record the state and terminal-restoration contract in tests.
3. Implement shell selection and owned PTY spawn.
4. Add input/output and resize forwarding.
5. Implement `exit`, toggle, crash, and shutdown paths.
6. Integrate active-turn policy and full redraw.
7. Add help/discoverability text.
8. Stress repeated entry/exit and descendant cleanup.

## Acceptance criteria

1. Toggle enters shell mode without replacing the Omegon process/session.
2. Shell receives the intended cwd and terminal dimensions.
3. Interactive input/output and resize behavior work.
4. `exit` restores raw mode, alternate screen, cursor, and full TUI redraw.
5. Toggle exit has the same restoration guarantees and no surviving child tree.
6. Child crash and Omegon shutdown restore safely.
7. Repeated cycles preserve conversation and active-session state without leaks.
8. Active-turn behavior is explicit and tested.
9. Keybinding appears in help.

## Regression plan

Use a real PTY where supported. Cover natural exit, toggle exit, failed spawn, child crash, resize, repeated cycles, active turn, shutdown during shell mode, and a shell child that launches a descendant.

## Validation

Run focused TUI/terminal tests and:

```bash
cargo test -p omegon <shell-mode-filter>
just clippy-changed
git diff --check
```

Manual terminal verification is required in at least one supported terminal after automated tests.

## Dependencies and conflict risks

This overlaps with process-lifecycle cleanup and TUI event/render work. Avoid parallel edits to keymaps, alternate-screen setup, terminal registry, and shutdown without coordination. The process-leak branch should provide or agree on child ownership primitives.

## Definition of done

All entry and exit paths restore the terminal, PTY ownership is explicit, no process leaks remain in stress tests, active-turn semantics are documented and enforced, help is updated, and automated/manual validation passes.
