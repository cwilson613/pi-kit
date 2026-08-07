+++
title = "Improvement inventory"
tags = ["backlog", "tui", "ux", "improvements"]
+++

# Improvement inventory

A collection of operator-requested improvements deferred from the main Omegon work. Entries preserve the original intent while adding testable behavior and implementation notes as they become known.

## Assignable designs

- [TUI shell mode toggle and exit semantics](tui-shell-mode-toggle-and-exit-semantics.md) — reversible PTY shell mode with deterministic TUI restoration.
- [TUI hyperlink activation and target dispatch](tui-hyperlink-activation-and-target-dispatch.md) — semantic link rendering, hit-testing, and safe editor/OS dispatch.
- [Provider-neutral multimodal content validation](provider-neutral-multimodal-content-validation.md) — cross-provider capability and wire-shape validation before dispatch.

## TUI shell mode toggle and exit semantics

**Type:** Planned improvement  
**Status:** Proposed; not yet assessed  
**Surface:** Interactive TUI  
**Input:** `Ctrl+\``

### Requested behavior

- Pressing `Ctrl+\`` while in the normal TUI enters an interactive shell mode.
- Shell mode behaves like a normal terminal session.
- Typing `exit` leaves shell mode and returns to the existing Omegon TUI session.
- Pressing `Ctrl+\`` again also leaves shell mode and returns to the TUI.

### Acceptance criteria

1. `Ctrl+\`` enters shell mode without terminating or replacing the active Omegon session.
2. The shell inherits the appropriate working directory and terminal dimensions.
3. Ordinary terminal input and output work while shell mode is active.
4. `exit` cleanly restores the TUI, including terminal mode, alternate-screen state, cursor state, and redraw.
5. `Ctrl+\`` cleanly restores the TUI without leaking the shell process.
6. Repeated entry and exit do not accumulate child processes, corrupt the terminal, or lose conversation/session state.
7. The keybinding is discoverable in TUI help.

### Open implementation questions

- Whether shell mode should suspend the TUI renderer around a real PTY or be presented as an embedded terminal pane.
- Which shell is selected: `$SHELL`, the platform login shell, or an Omegon setting.
- Whether an active agent turn continues in the background while shell mode is open.
- How `Ctrl+\`` is intercepted when a foreground shell program has switched terminal modes.

## TUI hyperlink activation and target dispatch

**Type:** Investigation attached to the hyperlink-rendering bugfix  
**Status:** Proposed; not yet assessed  
**Surface:** Agent-response conversation segments

Investigate how a rendered OSC 8 or Markdown hyperlink should be opened when the operator clicks it. Rendering and hit-testing alone are insufficient: target activation needs a deterministic dispatch policy.

### Dispatch considerations

- Web URLs should normally open through the operating system's configured default browser.
- Local files may need to open through the operating system's default application for the target file extension.
- Source and text files may instead need to honor Omegon's configured `/editor` choice and its existing path/line/column invocation contract.
- Directory targets may need to open in the platform file manager rather than the editor.
- Relative paths must resolve against the correct project or message context before dispatch.
- Fragment identifiers, line anchors, and `file://` targets need explicit normalization rules.
- Unsupported, missing, malformed, or unsafe targets should produce a visible non-destructive error instead of being passed blindly to a shell.
- Target launch must use argument-safe process spawning and must not interpolate hyperlink contents into a shell command.

### Open policy question

Define precedence between Omegon's `/editor` configuration and OS file-association defaults. In particular, decide whether `/editor` applies to every local file, only recognized source/text extensions, or only links carrying an explicit editor intent.

## Provider-neutral multimodal content validation

**Type:** Bugfix and architectural hardening  
**Status:** Confirmed from runtime evidence; not yet assessed  
**Surface:** Provider switching, canonical conversation projection, multimodal requests

### Observed failure

After switching to an Anthropic route, a conversation turn containing an image attachment failed with:

```text
Anthropic 400 Bad Request: messages: text content blocks must be non-empty
```

The rendered turn contained an image and local path, but the provider request included an empty Anthropic text content block. This demonstrates that canonical content accepted on one provider route can become structurally invalid when projected onto another provider's wire format.

### Required controls

- Validate canonical message content before provider-specific serialization.
- Validate the serialized request against the selected provider's content-block invariants before network dispatch.
- Never emit empty text blocks for providers that reject them; omit the block or reject the turn locally with a typed diagnostic.
- Preserve image-only, tool-result, mixed text/image, and attachment-only messages across provider switches.
- Make unsupported cross-provider content conversion explicit rather than silently dropping, coercing, or fabricating placeholder content.
- Keep canonical conversation types provider-neutral while requiring each provider adapter to declare and enforce its accepted input capabilities.
- Perform validation after route selection and again after fallback/reroute, because the target provider may change between turns.
- Include provider, message role, block index/type, and violated invariant in local diagnostics without leaking attachment data or secrets.

### Regression matrix

Cover at minimum:

1. Image-only user message routed to Anthropic.
2. Mixed text and image message routed to Anthropic.
3. Tool result containing an image routed to Anthropic.
4. Conversation created under OpenAI/Codex and continued under Anthropic.
5. Conversation created under Anthropic and continued under OpenAI/Codex.
6. Empty canonical text adjacent to a valid image.
7. Provider fallback after the original adapter accepted a different content shape.
8. Unsupported media type or inaccessible local attachment.

### Architectural question

Determine whether capability negotiation should normalize canonical blocks into a provider-safe intermediate projection or whether adapters should return a typed `UnsupportedContent`/`InvalidContentBlock` result. The boundary must prevent malformed requests before provider token use and network dispatch.
