# Native terminal compatibility testing

Routine testing uses the [headless captured runner](tui-captured-acceptance.md).
It exercises the real TUI in a private PTY with local fixture replies and no
visible windows or operator input. Native GUI tests are reserved for a dedicated
compatibility session because new windows can interrupt desktop work.

## Prepare a fixed artifact

```sh
cargo build -p omegon --locked
python3 scripts/tui_operator_test.py prepare \
  --binary target/debug/omegon --tui inline --ui active --entry om \
  --output /absolute/path/outside/checkout/operator-kit
```

Preparation requires Python 3.11+ and asciinema. It detects installed Ghostty,
WezTerm, iTerm2, kitty and Apple Terminal clients without opening them. The kit
copies the executable, launcher when selected, fixture and runner, and records
source revision, dirty paths, hashes and client versions. Verification rejects
changed executable or support files. Use a new bundle for a changed artifact.

Each trial uses a temporary workspace and isolated HOME/configuration. Terminal
capability variables survive; normal project credentials do not. A local provider
supplies deterministic replies and a denied-write scenario. No paid inference,
upload or nested tmux layer is involved in native trials.

## Dedicated native session

Compile the current helper; an old helper might omit hidden windows or other
Spaces from cleanup checks. Select only the clients needed for the hypothesis.

```sh
swiftc scripts/tui_native_macos.swift -o /tmp/omegon-tui-native-macos
python3 scripts/tui_native_acceptance.py \
  --bundle /absolute/path/outside/checkout/operator-kit \
  --helper /tmp/omegon-tui-native-macos \
  --output /absolute/path/outside/checkout/native-results \
  --interactive-gui --clients ghostty --usability
```

Both explicit GUI opt-in and client selection are required. The output directory
must be new. The agent drives input and captures the owned window; no operator
keystrokes, screenshots or assessment sheet are needed for this path.

Each trial records the window/session identity, current text, PNG captures,
recording path, hashes, fixture request count and denied-file outcome. The driver
attempts window cleanup on success and failure and checks closure separately from
recorder exit. Apple Terminal and iTerm windows containing other tabs/sessions are
not closed. Failed cleanup stops the matrix before another client opens.

The kit retains manual Launch.command/Run.command entry points for a deliberately
manual session. Their recorder prompt waits for Enter; that prompt alone never
proved GUI window closure. Routine automation uses the drivers described above.

## Coverage and limits

Ghostty uses native key/paste actions and screen export; the helper preserves
clipboard item types. iTerm2 crops retained history from current-view assertions.
Kitty remote control is confined to the owned instance's Unix socket. WezTerm uses
the socket of its owned GUI process. Apple Terminal do-script appends Return, so
physical Escape and native bracketed paste are not claimed there.

Ghostty resize uses font zoom and WezTerm uses an owned split. iTerm2, kitty and
Terminal use window geometry. Captured geometry records the actual result.
Screenshots establish appearance for those actions; injected input does not prove
physical key mappings, drag selection or system clipboard shortcuts.

The completed inline/fullscreen matrix and remaining fault/input limitations are
in [dual-presentation verification](../openspec/changes/tui-dual-presentation/verification.md).
The earlier matrix proved TUI outcomes and recorder exit, not native window
cleanup. Cleanup corrections after the desktop disruption report were validated
headlessly; no further GUI trials were launched.
