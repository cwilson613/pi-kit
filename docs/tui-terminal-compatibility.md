# Operator terminal compatibility testing

Prepare a fixed test build and launchers for installed macOS terminal clients:

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

Known browser limitation: the generic menu renders a `/` search hint, but this
browser increment does not yet route search input. Work shows current Workbench
summaries; execution/evidence navigation and persistent inline layout remain pending.
