+++
title = "Improvement: TUI hyperlink activation and target dispatch"
tags = ["improvement", "bug", "tui", "hyperlinks", "osc8", "editor"]
+++

# Improvement: TUI hyperlink activation and target dispatch

**Status:** Rendering/click failure observed; activation policy unresolved  
**Suggested branch:** `fix/tui-hyperlink-activation`  
**Primary surfaces:** Conversation projection, wrapping/rendering, mouse input, target launching

## Assignment brief

Trace Markdown and OSC 8 links from agent output through semantic conversation projection, wrapping, TUI rendering, terminal output, and mouse hit-testing. Restore correct display and activation, then implement a safe target-dispatch policy covering web URLs, local files, directories, fragments, and configured `/editor` behavior.

## Observed evidence

Agent responses containing Markdown links render malformed text—for example, URL fragments visibly leak into tables—and links cannot be clicked or followed. This indicates loss or corruption of semantic link spans before they reach a terminal hyperlink region or TUI hit target.

## Scope

- Markdown inline-link and OSC 8 ingestion.
- Renderer-neutral semantic link spans.
- Width calculation, wrapping, clipping, tables, selection, and scrolling.
- Terminal OSC 8 emission where supported.
- Mouse hit-testing and keyboard fallback.
- Safe target normalization and dispatch.
- `/editor` versus OS-default precedence.

## Non-goals

- A general embedded web browser.
- Executing arbitrary URI schemes.
- Passing user/model-controlled targets to an interpolated shell command.
- Rewriting unrelated Markdown rendering.
- Assuming all terminals support clickable OSC 8 links or mouse events.

## Investigation targets

Search for Markdown parsing, ANSI/OSC sanitization, `OSC 8`, hyperlink, spans, visible width, wrapping, tables, mouse events, hit regions, terminal capability detection, `/editor`, file open, URL open, `open`/`xdg-open`/platform launch, and path/line/column parsing. Inspect the shared semantic projection before adding TUI-only parsing.

## Semantic contract

A projected link should preserve:

- visible label;
- normalized target;
- original target for diagnostics if safe;
- source kind (Markdown, OSC 8, explicit file reference);
- optional file line/column or fragment;
- wrapped display ranges/hit regions;
- activation intent when explicit.

Wrapping may split display spans, but all resulting cells must retain the same link identity. Width calculations operate on visible labels, never escape bytes or hidden URL text.

## Activation policy to resolve

Suggested baseline:

| Target | Default action |
|---|---|
| `http`/`https` | OS default browser |
| Directory | Platform file manager |
| Recognized source/text file | Configured `/editor` with path/line/column |
| Other local file | OS extension association |
| Unsupported scheme | Reject visibly and non-destructively |

Explicit editor intent may override extension policy. Define whether `/editor` applies to all local files, recognized text/source files, or explicit editor links only.

Relative paths resolve against explicit message/project provenance, not the current process cwd by accident. Normalize `file://`, fragments, and line anchors without allowing traversal outside an intended boundary when the action requires one.

## Security constraints

- Use argument-array process spawning only.
- Allowlist schemes; reject control characters and malformed targets.
- Do not execute links or command-like schemes.
- Require ordinary operator activation; rendering alone never launches.
- Avoid leaking private local paths into OSC 8 output when the display policy forbids it.
- Report launch failures without exposing sensitive environment details.

## Implementation sequence

1. Reproduce Markdown and raw OSC 8 failures in projection/render tests.
2. Introduce or repair semantic link spans below the TUI renderer.
3. Make wrapping/table/selection calculations link-aware.
4. Emit balanced OSC 8 boundaries and/or stable hit regions.
5. Add click and keyboard activation.
6. Implement normalized target classification.
7. Route web/file/directory/editor targets through safe platform launch adapters.
8. Add capability fallback and visible errors.

## Acceptance criteria

1. Markdown links render their labels without leaking syntax or hidden URLs.
2. OSC 8 links survive projection and render with balanced escape boundaries.
3. Links remain correct through wrapping, tables, clipping, scrolling, and selection.
4. Clicking a visible link activates exactly that target when mouse support is enabled.
5. A keyboard fallback can open the selected/focused link.
6. Web links use the OS browser; directories use the file manager.
7. Source/text links honor the resolved `/editor` policy and line/column data.
8. Other local files follow the selected OS-association policy.
9. Unsupported or malformed targets are rejected safely.
10. Non-supporting terminals still show readable labels and copyable targets.

## Regression plan

Cover inline Markdown links, labels differing from URLs, raw OSC 8, multiple links per line, wrapped links, Markdown tables, Unicode labels, nested formatting, selected text, clipped/scrolled content, web/file/directory targets, editor line anchors, unsupported schemes, launch failure, and terminal capability off.

## Validation

Run projection/render/input tests plus:

```bash
cargo test -p omegon <hyperlink-filter>
just clippy-changed
git diff --check
```

Perform headed TUI verification in terminals with and without OSC 8/mouse support.

## Dependencies and conflict risks

Likely conflicts include semantic conversation projections, Markdown rendering, wrapping, mouse dispatch, `/editor`, and platform process launching. Keep target dispatch behind a narrow adapter so rendering can be reviewed independently from OS launching.

## Definition of done

Semantic links survive every rendering transformation, hit regions are correct, activation follows a documented safe policy, editor/OS precedence is tested, unsupported terminals degrade readably, and automated/manual validation passes.
