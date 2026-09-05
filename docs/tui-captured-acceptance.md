# Captured TUI acceptance

Run the actual TUI with deterministic local streaming replies before and after shell changes. The runner requires Python 3.11+ and tmux on macOS or Linux. It does not install dependencies or call a paid inference provider.

```sh
just test-tui-captured /tmp/omegon-tui-run-001
```

The recipe builds the current debug executable, then launches it on a private tmux server. Use a new output directory for each changed hypothesis. To exercise a separately built artifact:

```sh
python3 scripts/tui_acceptance.py --binary target/dev-release/omegon --output /tmp/omegon-tui-run-002
```

The runner uses a temporary workspace and explicit user environment. Before sending the first draft, it opens F2, inspects the current session, switches to Work, and returns to the preserved draft. It submits two prompts through terminal keystrokes, checks for distinct fixture replies, resizes from 120×40 to 90×30, and captures the resulting terminal cells. It invokes `/session-export scrollback`, verifies the saved primary screen contains the second reply, and checks that fullscreen content and terminal mode preferences are restored. It then opens Settings during a gated provider request, switches to the Project browser Work tab, denies a real write-tool permission prompt, verifies that the Work tab is restored, and checks that the denied file remains absent. The isolated profile explicitly requires write approval; temporary paths alone do not require it. A timeout fails the run and retains the last screen. Cleanup closes only the owned tmux server and terminates its process group if terminal closure cannot stop it.

Inspect the numbered `.txt` screens, `omegon.log`, and `manifest.json`. The manifest identifies source revision and dirty files, executable path and SHA-256, process/start identity, capture times and dimensions, hashes, and request count. Keep these artifacts outside Git. The log may contain local fixture prompt/context data.

The fixture contract tests run without tmux or Cargo:

```sh
python3 scripts/tests/test_tui_acceptance.py
```

This foundation covers fresh-session startup, terminal input, streaming completion, second submission, resize, native transcript printing and fullscreen restoration, denied tool execution, project Sessions/Work navigation, draft preservation, approval visibility above the Project browser, and return to the same Work tab. It uses four local provider requests and no paid inference. It does not yet cover active-turn cancellation, saved-session resume, populated work-item execution/evidence drill-down, colors, or terminal-emulator portability. Add those scenarios as their shell contracts are implemented. The active reconstruction plan is `openspec/changes/tui-project-shell/`.
