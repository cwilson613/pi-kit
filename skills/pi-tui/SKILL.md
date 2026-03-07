---
name: pi-tui
description: Pi TUI component patterns and conventions for building extension UIs. Covers the Component interface, overlays, built-in components, keyboard handling, theming, footer/widget/header APIs, and common pitfalls. Use when creating or modifying TUI components in pi extensions.
globs:
  - extensions/**/overlay*.ts
  - extensions/**/footer*.ts
  - extensions/**/dashboard/**
---

# Pi TUI — Extension UI Patterns

This skill covers the TUI layer for pi extensions. Read alongside `pi-extensions/SKILL.md` for the full extension API.

## Package Geography

```
@mariozechner/pi-coding-agent     — ExtensionAPI, Theme, DynamicBorder, BorderedLoader, CustomEditor, etc.
@mariozechner/pi-tui              — Component, Container, TUI, Text, Box, SelectList, etc.
@sinclair/typebox                 — Type schema for tool parameters
```

All three are available via pi's jiti loader at runtime. **Do NOT `npm install` them** — they resolve through the alias map in the extension loader.

> ⚠️ Value imports from `@mariozechner/pi-tui` (e.g. `truncateToWidth`, `matchesKey`, `visibleWidth`) work inside pi but **fail under `tsx` or `node` directly** because pi-tui is a nested dependency of pi-coding-agent. Tests that need these must either mock them or run through pi.

## Component Interface

Every TUI component implements:

```typescript
interface Component {
  render(width: number): string[];   // Lines ≤ width chars each
  handleInput?(data: string): void;  // Keyboard when focused
  wantsKeyRelease?: boolean;         // Kitty protocol key releases
  invalidate(): void;                // Clear cached render state
}
```

**Critical rules:**
1. Each line from `render()` must not exceed `width`. Use `truncateToWidth(line, width)`.
2. Styles reset at line boundaries — reapply ANSI per line or use `wrapTextWithAnsi()`.
3. Cache render output; invalidate on state change + call `tui.requestRender()`.

## Imports Cheat Sheet

```typescript
// Types (compile-time only — safe everywhere)
import type { ExtensionAPI, ExtensionCommandContext, Theme } from "@mariozechner/pi-coding-agent";
import type { Component, OverlayHandle, OverlayOptions, TUI } from "@mariozechner/pi-tui";

// Values from pi-coding-agent (re-exported, always available)
import { DynamicBorder, BorderedLoader, CustomEditor } from "@mariozechner/pi-coding-agent";
import { getMarkdownTheme, getSelectListTheme, getSettingsListTheme, highlightCode } from "@mariozechner/pi-coding-agent";

// Values from pi-tui (work via jiti at runtime only)
import { matchesKey, Key, truncateToWidth, visibleWidth, wrapTextWithAnsi } from "@mariozechner/pi-tui";
import { Container, Text, Box, Spacer, Markdown, Image } from "@mariozechner/pi-tui";
import { SelectList, SettingsList } from "@mariozechner/pi-tui";
import type { SelectItem, SelectListTheme, SettingItem } from "@mariozechner/pi-tui";

// TypeBox for tool parameter schemas
import { Type } from "@sinclair/typebox";
```

## Displaying UI — The Five Surfaces

### 1. `ctx.ui.custom<T>()` — Full-screen or Overlay Component

The primary way to show interactive UI. Blocks until `done()` is called.

```typescript
const result = await ctx.ui.custom<string | null>(
  (tui, theme, keybindings, done) => {
    // Factory — return a Component (with optional dispose)
    return new MyComponent(tui, theme, done);
  },
  {
    overlay: true,                    // Render on top of chat
    overlayOptions: { ... },          // Position/size
    onHandle: (handle) => { ... },    // OverlayHandle for visibility control
  }
);
```

**Factory signature:** `(tui: TUI, theme: Theme, keybindings: KeybindingsManager, done: (result: T) => void) => Component & { dispose?(): void }`

**Return type options:**
- A class implementing `Component` + optional `dispose()`
- An inline object: `{ render, invalidate, handleInput, dispose? }`

### 2. `ctx.ui.setFooter()` — Custom Footer

```typescript
ctx.ui.setFooter((tui, theme, footerData) => ({
  render(width: number): string[] {
    const branch = footerData.getGitBranch();
    const statuses = footerData.getExtensionStatuses();
    return [truncateToWidth(`${branch} | ...`, width)];
  },
  invalidate() {},
  dispose: footerData.onBranchChange(() => tui.requestRender()),
}));

ctx.ui.setFooter(undefined);  // Restore default
```

**`footerData` provides:** `getGitBranch(): string | null`, `getExtensionStatuses(): ReadonlyMap<string, string>`, `onBranchChange(cb): unsubscribe`.

> ⚠️ Default footer renders 2 lines. Your replacement must handle all display.

### 3. `ctx.ui.setWidget()` — Persistent Content Above/Below Editor

```typescript
// Simple string array
ctx.ui.setWidget("my-key", ["Line 1", "Line 2"], { placement: "belowEditor" });

// Component factory (has theme access)
ctx.ui.setWidget("my-key", (tui, theme) => ({
  render: () => [theme.fg("accent", "● Active")],
  invalidate: () => {},
}));

ctx.ui.setWidget("my-key", undefined);  // Remove
```

### 4. `ctx.ui.setStatus()` — Footer Status Line

```typescript
ctx.ui.setStatus("my-ext", theme.fg("accent", "● running"));
ctx.ui.setStatus("my-ext", undefined);  // Clear
```

### 5. `ctx.ui.setHeader()` — Custom Header

```typescript
ctx.ui.setHeader((tui, theme) => ({
  render(width) { return [theme.fg("accent", "═".repeat(width))]; },
  invalidate() {},
}));
ctx.ui.setHeader(undefined);  // Restore default
```

## Overlay Positioning

```typescript
overlayOptions: {
  // Size
  width: "40%",           // SizeValue: number | `${number}%`
  minWidth: 40,           // Minimum columns
  maxHeight: "80%",       // Truncates render output

  // Position — anchor-based (default: "center")
  anchor: "right-center", // 9 positions: center, top-left, top-center, top-right,
                           //   left-center, right-center, bottom-left, bottom-center, bottom-right
  offsetX: -2,            // Offset from anchor position
  offsetY: 0,

  // Position — absolute/percentage (alternative to anchor)
  row: "25%",             // From top
  col: 10,                // From left

  // Margins
  margin: 2,              // All sides, or { top, right, bottom, left }

  // Responsive hiding
  visible: (termWidth, termHeight) => termWidth >= 100,
}
```

**OverlayHandle** (from `onHandle` callback):
- `handle.setHidden(true/false)` — Toggle visibility
- `handle.isHidden()` — Check state
- `handle.hide()` — Permanently remove

## Keyboard Handling

```typescript
import { matchesKey, Key } from "@mariozechner/pi-tui";

handleInput(data: string): void {
  if (matchesKey(data, Key.escape))     { /* ... */ }
  if (matchesKey(data, Key.enter))      { /* ... */ }
  if (matchesKey(data, Key.up))         { /* ... */ }
  if (matchesKey(data, Key.down))       { /* ... */ }
  if (matchesKey(data, Key.tab))        { /* ... */ }
  if (matchesKey(data, Key.ctrl("c")))  { /* ... */ }
  if (matchesKey(data, Key.ctrlShift("b")))  { /* ... */ }
  // String format also works: "escape", "ctrl+c", "shift+tab"
}
```

After handling input, call `tui.requestRender()` to trigger a redraw.

## Built-in Components

| Component | Import | Use for |
|-----------|--------|---------|
| `Container` | pi-tui | Group children vertically |
| `Text` | pi-tui | Multi-line text with wrapping |
| `Box` | pi-tui | Container with padding/background |
| `Spacer` | pi-tui | Empty vertical space (`new Spacer(2)`) |
| `Markdown` | pi-tui | Rendered markdown with syntax highlighting |
| `Image` | pi-tui | Terminal images (Kitty/iTerm2/Ghostty/WezTerm) |
| `SelectList` | pi-tui | Interactive item selection |
| `SettingsList` | pi-tui | Toggle-style settings |
| `DynamicBorder` | pi-coding-agent | Styled horizontal border |
| `BorderedLoader` | pi-coding-agent | Spinner with cancel support |
| `CustomEditor` | pi-coding-agent | Base class for custom editors |

### SelectList

```typescript
import { SelectList } from "@mariozechner/pi-tui";
import type { SelectItem } from "@mariozechner/pi-tui";

const items: SelectItem[] = [
  { value: "a", label: "Option A", description: "Details" },
  { value: "b", label: "Option B" },
];

const list = new SelectList(items, visibleCount, {
  selectedPrefix: (t) => theme.fg("accent", t),
  selectedText: (t) => theme.fg("accent", t),
  description: (t) => theme.fg("muted", t),
  scrollInfo: (t) => theme.fg("dim", t),
  noMatch: (t) => theme.fg("warning", t),
});
list.onSelect = (item) => done(item.value);
list.onCancel = () => done(null);
```

## Theming

**Always use the `theme` from the callback** — never import or hardcode colors.

```typescript
// Foreground
theme.fg("accent", "text")      // Styled text
theme.fg("muted", "text")       // Subdued
theme.fg("dim", "text")         // Very subdued
theme.fg("success", "text")     // Green
theme.fg("error", "text")       // Red
theme.fg("warning", "text")     // Yellow
theme.fg("border", "─")         // Border color
theme.bold("text")              // Bold

// Background
theme.bg("selectedBg", "text")
theme.bg("toolSuccessBg", "text")
```

**Full color list:** text, accent, muted, dim, success, error, warning, border, borderAccent, borderMuted, mdHeading, mdLink, mdCode, syntaxKeyword, syntaxString, syntaxComment, etc.

### Theme Change Handling

When the theme changes, `invalidate()` is called. If your component caches themed strings, rebuild them:

```typescript
class MyComponent extends Container {
  private message: string;

  constructor(message: string) {
    super();
    this.message = message;
    this.rebuild();
  }

  private rebuild(): void {
    this.clear();
    // Use current theme colors — will be re-called on theme change
    this.addChild(new Text(theme.fg("accent", this.message), 1, 0));
  }

  override invalidate(): void {
    super.invalidate();
    this.rebuild();
  }
}
```

## Common Patterns

### Bordered Box Helper

```typescript
protected box(lines: string[], width: number, title?: string): string[] {
  const th = this.theme;
  const innerW = Math.max(1, width - 2);
  const result: string[] = [];

  const titleStr = title ? truncateToWidth(` ${title} `, innerW) : "";
  const titleW = visibleWidth(titleStr);
  const topLeft = "─".repeat(Math.floor((innerW - titleW) / 2));
  const topRight = "─".repeat(Math.max(0, innerW - titleW - topLeft.length));
  result.push(th.fg("border", `╭${topLeft}`) + th.fg("accent", titleStr) + th.fg("border", `${topRight}╮`));

  for (const line of lines) {
    result.push(th.fg("border", "│") + truncateToWidth(line, innerW, "…", true) + th.fg("border", "│"));
  }

  result.push(th.fg("border", `╰${"─".repeat(innerW)}╯`));
  return result;
}
```

### Live-Updating Overlay (~30 FPS)

```typescript
class LivePanel {
  private interval: ReturnType<typeof setInterval> | null = null;

  constructor(private tui: TUI, private theme: Theme, private done: () => void) {
    this.interval = setInterval(() => {
      this.tui.requestRender();  // Triggers render() call
    }, 1000 / 30);
  }

  render(width: number): string[] { /* ... */ }
  handleInput(data: string): void {
    if (matchesKey(data, "escape")) { this.dispose(); this.done(); }
  }
  invalidate(): void {}
  dispose(): void {
    if (this.interval) { clearInterval(this.interval); this.interval = null; }
  }
}
```

### Responsive Sidepanel

```typescript
await ctx.ui.custom<void>(
  (tui, theme, _kb, done) => new SidePanel(tui, theme, done),
  {
    overlay: true,
    overlayOptions: {
      anchor: "right-center",
      width: "25%",
      minWidth: 30,
      margin: { right: 1 },
      visible: (termWidth) => termWidth >= 100,  // Auto-hide on narrow terminals
    },
  }
);
```

### Toggle Overlay Visibility

```typescript
let handle: OverlayHandle | null = null;

await ctx.ui.custom<void>(
  (tui, theme, _kb, done) => new MyPanel(tui, theme, done),
  {
    overlay: true,
    overlayOptions: { anchor: "right-center", width: "30%" },
    onHandle: (h) => { handle = h; },
  }
);

// Elsewhere (e.g. in a shortcut handler):
handle?.setHidden(!handle.isHidden());
```

### Cross-Extension Event-Driven Updates

```typescript
// In extension init:
const EVENT_NAME = "my-ext:update";
pi.events.emit(EVENT_NAME, { status: "active" });

// In overlay constructor:
this.unsubscribe = pi.events.on(EVENT_NAME, (data) => {
  this.state = data;
  this.tui.requestRender();
});

// In dispose:
dispose(): void { this.unsubscribe?.(); }
```

### Shared State via Symbol.for

```typescript
const STATE_KEY = Symbol.for("pi:my-extension-state");

// Writer
(globalThis as any)[STATE_KEY] = { count: 0, items: [] };

// Reader (any extension)
const state = (globalThis as any)[Symbol.for("pi:my-extension-state")];
```

## Testability Pattern — ThemeFn Bridge

Separate pure rendering logic from pi-tui dependencies for unit testing:

```typescript
// overlay-data.ts (pure logic, testable)
type ThemeFn = (color: string, text: string) => string;

export function renderStatusLine(state: MyState, thFn: ThemeFn, width: number): string {
  return thFn("accent", state.label) + " " + thFn("dim", `${state.count} items`);
}

// overlay.ts (pi-tui bridge, not unit-tested)
import type { Theme } from "@mariozechner/pi-coding-agent";
const thFn: ThemeFn = (color, text) => theme.fg(color as any, text);
const line = renderStatusLine(state, thFn, width);
```

In tests, use an identity `thFn`: `const thFn = (_c: string, t: string) => t;`

## Pitfalls

| Pitfall | Fix |
|---------|-----|
| Lines exceed `width` | Always `truncateToWidth(line, width)` |
| Overlay doesn't update | Call `tui.requestRender()` after state changes |
| Theme colors stale after theme switch | Override `invalidate()` to rebuild themed content |
| `Ctrl+Shift+D` shortcut doesn't fire | Hardcoded as pi-tui debug key — use a different binding |
| `stdio: "inherit"` in child process | Use `stdio: "pipe"` — inherit corrupts TUI |
| Overlay reuse after close | Create fresh instances — overlays dispose on close |
| `process.stderr.write()` corrupts TUI | Write to a log file instead |
| Footer `render()` returns wrong line count | Default footer is 2 lines; match or replace entirely |
| pi-tui imports fail in tests | Mock them, or only test pure logic (ThemeFn pattern) |
| `ctx.ui.custom()` called without `overlay: true` | Takes over the full screen — usually you want overlay |

## Render Caching Pattern

```typescript
class CachedComponent {
  private cachedWidth?: number;
  private cachedLines?: string[];

  render(width: number): string[] {
    if (this.cachedLines && this.cachedWidth === width) return this.cachedLines;
    this.cachedLines = this.computeLines(width);
    this.cachedWidth = width;
    return this.cachedLines;
  }

  invalidate(): void {
    this.cachedWidth = undefined;
    this.cachedLines = undefined;
  }
}
```

## IME / Focusable Support

For components with text cursors (CJK input method support):

```typescript
import { CURSOR_MARKER, type Focusable } from "@mariozechner/pi-tui";

class MyInput implements Component, Focusable {
  focused = false;

  render(width: number): string[] {
    const marker = this.focused ? CURSOR_MARKER : "";
    return [`> ${beforeCursor}${marker}\x1b[7m${atCursor}\x1b[27m${afterCursor}`];
  }
}
```

Container components with embedded inputs must propagate `focused` to child inputs for correct IME cursor positioning.

## Custom Editor

Extend `CustomEditor` (not `Editor`) for vim-mode or alternative key handling:

```typescript
import { CustomEditor } from "@mariozechner/pi-coding-agent";

class VimEditor extends CustomEditor {
  private mode: "normal" | "insert" = "insert";

  handleInput(data: string): void {
    if (this.mode === "insert") { super.handleInput(data); return; }
    // Normal mode: remap keys, call super.handleInput() for unhandled
    if (data === "i") { this.mode = "insert"; return; }
    if (data === "h") { super.handleInput("\x1b[D"); return; }  // Left
    super.handleInput(data);
  }
}

// Register:
ctx.ui.setEditorComponent((tui, theme, keybindings) => new VimEditor(tui, theme, keybindings));
ctx.ui.setEditorComponent(undefined);  // Restore default
```

## Tool Rendering

Tools can customize their display in the chat:

```typescript
pi.registerTool({
  name: "my_tool",
  // ...
  renderCall: (args, theme) => {
    return new Text(theme.fg("accent", `Running: ${args.action}`), 1, 0);
  },
  renderResult: (result, options, theme) => {
    if (!options.expanded) return undefined;  // Collapsed = default
    const mdTheme = getMarkdownTheme();
    return new Markdown(result.content[0]?.text ?? "", 0, 0, mdTheme);
  },
});
```
