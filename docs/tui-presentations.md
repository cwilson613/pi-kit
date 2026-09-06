# Inline and fullscreen TUI

`om` opens a small inline composer with Active detail. Completed prompts and
responses enter the terminal's normal scrollback. `omegon` opens the fullscreen
workspace with Full detail. Both use the same session, editor, tools, permissions,
Project browser, and runtime.

| Selection | Terminal layout | Detail |
|---|---|---|
| `om` | Inline, up to eight live rows | Active |
| `omegon` | Fullscreen workspace | Full |
| `om --ui full` | Inline | Full |
| `omegon --ui active` | Fullscreen | Active |

`--tui inline|fullscreen` selects layout independently. Each preference resolves
from the command line, then the selected profile, then the entry default. Running
the binary directly uses the `omegon` defaults. Headless commands do not acquire
terminal modes because of these preferences.

Use `/ui` or `/settings ui` to discover both controls. `/ui terminal inline` and
`/ui terminal fullscreen` change the current session's base layout. `/ui active`
and `/ui full` save an explicit detail preference. Ctrl+G cycles Active and Full.
Legacy `om`, `lean`, and `slim` detail values now select Active.

An explicit profile overrides either entry's corresponding default:

```json
{
  "uiTerminal": "inline",
  "uiPresentation": "full"
}
```

Leave a field absent to retain that entry's default. Starting a session with a
command-line override, or saving an unrelated setting, does not save the inferred
layout or detail. Existing explicit Active/Full preferences continue to apply.

F2 opens Project in fullscreen. Menus, reference pickers, inspectors, tutorials,
and permission decisions also use the shared fullscreen widgets. Closing them
returns to the selected base and preserves the draft. Changing the base while a
rich view is open takes effect after closing that view. Mouse capture is disabled
in inline so the terminal can select text; fullscreen retains the mouse preference.

Active scrollback groups completed tool outcomes. Full includes detailed tool
arguments/results and reasoning when available. Images have text and path
references in scrollback. Full detail in inline does not create persistent
workspace panels; requested panels become available in fullscreen.

Completed output publishes in bounded batches. While the model streams, only the
small live preview changes. Returning from Project catches up without replaying
already published output. `/session-export scrollback` is an explicit snapshot and
can intentionally repeat history; it does not reset automatic publication. After
an uncertain terminal write, automatic publication stops to avoid duplication.
The status points to fullscreen history or `/session-export`. Resuming or replacing
a conversation starts a new publication boundary; existing terminal history remains.

See [captured acceptance](tui-captured-acceptance.md) for automated fixture runs and
[terminal compatibility](tui-terminal-compatibility.md) for native client evidence.
