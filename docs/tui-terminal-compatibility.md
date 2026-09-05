# Native terminal compatibility testing

The agent can drive installed macOS clients and inspect native window captures without operator input. Prepare a fixed test build first:

```sh
cargo build -p omegon --locked
python3 scripts/tui_operator_test.py prepare \
  --binary target/debug/omegon \
  --output /absolute/path/outside/checkout/operator-kit
```

Preparation requires Python 3.11+ and asciinema. It detects Ghostty, WezTerm,
iTerm2, kitty and Apple Terminal where installed; it installs nothing. The kit
contains a copy of the executable, runner, fixture, source/build manifest,
client versions and an operator checklist. Changed executable or runner hashes
fail verification before launch.

Open the desired client's `Launch.command` from Finder. The launcher opens a new
window using the client's existing configuration and fonts. WezTerm starts a
separate GUI instance; iTerm2 uses its documented AppleScript window command.
`Run.command` is for execution inside the intended terminal. The kit's local
`README.md` lists the complete test sequence.

Each trial starts in a temporary workspace with isolated HOME/configuration and
a local provider fixture. The first two prompts receive distinct replies. The
third requests a write after five seconds, allowing time to open F2 and switch
to Work before denying the request. A separate trial can cancel during that delay.
Exit with `/quit`; the wrapper then records results and waits for Enter before
closing. Short `/tmp` paths keep macOS Unix sockets within their path limit.

Evidence is written to `runs/<client>-<timestamp>/`:

- `terminal.cast`: local asciinema output recording, including resize events.
- `omegon.log` and `process.json`: application diagnostics and child identity.
- `manifest.json`: build/runner hashes, client environment, geometry, readiness,
  request count, denied-file check, exit information and artifact hashes.
- `RESULTS.md`: operator assessment, initially unassessed.

The fixture uses no paid inference. Recordings stay local. Terminal capability
variables are retained; real credentials and normal project configuration are
not inherited. No tmux layer is inserted. A nested asciinema/tmux preparation
attempt stalled during terminal queries; use the native launchers for this
baseline and treat multiplexer compatibility as a separate test.

Capture a native window screenshot with Shift+Cmd+4, then Space, and place it in
the run directory. A terminal-byte recording does not prove fonts, colors or GUI
rendering fidelity. Note the profile/font, viewport and failed step in RESULTS.md.
Startup readiness is not a complete compatibility pass.

Project browser filtering uses `/`, text entry and Backspace. Enter inspects a
match; empty results remain navigable. Escape leaves search, clears the filter,
and then closes the browser; F2 returns directly. Work shows current Workbench
summaries; execution/evidence navigation and persistent inline layout remain pending.

## Automated native trials

Build the small macOS helper and run the native driver against the prepared kit:

```sh
swiftc scripts/tui_native_macos.swift -o /tmp/omegon-tui-native-macos
python3 scripts/tui_native_acceptance.py \
  --bundle /absolute/path/outside/checkout/operator-kit \
  --helper /tmp/omegon-tui-native-macos \
  --output /absolute/path/outside/checkout/native-results
```

The output directory must be new. `--clients ghostty iterm kitty wezterm terminal`
selects clients; that list is also the default. Each trial stores window identity,
current text, native PNG captures, hashes and a machine-readable result. It links
the corresponding kit recording. Failures do not stop the other client trials.
The driver stops its own application process after a failed trial.

Ghostty uses native AppleScript key/paste actions and screen export. Its screen
export helper preserves all clipboard item types around the operation. iTerm2 uses
an explicit window ID and excludes retained history from current-view assertions.
Kitty enables remote control only on the new test instance's local Unix socket.
WezTerm targets the socket of the new GUI PID, avoiding an unrelated mux server.
Apple Terminal's scripting command appends Return, so its navigation scenario uses
combined sequences and does not claim physical-key or bracketed-paste coverage.

Ghostty changes its viewport through font zoom; WezTerm creates and removes an
owned split. iTerm2, kitty and Terminal resize their windows. The driver records
these distinctions rather than claiming identical window-manager coverage.

Native screenshot inspection can identify clipping and glyph problems. The driver
itself checks behavior and fixture results, not pixel aesthetics. A successful
trial is scoped to its recorded actions; physical keyboard layouts, drag selection
and system clipboard shortcuts are not implied by terminal input injection.

Use `--usability` with a build containing `tui-native-usability` to assert browser
search and empty results on the four clients with raw input controls, unique
permission choices on all five, and the send hint after narrowing the viewport.
The flag is explicit so older fixed kits remain usable as diagnostic baselines.
