---
id: clipboard-image-paste
title: "Clipboard image paste into chat/messages"
status: exploring
parent: markdown-viewport
tags: [ux, clipboard, images, attachments, chat]
open_questions: []
issue_type: bug
priority: 2
---

# Clipboard image paste into chat/messages

## Overview

Investigate and fix the failure path where a user cannot paste an image into the chat/message composer for inspection. Design should cover attachment intake, user feedback on paste success/failure, and compatibility with the existing read/view/render image flows.

## Research

### Current failure path diagnosis

Investigated vendor/pi-mono clipboard image paste path. In interactive mode, Ctrl+V invokes handleClipboardImagePaste() in vendor/pi-mono/packages/coding-agent/src/modes/interactive/interactive-mode.ts, which calls readClipboardImage() and silently returns on any failure. On this macOS environment, the native clipboard bridge is unavailable because vendor/pi-mono/packages/coding-agent/src/utils/clipboard-native.ts requires '@cwilson613/clipboard', but the vendored workspace currently has '@mariozechner/clipboard' installed in vendor/pi-mono/node_modules and no '@cwilson613/clipboard' module. Result: clipboard import resolves to null, readClipboardImage() returns null, and paste fails silently with no operator feedback.

### Rust TUI diagnosis (March 2026)

The TS-era fix is obsolete — Omegon is now a Rust TUI. The Rust implementation in `tui/mod.rs` has `clipboard_image_to_temp()` which correctly extracts images from the macOS clipboard via AppleScript. Tested: works standalone.

The bug is in event routing:
1. `EnableBracketedPaste` is active, which causes the terminal to intercept Ctrl+V
2. When clipboard has text, terminal sends an `Event::Paste(text)` — this works for text paste
3. When clipboard has an image (no text), the terminal sends... nothing. No Key event, no Paste event.
4. The `Event::Key(Char('v'), CONTROL)` handler that calls `clipboard_image_to_temp()` never fires

This is a fundamental issue with bracketed paste mode — the terminal owns Ctrl+V and doesn't forward it as a key event.

Possible fixes:
- **Poll clipboard on a timer** — Check clipboard content periodically and show a paste indicator. Too invasive.
- **Use a different keybinding** — e.g., Ctrl+Shift+V or a slash command `/paste`. Discoverable?
- **Disable bracketed paste** — Then Ctrl+V arrives as a Key event, but multi-line paste breaks (each line becomes a separate event).
- **Add a /paste command** — Most reliable. `/paste` checks clipboard for images, attaches if found. Works regardless of terminal paste mode.
- **Use OSC 52 clipboard protocol** — Some terminals support reading clipboard via escape sequences, but this is not universal and has security implications.

## Decisions

### Decision: Clipboard image paste should tolerate both clipboard package scopes and surface operator-visible failure feedback

**Status:** decided
**Rationale:** The immediate regression is a missing native clipboard module due to package-scope mismatch in the vendored pi workspace. The paste path should try both known package scopes to remain compatible across rename/fork states, and the interactive handler should stop failing silently so operators can tell whether no image was present versus clipboard access/setup failed.

## Open Questions

*No open questions.*

## Implementation Notes

### File Scope

- `vendor/pi-mono/packages/coding-agent/src/utils/clipboard-native.ts` (modified) — Load native clipboard bridge from either known package scope to survive vendored rename/fork mismatch.
- `vendor/pi-mono/packages/coding-agent/src/modes/interactive/interactive-mode.ts` (modified) — Stop failing silently on image paste; emit status/warning feedback and preserve temp-file insertion behavior.

### Constraints

- Preserve existing Ctrl+V paste-image workflow and temp-file handoff to view/read.
- Do not silently broaden behavior beyond image paste; only improve module loading compatibility and operator feedback.
- Keep non-image clipboard cases non-fatal, but visible enough to debug.
