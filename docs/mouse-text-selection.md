---
id: mouse-text-selection
title: Mouse text selection — EnableMouseCapture blocks native terminal selection
status: exploring
tags: [tui, ux, mouse, clipboard, accessibility]
open_questions: []
jj_change_id: xvwqvszpzzzssvzlospplutkqztvprwx
issue_type: bug
priority: 1
---

# Mouse text selection — EnableMouseCapture blocks native terminal selection

## Overview

EnableMouseCapture grabs all mouse events for scroll-wheel handling, which prevents the terminal emulator from doing native text selection (click-drag-copy). This is a fundamental tradeoff in crossterm/ratatui apps. Need to find an approach that preserves scroll support while restoring text selection.

## Research

### Approaches used by other TUI apps

1. **Shift+click bypass** — Most terminals (iTerm2, Kitty, Alacritty, WezTerm) let users hold Shift while clicking to bypass app mouse capture and use native selection. This is a terminal feature, not app-controlled. Problem: users don't know about it.

2. **Don't capture mouse** — Remove `EnableMouseCapture` entirely. Lose scroll-wheel support but regain native selection. Many TUI apps (htop, lazygit) don't capture mouse and still work fine. Scroll is handled by Page Up/Down or arrow keys instead.

3. **Toggle mouse capture** — Use a keybinding (e.g., Ctrl+M) to toggle mouse capture on/off. When off, native selection works. When on, scroll works. Zellij uses this approach.

4. **Use only scroll events** — crossterm has `EnableMouseCapture` (all events) vs more granular options. Unfortunately crossterm's mouse capture is all-or-nothing for standard terminals. Some terminals support SGR-Pixels or other extended protocols that could theoretically allow selective capture.

5. **Implement in-app text selection** — Handle mouse click+drag events ourselves, maintain a selection buffer, and copy to clipboard on release. This is what VS Code's integrated terminal does. Most complex but most complete.

Given the junior engineer persona: they don't know about Shift+click. They try to select text, it doesn't work, they think the app is broken. The simplest fix that preserves the most functionality is approach 3 (toggle) with a clear indicator in the footer, or approach 2 (just drop mouse capture — scroll via keyboard is fine).

## Open Questions

*No open questions.*
